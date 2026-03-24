use anyhow::Result;
use rig::providers::openai;
use rig::agent::Agent;
use rig::client::CompletionClient;
use rig::completion::{Chat, Message};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

use crate::config::Config;
use crate::dispatch::{DispatchAction, Dispatcher};
use crate::identity::Identity;
use crate::logging::{DispatchLogger, DispatchLogEntry, preview};
use nocelium_channels::{Channel, Event, OutboundMessage, Payload};
use nocelium_memory::MemoryClient;
use nocelium_tools::{ShellTool, ReadFileTool, WriteFileTool, NomenSearchTool, NomenStoreTool,
    TelegramContext, TelegramSendTool, TelegramEditTool, TelegramDeleteTool, TelegramReactTool};

/// Per-chat conversation history.
type ChatHistories = Arc<RwLock<HashMap<String, Vec<Message>>>>;

/// Tracks active agent work for /stop cancellation.
struct ActiveTask {
    chat_id: String,
    cancel: CancellationToken,
    started: Instant,
}

/// Agent state shared across the loop.
pub struct AgentState {
    pub model: String,
    pub memory_connected: bool,
    pub start_time: Instant,
    pub npub: String,
    pub config_path: Option<PathBuf>,
}

/// Load the initial context memory from Nomen (`internal/{npub}/context`).
pub async fn load_initial_context(memory: &MemoryClient, npub: &str) -> Option<String> {
    let topic = format!("internal/{}/context", npub);
    tracing::info!(topic = %topic, "Loading initial context from Nomen");
    match memory.get(&topic).await {
        Ok(Some(mem)) => {
            tracing::info!(topic = %topic, len = mem.detail.len(), "Loaded initial context");
            Some(mem.detail)
        }
        Ok(None) => {
            tracing::info!(topic = %topic, "No initial context found");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, topic = %topic, "Failed to load initial context");
            None
        }
    }
}

/// Build a rig Agent from config using OpenAI-compatible provider (OpenRouter)
pub fn build_agent(
    config: &Config,
    identity: &Identity,
    memory: Option<Arc<MemoryClient>>,
    tg_ctx: Option<TelegramContext>,
    initial_context: Option<&str>,
) -> Result<Agent<openai::completion::CompletionModel>> {
    let api_key = config
        .provider
        .api_key
        .clone()
        .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
        .ok_or_else(|| anyhow::anyhow!("No API key. Set OPENROUTER_API_KEY or provider.api_key in config"))?;

    let base_url = config
        .provider
        .base_url
        .as_deref()
        .unwrap_or("https://openrouter.ai/api/v1");

    let client = openai::CompletionsClient::builder()
        .api_key(&api_key)
        .base_url(base_url)
        .build()?;

    let mut preamble = format!(
        "{}\n\nYour Nostr identity (npub): {}",
        config.agent.preamble,
        identity.npub()
    );

    if let Some(ctx) = initial_context {
        preamble.push_str("\n\n## Context\n");
        preamble.push_str(ctx);
    }

    tracing::debug!(preamble_len = preamble.len(), "Assembled preamble");

    let mut builder = client
        .agent(&config.provider.model)
        .preamble(&preamble)
        .max_tokens(config.agent.max_tokens)
        .default_max_turns(5)
        .tool(ShellTool)
        .tool(ReadFileTool)
        .tool(WriteFileTool);

    if let Some(ref mem) = memory {
        builder = builder
            .tool(NomenSearchTool::new(Arc::clone(mem)))
            .tool(NomenStoreTool::new(Arc::clone(mem)));
    }

    if let Some(ctx) = tg_ctx {
        builder = builder
            .tool(TelegramSendTool::new(ctx.clone()))
            .tool(TelegramEditTool::new(ctx.clone()))
            .tool(TelegramDeleteTool::new(ctx.clone()))
            .tool(TelegramReactTool::new(ctx));
    }

    Ok(builder.build())
}

/// Path to reload state file (written before exit, read on startup).
fn reload_state_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".nocelium/reload_state.json")
}

/// Save reload state so the next startup can send confirmation.
fn save_reload_state(chat_id: &str, channel: &str) {
    let state = serde_json::json!({
        "chat_id": chat_id,
        "channel": channel,
        "ts": chrono::Utc::now().to_rfc3339(),
    });
    if let Ok(json) = serde_json::to_string(&state) {
        let path = reload_state_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, json);
    }
}

/// Check for reload state and send confirmation. Returns true if confirmation was sent.
pub async fn send_reload_confirmation(channels: &HashMap<String, Arc<dyn Channel>>) -> bool {
    let path = reload_state_path();
    if !path.exists() {
        return false;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => { let _ = std::fs::remove_file(&path); return false; }
    };
    let _ = std::fs::remove_file(&path);

    let state: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return false,
    };

    let chat_id = state.get("chat_id").and_then(|v| v.as_str()).unwrap_or("");
    let channel_name = state.get("channel").and_then(|v| v.as_str()).unwrap_or("");

    if let Some(channel) = channels.get(channel_name) {
        let _ = channel.send(&OutboundMessage {
            chat_id: chat_id.to_string(),
            text: "✅ Reloaded successfully".into(),
            ..Default::default()
        }).await;
        tracing::info!(chat_id, channel = channel_name, "Sent reload confirmation");
        true
    } else {
        false
    }
}

/// The main agent loop: receive events → dispatch → think → respond
pub async fn run_loop(
    agent: &Agent<openai::completion::CompletionModel>,
    event_rx: &mut tokio::sync::mpsc::Receiver<Event>,
    channels: &HashMap<String, Arc<dyn Channel>>,
    dispatcher: &Dispatcher,
    memory: Option<&MemoryClient>,
    tg_ctx: Option<&TelegramContext>,
    state: AgentState,
) -> Result<()> {
    tracing::info!("Agent loop started. Waiting for events...");

    let histories: ChatHistories = Arc::new(RwLock::new(HashMap::new()));
    let active_tasks: Arc<RwLock<Vec<ActiveTask>>> = Arc::new(RwLock::new(Vec::new()));
    let dispatch_log = DispatchLogger::new().await;

    while let Some(event) = event_rx.recv().await {
        let rule = dispatcher.match_rule(&event.key);
        tracing::debug!(key = %event.key, "Dispatching event");

        match &rule.action {
            DispatchAction::Drop => {
                tracing::debug!(key = %event.key, "Dropping event");
                dispatch_log.log(DispatchLogEntry {
                    ts: chrono::Utc::now().to_rfc3339(),
                    key: event.key.clone(),
                    rule: rule.pattern.clone(),
                    action: "drop".into(),
                    channel: event.source.channel_name().map(|s| s.to_string()),
                    chat_id: event.source.chat_id().map(|s| s.to_string()),
                    sender_id: None, sender_name: None, message: None,
                    response: None, duration_ms: None, error: None,
                });
                continue;
            }
            DispatchAction::Handler { name } => {
                let handler_start = Instant::now();
                let mut error_msg = None;

                if let Some(prefix) = name.strip_prefix("store:") {
                    if let Some(mem) = memory {
                        if let Err(e) = crate::handlers::handle_store(&event, mem, prefix).await {
                            tracing::error!(error = %e, handler = %name, "Store handler failed");
                            error_msg = Some(e.to_string());
                        }
                    } else {
                        tracing::warn!(handler = %name, "Store handler requires memory, but memory is unavailable");
                        error_msg = Some("Memory unavailable".into());
                    }
                } else {
                    tracing::warn!(handler = %name, "Unknown handler");
                    error_msg = Some("Unknown handler".into());
                }

                dispatch_log.log(DispatchLogEntry {
                    ts: chrono::Utc::now().to_rfc3339(),
                    key: event.key.clone(),
                    rule: rule.pattern.clone(),
                    action: format!("handler:{name}"),
                    channel: event.source.channel_name().map(|s| s.to_string()),
                    chat_id: event.source.chat_id().map(|s| s.to_string()),
                    sender_id: None, sender_name: None,
                    message: match &event.payload {
                        Payload::Message(m) => Some(preview(&m.text, 200)),
                        _ => None,
                    },
                    response: None,
                    duration_ms: Some(handler_start.elapsed().as_millis() as u64),
                    error: error_msg,
                });
                continue;
            }
            DispatchAction::AgentTurn => {
                let text = match &event.payload {
                    Payload::Message(msg) => &msg.text,
                    _ => {
                        tracing::debug!(key = %event.key, "Non-message event, skipping agent turn");
                        continue;
                    }
                };

                if text.trim().is_empty() {
                    continue;
                }

                // Log dispatch immediately (before LLM processing)
                {
                    let (sender_id, sender_name) = match &event.source {
                        nocelium_channels::Source::Channel { sender_id, .. } => {
                            let name = match &event.payload {
                                Payload::Message(m) => m.sender_name.clone(),
                                _ => None,
                            };
                            (Some(sender_id.clone()), name)
                        }
                        _ => (None, None),
                    };
                    dispatch_log.log(DispatchLogEntry {
                        ts: chrono::Utc::now().to_rfc3339(),
                        key: event.key.clone(),
                        rule: rule.pattern.clone(),
                        action: "agent_turn".into(),
                        channel: event.source.channel_name().map(|s| s.to_string()),
                        chat_id: event.source.chat_id().map(|s| s.to_string()),
                        sender_id,
                        sender_name,
                        message: Some(preview(text, 200)),
                        response: None,
                        duration_ms: None,
                        error: None,
                    });
                }

                let trimmed = text.trim();
                let chat_key = event.source.chat_id().unwrap_or("local").to_string();
                let channel_name = event.source.channel_name().unwrap_or("stdio").to_string();

                // ── Commands ──

                if trimmed == "/reload" {
                    tracing::info!("Reload requested, validating config...");

                    // Validate config before restarting
                    let config_path = state.config_path.clone();
                    match Config::load_from_path(config_path.as_deref()) {
                        Ok(_) => {
                            if let Some(channel) = channels.get(channel_name.as_str()) {
                                let _ = channel.send(&OutboundMessage {
                                    chat_id: chat_key.clone(),
                                    text: "🔄 Reloading...".into(),
                                    ..Default::default()
                                }).await;
                            }
                            save_reload_state(&chat_key, &channel_name);
                            std::process::exit(0);
                        }
                        Err(e) => {
                            let msg = format!("❌ Config validation failed:\n```\n{e}\n```\nNot reloading.");
                            tracing::error!(error = %e, "Config validation failed");
                            if let Some(channel) = channels.get(channel_name.as_str()) {
                                let _ = channel.send(&OutboundMessage {
                                    chat_id: chat_key,
                                    text: msg,
                                    ..Default::default()
                                }).await;
                            }
                            continue;
                        }
                    }
                }

                if trimmed == "/reset" {
                    let cleared = {
                        let mut h = histories.write().await;
                        h.remove(&chat_key).map(|v| v.len()).unwrap_or(0)
                    };
                    tracing::info!(chat = %chat_key, messages_cleared = cleared, "Session reset");
                    if let Some(channel) = channels.get(channel_name.as_str()) {
                        let _ = channel.send(&OutboundMessage {
                            chat_id: chat_key,
                            text: format!("🔄 Session reset ({} messages cleared)", cleared),
                            ..Default::default()
                        }).await;
                    }
                    continue;
                }

                if trimmed == "/stop" {
                    let cancelled = {
                        let mut tasks = active_tasks.write().await;
                        let count = tasks.len();
                        let mut report = Vec::new();
                        for task in tasks.drain(..) {
                            let elapsed = task.started.elapsed();
                            task.cancel.cancel();
                            report.push(format!("  • chat {} ({:.1}s)", task.chat_id, elapsed.as_secs_f64()));
                        }
                        (count, report)
                    };
                    let msg = if cancelled.0 == 0 {
                        "🛑 No active tasks".into()
                    } else {
                        format!("🛑 Stopped {} task(s):\n{}", cancelled.0, cancelled.1.join("\n"))
                    };
                    tracing::info!(stopped = cancelled.0, "Stop command");
                    if let Some(channel) = channels.get(channel_name.as_str()) {
                        let _ = channel.send(&OutboundMessage {
                            chat_id: chat_key,
                            text: msg,
                            ..Default::default()
                        }).await;
                    }
                    continue;
                }

                if trimmed == "/status" {
                    let uptime = state.start_time.elapsed();
                    let history_info = {
                        let h = histories.read().await;
                        let total_msgs: usize = h.values().map(|v| v.len()).sum();
                        (h.len(), total_msgs)
                    };
                    let active_count = active_tasks.read().await.len();

                    let status = format!(
                        "📊 *Nocelium Status*\n\
                        \n\
                        *Identity:* `{}`\n\
                        *Model:* {}\n\
                        *Uptime:* {}m {}s\n\
                        *Memory:* {}\n\
                        *Active tasks:* {}\n\
                        *Chats:* {} ({} messages)",
                        state.npub,
                        state.model,
                        uptime.as_secs() / 60,
                        uptime.as_secs() % 60,
                        if state.memory_connected { "connected" } else { "unavailable" },
                        active_count,
                        history_info.0,
                        history_info.1,
                    );
                    if let Some(channel) = channels.get(channel_name.as_str()) {
                        let _ = channel.send(&OutboundMessage {
                            chat_id: chat_key,
                            text: status,
                            ..Default::default()
                        }).await;
                    }
                    continue;
                }

                // ── Agent Turn ──

                // Set Telegram context for tools
                if let (Some(ctx), Some(channel)) = (tg_ctx, channels.get("telegram")) {
                    let msg = match &event.payload {
                        Payload::Message(m) => Some(m),
                        _ => None,
                    };
                    ctx.set(
                        Arc::clone(channel),
                        chat_key.clone(),
                        msg.map(|m| m.id.clone()),
                        msg.and_then(|m| m.thread_id.clone()),
                    ).await;
                }

                tracing::debug!(message = %text, chat = %chat_key, "Processing message");
                let turn_start = Instant::now();

                let history = {
                    let h = histories.read().await;
                    h.get(&chat_key).cloned().unwrap_or_default()
                };

                // Register cancellable task
                let cancel_token = CancellationToken::new();
                {
                    let mut tasks = active_tasks.write().await;
                    tasks.push(ActiveTask {
                        chat_id: chat_key.clone(),
                        cancel: cancel_token.clone(),
                        started: Instant::now(),
                    });
                }

                // Run LLM with cancellation support
                let text_owned = text.to_string();
                let response = tokio::select! {
                    result = agent.chat(&text_owned, history.clone()) => {
                        match result {
                            Ok(resp) => Some(resp),
                            Err(e) => {
                                tracing::error!(error = %e, "Agent chat failed");
                                Some(format!("Error: {}", e))
                            }
                        }
                    }
                    _ = cancel_token.cancelled() => {
                        tracing::info!(chat = %chat_key, "Agent turn cancelled by /stop");
                        None
                    }
                };

                // Remove from active tasks
                {
                    let mut tasks = active_tasks.write().await;
                    tasks.retain(|t| t.chat_id != chat_key);
                }

                let response = match response {
                    Some(r) => r,
                    None => continue, // cancelled, no response
                };

                // Update conversation history
                {
                    let mut h = histories.write().await;
                    let entry = h.entry(chat_key.clone()).or_default();
                    entry.push(Message::User {
                        content: rig::one_or_many::OneOrMany::one(
                            rig::completion::message::UserContent::text(&text_owned)
                        ),
                    });
                    entry.push(Message::Assistant {
                        id: None,
                        content: rig::one_or_many::OneOrMany::one(
                            rig::completion::message::AssistantContent::text(&response)
                        ),
                    });

                    if entry.len() > 50 {
                        let drain_count = entry.len() - 50;
                        entry.drain(..drain_count);
                    }
                }

                // Send response
                let send_error = if let Some(channel) = channels.get(channel_name.as_str()) {
                    let outbound = OutboundMessage {
                        chat_id: chat_key.clone(),
                        text: response.clone(),
                        ..Default::default()
                    };
                    match channel.send(&outbound).await {
                        Ok(_) => None,
                        Err(e) => {
                            tracing::error!(error = %e, channel = %channel_name, "Failed to send response");
                            Some(e.to_string())
                        }
                    }
                } else {
                    tracing::error!(channel = %channel_name, "No channel found for response routing");
                    Some(format!("No channel: {channel_name}"))
                };

                // Log completion (separate from dispatch log above)
                if send_error.is_some() {
                    tracing::warn!(
                        key = %event.key,
                        duration_ms = turn_start.elapsed().as_millis() as u64,
                        error = ?send_error,
                        "Agent turn completed with send error"
                    );
                } else {
                    tracing::debug!(
                        key = %event.key,
                        duration_ms = turn_start.elapsed().as_millis() as u64,
                        "Agent turn completed"
                    );
                }
            }
        }
    }

    tracing::info!("Event channel closed, agent loop ending");
    Ok(())
}
