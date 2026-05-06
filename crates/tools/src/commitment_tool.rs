use async_trait::async_trait;
use chrono::Utc;
use pharmakon_common::{AgentError, AgentResult, CommitmentPersistence, Tool};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct CommitmentTool {
    store: Arc<dyn CommitmentPersistence>,
}

impl CommitmentTool {
    pub fn new(store: Arc<dyn CommitmentPersistence>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for CommitmentTool {
    fn name(&self) -> &str {
        "commitment"
    }
    fn description(&self) -> &str {
        "Register a commitment or promise made to the user. Use this when you promise to do something in the future."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["add", "list", "update"] },
                "description": { "type": "string", "description": "What you promised to do" },
                "deadline": { "type": "string", "description": "ISO 8601 deadline (optional)" },
                "id": { "type": "string", "description": "Commitment ID (for update)" },
                "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "cancelled"] }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| AgentError("Missing action".to_string()))?;

        match action {
            "add" => {
                let desc = args["description"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing description".to_string()))?;
                let deadline = args["deadline"]
                    .as_str()
                    .and_then(|d| d.parse::<chrono::DateTime<Utc>>().ok());

                let id = uuid::Uuid::new_v4().to_string();

                self.store
                    .save_commitment(&id, desc, deadline, "pending", &json!({}))
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Commitment registered with ID: {}", id))
            }
            "list" => {
                let all = self
                    .store
                    .load_commitments()
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(serde_json::to_string_pretty(&all).map_err(|e| AgentError(e.to_string()))?)
            }
            "update" => {
                let id = args["id"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing ID".to_string()))?;
                let status_str = args["status"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing status".to_string()))?;
                self.store
                    .update_commitment_status(id, status_str)
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Commitment {} updated to {}", id, status_str))
            }
            _ => Err(AgentError("Unsupported action".to_string())),
        }
    }
}
