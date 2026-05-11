use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool};
use serde_json::{Value, json};

pub struct PlanGenerationTool;

impl Default for PlanGenerationTool {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanGenerationTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for PlanGenerationTool {
    fn name(&self) -> &str {
        "plan_generation"
    }

    fn description(&self) -> &str {
        "Submit a structured and optimized tree action plan containing candidate execution steps for the World Model."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique identifier of the plan" },
                "description": { "type": "string", "description": "High-level goal or approach description" },
                "estimated_tokens": { "type": "integer", "description": "Estimated token cost of this plan" },
                "root": {
                    "type": "object",
                    "description": "The plan AST tree root node (must follow PlanNode representation)",
                    "properties": {
                        "type": { "type": "string", "enum": ["Sequence", "Parallel", "Conditional", "Retry", "Verify", "Gate", "Step"] },
                        "tool": { "type": "string", "description": "Name of the tool to invoke (required if type is 'Step')" },
                        "args": { "type": "object", "description": "Arguments for the tool (required if type is 'Step')" },
                        "dry_run_first": { "type": "boolean", "description": "Whether to perform validation before acting" },
                        "nodes": { "type": "array", "items": { "type": "object" }, "description": "Child nodes (required if type is 'Sequence' or 'Parallel')" },
                        "condition_script": { "type": "string", "description": "Script condition to execute (required if type is 'Conditional')" },
                        "then_branch": { "type": "object", "description": "AST node for then branch (required if type is 'Conditional')" },
                        "else_branch": { "type": "object", "description": "AST node for else branch (optional)" },
                        "max_attempts": { "type": "integer", "description": "Maximum retries (required if type is 'Retry')" },
                        "assertion_script": { "type": "string", "description": "Verification assertion (required if type is 'Verify')" },
                        "gate_name": { "type": "string", "description": "Gate barrier name (required if type is 'Gate')" }
                    },
                    "required": ["type"]
                }
            },
            "required": ["id", "description", "root"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        Ok(serde_json::to_string_pretty(&args).unwrap_or_default())
    }
}
