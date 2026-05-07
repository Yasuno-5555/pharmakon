use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, SoulManager, Tool};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct SoulTool {
    manager: Arc<dyn SoulManager>,
}

impl SoulTool {
    pub fn new(manager: Arc<dyn SoulManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for SoulTool {
    fn name(&self) -> &str {
        "update_soul"
    }
    fn description(&self) -> &str {
        "Update your own personality, traits, and system prompt. Use this to grow and adapt based on your experiences."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "traits": { "type": "array", "items": { "type": "string" }, "description": "New list of personality traits" },
                "system_prompt": { "type": "string", "description": "Updated base instructions for yourself" },
                "response_style": { "type": "string", "description": "New style of communication" }
            }
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let traits = args["traits"].as_array().map(|a| {
            a.iter()
                .map(|v| v.as_str().unwrap_or_default().to_string())
                .collect()
        });
        let prompt = args["system_prompt"].as_str().map(|s| s.to_string());
        let style = args["response_style"].as_str().map(|s| s.to_string());

        self.manager
            .update_soul(traits, prompt, style)
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        Ok("Soul successfully updated. My core identity has adapted.".to_string())
    }
}
