use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};

pub struct ToolRouterTool;

#[async_trait]
impl Tool for ToolRouterTool {
    fn name(&self) -> &str {
        "route_tools"
    }

    fn description(&self) -> &str {
        "Get a recommended subset of tools for a specific intent and estimate potential costs."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": {
                    "type": "string",
                    "enum": ["debugging", "research", "refactoring", "filesystem", "system_diagnostics"],
                    "description": "The high-level intent of the current step."
                }
            },
            "required": ["intent"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let intent = args["intent"].as_str().unwrap_or("general");

        let recommendations = match intent {
            "debugging" => json!({
                "allowed_tools": ["lsp", "grep", "read_file", "view_file", "terminal"],
                "cost_grade": "Low",
                "recommended_chain": "lsp (find_definition) -> read_file -> view_file"
            }),
            "research" => json!({
                "allowed_tools": ["brave_search", "google_search", "web_fetch", "hydrate_context", "custom_scout"],
                "cost_grade": "High",
                "recommended_chain": "custom_scout -> web_fetch -> hydrate_context"
            }),
            "refactoring" => json!({
                "allowed_tools": ["lsp", "apply_patch", "read_file", "repomap", "structural_diff"],
                "cost_grade": "Medium",
                "recommended_chain": "repomap -> lsp -> apply_patch -> structural_diff"
            }),
            "filesystem" => json!({
                "allowed_tools": ["ls", "read_file", "write_file", "apply_patch"],
                "cost_grade": "Low",
                "recommended_chain": "ls -> read_file -> apply_patch"
            }),
            "system_diagnostics" => json!({
                "allowed_tools": ["self_diagnostic", "checkpoint", "reflect"],
                "cost_grade": "Low",
                "recommended_chain": "self_diagnostic -> reflect"
            }),
            _ => json!({
                "allowed_tools": ["all"],
                "cost_grade": "Variable"
            }),
        };

        Ok(recommendations.to_string())
    }
}
