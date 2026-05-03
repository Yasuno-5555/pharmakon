pub mod agent_types;
pub use crate::agent_types::MessageContent;
pub use agent_types::*;

pub mod voice;
pub mod visual_primitives;
pub mod providers;

rust_i18n::i18n!("locales");
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Context;
use std::fs;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
pub mod secrets;
pub use secrets::SecretStore;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub agent: AgentConfig,
    #[serde(default)]
    pub gateway: GatewayConfig,
    #[serde(default)]
    pub agents: std::collections::HashMap<String, AgentConfig>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AgentConfig {
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_thinking")]
    pub thinking: String,
}

fn default_provider() -> String {
    "gemini".to_string()
}

fn default_model() -> String {
    "gemini-1.5-pro".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct GatewayConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_dm_policy")]
    pub dm_policy: String,
    pub webhook_secret: Option<String>,
}

fn default_dm_policy() -> String {
    "pairing".to_string()
}

fn default_thinking() -> String {
    "medium".to_string()
}

fn default_port() -> u16 {
    18789
}

impl Default for Config {
    fn default() -> Self {
        Self {
            agent: AgentConfig {
                provider: "gemini".to_string(),
                model: "gemini-1.5-pro".to_string(),
                thinking: default_thinking(),
            },
            gateway: GatewayConfig {
                port: default_port(),
                dm_policy: default_dm_policy(),
                webhook_secret: None,
            },
            agents: std::collections::HashMap::new(),
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let config_path = Self::get_path()?;
        if !config_path.exists() {
            log::info!("Config file not found at {:?}, using defaults.", config_path);
            return Ok(Config::default());
        }

        let content = fs::read_to_string(&config_path)
            .context(format!("Failed to read config file at {:?}", config_path))?;
        
        let config: Config = serde_json::from_str(&content)
            .context("Failed to parse config JSON")?;
        
        Ok(config)
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let config_path = Self::get_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(config_path, content)?;
        Ok(())
    }

    fn get_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir().context("Could not find home directory")?;
        Ok(home.join(".pharmakon").join("config.json"))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Event {
    Message(Message),
    Action(String),
    CanvasUpdate { primitive: crate::visual_primitives::CanvasPrimitive },
    CanvasClear,
    ToolResult { result: String },
    AgentResponse { content: MessageContent },
    AgentThought { content: MessageContent },
    AgentResponseChunk { session_id: String, chunk: String },
    AgentThoughtChunk { session_id: String, chunk: String },
    ToolCall { name: String, args: serde_json::Value },
    ApprovalRequest { id: String, tool: String, args: serde_json::Value },
    Error { message: String },
    CronJobList { jobs: Vec<CronJobInfo> },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum Request {
    SendMessage { message: String },
    ProvideApproval { id: String, approved: bool },
    GetStatus,
    ResetHistory,
    InteractiveResponse { element_id: String, action: String, value: serde_json::Value },
    GetCronJobs,
    CancelCronJob { id: String },
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
    async fn update_soul(&self, traits: Option<Vec<String>>, prompt: Option<String>, style: Option<String>) -> anyhow::Result<()>;
}
