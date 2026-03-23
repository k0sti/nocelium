use anyhow::Result;
use rig::providers::openai;
use rig::agent::Agent;
use rig::client::CompletionClient;
use rig::completion::{Chat, Message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::dispatch::{DispatchAction, Dispatcher};
use crate::identity::Identity;
use nocelium_channels::{Channel, Event, OutboundMessage, Payload};
use nocelium_memory::MemoryClient;
use nocelium_tools::{ShellTool, ReadFileTool, WriteFileTool, NomenSearchTool, NomenStoreTool,
    TelegramContext, TelegramSendTool, TelegramEditTool, TelegramDeleteTool, TelegramReactTool};

/// Per-chat conversation history.
type ChatHistories = Arc<RwLock<HashMap<String, Vec<Message>>>>;

/// Load the initial context memory from Nomen (`internal/{npub}/context`).
/// Returns the content string, or None if not found.
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

    // Assemble preamble: config preamble + identity + initial context from Nomen
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

/// The main agent loop: receive events → dispatch → think → respond
pub async fn run_loop(
    agent: &Agent<openai::completion::CompletionModel>,
    event_rx: &mut tokio::sync::mpsc::Receiver<Event>,
    channels: &HashMap<String, Arc<dyn Channel>>,
    dispatcher: &Dispatcher,
    _memory: Option<&MemoryClient>,
    tg_ctx: Option<&TelegramContext>,
) -> Result<()> {
    tracing::info!("Agent loop started. Waiting for events...");

    let histories: ChatHistories = Arc::new(RwLock::new(HashMap::new()));

    while let Some(event) = event_rx.recv().await {
        let rule = dispatcher.match_rule(&event.key);
        tracing::debug!(key = %event.key, "Dispatching event");

        match &rule.action {
            DispatchAction::Drop => {
                tracing::debug!(key = %event.key, "Dropping event");
                continue;
            }
            DispatchAction::Handler(name) => {
                tracing::warn!(handler = %name, "Handler dispatch not yet implemented");
                continue;
            }
            DispatchAction::AgentTurn => {
                // Extract text from message payload
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

                // Handle commands
                let trimmed = text.trim();

                if trimmed == "/reload" {
                    tracing::info!("Reload requested, restarting process");
                    let channel_name = event.source.channel_name().unwrap_or("stdio");
                    if let Some(channel) = channels.get(channel_name) {
                        let chat_key = event.source.chat_id().unwrap_or("local").to_string();
                        let _ = channel.send(&OutboundMessage {
                            chat_id: chat_key,
                            text: "🔄 Reloading...".into(),
                            ..Default::default()
                        }).await;
                    }
                    // Exit cleanly — systemd RestartSec=5 will restart us
                    std::process::exit(0);
                }

                if trimmed == "/reset" {
                    let chat_key = event.source.chat_id().unwrap_or("local").to_string();
                    let cleared = {
                        let mut h = histories.write().await;
                        h.remove(&chat_key).map(|v| v.len()).unwrap_or(0)
                    };
                    tracing::info!(chat = %chat_key, messages_cleared = cleared, "Session reset");
                    let channel_name = event.source.channel_name().unwrap_or("stdio");
                    if let Some(channel) = channels.get(channel_name) {
                        let _ = channel.send(&OutboundMessage {
                            chat_id: chat_key,
                            text: format!("🔄 Session reset ({} messages cleared)", cleared),
                            ..Default::default()
                        }).await;
                    }
                    continue;
                }

                // Set Telegram context for tools if this is a Telegram message
                if let (Some(ctx), Some(channel)) = (tg_ctx, channels.get("telegram")) {
                    let chat_id = event.source.chat_id().unwrap_or("").to_string();
                    let msg = match &event.payload {
                        Payload::Message(m) => Some(m),
                        _ => None,
                    };
                    ctx.set(
                        Arc::clone(channel),
                        chat_id,
                        msg.map(|m| m.id.clone()),
                        msg.and_then(|m| m.thread_id.clone()),
                    ).await;
                }

                let chat_key = event.source.chat_id().unwrap_or("local").to_string();
                tracing::debug!(message = %text, chat = %chat_key, "Processing message");

                // Get conversation history for this chat
                let history = {
                    let h = histories.read().await;
                    h.get(&chat_key).cloned().unwrap_or_default()
                };

                tracing::debug!(chat = %chat_key, history_len = history.len(), "Chat history");

                // Call LLM with conversation history
                let response = match agent.chat(text, history.clone()).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        tracing::error!(error = %e, "Agent chat failed");
                        format!("Error: {}", e)
                    }
                };

                // Update conversation history
                {
                    let mut h = histories.write().await;
                    let entry = h.entry(chat_key.clone()).or_default();
                    entry.push(Message::User {
                        content: rig::one_or_many::OneOrMany::one(
                            rig::completion::message::UserContent::text(text)
                        ),
                    });
                    entry.push(Message::Assistant {
                        id: None,
                        content: rig::one_or_many::OneOrMany::one(
                            rig::completion::message::AssistantContent::text(&response)
                        ),
                    });

                    // Keep history bounded (last 50 messages = 25 turns)
                    if entry.len() > 50 {
                        let drain_count = entry.len() - 50;
                        entry.drain(..drain_count);
                    }
                }

                // Route response back to the source channel
                let channel_name = event.source.channel_name().unwrap_or("stdio");
                if let Some(channel) = channels.get(channel_name) {
                    let outbound = OutboundMessage {
                        chat_id: chat_key,
                        text: response,
                        ..Default::default()
                    };
                    if let Err(e) = channel.send(&outbound).await {
                        tracing::error!(error = %e, channel = %channel_name, "Failed to send response");
                    }
                } else {
                    tracing::error!(channel = %channel_name, "No channel found for response routing");
                }
            }
        }
    }

    tracing::info!("Event channel closed, agent loop ending");
    Ok(())
}
