use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use std::fs;
use std::process::Command;
use crate::codex_utils::scan_diff_risks;

pub struct DryRunTool;

#[async_trait]
impl Tool for DryRunTool {
    fn name(&self) -> &str {
        "dry_run"
    }

    fn description(&self) -> &str {
        "Simulate shell commands, patches, or API calls without performing side effects."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["shell", "patch", "api"] },
                "command": { "type": "string" },
                "path": { "type": "string" },
                "patch": { "type": "string" },
                "method": { "type": "string" },
                "url": { "type": "string" },
                "body": { "type": "object" }
            },
            "required": ["kind"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        match args["kind"].as_str().unwrap_or("shell") {
            "shell" => {
                let command = args["command"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing command".to_string()))?;
                let risks = scan_diff_risks(command);
                let syntax_check = if command.contains('\n') {
                    Command::new("sh")
                        .arg("-n")
                        .arg("-c")
                        .arg(command)
                        .output()
                        .ok()
                        .map(|o| {
                            json!({
                                "ok": o.status.success(),
                                "stderr": String::from_utf8_lossy(&o.stderr).to_string()
                            })
                        })
                } else {
                    None
                };
                Ok(json!({ "would_execute": command, "side_effects": "not executed", "risks": risks, "syntax_check": syntax_check }).to_string())
            }
            "patch" => {
                let path = args["path"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing path".to_string()))?;
                let patch_str = args["patch"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing patch".to_string()))?;
                let original = fs::read_to_string(path)
                    .map_err(|e| AgentError(format!("Failed to read {}: {}", path, e)))?;
                let patch = diffy::Patch::from_str(patch_str)
                    .map_err(|e| AgentError(format!("Invalid patch: {}", e)))?;
                let patched = diffy::apply(&original, &patch)
                    .map_err(|e| AgentError(format!("Patch would fail: {}", e)))?;
                Ok(json!({
                    "path": path,
                    "applicable": true,
                    "original_bytes": original.len(),
                    "patched_bytes": patched.len(),
                    "risks": scan_diff_risks(patch_str)
                })
                .to_string())
            }
            "api" => Ok(json!({
                "method": args["method"].as_str().unwrap_or("GET"),
                "url": args["url"].as_str().unwrap_or_default(),
                "body_preview": args.get("body"),
                "side_effects": "request not sent"
            })
            .to_string()),
            _ => Err(AgentError("Unknown dry_run kind".to_string())),
        }
    }
}
