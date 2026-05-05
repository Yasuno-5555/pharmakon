use async_trait::async_trait;
use serde_json::{Value, json};
use pharmakon_common::{Tool, AgentResult, AgentError, Config, SecretStore};

pub struct ConfigTool;

#[async_trait]
impl Tool for ConfigTool {
    fn name(&self) -> &str { "manage_config" }
    fn description(&self) -> &str { "Read or update Pharmakon configuration and secrets. Use this to help the user setup their assistant." }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["get", "set", "set_secret"] },
                "key": { "type": "string", "description": "Config key (e.g., 'default_agent.model') or secret name" },
                "value": { "type": "string", "description": "Value to set" }
            },
            "required": ["action", "key"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().ok_or_else(|| AgentError("Missing action".to_string()))?;
        let key = args["key"].as_str().ok_or_else(|| AgentError("Missing key".to_string()))?;
        
        let mut config = Config::load().unwrap_or_default();
        let secret_store = SecretStore::new();

        match action {
            "get" => {
                // Simplified getter for specific keys
                match key {
                    "default_agent.provider" => Ok(config.default_agent.provider),
                    "default_agent.model" => Ok(config.default_agent.model),
                    "gateway.port" => Ok(config.gateway.port.to_string()),
                    _ => Err(AgentError("Unsupported config key for reading".to_string())),
                }
            }
            "set" => {
                let value = args["value"].as_str().ok_or_else(|| AgentError("Missing value for set action".to_string()))?;
                match key {
                    "default_agent.provider" => config.default_agent.provider = value.to_string(),
                    "default_agent.model" => config.default_agent.model = value.to_string(),
                    "gateway.port" => config.gateway.port = value.parse().map_err(|e: std::num::ParseIntError| AgentError(e.to_string()))?,
                    _ => return Err(AgentError("Unsupported config key for writing".to_string())),
                }
                config.save().map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Successfully set {} to {}", key, value))
            }
            "set_secret" => {
                let value = args["value"].as_str().ok_or_else(|| AgentError("Missing value for set_secret action".to_string()))?;
                let mut secret_key = key.to_string();
                // Automatically uppercase keys that look like API keys or tokens for consistency
                if secret_key.to_lowercase().contains("api_key") || secret_key.to_lowercase().contains("token") {
                    secret_key = secret_key.to_uppercase();
                }
                secret_store.set_secret(&secret_key, value).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Successfully saved secret '{}' to security layer.", secret_key))
            }
            _ => Err(AgentError("Unknown action".to_string())),
        }
    }
}
