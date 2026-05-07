use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::process::Command;

pub struct CargoQualityTool;

#[async_trait]
impl Tool for CargoQualityTool {
    fn name(&self) -> &str {
        "cargo_check"
    }
    fn description(&self) -> &str {
        "Run cargo check or tests to verify code quality and correctness. Inspired by Aider/SWE-agent verification."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "enum": ["check", "test", "fmt", "clippy"], "default": "check" },
                "package": { "type": "string", "description": "Specific package to check" }
            }
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }
    async fn call(&self, args: Value) -> AgentResult<String> {
        let command = args["command"].as_str().unwrap_or("check");
        let mut cmd = Command::new("cargo");
        cmd.arg(command);
        if let Some(pkg) = args["package"].as_str() {
            cmd.arg("-p").arg(pkg);
        }

        let out = cmd.output().map_err(|e| AgentError(e.to_string()))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);

        Ok(format!(
            "### Cargo {}\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
            command, stdout, stderr
        ))
    }
}
