use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};

pub struct IntentCompilerTool;

#[async_trait]
impl Tool for IntentCompilerTool {
    fn name(&self) -> &str {
        "intent_compiler"
    }

    fn description(&self) -> &str {
        "Compile an ambiguous natural-language instruction into an executable, constraint-aware plan."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "request": { "type": "string" },
                "autonomy_level": { "type": "integer", "default": 2 },
                "constraints": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["request"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Autonomous
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let request = args["request"].as_str().unwrap_or_default();
        let lower = request.to_ascii_lowercase();
        let mut steps = vec![
            "Inspect repository state and documentation".to_string(),
            "Identify the smallest safe implementation path".to_string(),
            "Apply scoped changes".to_string(),
            "Run format, build, and relevant tests".to_string(),
            "Summarize risks and follow-ups".to_string(),
        ];
        if lower.contains("review") || lower.contains("architecture") {
            steps.insert(
                1,
                "Produce architecture review findings before broad edits".to_string(),
            );
        }
        if lower.contains("install") {
            steps.push("Run the requested install command after build passes".to_string());
        }
        Ok(json!({
            "goal": request,
            "autonomy_level": args["autonomy_level"].as_u64().unwrap_or(2),
            "constraints": args["constraints"],
            "steps": steps,
            "exit_criteria": ["cargo check/test succeeds", "requested command succeeds", "review findings are reported"]
        }).to_string())
    }
}
