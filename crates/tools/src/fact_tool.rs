use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use pharmakon_memory::BeliefSystem;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct FactTool {
    memory: Arc<Mutex<BeliefSystem>>,
}

impl FactTool {
    pub fn new(memory: Arc<Mutex<BeliefSystem>>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for FactTool {
    fn name(&self) -> &str {
        "manage_beliefs"
    }
    fn description(&self) -> &str {
        "Store or retrieve beliefs about the user or system to remember them across sessions."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "get_all"] },
                "claim": { "type": "string", "description": "The fact or belief to store" },
                "confidence": { "type": "number", "description": "Confidence level (0.0 to 1.0)" }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| AgentError("Missing action".to_string()))?;
        let mut memory = self.memory.lock().await;

        match action {
            "add" => {
                let claim = args["claim"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing claim".to_string()))?;
                let confidence = args["confidence"].as_f64().unwrap_or(0.9) as f32;
                
                memory
                    .add_belief(claim, confidence, "user_interaction")
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Belief recorded: {}", claim))
            }
            "get_all" => {
                let beliefs = memory.all_beliefs();
                let output = beliefs
                    .iter()
                    .map(|b| format!("[{:.2}] {}", b.confidence, b.claim))
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(if output.is_empty() {
                    "No beliefs found.".to_string()
                } else {
                    output
                })
            }
            _ => Err(AgentError("Unknown action".to_string())),
        }
    }
}
