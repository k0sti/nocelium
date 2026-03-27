use nomen_wire::ReconnectingClient;
use serde_json::{json, Value};

use crate::error::MemoryError;
use crate::types::{
    CollectedMessageQueryResult, Memory, MessageContextParams, MessageQueryParams, Visibility,
};

/// Client for the Nomen memory service over Unix socket.
pub struct MemoryClient {
    wire: ReconnectingClient,
    nsec: Option<String>,
    authenticated: std::sync::atomic::AtomicBool,
}

impl MemoryClient {
    /// Create a new client. Does not connect until the first request.
    pub fn new(socket_path: &str, max_retries: usize) -> Self {
        Self {
            wire: ReconnectingClient::new(socket_path, max_retries),
            nsec: None,
            authenticated: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Create a new client with nsec for per-session identity.
    pub fn with_nsec(socket_path: &str, max_retries: usize, nsec: String) -> Self {
        Self {
            wire: ReconnectingClient::new(socket_path, max_retries),
            nsec: Some(nsec),
            authenticated: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Authenticate with Nomen using identity.auth if nsec is configured.
    /// Called automatically before the first request.
    async fn ensure_auth(&self) -> Result<(), MemoryError> {
        if self.nsec.is_none()
            || self
                .authenticated
                .load(std::sync::atomic::Ordering::Relaxed)
        {
            return Ok(());
        }
        let nsec = self.nsec.as_ref().unwrap();
        let resp = self
            .wire
            .request("identity.auth", json!({"nsec": nsec}))
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))?;
        if resp.ok {
            self.authenticated
                .store(true, std::sync::atomic::Ordering::Relaxed);
            if let Some(result) = &resp.result {
                tracing::info!(npub = %result.get("npub").and_then(|v| v.as_str()).unwrap_or("?"), "Nomen identity authenticated");
            }
        } else {
            // Auth not supported (old Nomen) or failed — continue without per-session identity
            let msg = resp
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "Unknown".into());
            tracing::warn!("Nomen identity.auth failed: {msg} (continuing with default identity)");
            self.authenticated
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }

    /// Send a request to Nomen, ensuring auth is done first.
    async fn request(
        &self,
        action: &str,
        params: Value,
    ) -> Result<nomen_wire::Response, MemoryError> {
        self.ensure_auth().await?;
        self.wire
            .request(action, params)
            .await
            .map_err(|e| MemoryError::Connection(e.to_string()))
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

        let resp = self.request("memory.search", params).await?;

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
        detail: &str,
        visibility: Option<&Visibility>,
        scope: Option<&str>,
    ) -> Result<String, MemoryError> {
        let mut params = json!({
            "topic": topic,
            "detail": detail,
        });
        if let Some(vis) = visibility {
            params["visibility"] = json!(vis.as_str());
        }
        if let Some(s) = scope {
            params["scope"] = json!(s);
        }

        let resp = self.request("memory.put", params).await?;

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
        let resp = self.request("memory.get", json!({"topic": topic})).await?;

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

        let resp = self.request("memory.list", params).await?;

        let result = Self::extract_result(resp)?;
        let memories = result
            .get("memories")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        serde_json::from_value(memories).map_err(|e| MemoryError::Deserialize(e.to_string()))
    }

    /// Health check — try a lightweight search to verify Nomen is reachable.
    pub async fn health_check(&self) -> bool {
        match self.search("health", 1, None, None).await {
            Ok(_) => true,
            Err(e) => {
                tracing::warn!(error = %e, "Nomen health check failed");
                false
            }
        }
    }

    /// Store a message as a kind 30100 event via Nomen.
    pub async fn message_store(&self, event: Value) -> Result<(), MemoryError> {
        let resp = self
            .request("message.store", json!({ "event": event }))
            .await?;
        Self::extract_result(resp)?;
        Ok(())
    }

    /// Query collected messages using canonical normalized filters.
    pub async fn message_query(
        &self,
        params: &MessageQueryParams,
    ) -> Result<CollectedMessageQueryResult, MemoryError> {
        let params =
            serde_json::to_value(params).map_err(|e| MemoryError::Deserialize(e.to_string()))?;
        let resp = self.request("message.query", params).await?;
        let result = Self::extract_result(resp)?;
        serde_json::from_value(result).map_err(|e| MemoryError::Deserialize(e.to_string()))
    }

    /// Retrieve surrounding conversation context for collected messages.
    pub async fn message_context(
        &self,
        params: &MessageContextParams,
    ) -> Result<CollectedMessageQueryResult, MemoryError> {
        let params =
            serde_json::to_value(params).map_err(|e| MemoryError::Deserialize(e.to_string()))?;
        let resp = self.request("message.context", params).await?;
        let result = Self::extract_result(resp)?;
        serde_json::from_value(result).map_err(|e| MemoryError::Deserialize(e.to_string()))
    }

    /// Delete a memory by topic.
    pub async fn delete(&self, topic: &str) -> Result<(), MemoryError> {
        let resp = self
            .request("memory.delete", json!({"topic": topic}))
            .await?;

        Self::extract_result(resp)?;
        Ok(())
    }
}
