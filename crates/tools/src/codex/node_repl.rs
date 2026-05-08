use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use std::process::Command;

pub struct NodeReplTool;

#[async_trait]
impl Tool for NodeReplTool {
    fn name(&self) -> &str {
        "node_repl"
    }

    fn description(&self) -> &str {
        "Run a small JavaScript snippet through local node, similar to Codex node_repl for deterministic scripting."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": { "code": { "type": "string" } },
            "required": ["code"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let code = args["code"]
            .as_str()
            .ok_or_else(|| AgentError("Missing code".to_string()))?;
        let output = Command::new("node")
            .arg("-e")
            .arg(code)
            .output()
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json!({
            "stdout": String::from_utf8_lossy(&output.stdout).to_string(),
            "stderr": String::from_utf8_lossy(&output.stderr).to_string(),
            "success": output.status.success()
        })
        .to_string())
    }
}
