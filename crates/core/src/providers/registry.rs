use crate::model::AgentModel;
use crate::providers::*;
use std::sync::Arc;

pub struct ModelRegistry;

impl ModelRegistry {
    pub fn get_model(model_id: &str) -> Option<Arc<dyn AgentModel>> {
        let first_slash = model_id.find('/')?;
        let provider = &model_id[..first_slash];
        let mut model_name = model_id[first_slash + 1..].to_string();

        // LEGACY REDIRECTION: Map retired models to modern equivalents (2026 Support)
        if provider == "gemini" {
            if model_name.contains("1.5-pro") || model_name.contains("1.0-pro") {
                log::warn!("Gemini 1.x models are retired. Redirecting to 'gemini-2.5-pro'.");
                model_name = "gemini-2.5-pro".to_string();
            } else if model_name.contains("1.5-flash") {
                log::warn!("Gemini 1.5 Flash is retired. Redirecting to 'gemini-3-flash'.");
                model_name = "gemini-3-flash".to_string();
            }
        }

        // Normalize Gemini 3 model names to include -preview suffix if missing (based on 2026 API)
        if provider == "gemini"
            && (model_name.starts_with("gemini-3") || model_name.starts_with("gemini-3.1"))
            && !model_name.ends_with("-preview")
            && !model_name.contains("-deep-think")
        {
            model_name = format!("{}-preview", model_name);
        }

        match provider {
            "openai" => {
                let api_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
                if api_key.is_empty() {
                    return None;
                }
                Some(Arc::new(OpenAIModel::new(api_key, model_name)))
            }
            "gemini" => {
                let api_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
                if api_key.is_empty() {
                    return None;
                }
                Some(Arc::new(GeminiModel::new(api_key, model_name)))
            }
            "anthropic" => {
                let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
                if api_key.is_empty() {
                    return None;
                }
                Some(Arc::new(AnthropicModel::new(api_key, model_name)))
            }
            "ollama" => Some(Arc::new(OllamaModel::new(
                Some("http://localhost:11434".to_string()),
                model_name,
            ))),
            "groq" => {
                let api_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
                if api_key.is_empty() {
                    return None;
                }
                Some(Arc::new(GroqModel::new(api_key, model_name)))
            }
            "perplexity" => {
                let api_key = std::env::var("PERPLEXITY_API_KEY").unwrap_or_default();
                if api_key.is_empty() {
                    return None;
                }
                Some(Arc::new(PerplexityModel::new(api_key, model_name)))
            }
            _ => None,
        }
    }

    pub fn list_available_models() -> Vec<String> {
        let mut models = Vec::new();

        let gemini_key = std::env::var("GEMINI_API_KEY").unwrap_or_default();
        if !gemini_key.is_empty() {
            models.push("gemini/gemini-3.1-pro-preview".to_string());
            models.push("gemini/gemini-3-flash-preview".to_string());
            models.push("gemini/gemini-2.5-pro".to_string());
            models.push("gemini/gemini-2.5-flash".to_string());
            models.push("gemini/gemini-2.5-flash-lite".to_string());
        }

        let openai_key = std::env::var("OPENAI_API_KEY").unwrap_or_default();
        if !openai_key.is_empty() {
            models.push("openai/gpt-4o".to_string());
            models.push("openai/gpt-4o-mini".to_string());
            models.push("openai/o1-preview".to_string());
        }

        let anthropic_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
        if !anthropic_key.is_empty() {
            models.push("anthropic/claude-3-5-sonnet-latest".to_string());
            models.push("anthropic/claude-3-5-haiku-latest".to_string());
        }

        let groq_key = std::env::var("GROQ_API_KEY").unwrap_or_default();
        if !groq_key.is_empty() {
            models.push("groq/llama-3.3-70b-versatile".to_string());
        }

        let perplexity_key = std::env::var("PERPLEXITY_API_KEY").unwrap_or_default();
        if !perplexity_key.is_empty() {
            models.push("perplexity/sonar-large".to_string());
        }

        models.push("ollama/llama3.2".to_string());

        models
    }
}
