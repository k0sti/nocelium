use nomen_wire::ReconnectingClient;
use serde_json::{json, Value};

use crate::error::MemoryError;
use crate::types::{Memory, Visibility};

/// Client for the Nomen memory service over Unix socket.
pub struct MemoryClient {
    wire: ReconnectingClient,
}

impl MemoryClient {
    /// Create a new client. Does not connect until the first request.
    pub fn new(socket_path: &str, max_retries: usize) -> Self {
        Self {
            wire: ReconnectingClient::new(socket_path, max_retries),
        }
    }

    /// Extract result from a nomen-wire Response, mapping errors.
    fn extract_result(resp: nomen_wire::Response) -> Result<Value, MemoryError> {
        if !resp.ok {
            let err = resp.error.unwrap_or(nomen_wire::ErrorBody {
                code: "unknown".to_string(),
                message: "Unknown error".to_string(),
            });
            return Err(MemoryError::Api {
                code: err.code,
                message: err.message,
            });
        }
        Ok(resp.result.unwrap_or(Value::Null))
    }

    /// Semantic search over memories.
    pub async fn search(
        &self,
        query: &str,
        limit: usize,
        visibility: Option<&Visibility>,
        scope: Option<&str>,
    ) -> Result<Vec<Memory>, MemoryError> {
        let mut params = json!({
            "query": query,
            "limit": limit,
        });
        if let Some(vis) = visibility {
            params["visibility"] = json!(vis.as_str());
        }
        if let Some(s) = scope {
            params["scope"] = json!(s);
        }

        let resp = self
            .wire
            .request("memory.search", params)
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))?;

        let result = Self::extract_result(resp)?;
        let results = result
            .get("results")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        serde_json::from_value(results).map_err(|e| MemoryError::Deserialize(e.to_string()))
    }

    /// Store a memory.
    pub async fn store(
        &self,
        topic: &str,
        summary: &str,
        detail: &str,
        visibility: Option<&Visibility>,
        scope: Option<&str>,
    ) -> Result<String, MemoryError> {
        let mut params = json!({
            "topic": topic,
            "summary": summary,
            "detail": detail,
        });
        if let Some(vis) = visibility {
            params["visibility"] = json!(vis.as_str());
        }
        if let Some(s) = scope {
            params["scope"] = json!(s);
        }

        let resp = self
            .wire
            .request("memory.put", params)
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))?;

        let result = Self::extract_result(resp)?;
        let d_tag = result
            .get("d_tag")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        Ok(d_tag)
    }

    /// Get a memory by topic.
    pub async fn get(&self, topic: &str) -> Result<Option<Memory>, MemoryError> {
        let resp = self
            .wire
            .request("memory.get", json!({"topic": topic}))
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))?;

        let result = Self::extract_result(resp)?;
        if result.is_null() {
            return Ok(None);
        }
        let mem =
            serde_json::from_value(result).map_err(|e| MemoryError::Deserialize(e.to_string()))?;
        Ok(Some(mem))
    }

    /// List memories with optional visibility filter.
    pub async fn list(
        &self,
        visibility: Option<&Visibility>,
        limit: usize,
    ) -> Result<Vec<Memory>, MemoryError> {
        let mut params = json!({"limit": limit});
        if let Some(vis) = visibility {
            params["visibility"] = json!(vis.as_str());
        }

        let resp = self
            .wire
            .request("memory.list", params)
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))?;

        let result = Self::extract_result(resp)?;
        let memories = result
            .get("memories")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        serde_json::from_value(memories).map_err(|e| MemoryError::Deserialize(e.to_string()))
    }

    /// Delete a memory by topic.
    pub async fn delete(&self, topic: &str) -> Result<(), MemoryError> {
        let resp = self
            .wire
            .request("memory.delete", json!({"topic": topic}))
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))?;

        Self::extract_result(resp)?;
        Ok(())
    }
}
