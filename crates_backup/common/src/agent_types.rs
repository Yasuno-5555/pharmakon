use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

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
        if self.0.starts_with("[RateLimit]") {
            AgentErrorCode::RateLimit
        } else if self.0.starts_with("[InvalidRequest]") {
            AgentErrorCode::InvalidRequest
        } else if self.0.starts_with("[AuthenticationFailed]") {
            AgentErrorCode::AuthenticationFailed
        } else if self.0.starts_with("[ContextExceeded]") {
            AgentErrorCode::ContextExceeded
        } else if self.0.starts_with("[ModelError]") {
            AgentErrorCode::ModelError
        } else if self.0.starts_with("[ToolNotFound]") {
            AgentErrorCode::ToolNotFound
        } else if self.0.starts_with("[ToolExecutionFailed]") {
            AgentErrorCode::ToolExecutionFailed
        } else if self.0.starts_with("[HangDetected]") {
            AgentErrorCode::HangDetected
        } else if self.0.starts_with("[NetworkError]") {
            AgentErrorCode::NetworkError
        } else if self.0.starts_with("[EnvironmentError]") {
            AgentErrorCode::EnvironmentError
        } else if self.0.contains("429") || self.0.to_lowercase().contains("rate limit") {
            AgentErrorCode::RateLimit
        } else {
            AgentErrorCode::InternalError
        }
    }

    pub fn is_rate_limit(&self) -> bool {
        self.code() == AgentErrorCode::RateLimit
    }
}

pub type AgentResult<T> = std::result::Result<T, AgentError>;
pub type Result<T> = AgentResult<T>;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Message {
    pub role: String,
    pub content: Option<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>, // For tool role, the name of the tool
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Multimodal(Vec<ContentPart>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image_url")]
    Image { image_url: ImageUrl },
    #[serde(rename = "input_audio")]
    Audio { input_audio: InputAudio },
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ImageUrl {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InputAudio {
    pub data: String, // Base64
    pub format: String,
}

impl MessageContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(t) => Some(t),
            MessageContent::Multimodal(parts) => {
                for part in parts {
                    if let ContentPart::Text { text } = part {
                        return Some(text);
                    }
                }
                None
            }
        }
    }
}

impl std::fmt::Display for MessageContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MessageContent::Text(t) => write!(f, "{}", t),
            MessageContent::Multimodal(parts) => {
                for part in parts {
                    if let ContentPart::Text { text } = part {
                        write!(f, "{}", text)?;
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolCall {
    pub id: String,
    pub r#type: String,
    pub function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_signature: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolDefinition>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ToolDefinition {
    pub r#type: String,
    pub function: FunctionDefinition,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FunctionDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CompletionResponse {
    pub content: Option<MessageContent>,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub usage: Option<Usage>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thoughts_tokens: Option<u32>,
}

#[async_trait]
pub trait AgentModel: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse>;
    async fn stream_complete(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    >;
    fn name(&self) -> &str;
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ToolCategory {
    FileSystem,
    Network,
    Media,
    Autonomous,
    System,
    Custom(String),
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> AgentResult<String>;
    fn category(&self) -> ToolCategory {
        ToolCategory::Custom("generic".to_string())
    }
    fn metadata(&self) -> std::collections::HashMap<String, String> {
        std::collections::HashMap::new()
    }
    fn requires_approval(&self, _args: &serde_json::Value) -> bool {
        false
    }
    fn approval_description(&self, _args: &serde_json::Value) -> String {
        String::new()
    }
}

#[async_trait]
pub trait ToolRegistry: Send + Sync {
    async fn add_tool(&self, tool: Arc<dyn Tool>);
}

#[async_trait]
pub trait EmbeddingModel: Send + Sync {
    async fn generate_embedding(&self, text: &str) -> AgentResult<Vec<f32>>;
}

#[async_trait]
pub trait CommitmentPersistence: Send + Sync {
    async fn save_commitment(
        &self,
        id: &str,
        description: &str,
        deadline: Option<DateTime<Utc>>,
        status: &str,
        metadata: &Value,
    ) -> anyhow::Result<()>;
    async fn load_commitments(&self) -> anyhow::Result<Vec<Value>>;
    async fn update_commitment_status(&self, id: &str, status: &str) -> anyhow::Result<()>;
}
