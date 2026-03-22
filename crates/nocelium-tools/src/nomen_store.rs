use std::sync::Arc;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

use nocelium_memory::MemoryClient;
use crate::error::NomenToolError;

#[derive(Deserialize, JsonSchema)]
pub struct NomenStoreInput {
    /// Topic path for the memory (e.g. "project/design-decisions")
    pub topic: String,
    /// Short summary of the memory (one sentence)
    pub summary: String,
    /// Detailed content (optional, defaults to summary)
    pub detail: Option<String>,
}

pub struct NomenStoreTool {
    client: Arc<MemoryClient>,
}

impl NomenStoreTool {
    pub fn new(client: Arc<MemoryClient>) -> Self {
        Self { client }
    }
}

impl Tool for NomenStoreTool {
    const NAME: &'static str = "nomen_store";

    type Error = NomenToolError;
    type Args = NomenStoreInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "nomen_store".to_string(),
            description: "Store knowledge in collective memory. Use for facts, decisions, or context worth remembering across conversations.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(NomenStoreInput))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let detail = args.detail.as_deref().unwrap_or(&args.summary);
        tracing::info!(topic = %args.topic, "Storing memory");

        let d_tag = self
            .client
            .store(&args.topic, &args.summary, detail, None, None)
            .await
            .map_err(|e| NomenToolError::Memory(e.to_string()))?;

        Ok(format!("Stored memory: {} (d_tag: {})", args.topic, d_tag))
    }
}
