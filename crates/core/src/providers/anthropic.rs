use async_trait::async_trait;
use crate::model::{CompletionRequest, CompletionResponse, Message, AgentModel, MessageContent, AgentResult, AgentError};
use reqwest::Client;
use serde_json::{json, Value};

pub struct AnthropicModel {
    api_key: String,
    model_name: String,
    client: Client,
}

impl AnthropicModel {
    pub fn new(api_key: String, model_name: String) -> Self {
        Self {
            api_key,
            model_name,
            client: Client::new(),
        }
    }

    fn map_messages(&self, messages: Vec<Message>) -> Vec<Value> {
        messages.into_iter().filter(|m| m.role != "system").map(|m| {
            json!({
                "role": if m.role == "assistant" { "assistant" } else { "user" },
                "content": m.content.as_ref().map(|c| c.to_string()).unwrap_or_default()
            })
        }).collect()
    }
}

#[async_trait]
impl AgentModel for AnthropicModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let system_prompt = request.messages.iter()
            .find(|m| m.role == "system")
            .and_then(|m| m.content.as_ref().map(|c| c.to_string()));

        let messages = self.map_messages(request.messages);

        let mut body = json!({
            "model": self.model_name,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "messages": messages,
        });

        if let Some(system) = system_prompt {
            body["system"] = json!(system);
        }

        let res = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !res.status().is_success() {
            let err = res.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Anthropic API error: {}", err)));
        }

        let json: Value = res.json().await.map_err(|e| AgentError(e.to_string()))?;
        
        let content = json["content"][0]["text"].as_str()
            .ok_or_else(|| AgentError(format!("Failed to parse Anthropic response: {:?}", json)))?;

        Ok(CompletionResponse {
            content: Some(MessageContent::Text(content.to_string())),
            tool_calls: None,
            usage: None,
        })
    }

    async fn stream_complete(&self, request: CompletionRequest) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>> {
        let system_prompt = request.messages.iter()
            .find(|m| m.role == "system")
            .and_then(|m| m.content.as_ref().map(|c| c.to_string()));

        let messages = self.map_messages(request.messages);

        let mut body = json!({
            "model": self.model_name,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "messages": messages,
            "stream": true,
        });

        if let Some(system) = system_prompt {
            body["system"] = json!(system);
        }

        let response = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Anthropic streaming API error: {}", error_text)));
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
                        if line.starts_with("event:") { continue; }
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(json) = serde_json::from_str::<Value>(data) {
                                if json["type"] == "content_block_delta" {
                                    if let Some(text) = json["delta"]["text"].as_str() {
                                        return Some((Ok(text.to_string()), (byte_stream, buffer)));
                                    }
                                } else if json["type"] == "message_stop" {
                                    return None;
                                }
                            }
                        }
                        continue;
                    }

                    match byte_stream.try_next().await {
                        Ok(Some(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                        Ok(None) => return None,
                        Err(e) => return Some((Err(AgentError(format!("Anthropic stream error: {}", e))), (byte_stream, buffer))),
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        &self.model_name
    }
}
