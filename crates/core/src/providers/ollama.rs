use crate::model::{
    AgentError, AgentModel, AgentResult, CompletionRequest, CompletionResponse, MessageContent,
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct OllamaModel {
    host: String,
    model_name: String,
    client: Client,
}

impl OllamaModel {
    pub fn new(host: Option<String>, model_name: String) -> Self {
        Self {
            host: host.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model_name,
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }
}

fn map_ollama_finish_reason(fr: Option<&str>) -> Option<pharmakon_common::FinishReason> {
    fr.map(|s| match s.to_lowercase().as_str() {
        "stop" => pharmakon_common::FinishReason::Stop,
        "length" => pharmakon_common::FinishReason::MaxTokens,
        _ => pharmakon_common::FinishReason::Unknown,
    })
}

#[async_trait]
impl AgentModel for OllamaModel {
    fn name(&self) -> &str {
        &self.model_name
    }

    fn context_window(&self) -> usize {
        8192
    }

    fn max_output_tokens(&self) -> usize {
        4096
    }

    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let mut ollama_messages: Vec<serde_json::Value> = Vec::new();
        if let Some(ref sys) = request.system_instruction {
            ollama_messages.push(json!({
                "role": "system",
                "content": sys
            }));
        }
        ollama_messages.extend(request.messages.iter().map(|m| {
            json!({
                "role": m.role,
                "content": m.content.as_ref().map(|c| c.to_string()).unwrap_or_default()
            })
        }));

        let response = self
            .client
            .post(format!("{}/api/chat", self.host))
            .json(&json!({
                "model": self.model_name,
                "messages": ollama_messages,
                "stream": false
            }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let err = response
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Ollama error: {}", err)));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let content = body["message"]["content"].as_str().ok_or_else(|| {
            AgentError("Failed to parse content from Ollama response".to_string())
        })?;

        let done_reason = body["done_reason"].as_str();

        Ok(CompletionResponse {
            content: Some(MessageContent::Text(content.to_string())),
            tool_calls: None,
            usage: None,
            finish_reason: map_ollama_finish_reason(done_reason),
        })
    }

    async fn stream_complete(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        let mut ollama_messages: Vec<serde_json::Value> = Vec::new();
        if let Some(ref sys) = request.system_instruction {
            ollama_messages.push(json!({
                "role": "system",
                "content": sys
            }));
        }
        ollama_messages.extend(request.messages.iter().map(|m| {
            json!({
                "role": m.role,
                "content": m.content.as_ref().map(|c| c.to_string()).unwrap_or_default()
            })
        }));

        let response = self
            .client
            .post(format!("{}/api/chat", self.host))
            .json(&json!({
                "model": self.model_name,
                "messages": ollama_messages,
                "stream": true
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
                "Ollama streaming error: {}",
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
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                            if let Some(content) = json["message"]["content"].as_str()
                                && !content.is_empty()
                            {
                                return Some((Ok(content.to_string()), (byte_stream, buffer)));
                            }
                            if json["done"].as_bool().unwrap_or(false) {
                                return None;
                            }
                        }
                        continue;
                    }

                    match byte_stream.try_next().await {
                        Ok(Some(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                        Ok(None) => return None,
                        Err(e) => {
                            return Some((
                                Err(AgentError(format!("Ollama stream error: {}", e))),
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
