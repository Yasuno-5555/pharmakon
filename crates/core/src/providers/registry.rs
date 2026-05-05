use std::sync::Arc;
use crate::model::AgentModel;
use crate::providers::*;

pub struct ModelRegistry;

impl ModelRegistry {
    pub fn get_model(model_id: &str) -> Option<Arc<dyn AgentModel>> {
        let parts: Vec<&str> = model_id.split('/').collect();
        if parts.len() < 2 {
            return None;
        }

        let provider = parts[0];
        let model_name = parts[1];

        match provider {
            "openai" => {
                let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(OpenAIModel::new(api_key, model_name.to_string())))
            }
            "gemini" => {
                let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(GeminiModel::new(api_key, model_name.to_string())))
            }
            "anthropic" => {
                let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(AnthropicModel::new(api_key, model_name.to_string())))
            }
            "ollama" => Some(Arc::new(OllamaModel::new(Some("http://localhost:11434".to_string()), model_name.to_string()))),
            "groq" => {
                let api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(GroqModel::new(api_key, model_name.to_string())))
            }
            "perplexity" => {
                let api_key = std::env::var("PERPLEXITY_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(PerplexityModel::new(api_key, model_name.to_string())))
            }
            _ => None,
        }
    }
}
