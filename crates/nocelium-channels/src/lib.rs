//! Nocelium Channels - Message I/O abstraction
//!
//! Channels provide the interface between external messaging systems
//! (Telegram, Nostr, stdio) and the agent loop.

pub mod event;
pub mod outbound;
pub mod stdio;
#[cfg(feature = "telegram")]
pub mod telegram;

pub use event::*;
pub use outbound::*;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;

/// Trait for bidirectional message channels.
///
/// Channels push inbound events via `listen()` and handle outbound messages via `send()`.
/// Optional methods have default no-op implementations.
#[async_trait]
pub trait Channel: Send + Sync {
    /// Channel name (e.g. "stdio", "telegram")
    fn name(&self) -> &str;

    /// Declare what this channel supports
    fn capabilities(&self) -> ChannelCapabilities;

    /// Start listening and push events to the shared queue.
    /// This should run until the channel is closed.
    async fn listen(&self, tx: mpsc::Sender<Event>) -> Result<()>;

    /// Send a message via this channel
    async fn send(&self, message: &OutboundMessage) -> Result<SendResult>;

    /// Edit a previously sent message
    async fn edit(&self, _chat_id: &str, _message_id: &str, _text: &str) -> Result<()> {
        anyhow::bail!("edit not supported")
    }

    /// Delete a message
    async fn delete(&self, _chat_id: &str, _message_id: &str) -> Result<()> {
        anyhow::bail!("delete not supported")
    }

    /// Send typing indicator
    async fn start_typing(&self, _chat_id: &str) -> Result<()> {
        Ok(())
    }

    /// Health check
    async fn health_check(&self) -> bool {
        true
    }
}

/// What a channel supports.
#[derive(Debug, Clone, Default)]
pub struct ChannelCapabilities {
    pub media: bool,
    pub reactions: bool,
    pub reply: bool,
    pub edit: bool,
    pub delete: bool,
    pub threads: bool,
    pub buttons: bool,
    pub polls: bool,
    pub typing: bool,
    pub pins: bool,
    pub voice: bool,
    pub location: bool,
    pub live_location: bool,
}
