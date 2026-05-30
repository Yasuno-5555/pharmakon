use crate::codex_utils::scan_diff_risks;
use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};

pub struct DiffSecurityAuditorTool;

#[async_trait]
impl Tool for DiffSecurityAuditorTool {
    fn name(&self) -> &str {
        "diff_security_auditor"
    }

    fn description(&self) -> &str {
        "Audit a diff or patch for likely security regressions before applying it."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "diff": { "type": "string" }
            },
            "required": ["diff"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let diff = args["diff"].as_str().unwrap_or_default();
        let risks = scan_diff_risks(diff);
        Ok(json!({
            "approved": risks.is_empty(),
            "risk_count": risks.len(),
            "risks": risks
        })
        .to_string())
    }
}
