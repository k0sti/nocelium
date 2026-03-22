//! Nocelium Memory - Nomen client for collective memory on Nostr
//!
//! This crate provides the interface to Nomen, a semantic memory layer
//! stored on Nostr relays. Currently a stub for future implementation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct NomenClient {
    base_url: String,
    client: reqwest::Client,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Memory {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub score: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StoreRequest {
    pub content: String,
    pub tags: Vec<String>,
}

impl NomenClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Search collective memory
    pub async fn search(&self, query: &str, limit: usize) -> anyhow::Result<Vec<Memory>> {
        tracing::info!(query = %query, limit = limit, "Searching memory (stub)");
        // TODO: Implement actual Nomen API call
        Ok(vec![])
    }

    /// Store a memory
    pub async fn store(&self, content: &str, _tags: Vec<String>) -> anyhow::Result<String> {
        tracing::info!(content_len = content.len(), "Storing memory (stub)");
        // TODO: Implement actual Nomen API call
        Ok("stub-id".to_string())
    }
}
