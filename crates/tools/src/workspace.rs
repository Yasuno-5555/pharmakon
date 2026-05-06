use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::process::Command;

pub struct WorkspacePerceptionTool;

impl WorkspacePerceptionTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for WorkspacePerceptionTool {
    fn name(&self) -> &str {
        "perceive_workspace"
    }
    fn description(&self) -> &str {
        "Get a comprehensive view of the current workspace directory structure and project layout."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "depth": { "type": "integer", "default": 2, "description": "How deep to traverse the directory tree" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let depth = args["depth"].as_u64().unwrap_or(2);

        let output = Command::new("tree")
            .arg("-L")
            .arg(depth.to_string())
            .arg("-I")
            .arg("target|node_modules|.git|.venv|dist")
            .output();

        match output {
            Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
            _ => {
                // Fallback to find if tree is not installed
                let fallback = Command::new("find")
                    .arg(".")
                    .arg("-maxdepth")
                    .arg(depth.to_string())
                    .arg("-not")
                    .arg("-path")
                    .arg("*/.*")
                    .arg("-not")
                    .arg("-path")
                    .arg("*/target*")
                    .output()
                    .map_err(|e| AgentError(format!("Workspace perception failed: {}", e)))?;

                Ok(String::from_utf8_lossy(&fallback.stdout).to_string())
            }
        }
    }
}
