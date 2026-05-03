use async_trait::async_trait;
use serde_json::{Value, json};
use pharmakon_common::{Tool, AgentResult, AgentError};
use pharmakon_memory::FactMemory;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct FactTool {
    memory: Arc<Mutex<FactMemory>>,
}

impl FactTool {
    pub fn new(memory: Arc<Mutex<FactMemory>>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for FactTool {
    fn name(&self) -> &str { "manage_facts" }
    fn description(&self) -> &str { "Store or retrieve facts about the user to remember them across sessions." }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["set", "get_all"] },
                "key": { "type": "string", "description": "The key of the fact (e.g., 'user_birthday')" },
                "value": { "type": "string", "description": "The value of the fact" }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().ok_or_else(|| AgentError("Missing action".to_string()))?;
        let mut memory = self.memory.lock().await;

        match action {
            "set" => {
                let key = args["key"].as_str().ok_or_else(|| AgentError("Missing key".to_string()))?;
                let value = args["value"].as_str().ok_or_else(|| AgentError("Missing value".to_string()))?;
                memory.set_fact(key, value, 1.0).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Fact '{}' saved.", key))
            }
            "get_all" => {
                let facts = memory.all_facts();
                let output = facts.iter()
                    .map(|f| format!("{}: {}", f.key, f.value))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(if output.is_empty() { "No facts found.".to_string() } else { output })
            }
            _ => Err(AgentError("Unknown action".to_string())),
        }
    }
}
