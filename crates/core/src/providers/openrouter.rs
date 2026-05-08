use crate::model::{
    AgentError, AgentModel, AgentResult, CompletionRequest, CompletionResponse, ContentPart,
    FunctionCall, MessageContent, ToolCall, Usage,
};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;

/// OpenRouter provider – OpenAI-compatible API endpoint.
/// API reference: https://openrouter.ai/docs/api-reference
pub struct OpenRouterModel {
    pub client: Client,
    pub api_key: String,
    pub model_id: String,
    /// Optional: identifies your app in the `HTTP-Referer` / `X-Title` headers.
    pub site_url: Option<String>,
    pub site_name: Option<String>,
}

// ── request / response types (OpenAI-compatible) ──────────────────────────────

#[derive(Serialize)]
struct ORRequest {
    model: String,
    messages: Vec<ORMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ORTool>>,
}

#[derive(Serialize, Deserialize)]
struct ORTool {
    r#type: String,
    function: ORFunction,
}

#[derive(Serialize, Deserialize)]
struct ORFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct ORMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ORToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ORToolCall {
    id: String,
    r#type: String,
    function: ORFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct ORFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct ORResponse {
    choices: Vec<ORChoice>,
    usage: Option<ORUsage>,
}

#[derive(Deserialize)]
struct ORChoice {
    message: ORMessage,
}

#[derive(Deserialize)]
struct ORUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// ── helpers ────────────────────────────────────────────────────────────────────

fn map_content(content: &MessageContent) -> serde_json::Value {
    match content {
        MessageContent::Text(t) => serde_json::json!(t),
        MessageContent::Multimodal(parts) => {
            let items: Vec<serde_json::Value> = parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::Text { text } => Some(serde_json::json!({
                        "type": "text",
                        "text": text
                    })),
                    ContentPart::Image { image_url } => Some(serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": image_url.url }
                    })),
                    ContentPart::Audio { .. } => None,
                })
                .collect();
            serde_json::json!(items)
        }
    }
}

fn build_or_messages(request: &CompletionRequest) -> Vec<ORMessage> {
    request
        .messages
        .iter()
        .map(|m| ORMessage {
            role: m.role.clone(),
            content: m.content.as_ref().map(map_content),
            tool_calls: m.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|c| ORToolCall {
                        id: c.id.clone(),
                        r#type: c.r#type.clone(),
                        function: ORFunctionCall {
                            name: c.function.name.clone(),
                            arguments: c.function.arguments.clone(),
                        },
                    })
                    .collect()
            }),
            tool_call_id: m.tool_call_id.clone(),
        })
        .collect()
}

// ── implementation ─────────────────────────────────────────────────────────────

impl OpenRouterModel {
    pub fn new(api_key: String, model_id: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model_id,
            site_url: None,
            site_name: None,
        }
    }

    pub fn with_site(mut self, url: impl Into<String>, name: impl Into<String>) -> Self {
        self.site_url = Some(url.into());
        self.site_name = Some(name.into());
        self
    }

    fn request_builder(
        &self,
        body: &impl Serialize,
    ) -> reqwest::RequestBuilder {
        let mut builder = self
            .client
            .post("https://openrouter.ai/api/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(body);

        if let Some(url) = &self.site_url {
            builder = builder.header("HTTP-Referer", url);
        }
        if let Some(name) = &self.site_name {
            builder = builder.header("X-Title", name);
        }
        builder
    }
}

#[async_trait]
impl AgentModel for OpenRouterModel {
    fn name(&self) -> &str {
        &self.model_id
    }

    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let body = ORRequest {
            model: self.model_id.clone(),
            messages: build_or_messages(&request),
            temperature: request.temperature,
            tools: request.tools.as_ref().map(|tools| {
                tools
                    .iter()
                    .map(|t| ORTool {
                        r#type: t.r#type.clone(),
                        function: ORFunction {
                            name: t.function.name.clone(),
                            description: t.function.description.clone(),
                            parameters: t.function.parameters.clone(),
                        },
                    })
                    .collect()
            }),
        };

        let response = self
            .request_builder(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let err = response
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("OpenRouter API error: {}", err)));
        }

        let resp: ORResponse = response
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let choice = resp
            .choices
            .first()
            .ok_or_else(|| AgentError("No choices returned from OpenRouter".to_string()))?;

        Ok(CompletionResponse {
            content: choice.message.content.as_ref().and_then(|v| {
                v.as_str()
                    .map(|t| MessageContent::Text(t.to_string()))
            }),
            tool_calls: choice.message.tool_calls.as_ref().map(|calls| {
                calls
                    .iter()
                    .map(|c| ToolCall {
                        id: c.id.clone(),
                        r#type: c.r#type.clone(),
                        function: FunctionCall {
                            name: c.function.name.clone(),
                            arguments: c.function.arguments.clone(),
                            thought_signature: None,
                        },
                    })
                    .collect()
            }),
            usage: resp.usage.map(|u| Usage {
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
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<String>> + Send>>> {
        let body = ORRequest {
            model: self.model_id.clone(),
            messages: build_or_messages(&request),
            temperature: request.temperature,
            tools: request.tools.as_ref().map(|tools| {
                tools
                    .iter()
                    .map(|t| ORTool {
                        r#type: t.r#type.clone(),
                        function: ORFunction {
                            name: t.function.name.clone(),
                            description: t.function.description.clone(),
                            parameters: t.function.parameters.clone(),
                        },
                    })
                    .collect()
            }),
        };

        let mut json_body =
            serde_json::to_value(&body).map_err(|e| AgentError(e.to_string()))?;
        json_body
            .as_object_mut()
            .unwrap()
            .insert("stream".to_string(), Value::Bool(true));

        let response = self
            .request_builder(&json_body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let err = response
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!(
                "OpenRouter streaming API error: {}",
                err
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
                            && let Ok(json) = serde_json::from_str::<Value>(data)
                                && let Some(content) =
                                    json["choices"][0]["delta"]["content"].as_str()
                                    && !content.is_empty()
                                    {
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
                                Err(AgentError(format!("OpenRouter stream error: {}", e))),
                                (byte_stream, buffer),
                            ));
                        }
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}
