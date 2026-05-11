use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use crate::codex_utils::{state_dir, read_json, write_json, now, short_hash};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct DecisionRecord {
    id: String,
    created_at: String,
    git_head: String,
    target: String,
    chosen: String,
    rejected: Vec<String>,
    rationale: String,
}
pub struct TimeTravelDebuggerTool;

#[async_trait]
impl Tool for TimeTravelDebuggerTool {
    fn name(&self) -> &str {
        "time_travel_debugger"
    }

    fn description(&self) -> &str {
        "Record and inspect design intent tied to the current git commit for later semantic debugging."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["record", "list", "show"] },
                "id": { "type": "string" },
                "target": { "type": "string" },
                "chosen": { "type": "string" },
                "rejected": { "type": "array", "items": { "type": "string" } },
                "rationale": { "type": "string" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("time_travel")?.join("decisions.json");
        let mut records: Vec<DecisionRecord> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "record" => {
                let target = args["target"].as_str().unwrap_or_default().to_string();
                let chosen = args["chosen"].as_str().unwrap_or_default().to_string();
                let rationale = args["rationale"].as_str().unwrap_or_default().to_string();
                let rejected = args["rejected"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect::<Vec<_>>();
                let git_head = Command::new("git")
                    .args(["rev-parse", "HEAD"])
                    .output()
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "unknown".to_string());
                let id = args["id"]
                    .as_str()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| short_hash(&(target.clone() + &chosen + &rationale)));
                records.retain(|r| r.id != id);
                records.push(DecisionRecord {
                    id: id.clone(),
                    created_at: now(),
                    git_head,
                    target,
                    chosen,
                    rejected,
                    rationale,
                });
                write_json(&path, &records)?;
                Ok(json!({ "recorded": id, "path": path }).to_string())
            }
            "show" => {
                let id = args["id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing id".to_string()))?;
                let record = records
                    .into_iter()
                    .find(|r| r.id == id)
                    .ok_or_else(|| AgentError(format!("Decision not found: {}", id)))?;
                Ok(serde_json::to_string_pretty(&record).unwrap_or_default())
            }
            "list" => Ok(serde_json::to_string_pretty(&records).unwrap_or_default()),
            _ => Err(AgentError(
                "Unknown time_travel_debugger action".to_string(),
            )),
        }
    }
}
