use anyhow::Result;
use rig::providers::openai;
use rig::agent::Agent;
use rig::client::CompletionClient;
use rig::completion::Prompt;
use rig::streaming::StreamingPrompt;
use std::sync::Arc;

use crate::config::Config;
use crate::identity::Identity;
use nocelium_channels::Channel;
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

/// The main agent loop: receive → enrich → think → act → respond
pub async fn run_loop(
    agent: &Agent<openai::completion::CompletionModel>,
    channel: &mut dyn Channel,
    streaming: bool,
    memory: Option<&MemoryClient>,
) -> Result<()> {
    tracing::info!(streaming = streaming, "Agent loop started. Waiting for messages...");

    loop {
        match channel.receive().await {
            Ok(Some(message)) => {
                if message.trim().is_empty() {
                    continue;
                }

                if message.trim() == "/quit" || message.trim() == "/exit" {
                    tracing::info!("Exit command received");
                    channel.send("Goodbye!").await?;
                    break;
                }

                tracing::debug!(message = %message, "Received message");

                // Context enrichment: search memory for relevant context
                let enriched = match memory {
                    Some(mem) => enrich_with_context(mem, &message).await,
                    None => message.clone(),
                };

                if streaming {
                    use futures::StreamExt;
                    use rig::agent::{MultiTurnStreamItem, Text};
                    use rig::streaming::StreamedAssistantContent;

                    let mut stream = agent.stream_prompt(&enriched).await;

                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(MultiTurnStreamItem::StreamAssistantItem(
                                StreamedAssistantContent::Text(Text { text }),
                            )) => {
                                channel.send_chunk(&text).await?;
                            }
                            Ok(MultiTurnStreamItem::FinalResponse(_)) => {}
                            Err(e) => {
                                let error_msg = format!("\nError: {}", e);
                                tracing::error!(error = %e, "Streaming error");
                                channel.send(&error_msg).await?;
                            }
                            _ => {}
                        }
                    }
                    channel.send_chunk("\n").await?;
                    channel.flush().await?;
                } else {
                    match agent.prompt(&enriched).await {
                        Ok(response) => {
                            channel.send(&response).await?;
                        }
                        Err(e) => {
                            let error_msg = format!("Error: {}", e);
                            tracing::error!(error = %e, "Agent prompt failed");
                            channel.send(&error_msg).await?;
                        }
                    }
                }
            }
            Ok(None) => {
                tracing::info!("Channel closed");
                break;
            }
            Err(e) => {
                tracing::error!(error = %e, "Channel receive error");
                break;
            }
        }
    }

    Ok(())
}

/// Search memory for context relevant to the user message.
async fn enrich_with_context(mem: &MemoryClient, message: &str) -> String {
    match mem.search(message.trim(), 5, None, None).await {
        Ok(memories) if !memories.is_empty() => {
            let context: Vec<String> = memories
                .iter()
                .map(|m| format!("- [{}]: {}", m.topic, m.summary))
                .collect();
            format!("{message}\n\n## Relevant Context\n{}", context.join("\n"))
        }
        Ok(_) => message.to_string(),
        Err(e) => {
            tracing::warn!(error = %e, "Memory search failed, proceeding without context");
            message.to_string()
        }
    }
}
