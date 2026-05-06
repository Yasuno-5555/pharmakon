use async_trait::async_trait;
use crate::model::{CompletionRequest, CompletionResponse, AgentModel, Usage, ToolCall, FunctionCall, MessageContent, ContentPart, AgentResult, AgentError};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct OpenAIModel {
    pub client: Client,
    pub api_key: String,
    pub model_id: String,
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<OpenAIMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAITool>>,
}

#[derive(Serialize, Deserialize)]
struct OpenAITool {
    r#type: String,
    function: OpenAIFunction,
}

#[derive(Serialize, Deserialize)]
struct OpenAIFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct OpenAIMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAIToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

fn map_to_openai_content(content: &MessageContent) -> serde_json::Value {
    match content {
        MessageContent::Text(t) => serde_json::json!(t),
        MessageContent::Multimodal(parts) => {
            let mut openai_parts = Vec::new();
            for part in parts {
                match part {
                    ContentPart::Text { text } => {
                        openai_parts.push(serde_json::json!({
                            "type": "text",
                            "text": text
                        }));
                    }
                    ContentPart::Image { image_url } => {
                        openai_parts.push(serde_json::json!({
                            "type": "image_url",
                            "image_url": {
                                "url": image_url.url
                            }
                        }));
                    }
                    ContentPart::Audio { .. } => {}
                }
            }
            serde_json::json!(openai_parts)
        }
    }
}

#[derive(Serialize, Deserialize)]
struct OpenAIToolCall {
    id: String,
    r#type: String,
    function: OpenAIFunctionCall,
}

#[derive(Serialize, Deserialize)]
struct OpenAIFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
    usage: Option<OpenAIUsage>,
}

#[derive(Deserialize)]
struct Choice {
    message: OpenAIMessage,
}

#[derive(Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

impl OpenAIModel {
    pub fn new(api_key: String, model_id: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model_id,
        }
    }
}

#[async_trait]
impl pharmakon_common::EmbeddingModel for OpenAIModel {
    async fn generate_embedding(&self, text: &str) -> AgentResult<Vec<f32>> {
        let body = serde_json::json!({
            "model": "text-embedding-3-small",
            "input": text
        });

        let response = self.client.post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let err = response.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("OpenAI Embeddings API error: {}", err)));
        }

        let json: serde_json::Value = response.json().await.map_err(|e| AgentError(e.to_string()))?;
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .ok_or_else(|| AgentError("Failed to parse embedding response".to_string()))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect();

        Ok(embedding)
    }
}

#[async_trait]
impl AgentModel for OpenAIModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let openai_req = OpenAIRequest {
            model: self.model_id.clone(),
            messages: request.messages.into_iter().map(|m| OpenAIMessage {
                role: m.role,
                content: m.content.as_ref().map(|c| map_to_openai_content(c)),
                tool_calls: m.tool_calls.map(|calls| calls.into_iter().map(|c| OpenAIToolCall {
                    id: c.id,
                    r#type: c.r#type,
                    function: OpenAIFunctionCall {
                        name: c.function.name,
                        arguments: c.function.arguments,
                    }
                }).collect()),
                tool_call_id: m.tool_call_id,
            }).collect(),
            temperature: request.temperature,
            tools: request.tools.map(|tools| tools.into_iter().map(|t| OpenAITool {
                r#type: t.r#type,
                function: OpenAIFunction {
                    name: t.function.name,
                    description: t.function.description,
                    parameters: t.function.parameters,
                }
            }).collect()),
        };

        let response = self.client.post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&openai_req)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("OpenAI API error: {}", error_text)));
        }

        let openai_resp: OpenAIResponse = response.json().await.map_err(|e| AgentError(e.to_string()))?;
        let choice = openai_resp.choices.get(0).ok_or_else(|| AgentError("No choices returned from OpenAI".to_string()))?;

        Ok(CompletionResponse {
            content: choice.message.content.as_ref().and_then(|v| {
                if let Some(t) = v.as_str() {
                    Some(MessageContent::Text(t.to_string()))
                } else {
                    None
                }
            }),
            tool_calls: choice.message.tool_calls.as_ref().map(|calls| calls.into_iter().map(|c| ToolCall {
                id: c.id.clone(),
                r#type: c.r#type.clone(),
                function: FunctionCall {
                    name: c.function.name.clone(),
                    arguments: c.function.arguments.clone(),
                }
            }).collect()),
            usage: openai_resp.usage.map(|u| Usage {
                prompt_tokens: u.prompt_tokens,
                completion_tokens: u.completion_tokens,
                total_tokens: u.total_tokens,
                thoughts_tokens: None,
            }),
        })
    }

    async fn stream_complete(&self, request: CompletionRequest) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>> {
        let openai_req = OpenAIRequest {
            model: self.model_id.clone(),
            messages: request.messages.into_iter().map(|m| OpenAIMessage {
                role: m.role,
                content: m.content.as_ref().map(|c| map_to_openai_content(c)),
                tool_calls: m.tool_calls.map(|calls| calls.into_iter().map(|c| OpenAIToolCall {
                    id: c.id,
                    r#type: c.r#type,
                    function: OpenAIFunctionCall {
                        name: c.function.name,
                        arguments: c.function.arguments,
                    }
                }).collect()),
                tool_call_id: m.tool_call_id,
            }).collect(),
            temperature: request.temperature,
            tools: request.tools.map(|tools| tools.into_iter().map(|t| OpenAITool {
                r#type: t.r#type,
                function: OpenAIFunction {
                    name: t.function.name,
                    description: t.function.description,
                    parameters: t.function.parameters,
                }
            }).collect()),
        };

        let mut body = serde_json::to_value(&openai_req).map_err(|e| AgentError(e.to_string()))?;
        body.as_object_mut().unwrap().insert("stream".to_string(), serde_json::Value::Bool(true));

        let response = self.client.post("https://api.openai.com/v1/chat/completions")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("OpenAI streaming API error: {}", error_text)));
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

                        if line.is_empty() { continue; }
                        if line == "data: [DONE]" { return None; }
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(content) = json["choices"][0]["delta"]["content"].as_str() {
                                    if !content.is_empty() {
                                        return Some((Ok(content.to_string()), (byte_stream, buffer)));
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    match byte_stream.try_next().await {
                        Ok(Some(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                        Ok(None) => return None,
                        Err(e) => return Some((Err(AgentError(format!("Stream error: {}", e))), (byte_stream, buffer))),
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
