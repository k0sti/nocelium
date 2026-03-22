use anyhow::Result;
use rig::providers::openai;
use rig::agent::Agent;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use std::collections::HashMap;
use std::sync::Arc;

use crate::config::Config;
use crate::dispatch::{DispatchAction, Dispatcher};
use crate::identity::Identity;
use nocelium_channels::{Channel, Event, OutboundMessage, Payload};
use nocelium_memory::MemoryClient;
use nocelium_tools::{ShellTool, ReadFileTool, WriteFileTool, NomenSearchTool, NomenStoreTool};

/// Build a rig Agent from config using OpenAI-compatible provider (OpenRouter)
pub fn build_agent(
    config: &Config,
    identity: &Identity,
    memory: Option<Arc<MemoryClient>>,
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

    let preamble = format!(
        "{}\n\nYour Nostr identity (npub): {}",
        config.agent.preamble,
        identity.npub()
    );

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

    Ok(builder.build())
}

/// The main agent loop: receive events → dispatch → enrich → think → respond
pub async fn run_loop(
    agent: &Agent<openai::completion::CompletionModel>,
    event_rx: &mut tokio::sync::mpsc::Receiver<Event>,
    channels: &HashMap<String, Arc<dyn Channel>>,
    dispatcher: &Dispatcher,
    memory: Option<&MemoryClient>,
) -> Result<()> {
    tracing::info!("Agent loop started. Waiting for events...");

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

                tracing::debug!(message = %text, source = %event.key, "Processing message");

                // Context enrichment
                let enriched = match memory {
                    Some(mem) => enrich_with_context(mem, text).await,
                    None => text.clone(),
                };

                // Call LLM
                let response = match agent.prompt(&enriched).await {
                    Ok(resp) => resp,
                    Err(e) => {
                        tracing::error!(error = %e, "Agent prompt failed");
                        format!("Error: {}", e)
                    }
                };

                // Route response back to the source channel
                let channel_name = event.source.channel_name().unwrap_or("stdio");
                if let Some(channel) = channels.get(channel_name) {
                    let chat_id = event.source.chat_id().unwrap_or("local").to_string();
                    let outbound = OutboundMessage {
                        chat_id,
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

/// Search memory for context relevant to the user message.
async fn enrich_with_context(mem: &MemoryClient, message: &str) -> String {
    tracing::debug!(query = %message.trim(), "Searching memory for context");
    match mem.search(message.trim(), 5, None, None).await {
        Ok(memories) if !memories.is_empty() => {
            tracing::info!(count = memories.len(), "Found relevant memories");
            let context: Vec<String> = memories
                .iter()
                .map(|m| format!("- [{}]: {}", m.topic, m.detail))
                .collect();
            let enriched = format!("{message}\n\n## Relevant Context\n{}", context.join("\n"));
            tracing::debug!(enriched = %enriched, "Enriched prompt");
            enriched
        }
        Ok(_) => {
            tracing::debug!("No relevant memories found");
            message.to_string()
        }
        Err(e) => {
            tracing::warn!(error = %e, "Memory search failed, proceeding without context");
            message.to_string()
        }
    }
}
