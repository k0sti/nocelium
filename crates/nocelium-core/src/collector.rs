use std::sync::Arc;

use nocelium_channels::Event;
use nocelium_memory::MemoryClient;

use crate::collected_message::{build_inbound_30100, build_outbound_30100};

/// Collects messages as kind 30100 events via Nomen's `message.store`.
///
/// Fire-and-forget: errors are logged, never propagated.
#[derive(Clone)]
pub struct MessageCollector {
    nomen: Arc<MemoryClient>,
    pub enabled: bool,
}

impl MessageCollector {
    pub fn new(nomen: Arc<MemoryClient>) -> Self {
        Self {
            nomen,
            enabled: true,
        }
    }

    /// Collect an inbound event. Skips non-message and non-channel events.
    pub async fn collect_inbound(&self, event: &Event) {
        if !self.enabled {
            return;
        }
        let Some(event_json) = build_inbound_30100(event) else {
            return;
        };
        if let Err(e) = self.nomen.message_store(event_json).await {
            tracing::warn!(error = %e, key = %event.key, "Failed to collect inbound message");
        }
    }

    /// Collect an outbound agent response.
    pub async fn collect_outbound(
        &self,
        channel_name: &str,
        chat_id: &str,
        text: &str,
        message_id: &str,
        identity_npub: &str,
    ) {
        if !self.enabled {
            return;
        }
        let event_json =
            build_outbound_30100(channel_name, chat_id, text, message_id, identity_npub);
        if let Err(e) = self.nomen.message_store(event_json).await {
            tracing::warn!(
                error = %e,
                channel = channel_name,
                chat_id = chat_id,
                "Failed to collect outbound message"
            );
        }
    }
}
