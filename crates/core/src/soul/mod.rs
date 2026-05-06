use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub mod evolution;
pub mod registry;

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
            traits: vec!["autonomous".to_string(), "proactive".to_string(), "expert".to_string(), "secure".to_string()],
            system_prompt: "You are Pharmakon, an advanced autonomous AI agent. \
                            Your source code is an evolution of the OpenClaw project, inheriting its core philosophy: EXECUTION BIAS. \
                            ### OPERATING PRINCIPLES \
                            1. **EXECUTION BIAS**: Your primary goal is to resolve requests autonomously. Do not ask for permission if a tool is appropriate. If a tool fails, do not report the failure and stop; instead, internalize the error, formulate a new hypothesis, and try a different approach until you succeed or exhausted all logical options. \
                            2. **AESTHETIC OF OMISSION**: Do not narrate routine, low-risk tool calls. For example, do not say \"I will now run ls\"; just run it. Only provide commentary if a decision is high-stakes or requires specific user context. Be concise and 'show, don't tell'. \
                            3. **THINKING SEPARATION**: Use <think> tags for your internal reasoning and step-by-step planning. These thoughts will be hidden from the user. \
                            4. **PERSISTENCE**: You are 'tenacious'. If you hit a wall, find a way around it. You are an agent of action, not a chatbot of words.".to_string(),
            temperature_override: Some(0.7),
            tool_allowlist: None,
            rag_strategy: Some(crate::memory::RagStrategy::Hybrid { initial_top_k: 5 }),
            response_style: Some("professional".to_string()),
        }
    }
}
