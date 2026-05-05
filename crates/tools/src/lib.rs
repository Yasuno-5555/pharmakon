pub mod docker_sandbox;
pub mod wasm_tool;
pub mod browser;
pub mod search;
pub mod media;
pub mod subagent;
pub mod probe;
pub mod connectors;
pub mod link_understanding;
pub mod canvas;
pub mod memory;
pub mod web_fetch;
pub mod config_tool;
pub mod fact_tool;
pub mod mcp_tool;
pub mod web_search;
pub mod terminal;
pub mod commitment_tool;
pub mod media_understanding;
pub mod soul_tool;
pub mod registry;

use async_trait::async_trait;
use serde_json::Value;
use std::process::Command;
use std::fs;
use crate::docker_sandbox::DockerSandbox;

pub use pharmakon_common::{Tool, AgentResult, AgentError};
pub type Result<T> = AgentResult<T>;

// Re-exports for convenience
pub use crate::browser::BrowserTool;
pub use crate::search::brave::BraveSearchTool;
pub use crate::fact_tool::FactTool;
pub use crate::canvas::CanvasTool;
pub use crate::commitment_tool::CommitmentTool;
pub use crate::terminal::TerminalTool;
pub use crate::media_understanding::MediaUnderstandingTool;
pub use crate::link_understanding::LinkUnderstandingTool;
pub use crate::web_fetch::WebFetchTool;
pub use crate::media::capture::{ScreenshotTool, CameraTool};
pub use crate::connectors::ContextConnectorTool;
pub use crate::soul_tool::SoulTool;

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str { "Execute a shell command" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let cmd = args["command"].as_str().ok_or_else(|| AgentError("Missing command".to_string()))?;
        let output = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .output()
            .map_err(|e| AgentError(e.to_string()))?;
        
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        
        Ok(format!("stdout: {}\nstderr: {}", stdout, stderr))
    }

    fn requires_approval(&self, _args: &Value) -> bool { true }
    fn approval_description(&self, args: &Value) -> String {
        format!("Run shell command: {}", args["command"].as_str().unwrap_or("unknown"))
    }
}

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str { "read_file" }
    fn description(&self) -> &str { "Read the contents of a file" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let path = args["path"].as_str().ok_or_else(|| AgentError("Missing path".to_string()))?;
        let content = fs::read_to_string(path).map_err(|e| AgentError(e.to_string()))?;
        Ok(content)
    }
}

pub struct SandboxedShellTool {
    sandbox: DockerSandbox,
}

impl SandboxedShellTool {
    pub fn new(image: &str) -> anyhow::Result<Self> {
        Ok(Self {
            sandbox: DockerSandbox::new(image)?,
        })
    }
}

#[async_trait]
impl Tool for SandboxedShellTool {
    fn name(&self) -> &str { "sandboxed_shell" }
    fn description(&self) -> &str { "Execute a shell command inside a Docker container" }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string" }
            },
            "required": ["command"]
        })
    }
    async fn call(&self, args: Value) -> Result<String> {
        let cmd = args["command"].as_str().ok_or_else(|| AgentError("Missing command".to_string()))?;
        let (stdout, stderr) = self.sandbox.run_command(cmd).await.map_err(|e| AgentError(e.to_string()))?;
        Ok(format!("stdout: {}\nstderr: {}", stdout, stderr))
    }
}
