use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::process::Command;

pub struct SpecFirstTestTool;
#[async_trait]
impl Tool for SpecFirstTestTool {
    fn name(&self) -> &str {
        "spec_first_test"
    }

    fn description(&self) -> &str {
        "Run a spec-first verification command, usually cargo test/check, and return structured compiler feedback."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "default": "cargo test" },
                "cwd": { "type": "string", "default": "." }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"].as_str().unwrap_or("cargo test");
        let cwd = args["cwd"].as_str().unwrap_or(".");
        let output = Command::new("sh")
            .arg("-lc")
            .arg(command)
            .current_dir(cwd)
            .output()
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json!({
            "command": command,
            "cwd": cwd,
            "success": output.status.success(),
            "stdout": String::from_utf8_lossy(&output.stdout).chars().take(6000).collect::<String>(),
            "stderr": String::from_utf8_lossy(&output.stderr).chars().take(6000).collect::<String>()
        }).to_string())
    }
}
