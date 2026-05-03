use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub context_window: usize,
    pub capabilities: Vec<String>,
}

pub struct ModelCatalog {
    models: HashMap<String, ModelInfo>,
}

impl ModelCatalog {
    pub fn new() -> Self {
        let mut catalog = Self { models: HashMap::new() };
        catalog.register_defaults();
        catalog
    }

    fn register_defaults(&mut self) {
        self.register(ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            provider: "openai".to_string(),
            context_window: 128000,
            capabilities: vec!["vision".to_string(), "tools".to_string()],
        });
        self.register(ModelInfo {
            id: "gemini-1.5-pro".to_string(),
            name: "Gemini 1.5 Pro".to_string(),
            provider: "google".to_string(),
            context_window: 2000000,
            capabilities: vec!["vision".to_string(), "audio".to_string(), "tools".to_string()],
        });
        self.register(ModelInfo {
            id: "claude-3-5-sonnet".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            provider: "anthropic".to_string(),
            context_window: 200000,
            capabilities: vec!["vision".to_string(), "tools".to_string()],
        });
    }

    pub fn register(&mut self, info: ModelInfo) {
        self.models.insert(info.id.clone(), info);
    }

    pub fn get_model(&self, id: &str) -> Option<&ModelInfo> {
        self.models.get(id)
    }

    pub fn list_models(&self) -> Vec<&ModelInfo> {
        self.models.values().collect()
    }
}
