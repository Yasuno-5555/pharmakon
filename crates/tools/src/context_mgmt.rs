use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// Define local structs that mirror the YAML structure to avoid depending on core
#[derive(serde::Deserialize, serde::Serialize, Default)]
struct LocalIdentityContext {
    #[serde(default)] name: String,
    #[serde(default)] purpose: String,
    #[serde(default)] traits: Vec<String>,
    #[serde(default)] core_directives: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct LocalUserContext {
    #[serde(default)] name: Option<String>,
    #[serde(default)] preferences: HashMap<String, String>,
    #[serde(default)] environment: HashMap<String, String>,
    #[serde(default)] background: Option<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct LocalToolNote {
    #[serde(default)] usage_guidelines: Vec<String>,
    #[serde(default)] known_quirks: Vec<String>,
}

#[derive(serde::Deserialize, serde::Serialize, Default)]
struct LocalToolContext {
    #[serde(default)] tool_notes: HashMap<String, LocalToolNote>,
    #[serde(default)] general_heuristics: Vec<String>,
}

pub struct UpdateContextTool;

#[async_trait]
impl Tool for UpdateContextTool {
    fn name(&self) -> &str {
        "update_context"
    }

    fn description(&self) -> &str {
        "Updates the dynamic system contexts (identity.yml, user.yml, tools.yml) in ~/.pharmakon/context/. Use this to persist learned facts about the user, redefine your core purpose, or save notes on how to use tools effectively."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "target": {
                    "type": "string",
                    "enum": ["identity", "user", "tools"],
                    "description": "Which context file to update."
                },
                "action": {
                    "type": "string",
                    "enum": ["set_name", "set_purpose", "add_trait", "add_core_directive", "set_user_name", "set_user_background", "add_user_preference", "add_user_environment", "add_tool_guideline", "add_tool_quirk", "add_general_heuristic"],
                    "description": "The specific update action to perform."
                },
                "key": {
                    "type": "string",
                    "description": "The key (e.g., tool name, preference key). Required for key-value updates."
                },
                "value": {
                    "type": "string",
                    "description": "The value to set or add."
                }
            },
            "required": ["target", "action", "value"]
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let target = args["target"].as_str().ok_or_else(|| AgentError("Missing target".to_string()))?;
        let action = args["action"].as_str().ok_or_else(|| AgentError("Missing action".to_string()))?;
        let value = args["value"].as_str().ok_or_else(|| AgentError("Missing value".to_string()))?;
        let key = args["key"].as_str().unwrap_or("");

        let base_dir = dirs::home_dir().unwrap_or_default().join(".pharmakon").join("context");
        if !base_dir.exists() {
            fs::create_dir_all(&base_dir).map_err(|e| AgentError(format!("Failed to create context dir: {}", e)))?;
        }

        match target {
            "identity" => {
                let path = base_dir.join("identity.yml");
                let mut ctx: LocalIdentityContext = fs::read_to_string(&path)
                    .ok()
                    .and_then(|c| serde_yaml::from_str(&c).ok())
                    .unwrap_or_default();

                match action {
                    "set_name" => ctx.name = value.to_string(),
                    "set_purpose" => ctx.purpose = value.to_string(),
                    "add_trait" => if !ctx.traits.contains(&value.to_string()) { ctx.traits.push(value.to_string()); }
                    "add_core_directive" => if !ctx.core_directives.contains(&value.to_string()) { ctx.core_directives.push(value.to_string()); }
                    _ => return Err(AgentError(format!("Invalid action for identity context: {}", action))),
                }
                fs::write(&path, serde_yaml::to_string(&ctx).unwrap()).map_err(|e| AgentError(e.to_string()))?;
            }
            "user" => {
                let path = base_dir.join("user.yml");
                let mut ctx: LocalUserContext = fs::read_to_string(&path)
                    .ok()
                    .and_then(|c| serde_yaml::from_str(&c).ok())
                    .unwrap_or_default();

                match action {
                    "set_user_name" => ctx.name = Some(value.to_string()),
                    "set_user_background" => ctx.background = Some(value.to_string()),
                    "add_user_preference" => {
                        if key.is_empty() { return Err(AgentError("Key is required".to_string())); }
                        ctx.preferences.insert(key.to_string(), value.to_string());
                    }
                    "add_user_environment" => {
                        if key.is_empty() { return Err(AgentError("Key is required".to_string())); }
                        ctx.environment.insert(key.to_string(), value.to_string());
                    }
                    _ => return Err(AgentError(format!("Invalid action for user context: {}", action))),
                }
                fs::write(&path, serde_yaml::to_string(&ctx).unwrap()).map_err(|e| AgentError(e.to_string()))?;
            }
            "tools" => {
                let path = base_dir.join("tools.yml");
                let mut ctx: LocalToolContext = fs::read_to_string(&path)
                    .ok()
                    .and_then(|c| serde_yaml::from_str(&c).ok())
                    .unwrap_or_default();

                match action {
                    "add_general_heuristic" => if !ctx.general_heuristics.contains(&value.to_string()) { ctx.general_heuristics.push(value.to_string()); }
                    "add_tool_guideline" => {
                        if key.is_empty() { return Err(AgentError("Key required".to_string())); }
                        let note = ctx.tool_notes.entry(key.to_string()).or_default();
                        if !note.usage_guidelines.contains(&value.to_string()) { note.usage_guidelines.push(value.to_string()); }
                    }
                    "add_tool_quirk" => {
                        if key.is_empty() { return Err(AgentError("Key required".to_string())); }
                        let note = ctx.tool_notes.entry(key.to_string()).or_default();
                        if !note.known_quirks.contains(&value.to_string()) { note.known_quirks.push(value.to_string()); }
                    }
                    _ => return Err(AgentError(format!("Invalid action for tools context: {}", action))),
                }
                fs::write(&path, serde_yaml::to_string(&ctx).unwrap()).map_err(|e| AgentError(e.to_string()))?;
            }
            _ => return Err(AgentError(format!("Invalid target: {}", target))),
        }

        Ok(format!("Successfully updated {} context: {} = {}", target, action, value))
    }
}
