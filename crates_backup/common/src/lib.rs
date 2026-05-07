pub mod agent;
pub mod agent_types;
pub use crate::agent_types::MessageContent;
pub use agent_types::*;

pub mod providers;
pub mod visual_primitives;
pub mod voice;

rust_i18n::i18n!("locales");
use anyhow::Context;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
pub mod plugin_context;
pub mod secrets;
pub mod telemetry;
pub use secrets::SecretStore;

use crate::agent::AgentConfig;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub gateway: GatewayConfig,

    /// This field is being deprecated in favor of a separate agents.toml file.
    /// It is kept for backward compatibility for now.
    #[serde(default)]
    pub agents: HashMap<String, AgentConfig>,

    #[serde(default, alias = "agent")]
    pub default_agent: DefaultAgentConfig,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DefaultAgentConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_fallback_models")]
    pub fallback_models: Vec<String>,
}

impl Default for DefaultAgentConfig {
    fn default() -> Self {
        Self {
            provider: default_provider(),
            model: default_model(),
            fallback_models: default_fallback_models(),
        }
    }
}

fn default_fallback_models() -> Vec<String> {
    vec![
        "gemini/gemini-2.5-flash".to_string(),
        "groq/llama-3.3-70b-versatile".to_string(),
    ]
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_dm_policy")]
    pub dm_policy: String,
    pub webhook_secret: Option<String>,
}

fn default_provider() -> String {
    "gemini".to_string()
}

fn default_model() -> String {
    "gemini-2.5-flash".to_string()
}

fn default_port() -> u16 {
    19999
}

fn default_dm_policy() -> String {
    "pairing".to_string()
}

// ... (rest of the file)

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::get_path()?;
        if !config_path.exists() {
            log::info!(
                "Config file not found at {:?}, creating default.",
                config_path
            );
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&config_path)
            .context(format!("Failed to read config file at {:?}", config_path))?;

        let mut config: Config =
            serde_json::from_str(&content).context("Failed to parse config JSON")?;

        // Now, try to load agents from agents.toml
        let agents_path = Self::get_agents_path()?;
        if agents_path.exists() {
            let agents_content = fs::read_to_string(&agents_path)
                .context(format!("Failed to read agents config at {:?}", agents_path))?;

            #[derive(Deserialize)]
            struct AgentsFile {
                agent: HashMap<String, AgentConfig>,
            }

            let parsed_agents: AgentsFile =
                toml::from_str(&agents_content).context("Failed to parse agents.toml")?;

            // Merge the loaded agents into the main config
            config.agents.extend(parsed_agents.agent);

            log::debug!(
                "Loaded {} agents from agents.toml: {:?}",
                config.agents.len(),
                config.agents.keys()
            );
        }

        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::get_path()?;
        let parent_dir = config_path.parent().context("Invalid config path")?;
        fs::create_dir_all(parent_dir)?;

        let content = serde_json::to_string_pretty(self)?;
        fs::write(&config_path, content)
            .context(format!("Failed to write config to {:?}", config_path))?;
        Ok(())
    }

    fn get_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".pharmakon").join("config.json"))
    }

    fn get_agents_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".pharmakon").join("agents.toml"))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            gateway: GatewayConfig::default(),
            agents: HashMap::<String, AgentConfig>::new(),
            default_agent: DefaultAgentConfig {
                provider: "gemini".to_string(),
                model: "gemini-2.5-flash".to_string(),
                fallback_models: default_fallback_models(),
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    Message(Message),
    Action(String),
    InteractionFinished {
        response: CompletionResponse,
    },
    CanvasUpdate {
        primitive: crate::visual_primitives::CanvasPrimitive,
    },
    CanvasClear,
    ToolResult {
        result: String,
    },
    AgentResponse {
        content: MessageContent,
    },
    AgentThought {
        content: MessageContent,
    },
    AgentResponseChunk {
        session_id: String,
        chunk: String,
    },
    AgentThoughtChunk {
        session_id: String,
        chunk: String,
    },
    ToolCall {
        name: String,
        args: serde_json::Value,
    },
    ApprovalRequest {
        id: String,
        tool: String,
        args: serde_json::Value,
    },
    Error {
        message: String,
    },
    CronJobList {
        jobs: Vec<CronJobInfo>,
    },
    SessionList {
        sessions: Vec<String>,
    },
    OrchestrationState {
        supervisor_active: bool,
        sub_agents: Vec<SubAgentInfo>,
    },
    GatewayStatus {
        uptime: u64,
        connected_clients: usize,
        memory_usage: u64,
    },
    AgentInsight {
        insight: String,
    },
    VisionUpdate {
        frames: Vec<VisionFrameInfo>,
    },
    GraphUpdate {
        relations: Vec<String>,
    },
    ModelList {
        models: Vec<String>,
    },
    ModelSwitched {
        model_id: String,
    },
    HistoryList {
        messages: Vec<Message>,
    },
    AgentHangDetected {
        reason: String,
    },
    SystemLog {
        level: String,
        message: String,
    },
    TokenUsageUpdate {
        total_tokens: u64,
        total_cost: f64,
    },
    ToolList {
        tools: Vec<ToolInfo>,
    },
    UsageHistory {
        history: Vec<UsageEntry>,
    },
    McpStats {
        stats: Vec<McpStatEntry>,
    },
    ResearchNotebookUpdate {
        notebook: ResearchNotebook,
    },
    SettingsUpdate {
        settings: serde_json::Value,
    },
    ForensicLog {
        id: String,
        action: String,
        hypothesis: String,
        observation: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UsageEntry {
    pub timestamp: String,
    pub tokens: u64,
    pub cost: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct McpStatEntry {
    pub name: String,
    pub call_count: u32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VisionFrameInfo {
    pub path: String,
    pub captured_at: String,
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SubAgentInfo {
    pub name: String,
    pub role: String,
    pub last_task: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Request {
    SendMessage {
        message: String,
    },
    ProvideApproval {
        id: String,
        approved: bool,
    },
    GetStatus,
    ResetHistory,
    InteractiveResponse {
        element_id: String,
        action: String,
        value: serde_json::Value,
    },
    GetCronJobs,
    CancelCronJob {
        id: String,
    },
    GetSessions,
    SwitchSession {
        id: String,
    },
    GetOrchestration,
    GetGatewayStatus,
    GetMcpStats,
    GetVisionFrames,
    GetGraphMemory {
        query: String,
    },
    GetModels,
    SwitchModel {
        model_id: String,
    },
    GetHistory {
        session_id: String,
    },
    SearchSessions {
        query: String,
    },
    GetTools,
    GetUsageHistory,
    GetResearchNotebook,
    GetSettings,
    UpdateSettings {
        settings: serde_json::Value,
    },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CronJobInfo {
    pub id: String,
    pub schedule_type: String,
    pub expr: String,
    pub message: String,
}

#[async_trait]
pub trait AgentSpawner: Send + Sync {
    async fn spawn(&self, task: &str, soul: Option<String>, depth: u8) -> anyhow::Result<String>;
}

#[async_trait]
pub trait KnowledgeConnector: Send + Sync {
    fn name(&self) -> &str;
    async fn fetch_context(&self, query: &str) -> anyhow::Result<Vec<String>>;
}

#[async_trait]
pub trait SoulManager: Send + Sync {
    async fn update_soul(
        &self,
        traits: Option<Vec<String>>,
        prompt: Option<String>,
        style: Option<String>,
    ) -> anyhow::Result<()>;
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum ResearchDepth {
    Skim,    // Title + Snippet (~100 tokens)
    Summary, // Extracted key points (~300 tokens)
    Deep,    // Full text / RAG blocks (~2000+ tokens)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Fact {
    pub content: String,
    pub source_url: String,
    pub confidence: f32,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct ResearchNotebook {
    pub current_goal: String,
    pub verified_facts: Vec<Fact>,
    pub pending_questions: Vec<String>,
    pub visited_urls: HashMap<String, ResearchDepth>,
    pub dead_ends: Vec<String>,
    pub research_tree: HashMap<String, Vec<String>>, // Query -> List of URLs
    pub step_count: u32,
    pub max_steps: u32,
    pub last_information_gain: f32,
    pub min_information_gain: f32,
}

impl ResearchNotebook {
    pub fn new(goal: &str) -> Self {
        Self {
            current_goal: goal.to_string(),
            max_steps: 10,
            min_information_gain: 0.1,
            ..Default::default()
        }
    }

    pub fn should_stop(&self) -> bool {
        if self.step_count >= self.max_steps {
            return true;
        }
        if self.step_count > 2 && self.last_information_gain < self.min_information_gain {
            return true;
        }
        false
    }

    pub fn to_summary_string(&self) -> String {
        let mut s = format!("## Research Goal: {}\n\n", self.current_goal);

        s.push_str("### Verified Facts:\n");
        for fact in &self.verified_facts {
            s.push_str(&format!(
                "- {} (Source: {})\n",
                fact.content, fact.source_url
            ));
        }

        s.push_str("\n### Pending Questions:\n");
        for q in &self.pending_questions {
            s.push_str(&format!("- {}\n", q));
        }

        s.push_str("\n### Dead Ends:\n");
        for d in &self.dead_ends {
            s.push_str(&format!("- {}\n", d));
        }

        s
    }
}

#[async_trait]
pub trait ResearchPersistence: Send + Sync {
    async fn get_research_cache(
        &self,
        url: &str,
    ) -> anyhow::Result<Option<(String, String, serde_json::Value)>>;
    async fn save_research_cache(
        &self,
        url: &str,
        content: &str,
        depth: &str,
        metadata: &serde_json::Value,
    ) -> anyhow::Result<()>;
}

pub struct CodeUtils;

impl CodeUtils {
    pub fn skeletonize_code(code: &str) -> String {
        let mut skeleton = String::new();
        let mut in_body = 0;
        let mut brace_count = 0;
        for line in code.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            if in_body == 0 {
                skeleton.push_str(line);
                if trimmed.ends_with('{') || trimmed.contains('{') {
                    skeleton.push_str(" { ... }\n");
                    in_body = 1;
                    brace_count = trimmed.chars().filter(|&c| c == '{').count() as i32
                        - trimmed.chars().filter(|&c| c == '}').count() as i32;
                    if brace_count <= 0 {
                        in_body = 0;
                    }
                } else {
                    skeleton.push('\n');
                }
            } else {
                brace_count += trimmed.chars().filter(|&c| c == '{').count() as i32
                    - trimmed.chars().filter(|&c| c == '}').count() as i32;
                if brace_count <= 0 {
                    in_body = 0;
                }
            }
        }
        if skeleton.is_empty() {
            code.to_string()
        } else {
            skeleton
        }
    }
}
