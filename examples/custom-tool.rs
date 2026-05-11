use async_trait::async_trait;
use pharmakon_common::{Tool, ToolCategory, AgentResult, ExecutionProfile};
use serde_json::{json, Value};

/// A ROT13 encoder tool — demonstrates how to implement a custom Tool.
pub struct Rot13Tool;

#[async_trait]
impl Tool for Rot13Tool {
    fn name(&self) -> &str {
        "rot13"
    }

    fn description(&self) -> &str {
        "ROT13-encode the input text."
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Custom("utility".into())
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to ROT13-encode"
                }
            },
            "required": ["text"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let text = args["text"].as_str().unwrap_or("");
        let encoded: String = text.chars().map(|c| match c {
            'a'..='m' | 'A'..='M' => ((c as u8) + 13) as char,
            'n'..='z' | 'N'..='Z' => ((c as u8) - 13) as char,
            _ => c,
        }).collect();
        Ok(encoded)
    }

    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: pharmakon_common::SideEffectLevel::None,
            filesystem_scope: pharmakon_common::FilesystemScope::None,
            reversibility: pharmakon_common::Reversibility::Trivial,
            ..Default::default()
        }
    }
}
