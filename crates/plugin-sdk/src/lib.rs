//! Pharmakon Plugin SDK for external tools.
//!
//! External crates depend on this lightweight SDK to implement tools
//! that the Pharmakon agent can discover and invoke at runtime.
//!
//! Minimum dependency footprint — re-exports essentials from `pharmakon-common`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

// ─── Error Model ───

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentErrorCode {
    RateLimit,
    InvalidRequest,
    AuthenticationFailed,
    ContextExceeded,
    ModelError,
    ToolNotFound,
    ToolExecutionFailed,
    HangDetected,
    NetworkError,
    InternalError,
    EnvironmentError,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AgentError {}

impl AgentError {
    pub fn new(code: AgentErrorCode, message: impl Into<String>) -> Self {
        Self(format!("[{:?}] {}", code, message.into()))
    }

    pub fn code(&self) -> AgentErrorCode {
        if self.0.starts_with("[RateLimit]") { AgentErrorCode::RateLimit }
        else if self.0.starts_with("[InvalidRequest]") { AgentErrorCode::InvalidRequest }
        else if self.0.starts_with("[AuthenticationFailed]") { AgentErrorCode::AuthenticationFailed }
        else if self.0.starts_with("[ContextExceeded]") { AgentErrorCode::ContextExceeded }
        else if self.0.starts_with("[ModelError]") { AgentErrorCode::ModelError }
        else if self.0.starts_with("[ToolNotFound]") { AgentErrorCode::ToolNotFound }
        else if self.0.starts_with("[ToolExecutionFailed]") { AgentErrorCode::ToolExecutionFailed }
        else if self.0.starts_with("[HangDetected]") { AgentErrorCode::HangDetected }
        else if self.0.starts_with("[NetworkError]") { AgentErrorCode::NetworkError }
        else if self.0.starts_with("[EnvironmentError]") { AgentErrorCode::EnvironmentError }
        else if self.0.contains("429") || self.0.to_lowercase().contains("rate limit") { AgentErrorCode::RateLimit }
        else { AgentErrorCode::InternalError }
    }

    pub fn is_rate_limit(&self) -> bool { self.code() == AgentErrorCode::RateLimit }
}

pub type AgentResult<T> = std::result::Result<T, AgentError>;

// ─── Tool Categories ───

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ToolCategory {
    Core,
    FileSystem,
    Network,
    Media,
    Autonomous,
    System,
    Orchestration,
    Coding,
    Custom(String),
}

impl ToolCategory {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Core => "core", Self::FileSystem => "filesystem",
            Self::Network => "network", Self::Media => "media",
            Self::Autonomous => "autonomous", Self::System => "system",
            Self::Orchestration => "orchestration", Self::Coding => "coding",
            Self::Custom(s) => s,
        }
    }

    pub fn from_str_tag(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "core" => Self::Core, "filesystem" => Self::FileSystem,
            "network" => Self::Network, "media" => Self::Media,
            "autonomous" => Self::Autonomous, "system" => Self::System,
            "orchestration" => Self::Orchestration, "coding" => Self::Coding,
            _ => Self::Custom(s.to_string()),
        }
    }

    pub fn all_categories() -> Vec<&'static str> {
        vec!["core", "filesystem", "network", "media", "autonomous", "system", "orchestration", "coding"]
    }
}

// ─── Execution Profile ───

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SideEffectLevel { None, Local, Irreversible }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum FilesystemScope { None, Confined, Unrestricted }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Reversibility { Trivial, Possible, Impractical }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ExecutionProfile {
    pub side_effect_level: SideEffectLevel,
    pub network_access: bool,
    pub filesystem_scope: FilesystemScope,
    pub reversibility: Reversibility,
    pub requires_human_approval: bool,
}

impl Default for ExecutionProfile {
    fn default() -> Self {
        Self {
            side_effect_level: SideEffectLevel::None, network_access: false,
            filesystem_scope: FilesystemScope::None, reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }
}

// ─── Tool Metadata ───

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub profile: ExecutionProfile,
}

// ─── Tool Trait (aligns with pharmakon_common::Tool) ───

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> AgentResult<String>;

    fn category(&self) -> ToolCategory { ToolCategory::Custom("generic".to_string()) }
    fn execution_profile(&self) -> ExecutionProfile { ExecutionProfile::default() }
    fn metadata(&self) -> HashMap<String, String> { HashMap::new() }
    fn requires_approval(&self, _args: &Value) -> bool { false }
    fn approval_description(&self, _args: &Value) -> String { String::new() }

    fn to_meta(&self) -> ToolMeta {
        ToolMeta { name: self.name().to_string(), description: self.description().to_string(),
                   category: self.category(), profile: self.execution_profile() }
    }
}

// ─── Plugin Trait (aligns with current Agent integration) ───

#[async_trait]
pub trait Plugin: Send + Sync {
    /// Called once when the plugin is loaded by Pharmakon.
    fn initialize(&self) -> AgentResult<()> { Ok(()) }

    /// Return all tools provided by this plugin.
    fn tools(&self) -> Vec<Arc<dyn Tool>>;

    /// Optional: clean shutdown.
    fn shutdown(&self) -> AgentResult<()> { Ok(()) }

    /// Optional: health check for monitoring.
    fn health_check(&self) -> AgentResult<PluginHealth> {
        Ok(PluginHealth { is_healthy: true, details: HashMap::new() })
    }

    /// Plugin identifier (e.g. "pharmakon-plugin-filesystem").
    fn plugin_id(&self) -> &str;

    /// Semantic version.
    fn plugin_version(&self) -> &str { "0.1.0" }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginHealth {
    pub is_healthy: bool,
    pub details: HashMap<String, String>,
}

// ─── Event Bridge ───

/// Events that a plugin can emit back to the agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginEvent {
    Log { level: String, message: String },
    StatusChange { tool: String, status: String },
    Error { message: String },
}

/// A channel for plugins to emit events back to the agent.
/// Plugins receive this through `initialize_with_context`.
#[derive(Clone)]
pub struct PluginEventTx {
    inner: Arc<dyn Fn(PluginEvent) -> AgentResult<()> + Send + Sync>,
}

impl PluginEventTx {
    pub fn new(f: impl Fn(PluginEvent) -> AgentResult<()> + Send + Sync + 'static) -> Self {
        Self { inner: Arc::new(f) }
    }

    pub fn send(&self, event: PluginEvent) -> AgentResult<()> {
        (self.inner)(event)
    }
}

// ─── Plugin Loading ───

/// Trait for the plugin host (agent) to register plugins.
#[async_trait]
pub trait PluginHost: Send + Sync {
    fn register_plugin(&self, plugin: Arc<dyn Plugin>) -> AgentResult<()>;
    fn unregister_plugin(&self, plugin_id: &str) -> AgentResult<()>;
    fn registered_plugins(&self) -> Vec<String>;
}

