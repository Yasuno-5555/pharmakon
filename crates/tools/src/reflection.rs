use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;

pub struct ReflectionTool;

#[async_trait]
impl Tool for ReflectionTool {
    fn name(&self) -> &str {
        "reflect"
    }

    fn description(&self) -> &str {
        "Perform a self-reflection on the current task trajectory. Identify what worked, what failed, and extract new 'Rules of Thumb' for the project."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "trajectory_summary": { "type": "string", "description": "A brief summary of the steps taken and their outcomes." },
                "lessons_learned": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of specific insights or rules extracted from this task."
                }
            },
            "required": ["trajectory_summary", "lessons_learned"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let summary = args["trajectory_summary"].as_str().ok_or_else(|| AgentError("Missing summary".to_string()))?;
        let lessons = args["lessons_learned"].as_array().ok_or_else(|| AgentError("Missing lessons".to_string()))?;

        let mut report = format!("### Self-Reflection Report\n\n**Trajectory Summary**: {}\n\n**Lessons Learned**:\n", summary);
        
        let mut new_rules = String::from("\n## Autonomous Rules (Extracted via Reflection)\n");
        for lesson in lessons {
            let lesson_str = lesson.as_str().unwrap_or_default();
            report.push_str(&format!("- {}\n", lesson_str));
            new_rules.push_str(&format!("- {}\n", lesson_str));
        }

        // Update PHARMAKON.md if it exists, or create it
        let rule_file = "PHARMAKON.md";
        let mut existing_content = if std::path::Path::new(rule_file).exists() {
            fs::read_to_string(rule_file).map_err(|e| AgentError(e.to_string()))?
        } else {
            "# Pharmakon Project Rules\n\nThis file contains architectural decisions and rules learned by the agent.\n".to_string()
        };

        existing_content.push_str(&new_rules);
        fs::write(rule_file, existing_content).map_err(|e| AgentError(e.to_string()))?;

        Ok(report)
    }
}
