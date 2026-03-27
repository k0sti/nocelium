use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::NomenToolError;
use nocelium_memory::MemoryClient;

#[derive(Deserialize, JsonSchema)]
pub struct NomenSearchInput {
    /// The search query — natural language description of what to find
    pub query: String,
    /// Maximum number of results to return (default: 5)
    pub limit: Option<usize>,
}

pub struct NomenSearchTool {
    client: Arc<MemoryClient>,
}

impl NomenSearchTool {
    pub fn new(client: Arc<MemoryClient>) -> Self {
        Self { client }
    }
}

impl Tool for NomenSearchTool {
    const NAME: &'static str = "nomen_search";

    type Error = NomenToolError;
    type Args = NomenSearchInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "nomen_search".to_string(),
            description: "Search collective memory for relevant knowledge. Returns memories ranked by relevance.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(NomenSearchInput))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let limit = args.limit.unwrap_or(5);
        tracing::info!(query = %args.query, limit, "Searching memory");

        let results = self
            .client
            .search(&args.query, limit, None, None)
            .await
            .map_err(|e| NomenToolError::Memory(e.to_string()))?;

        if results.is_empty() {
            return Ok("No memories found.".to_string());
        }

        let formatted: Vec<String> = results
            .iter()
            .enumerate()
            .map(|(i, m)| format!("{}. [{}]: {}", i + 1, m.topic, m.detail))
            .collect();

        Ok(formatted.join("\n"))
    }
}
