use std::sync::Arc;
use crate::model::AgentModel;
use crate::providers::*;

pub struct ModelRegistry;

impl ModelRegistry {
    pub fn get_model(model_id: &str) -> Option<Arc<dyn AgentModel>> {
        let first_slash = model_id.find('/')?;
        let provider = &model_id[..first_slash];
        let mut model_name = model_id[first_slash + 1..].to_string();

        // Normalize Gemini 3 model names to include -preview suffix if missing
        if provider == "gemini" && model_name.starts_with("gemini-3") && !model_name.ends_with("-preview") && !model_name.contains("-deep-think") {
            model_name = format!("{}-preview", model_name);
        }

        match provider {
            "openai" => {
                let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(OpenAIModel::new(api_key, model_name)))
            }
            "gemini" => {
                let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(GeminiModel::new(api_key, model_name)))
            }
            "anthropic" => {
                let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(AnthropicModel::new(api_key, model_name)))
            }
            "ollama" => Some(Arc::new(OllamaModel::new(Some("http://localhost:11434".to_string()), model_name))),
            "groq" => {
                let api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(GroqModel::new(api_key, model_name)))
            }
            "perplexity" => {
                let api_key = std::env::var("PERPLEXITY_API_KEY").unwrap_or_default();
                if api_key.is_empty() { return None; }
                Some(Arc::new(PerplexityModel::new(api_key, model_name)))
            }
            _ => None,
        }
    }

    pub fn list_available_models() -> Vec<String> {
        vec![
            "gemini/gemini-3.1-pro-preview".to_string(),
            "gemini/gemini-3-flash-preview".to_string(),
            "gemini/gemini-3.1-flash-lite-preview".to_string(),
            "gemini/gemini-2.5-pro".to_string(),
            "gemini/gemini-2.5-flash".to_string(),
            "gemini/gemini-1.5-pro".to_string(),
            "gemini/gemini-1.5-flash".to_string(),
            "openai/gpt-4o".to_string(),
            "openai/gpt-4o-mini".to_string(),
            "openai/o1-preview".to_string(),
            "anthropic/claude-3-5-sonnet-latest".to_string(),
            "anthropic/claude-3-5-haiku-latest".to_string(),
            "groq/llama-3.3-70b-versatile".to_string(),
            "perplexity/sonar-large".to_string(),
        ]
    }
}
