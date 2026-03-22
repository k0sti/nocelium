use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, Notify};

use crate::{
    Channel, ChannelCapabilities, ChatType, Event, Message, OutboundMessage, Payload, SendResult,
    Source,
};

/// Interactive stdin/stdout channel
pub struct StdioChannel {
    /// Notifies listen() that send() has completed, so the next prompt can be shown.
    response_ready: Notify,
}

impl StdioChannel {
    pub fn new() -> Self {
        Self {
            response_ready: Notify::new(),
        }
    }
}

impl Default for StdioChannel {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Channel for StdioChannel {
    fn name(&self) -> &str {
        "stdio"
    }

    fn capabilities(&self) -> ChannelCapabilities {
        ChannelCapabilities::default()
    }

    async fn listen(&self, tx: mpsc::Sender<Event>) -> Result<()> {
        let mut reader = BufReader::new(tokio::io::stdin());
        let mut stdout = tokio::io::stdout();

        loop {
            stdout.write_all(b"> ").await?;
            stdout.flush().await?;

            let mut line = String::new();
            let bytes_read = reader.read_line(&mut line).await?;

            if bytes_read == 0 {
                break; // EOF
            }

            let text = line.trim().to_string();
            if text.is_empty() {
                continue;
            }

            if text == "/quit" || text == "/exit" {
                stdout.write_all(b"Goodbye!\n").await?;
                stdout.flush().await?;
                break;
            }

            let event = Event::new(
                Source::Channel {
                    name: "stdio".into(),
                    chat_id: "local".into(),
                    sender_id: "user".into(),
                },
                Payload::Message(Box::new(Message {
                    text,
                    chat_type: ChatType::Direct,
                    ..Default::default()
                })),
            );

            if tx.send(event).await.is_err() {
                break; // receiver dropped
            }

            // Wait for the response to be sent before showing next prompt
            self.response_ready.notified().await;
        }

        Ok(())
    }

    async fn send(&self, message: &OutboundMessage) -> Result<SendResult> {
        let mut stdout = tokio::io::stdout();
        stdout.write_all(message.text.trim_end().as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
        self.response_ready.notify_one();
        Ok(SendResult {
            message_id: "0".into(),
        })
    }
}
