use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;

pub struct CheckpointTool;

#[async_trait]
impl Tool for CheckpointTool {
    fn name(&self) -> &str {
        "checkpoint"
    }

    fn description(&self) -> &str {
        "Save or resume the agent's current state (working memory and task context). Useful for long-running tasks."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["save", "resume", "list"],
                    "description": "Action to perform."
                },
                "name": { "type": "string", "description": "Name of the checkpoint." },
                "state": { "type": "object", "description": "The state to save (opaque object for the agent)." }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| AgentError("Missing action".to_string()))?;
        let home = dirs::home_dir().expect("Could not find home directory");
        let checkpoint_dir = home.join(".pharmakon").join("checkpoints");
        fs::create_dir_all(&checkpoint_dir).ok();

        match action {
            "save" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing name".to_string()))?;
                let state = &args["state"];
                let path = checkpoint_dir.join(format!("{}.json", name));
                fs::write(&path, state.to_string()).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("✅ Checkpoint '{}' saved.", name))
            }
            "resume" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing name".to_string()))?;
                let path = checkpoint_dir.join(format!("{}.json", name));
                if !path.exists() {
                    return Err(AgentError(format!("Checkpoint '{}' not found.", name)));
                }
                let content = fs::read_to_string(&path).map_err(|e| AgentError(e.to_string()))?;
                Ok(content)
            }
            "list" => {
                let mut entries = Vec::new();
                for entry in fs::read_dir(&checkpoint_dir).map_err(|e| AgentError(e.to_string()))? {
                    let entry = entry.map_err(|e| AgentError(e.to_string()))?;
                    if let Some(name) = entry.file_name().to_str()
                        && name.ends_with(".json")
                    {
                        entries.push(name.replace(".json", ""));
                    }
                }
                Ok(json!({ "checkpoints": entries }).to_string())
            }
            _ => Err(AgentError("Unknown action".to_string())),
        }
    }
}
