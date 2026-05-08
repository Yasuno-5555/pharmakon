use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use crate::codex::utils::scan_diff_risks;

pub struct ProactiveInterventionTool;

#[async_trait]
impl Tool for ProactiveInterventionTool {
    fn name(&self) -> &str {
        "proactive_intervention"
    }

    fn description(&self) -> &str {
        "Evaluate planned commands, diffs, or task state and produce prioritized stop/warn/continue interventions."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "content": { "type": "string" },
                "kind": { "type": "string", "default": "plan" }
            },
            "required": ["content"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let content = args["content"].as_str().unwrap_or_default();
        let mut interventions = Vec::new();
        for risk in scan_diff_risks(content) {
            interventions
                .push(json!({ "priority": 9, "action": "stop_and_review", "reason": risk }));
        }
        if content.contains("cargo install") && !content.contains("cargo check") {
            interventions.push(json!({ "priority": 5, "action": "run_build_first", "reason": "Install requested before an explicit build/check step." }));
        }
        if interventions.is_empty() {
            interventions.push(json!({ "priority": 1, "action": "continue", "reason": "No high-confidence intervention triggers fired." }));
        }
        Ok(json!({ "kind": args["kind"].as_str().unwrap_or("plan"), "interventions": interventions }).to_string())
    }
}
