use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use crate::codex_utils::{state_dir, read_json, write_json, now, short_hash};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct FailureRecord {
    id: String,
    created_at: String,
    task: String,
    failed_approach: String,
    symptom: String,
    avoidance: String,
}
pub struct FailureMemoryTool;

#[async_trait]
impl Tool for FailureMemoryTool {
    fn name(&self) -> &str {
        "failure_memory"
    }

    fn description(&self) -> &str {
        "Record and search failed approaches so future agents can avoid repeating them."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["record", "search", "list"] },
                "task": { "type": "string" },
                "failed_approach": { "type": "string" },
                "symptom": { "type": "string" },
                "avoidance": { "type": "string" },
                "query": { "type": "string" }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("memory")?.join("failures.json");
        let mut records: Vec<FailureRecord> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "record" => {
                let task = args["task"].as_str().unwrap_or_default().to_string();
                let failed_approach = args["failed_approach"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let symptom = args["symptom"].as_str().unwrap_or_default().to_string();
                let avoidance = args["avoidance"].as_str().unwrap_or_default().to_string();
                let id = short_hash(&(task.clone() + &failed_approach + &symptom));
                records.retain(|r| r.id != id);
                records.push(FailureRecord {
                    id: id.clone(),
                    created_at: now(),
                    task,
                    failed_approach,
                    symptom,
                    avoidance,
                });
                write_json(&path, &records)?;
                Ok(json!({ "recorded": id, "path": path }).to_string())
            }
            "search" => {
                let query = args["query"]
                    .as_str()
                    .unwrap_or_default()
                    .to_ascii_lowercase();
                let found: Vec<_> = records
                    .into_iter()
                    .filter(|r| {
                        format!(
                            "{} {} {} {}",
                            r.task, r.failed_approach, r.symptom, r.avoidance
                        )
                        .to_ascii_lowercase()
                        .contains(&query)
                    })
                    .collect();
                Ok(serde_json::to_string_pretty(&found).unwrap_or_default())
            }
            "list" => Ok(serde_json::to_string_pretty(&records).unwrap_or_default()),
            _ => Err(AgentError("Unknown failure_memory action".to_string())),
        }
    }
}
