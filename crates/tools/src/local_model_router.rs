use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{json, Value};
use std::time::Duration;

pub struct LocalModelRouterTool;

#[async_trait]
impl Tool for LocalModelRouterTool {
    fn name(&self) -> &str {
        "local_model_router"
    }

    fn description(&self) -> &str {
        "Recommend a local-first model route, preferring Ollama when suitable and external providers for harder tasks."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task": { "type": "string" },
                "complexity": { "type": "string", "enum": ["low", "medium", "high"], "default": "medium" },
                "requires_vision": { "type": "boolean", "default": false },
                "requires_current_web": { "type": "boolean", "default": false }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(|e| AgentError(e.to_string()))?;
        let ollama_tags = match client.get("http://localhost:11434/api/tags").send().await {
            Ok(response) => response.json::<Value>().await.ok(),
            Err(_) => None,
        };
        let complexity = args["complexity"].as_str().unwrap_or("medium");
        let requires_vision = args["requires_vision"].as_bool().unwrap_or(false);
        let requires_web = args["requires_current_web"].as_bool().unwrap_or(false);
        let route = if requires_vision || requires_web || complexity == "high" {
            "external_frontier"
        } else if ollama_tags.is_some() {
            "ollama_local"
        } else {
            "configured_default"
        };
        Ok(json!({
            "route": route,
            "reason": {
                "complexity": complexity,
                "requires_vision": requires_vision,
                "requires_current_web": requires_web,
                "ollama_available": ollama_tags.is_some()
            },
            "ollama": ollama_tags
        })
        .to_string())
    }
}
