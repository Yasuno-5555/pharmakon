use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    /// The specific model ID this agent should use (e.g., "openai/gpt-4o").
    /// If not specified, the default model will be used.
    pub model_id: Option<String>,

    /// Path to a Soul file (.md) that defines the agent's personality and system prompt.
    pub soul_path: Option<String>,

    /// A list of tool names that this agent is explicitly allowed to use.
    /// If this is `None` or empty, it may inherit a default toolset or have no tools.
    pub allowed_tools: Option<Vec<String>>,
}
