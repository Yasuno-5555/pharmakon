use crate::model::{AgentError, AgentModel, AgentResult, CompletionRequest, CompletionResponse};
use async_trait::async_trait;
use futures::Stream;
use reqwest::Client;
use serde_json::{Value, json};
use std::pin::Pin;

pub struct PerplexityModel {
    api_key: String,
    model_name: String,
    client: Client,
}

impl PerplexityModel {
    pub fn new(api_key: String, model_name: String) -> Self {
        Self {
            api_key,
            model_name,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl AgentModel for PerplexityModel {
    fn name(&self) -> &str {
        &self.model_name
    }

    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let res = self
            .client
            .post("https://api.perplexity.ai/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": self.model_name,
                "messages": request.messages,
            }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !res.status().is_success() {
            let err = res.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Perplexity API error: {}", err)));
        }

        let json: Value = res.json().await.map_err(|e| AgentError(e.to_string()))?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| pharmakon_common::MessageContent::Text(s.to_string()));

        Ok(CompletionResponse {
            content,
            tool_calls: None,
            usage: None,
            finish_reason: None,
        })
    }

    async fn stream_complete(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<Pin<Box<dyn Stream<Item = AgentResult<String>> + Send>>> {
        let response = self
            .client
            .post("https://api.perplexity.ai/chat/completions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": self.model_name,
                "messages": request.messages,
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
                "Perplexity streaming API error: {}",
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
                                Err(AgentError(format!("Perplexity stream error: {}", e))),
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
