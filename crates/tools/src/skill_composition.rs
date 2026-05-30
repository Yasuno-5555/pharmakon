use crate::codex_utils::{now, read_json, state_dir, write_json};
use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SkillRecipe {
    name: String,
    created_at: String,
    tools: Vec<String>,
    steps: Vec<String>,
}
pub struct SkillCompositionTool;

#[async_trait]
impl Tool for SkillCompositionTool {
    fn name(&self) -> &str {
        "skill_composition"
    }

    fn description(&self) -> &str {
        "Compose existing tools into reusable recipes such as search -> fetch -> summarize."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create", "list", "show"] },
                "name": { "type": "string" },
                "tools": { "type": "array", "items": { "type": "string" } },
                "steps": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["action"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Autonomous
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = state_dir("skills")?.join("compositions.json");
        let mut recipes: Vec<SkillRecipe> = read_json(&path)?;
        match args["action"].as_str().unwrap_or("list") {
            "create" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing name".to_string()))?
                    .to_string();
                let tools = args["tools"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                let steps = args["steps"]
                    .as_array()
                    .unwrap_or(&Vec::new())
                    .iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect();
                recipes.retain(|r| r.name != name);
                recipes.push(SkillRecipe {
                    name: name.clone(),
                    created_at: now(),
                    tools,
                    steps,
                });
                write_json(&path, &recipes)?;
                Ok(json!({ "created": name, "path": path }).to_string())
            }
            "show" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing name".to_string()))?;
                let recipe = recipes
                    .into_iter()
                    .find(|r| r.name == name)
                    .ok_or_else(|| AgentError(format!("Recipe not found: {}", name)))?;
                Ok(serde_json::to_string_pretty(&recipe).unwrap_or_default())
            }
            "list" => Ok(serde_json::to_string_pretty(&recipes).unwrap_or_default()),
            _ => Err(AgentError("Unknown skill_composition action".to_string())),
        }
    }
}
