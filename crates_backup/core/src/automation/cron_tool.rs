use crate::agent::Agent;
use crate::automation::cron::CronManager;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::Value;
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

pub struct CronTool {
    manager: Arc<CronManager>,
    agent: Weak<Mutex<Agent>>,
}

impl CronTool {
    pub fn new(manager: Arc<CronManager>, agent: Weak<Mutex<Agent>>) -> Self {
        Self { manager, agent }
    }
}

#[async_trait]
impl Tool for CronTool {
    fn name(&self) -> &str {
        "schedule_cron_job"
    }

    fn description(&self) -> &str {
        "Schedules a message to be sent to the agent at a specific time or after a delay. Use 'cron' for recurring jobs or 'delay' for one-shot timers."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "schedule_type": {
                    "type": "string",
                    "enum": ["cron", "delay"],
                    "description": "Whether to use a cron expression or a delay in seconds."
                },
                "cron_expr": {
                    "type": "string",
                    "description": "Cron expression (e.g., '1/10 * * * * * *' for every 10 seconds). Required if schedule_type is 'cron'."
                },
                "delay_secs": {
                    "type": "integer",
                    "description": "Delay in seconds before executing. Required if schedule_type is 'delay'."
                },
                "message": {
                    "type": "string",
                    "description": "The exact query or instruction to send to the agent when the schedule triggers."
                }
            },
            "required": ["schedule_type", "message"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let schedule_type = args
            .get("schedule_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let message = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if message.is_empty() {
            return Err(AgentError("Missing 'message' argument.".to_string()));
        }

        if schedule_type == "cron" {
            let expr = args.get("cron_expr").and_then(|v| v.as_str()).unwrap_or("");
            if expr.is_empty() {
                return Err(AgentError(
                    "Missing 'cron_expr' argument for cron schedule.".to_string(),
                ));
            }

            let id = self
                .manager
                .add_agent_job(expr, self.agent.clone(), message.clone())
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Ok(format!(
                "Successfully scheduled cron job with ID: {}. Trigger message: '{}'",
                id, message
            ));
        } else if schedule_type == "delay" {
            let delay_secs = args.get("delay_secs").and_then(|v| v.as_u64());
            if let Some(secs) = delay_secs {
                let id = self
                    .manager
                    .add_one_shot(secs, self.agent.clone(), message.clone())
                    .await
                    .map_err(|e| AgentError(e.to_string()))?;
                return Ok(format!(
                    "Successfully scheduled delayed job (in {} seconds) with ID: {}. Trigger message: '{}'",
                    secs, id, message
                ));
            } else {
                return Err(AgentError(
                    "Missing or invalid 'delay_secs' argument for delay schedule.".to_string(),
                ));
            }
        }

        Err(AgentError(
            "Invalid schedule_type. Must be 'cron' or 'delay'.".to_string(),
        ))
    }
}
