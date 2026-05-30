use crate::codex_utils::scan_diff_risks;
use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};

pub struct CognitiveMirrorTool;

#[async_trait]
impl Tool for CognitiveMirrorTool {
    fn name(&self) -> &str {
        "cognitive_mirror"
    }

    fn description(&self) -> &str {
        "Compress an agent state into human-readable Goal, Risk, Confidence, and Reason fields."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "goal": { "type": "string" },
                "plan": { "type": "string" },
                "evidence": { "type": "string" },
                "risk_signals": { "type": "array", "items": { "type": "string" } }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let plan = args["plan"].as_str().unwrap_or_default();
        let risk_count = args["risk_signals"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or_default()
            + scan_diff_risks(plan).len();
        let risk = if risk_count >= 3 {
            "high"
        } else if risk_count > 0 {
            "medium"
        } else {
            "low"
        };
        let confidence = match risk {
            "low" => 0.82,
            "medium" => 0.64,
            _ => 0.42,
        };
        Ok(json!({
            "goal": args["goal"].as_str().unwrap_or("unspecified"),
            "risk": risk,
            "confidence": confidence,
            "reason": if risk_count > 0 { "Risk signals or security patterns were detected." } else { "No major risk signal was detected in the supplied state." },
            "evidence": args["evidence"].as_str().unwrap_or_default()
        }).to_string())
    }
}
