use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};

pub struct SemanticConflictResolutionTool;
#[async_trait]
impl Tool for SemanticConflictResolutionTool {
    fn name(&self) -> &str {
        "semantic_conflict_resolution"
    }

    fn description(&self) -> &str {
        "Resolve conflicting beliefs by preferring source-code truth, explicit authority, and newer evidence."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "beliefs": { "type": "array", "items": { "type": "object" } }
            },
            "required": ["beliefs"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let beliefs = args["beliefs"]
            .as_array()
            .ok_or_else(|| AgentError("Missing beliefs".to_string()))?;
        let mut ranked = Vec::new();
        for belief in beliefs {
            let source = belief["source"].as_str().unwrap_or("note");
            let authority = belief["authority"].as_f64().unwrap_or(0.5);
            let source_boost = match source {
                "source_code" | "code" => 1.0,
                "test" | "compiler" => 0.9,
                "docs" => 0.6,
                _ => 0.4,
            };
            let updated = belief["updated_at"].as_str().unwrap_or("");
            ranked.push(json!({
                "belief": belief,
                "score": authority + source_boost + if updated.is_empty() { 0.0 } else { 0.1 }
            }));
        }
        ranked.sort_by(|a, b| {
            b["score"]
                .as_f64()
                .unwrap_or(0.0)
                .partial_cmp(&a["score"].as_f64().unwrap_or(0.0))
                .unwrap()
        });
        Ok(json!({
            "winner": ranked.first(),
            "deprecated": ranked.iter().skip(1).collect::<Vec<_>>(),
            "policy": "source_code > compiler/test > docs > notes; newer evidence breaks ties"
        })
        .to_string())
    }
}
