//! Built-in event handlers for dispatch actions.

use nocelium_channels::{Event, Payload};
use nocelium_memory::MemoryClient;

/// Store handler — stores the event content to Nomen memory.
///
/// Dispatch config example:
/// ```toml
/// [[dispatch.rules]]
/// pattern = "telegram:message:*"
/// action = { type = "handler", "0" = "store:events/telegram" }
/// ```
///
/// The topic is `{prefix}/{chat_id}` and the event text is appended.
pub async fn handle_store(
    event: &Event,
    memory: &MemoryClient,
    topic_prefix: &str,
) -> anyhow::Result<()> {
    let text = match &event.payload {
        Payload::Message(msg) => &msg.text,
        Payload::Callback(cb) => &cb.data,
        _ => return Ok(()), // skip non-text events
    };

    if text.trim().is_empty() {
        return Ok(());
    }

    let chat_id = event.source.chat_id().unwrap_or("unknown");
    let topic = format!("{topic_prefix}/{chat_id}");

    // Get sender info
    let sender = match &event.payload {
        Payload::Message(msg) => msg.sender_name.as_deref().unwrap_or("unknown"),
        _ => "unknown",
    };

    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC");
    let entry = format!("[{timestamp}] {sender}: {text}");

    // Try to get existing memory and append
    match memory.get(&topic).await {
        Ok(Some(existing)) => {
            let updated = format!("{}\n{}", existing.detail, entry);
            memory.store(&topic, &updated, None, None).await?;
            tracing::debug!(topic = %topic, "Appended to existing memory");
        }
        Ok(None) => {
            memory.store(&topic, &entry, None, None).await?;
            tracing::debug!(topic = %topic, "Created new memory");
        }
        Err(e) => {
            tracing::warn!(error = %e, topic = %topic, "Failed to get existing memory, creating new");
            memory.store(&topic, &entry, None, None).await?;
        }
    }

    Ok(())
}
