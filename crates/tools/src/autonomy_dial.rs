use crate::codex_utils::{now, read_json, state_dir, write_json};
use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};

pub struct AutonomyDialTool;

#[async_trait]
impl Tool for AutonomyDialTool {
    fn name(&self) -> &str {
        "autonomy_dial"
    }

    fn description(&self) -> &str {
        "Get or set the agent autonomy level: 0 propose, 1 light work, 2 edit allowed, 3 full autonomous."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["get", "set"], "default": "get" },
                "level": { "type": "integer", "minimum": 0, "maximum": 3 }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("settings")?.join("autonomy.json");
        let mut value: Value = read_json(&path)?;
        if value.is_null() {
            value = json!({ "level": 2, "updated_at": now() });
        }
        if args["action"].as_str().unwrap_or("get") == "set" {
            let level = args["level"].as_u64().unwrap_or(2).min(3);
            value = json!({ "level": level, "updated_at": now() });
            write_json(&path, &value)?;
        }
        Ok(value.to_string())
    }
}
