use anyhow::Result;
use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::Channel;

/// Interactive stdin/stdout channel for testing
pub struct StdioChannel {
    reader: BufReader<tokio::io::Stdin>,
    writer: tokio::io::Stdout,
}

impl Default for StdioChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl StdioChannel {
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
        }
    }
}

#[async_trait]
impl Channel for StdioChannel {
    async fn receive(&mut self) -> Result<Option<String>> {
        self.writer.write_all(b"\n> ").await?;
        self.writer.flush().await?;

        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            return Ok(None); // EOF
        }

        Ok(Some(line.trim().to_string()))
    }

    async fn send(&mut self, message: &str) -> Result<()> {
        self.writer.write_all(b"\n").await?;
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn send_chunk(&mut self, chunk: &str) -> Result<()> {
        self.writer.write_all(chunk.as_bytes()).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn flush(&mut self) -> Result<()> {
        self.writer.flush().await?;
        Ok(())
    }
}
