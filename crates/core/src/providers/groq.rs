use crate::model::{
    AgentError, AgentModel, AgentResult, CompletionRequest, CompletionResponse, FunctionCall,
    ToolCall,
};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::{Value, json};
use std::pin::Pin;

pub struct GroqModel {
    api_key: String,
    model_name: String,
    client: Client,
}

impl GroqModel {
    pub fn new(api_key: String, model_name: String) -> Self {
        Self {
            api_key,
            model_name,
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }
}

#[async_trait]
impl AgentModel for GroqModel {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let res = self
            .client
            .post("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": self.model_name,
                "messages": request.messages,
                "tools": request.tools,
                "temperature": request.temperature.unwrap_or(0.7),
            }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !res.status().is_success() {
            let err = res.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Groq API error: {}", err)));
        }

        let json: Value = res.json().await.map_err(|e| AgentError(e.to_string()))?;

        let choice = &json["choices"][0];
        let content = choice["message"]["content"]
            .as_str()
            .map(|s| pharmakon_common::MessageContent::Text(s.to_string()));

        let tool_calls = choice["message"]["tool_calls"].as_array().map(|tc| tc.iter()
                    .map(|t| ToolCall {
                        id: t["id"].as_str().unwrap_or_default().to_string(),
                        r#type: t["type"].as_str().unwrap_or_default().to_string(),
                        function: FunctionCall {
                            name: t["function"]["name"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            arguments: t["function"]["arguments"]
                                .as_str()
                                .unwrap_or_default()
                                .to_string(),
                            thought_signature: None,
                        },
                    })
                    .collect());

        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        Ok(CompletionResponse {
            content,
            tool_calls,
            usage: None,
            finish_reason,
        })
    }

    async fn stream_complete(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<String>> + Send>>> {
        let response = self
            .client
            .post("https://api.groq.com/openai/v1/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": self.model_name,
                "messages": request.messages,
                "tools": request.tools,
                "temperature": request.temperature.unwrap_or(0.7),
                "stream": true,
            }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!(
                "Groq streaming API error: {}",
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
                            && let Ok(json) = serde_json::from_str::<Value>(data)
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
                                Err(AgentError(format!("Groq stream error: {}", e))),
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