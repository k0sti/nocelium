//! Dispatch logger — writes structured JSONL to ~/.nocelium/logs/dispatch.jsonl

use serde::Serialize;
use std::path::PathBuf;
use tokio::fs::{self, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;

/// A single dispatch log entry.
#[derive(Serialize)]
pub struct DispatchLogEntry {
    /// ISO 8601 timestamp
    pub ts: String,
    /// Dispatch key
    pub key: String,
    /// Matched rule pattern
    pub rule: String,
    /// Action taken (agent_turn, handler:X, drop)
    pub action: String,
    /// Source transport/platform name (for example `telegram`)
    pub platform: Option<String>,
    /// Chat ID
    pub chat_id: Option<String>,
    /// Sender ID
    pub sender_id: Option<String>,
    /// Sender name
    pub sender_name: Option<String>,
    /// Message preview (first 200 chars)
    pub message: Option<String>,
    /// Response preview (first 200 chars), None for non-agent-turn
    pub response: Option<String>,
    /// Processing duration in ms
    pub duration_ms: Option<u64>,
    /// Error if any
    pub error: Option<String>,
}

/// Async dispatch logger that writes to a file via a background task.
pub struct DispatchLogger {
    tx: mpsc::Sender<DispatchLogEntry>,
}

impl DispatchLogger {
    /// Create a new logger. Spawns a background writer task.
    pub async fn new() -> Self {
        let log_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".nocelium/logs");

        if let Err(e) = fs::create_dir_all(&log_dir).await {
            tracing::error!(error = %e, "Failed to create log directory");
        }

        let log_path = log_dir.join("dispatch.jsonl");
        let (tx, mut rx) = mpsc::channel::<DispatchLogEntry>(256);

        tokio::spawn(async move {
            while let Some(entry) = rx.recv().await {
                let line = match serde_json::to_string(&entry) {
                    Ok(json) => format!("{json}\n"),
                    Err(e) => {
                        tracing::error!(error = %e, "Failed to serialize dispatch log entry");
                        continue;
                    }
                };

                match OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .await
                {
                    Ok(mut file) => {
                        if let Err(e) = file.write_all(line.as_bytes()).await {
                            tracing::error!(error = %e, "Failed to write dispatch log");
                        }
                    }
                    Err(e) => {
                        tracing::error!(error = %e, path = %log_path.display(), "Failed to open dispatch log");
                    }
                }
            }
        });

        tracing::info!(path = %log_dir.join("dispatch.jsonl").display(), "Dispatch logger initialized");
        Self { tx }
    }

    /// Log a dispatch entry (non-blocking).
    pub fn log(&self, entry: DispatchLogEntry) {
        if let Err(e) = self.tx.try_send(entry) {
            tracing::warn!(error = %e, "Dispatch log channel full, dropping entry");
        }
    }
}

/// Truncate a string to max chars, appending "..." if truncated.
pub fn preview(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}
