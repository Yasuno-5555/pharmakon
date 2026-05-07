use crate::agent::Agent;
use crate::trajectory::insight_synthesizer::InsightSynthesizer;
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use serde_json::{Value, json};
use std::sync::Weak;

pub struct InsightTool {
    agent_ref: Weak<Agent>,
}

impl InsightTool {
    pub fn new(agent: Weak<Agent>) -> Self {
        Self { agent_ref: agent }
    }
}

#[async_trait]
impl Tool for InsightTool {
    fn name(&self) -> &str {
        "synthesize_insights"
    }
    fn description(&self) -> &str {
        "Analyze the current session's trajectory and synthesize 'Lessons Learned' into the project's knowledge base. Use this at the end of a successful task or after a complex failure."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {}
        })
    }

    async fn call(&self, _args: Value) -> AgentResult<String> {
        if let Some(agent) = self.agent_ref.upgrade() {
            match InsightSynthesizer::synthesize(agent).await {
                Ok(insight) => Ok(format!(
                    "Insights successfully synthesized and saved:\n\n{}",
                    insight
                )),
                Err(e) => Err(AgentError(format!("Failed to synthesize insights: {}", e))),
            }
        } else {
            Err(AgentError("Agent reference lost".to_string()))
        }
    }
}
