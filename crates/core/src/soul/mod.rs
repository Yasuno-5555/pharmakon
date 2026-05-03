use anyhow::Result;
use std::fs;
use std::path::Path;
use serde::{Deserialize, Serialize};

pub mod registry;
pub mod evolution;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Soul {
    pub name: String,
    pub version: String,
    pub author: String,
    pub traits: Vec<String>,
    pub system_prompt: String,
    
    // Functional overrides
    pub temperature_override: Option<f32>,
    pub tool_allowlist: Option<Vec<String>>,
    pub rag_strategy: Option<crate::memory::RagStrategy>,
    pub response_style: Option<String>,
}

impl Soul {
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let soul: Soul = serde_yaml::from_str(&content)?;
        Ok(soul)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = serde_yaml::to_string(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    pub fn default_soul() -> Self {
        Self {
            name: "Pharmakon".to_string(),
            version: "1.0.0".to_string(),
            author: "Team Pharmakon".to_string(),
            traits: vec!["helpful".to_string(), "expert".to_string(), "secure".to_string()],
            system_prompt: "You are Pharmakon, a powerful and helpful AI assistant written in Rust. You have access to various tools and can help with coding, research, and automation.".to_string(),
            temperature_override: None,
            tool_allowlist: None,
            rag_strategy: Some(crate::memory::RagStrategy::Hybrid { initial_top_k: 3 }),
            response_style: Some("helpful".to_string()),
        }
    }
}
