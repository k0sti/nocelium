use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::Deserialize;

// --- ReadFileTool ---

#[derive(Deserialize, JsonSchema)]
pub struct ReadFileInput {
    /// Path to the file to read
    pub path: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FileToolError {
    #[error("IO error: {0}")]
    Io(String),
}

pub struct ReadFileTool;

impl Tool for ReadFileTool {
    const NAME: &'static str = "read_file";

    type Error = FileToolError;
    type Args = ReadFileInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file at the given path.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ReadFileInput))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!(path = %args.path, "Reading file");
        tokio::fs::read_to_string(&args.path)
            .await
            .map_err(|e| FileToolError::Io(e.to_string()))
    }
}

// --- WriteFileTool ---

#[derive(Deserialize, JsonSchema)]
pub struct WriteFileInput {
    /// Path to the file to write
    pub path: String,
    /// Content to write to the file
    pub content: String,
}

pub struct WriteFileTool;

impl Tool for WriteFileTool {
    const NAME: &'static str = "write_file";

    type Error = FileToolError;
    type Args = WriteFileInput;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file at the given path. Creates the file if it doesn't exist, overwrites if it does.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(WriteFileInput))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!(path = %args.path, "Writing file");
        if let Some(parent) = std::path::Path::new(&args.path).parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| FileToolError::Io(e.to_string()))?;
        }
        tokio::fs::write(&args.path, &args.content)
            .await
            .map_err(|e| FileToolError::Io(e.to_string()))?;
        Ok(format!(
            "Written {} bytes to {}",
            args.content.len(),
            args.path
        ))
    }
}
