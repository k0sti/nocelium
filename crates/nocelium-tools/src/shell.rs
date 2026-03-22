use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Deserialize, JsonSchema)]
pub struct ShellToolInput {
    /// The shell command to execute
    pub command: String,
    /// Working directory (optional, defaults to current dir)
    pub working_dir: Option<String>,
    /// Timeout in seconds (optional, defaults to 30)
    pub timeout_secs: Option<u64>,
}

#[derive(Serialize, Deserialize)]
pub struct ShellToolOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

impl std::fmt::Display for ShellToolOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if !self.stdout.is_empty() {
            write!(f, "{}", self.stdout)?;
        }
        if !self.stderr.is_empty() {
            if !self.stdout.is_empty() {
                writeln!(f)?;
            }
            write!(f, "STDERR: {}", self.stderr)?;
        }
        if let Some(code) = self.exit_code {
            if code != 0 {
                write!(f, "\n[exit code: {}]", code)?;
            }
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ShellToolError {
    #[error("Command execution failed: {0}")]
    ExecutionError(String),
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),
}

pub struct ShellTool;

impl Tool for ShellTool {
    const NAME: &'static str = "shell";

    type Error = ShellToolError;
    type Args = ShellToolInput;
    type Output = ShellToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "shell".to_string(),
            description: "Execute a shell command and return its output. Use for running programs, scripts, and system commands.".to_string(),
            parameters: serde_json::to_value(schemars::schema_for!(ShellToolInput))
                .unwrap_or_default(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let timeout = args.timeout_secs.unwrap_or(30);

        let mut cmd = tokio::process::Command::new("sh");
        cmd.arg("-c").arg(&args.command);

        if let Some(ref dir) = args.working_dir {
            cmd.current_dir(dir);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        tracing::info!(command = %args.command, "Executing shell command");

        let result = tokio::time::timeout(
            std::time::Duration::from_secs(timeout),
            cmd.output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => Ok(ShellToolOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code(),
            }),
            Ok(Err(e)) => Err(ShellToolError::ExecutionError(e.to_string())),
            Err(_) => Err(ShellToolError::Timeout(timeout)),
        }
    }
}
