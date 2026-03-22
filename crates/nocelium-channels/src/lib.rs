//! Nocelium Channels - Message I/O abstraction
//!
//! Channels provide the interface between external messaging systems
//! (Telegram, Nostr, stdio) and the agent loop.

pub mod stdio;

use anyhow::Result;
use async_trait::async_trait;

/// Trait for bidirectional message channels
#[async_trait]
pub trait Channel: Send {
    /// Receive the next message. Returns None when the channel is closed.
    async fn receive(&mut self) -> Result<Option<String>>;

    /// Send a complete response message
    async fn send(&mut self, message: &str) -> Result<()>;

    /// Send a streaming chunk (for token-by-token output)
    async fn send_chunk(&mut self, chunk: &str) -> Result<()>;

    /// Flush any buffered output
    async fn flush(&mut self) -> Result<()>;
}
