//! DeepSeek Model Provider — OpenAI-compatible API.
//!
//! DeepSeek's API (api.deepseek.com) is fully compatible with the OpenAI
//! chat completions format. Special features:
//! - `deepseek-v4-pro` model emits `reasoning_content` (thinking tokens)
//! - `deepseek-v4-flash` model for general-purpose tasks
//! - Supports function calling with the same tool schema

use crate::model::{
    AgentError, AgentModel, AgentResult, CompletionRequest, CompletionResponse,
};
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct DeepSeekModel {
    pub api_key: String,
    pub model_id: String,
    pub client: Client,
    base_url: String,
}

// --- Reuse OpenAI-compatible structs ---

#[derive(Serialize)]
struct DeepSeekRequest {
    model: String,
    messages: Vec<DeepSeekMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<DeepSeekTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct DeepSeekTool {
    r#type: String,
    function: DeepSeekFunction,
}

#[derive(Serialize, Deserialize)]
struct DeepSeekFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct DeepSeekMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<DeepSeekToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

fn map_to_deepseek_content(content: &crate::model::MessageContent) -> serde_json::Value {
    match content {
        crate::model::MessageContent::Text(t) => serde_json::json!(t),
        crate::model::MessageContent::Multimodal(parts) => {
            let mut openai_parts = Vec::new();
            for part in parts {
                match part {
                    crate::model::ContentPart::Text { text } => {
                        openai_parts.push(serde_json::json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                    crate::model::ContentPart::Image { image_url } => {
                        openai_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {
                                "url": image_url.url
                            }
                        }));
                    }
                    crate::model::ContentPart::Audio { .. } => {}
                }
            }
            serde_json::json!(openai_parts)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct DeepSeekToolCall {
    id: String,
    r#type: String,
    function: DeepSeekFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct DeepSeekFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct DeepSeekResponse {
    choices: Vec<DeepSeekChoice>,
    usage: Option<DeepSeekUsage>,
}

#[derive(Deserialize)]
struct DeepSeekChoice {
    message: DeepSeekMessage,
}

#[derive(Deserialize)]
struct DeepSeekUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

impl DeepSeekModel {
    /// Create a new DeepSeek model provider.
    /// Uses DEEPSEEK_API_KEY from environment, with optional override.
    pub fn new(api_key: String, model_id: String) -> Self {
        Self {
            api_key,
            model_id,
            client: Client::new(),
            base_url: "https://api.deepseek.com".to_string(),
        }
    }

    /// Use a custom API base URL (for proxies or self-hosted).
    pub fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    fn map_messages(&self, messages: Vec<crate::model::Message>) -> Vec<DeepSeekMessage> {
        messages
            .into_iter()
            .map(|m| DeepSeekMessage {
                role: m.role,
                content: m.content.as_ref().map(map_to_deepseek_content),
                tool_calls: m.tool_calls.map(|calls| {
                    calls
                        .into_iter()
                        .map(|c| DeepSeekToolCall {
                            id: c.id,
                            r#type: c.r#type,
                            function: DeepSeekFunctionCall {
                                name: c.function.name,
                                arguments: c.function.arguments,
                            },
                        })
                        .collect()
                }),
                tool_call_id: m.tool_call_id,
            })
            .collect()
    }

    fn map_tools(
        &self,
        tools: Vec<crate::model::ToolDefinition>,
    ) -> Vec<DeepSeekTool> {
        tools
            .into_iter()
            .map(|t| DeepSeekTool {
                r#type: t.r#type,
                function: DeepSeekFunction {
                    name: t.function.name,
                    description: t.function.description,
                    parameters: t.function.parameters,
                },
            })
            .collect()
    }
}

#[async_trait]
impl AgentModel for DeepSeekModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let body = DeepSeekRequest {
            model: self.model_id.clone(),
            messages: self.map_messages(request.messages),
            temperature: request.temperature,
            tools: request.tools.map(|t| self.map_tools(t)),
            stream: None,
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("DeepSeek API error: {}", error_text)));
        }

        let ds_resp: DeepSeekResponse = response
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let choice = ds_resp
            .choices
            .first()
            .ok_or_else(|| AgentError("No choices returned from DeepSeek".to_string()))?;

        Ok(CompletionResponse {
            content: choice.message.content.as_ref().and_then(|v| {
                v.as_str()
                    .map(|t| crate::model::MessageContent::Text(t.to_string()))
            }),
            tool_calls: choice.message.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|c| crate::model::ToolCall {
                        id: c.id.clone(),
                        r#type: c.r#type.clone(),
                        function: crate::model::FunctionCall {
                            name: c.function.name.clone(),
                            arguments: c.function.arguments.clone(),
                            thought_signature: None,
                        },
                    })
                    .collect()
            }),
            usage: ds_resp.usage.map(|u| crate::model::Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                thoughts_tokens: None,
            }),
        })
    }

    async fn stream_complete(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        let mut body = DeepSeekRequest {
            model: self.model_id.clone(),
            messages: self.map_messages(request.messages),
            temperature: request.temperature,
            tools: request.tools.map(|t| self.map_tools(t)),
            stream: Some(true),
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!(
                "DeepSeek streaming API error: {}",
                error_text
            )));
        }

        let byte_stream = response.bytes_stream();
        let stream = futures::stream::unfold(
            (byte_stream, String::new()),
            |(mut byte_stream, mut buffer)| async move {
                use futures::TryStreamExt;
                loop {
                    if let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].trim().to_string();
                        buffer = buffer[newline_pos + 1..].to_string();

                        if line.is_empty() {
                            continue;
                        }
                        if line == "data: [DONE]" {
                            return None;
                        }
                        if let Some(data) = line.strip_prefix("data: ")
                            && let Ok(json) = serde_json::from_str::<serde_json::Value>(data)
                            && let Some(content) =
                                json["choices"][0]["delta"]["content"].as_str()
                                && !content.is_empty() {
                                    return Some((
                                        Ok(content.to_string()),
                                        (byte_stream, buffer),
                                    ));
                                }
                        continue;
                    }

                    match byte_stream.try_next().await {
                        Ok(Some(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                        Ok(None) => return None,
                        Err(e) => {
                            return Some((
                                Err(AgentError(format!("Stream error: {}", e))),
                                (byte_stream, buffer),
                            ));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        &self.model_id
    }
}
