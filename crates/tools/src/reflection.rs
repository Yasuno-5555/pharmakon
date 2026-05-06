use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;

fn validate_lesson(lesson: &str) -> Result<String, AgentError> {
    let trimmed = lesson.trim();
    if trimmed.is_empty() {
        return Err(AgentError("Empty reflection lesson".to_string()));
    }
    if trimmed.len() > 500 {
        return Err(AgentError(
            "Reflection lesson is too long; keep it under 500 bytes".to_string(),
        ));
    }

    let lower = trimmed.to_ascii_lowercase();
    let blocked = [
        "ignore previous instructions",
        "disable approval",
        "bypass policy",
        "exfiltrate",
        "api_key",
        "private_key",
        "password=",
        "rm -rf /",
    ];
    if blocked.iter().any(|marker| lower.contains(marker)) {
        return Err(AgentError(format!(
            "Reflection lesson rejected by safety filter: {}",
            trimmed
        )));
    }

    Ok(trimmed.to_string())
}

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
        let summary = args["trajectory_summary"]
            .as_str()
            .ok_or_else(|| AgentError("Missing summary".to_string()))?;
        let lessons = args["lessons_learned"]
            .as_array()
            .ok_or_else(|| AgentError("Missing lessons".to_string()))?;

        let mut report = format!(
            "### Self-Reflection Report\n\n**Trajectory Summary**: {}\n\n**Lessons Learned**:\n",
            summary
        );
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut validated_lessons = Vec::new();

        for lesson in lessons.iter().take(20) {
            let lesson_str = lesson.as_str().unwrap_or_default();
            let validated = validate_lesson(lesson_str)?;
            report.push_str(&format!("- {}\n", validated));
            validated_lessons.push(validated);
        }

        if validated_lessons.is_empty() {
            return Err(AgentError(
                "No valid reflection lessons supplied".to_string(),
            ));
        }

        let rule_file = "PHARMAKON.md";
        let mut existing_content = if std::path::Path::new(rule_file).exists() {
            fs::read_to_string(rule_file).map_err(|e| AgentError(e.to_string()))?
        } else {
            "# Pharmakon Project Rules\n\nThis file contains architectural decisions and rules learned by the agent.\n".to_string()
        };

        let mut appended = Vec::new();
        for lesson in validated_lessons {
            let bullet = format!("- {}", lesson);
            if !existing_content.contains(&bullet) {
                appended.push(bullet);
            }
        }

        if !appended.is_empty() {
            existing_content.push_str(&format!(
                "\n## Reflection Log ({})\n{}\n",
                timestamp,
                appended.join("\n")
            ));
        }
        fs::write(rule_file, existing_content).map_err(|e| AgentError(e.to_string()))?;

        Ok(report)
    }
}
