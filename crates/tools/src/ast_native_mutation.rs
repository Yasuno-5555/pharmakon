use crate::codex_utils::find_rust_function_span;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

pub struct AstNativeMutationTool;
#[async_trait]
impl Tool for AstNativeMutationTool {
    fn name(&self) -> &str {
        "mutate_ast"
    }

    fn description(&self) -> &str {
        "Perform structured Rust mutations such as replacing a function body or whole function, then optionally run rustfmt."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["replace_body", "replace_function"] },
                "path": { "type": "string" },
                "target_node": { "type": "string", "description": "Example: function:calculate_total" },
                "new_code": { "type": "string" },
                "rustfmt": { "type": "boolean", "default": true }
            },
            "required": ["action", "path", "target_node", "new_code"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::FileSystem
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().unwrap_or("replace_body");
        let path = args["path"]
            .as_str()
            .ok_or_else(|| AgentError("Missing path".to_string()))?;
        let target = args["target_node"]
            .as_str()
            .ok_or_else(|| AgentError("Missing target_node".to_string()))?;
        let (_, name) = target
            .split_once(':')
            .ok_or_else(|| AgentError("target_node must look like function:name".to_string()))?;
        let new_code = args["new_code"]
            .as_str()
            .ok_or_else(|| AgentError("Missing new_code".to_string()))?;
        let mut content = fs::read_to_string(path).map_err(|e| AgentError(e.to_string()))?;
        let (fn_start, _brace_start, body_start, fn_end) = find_rust_function_span(&content, name)
            .ok_or_else(|| AgentError(format!("Function not found: {}", name)))?;
        match action {
            "replace_function" => content.replace_range(fn_start..fn_end, new_code),
            "replace_body" => content.replace_range(body_start..fn_end - 1, new_code),
            _ => return Err(AgentError("Unknown mutate_ast action".to_string())),
        }
        fs::write(path, content).map_err(|e| AgentError(e.to_string()))?;
        let rustfmt = args["rustfmt"].as_bool().unwrap_or(true);
        let mut rustfmt_status = None;
        if rustfmt {
            rustfmt_status = Command::new("rustfmt").arg(path).output().ok().map(|o| {
                json!({
                    "success": o.status.success(),
                    "stderr": String::from_utf8_lossy(&o.stderr).to_string()
                })
            });
        }
        Ok(json!({ "path": path, "target_node": target, "action": action, "rustfmt": rustfmt_status }).to_string())
    }
}
