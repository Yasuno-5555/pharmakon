use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::codex::utils::{state_dir, read_json, write_json, now};

pub struct MemoryActorStatusTool;

#[async_trait]
impl Tool for MemoryActorStatusTool {
    fn name(&self) -> &str {
        "memory_actor_status"
    }

    fn description(&self) -> &str {
        "Expose the event-sourcing contract for a single Memory Manager Actor and record lightweight memory events."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["status", "append"], "default": "status" },
                "event": { "type": "object" }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("memory")?.join("actor_events.json");
        let mut events: Vec<Value> = read_json(&path)?;
        if args["action"].as_str().unwrap_or("status") == "append" {
            let mut event = args.get("event").cloned().unwrap_or_else(|| json!({}));
            event["timestamp"] = json!(now());
            events.push(event);
            write_json(&path, &events)?;
        }
        Ok(json!({
            "actor": "single_writer_memory_manager",
            "events_recorded": events.len(),
            "contract": ["FactDiscovered", "MemoryAccessed", "DecayTriggered", "ConflictResolved", "SnapshotCreated"]
        }).to_string())
    }
}
