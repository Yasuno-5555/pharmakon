use crate::model::{
    AgentError, AgentModel, AgentResult, CompletionRequest, CompletionResponse, FunctionCall,
    Message, MessageContent, ToolCall,
};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::{Value, json};

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
            let role = if m.role == "assistant" { "assistant" } else { "user" };
            let mut content_parts = Vec::new();

            if let Some(content) = &m.content {
                content_parts.push(json!({
                    "type": "text",
                    "text": content.to_string()
                }));
            }

            if let Some(tool_calls) = &m.tool_calls {
                for tc in tool_calls {
                    content_parts.push(json!({
                        "type": "tool_use",
                        "id": tc.id,
                        "name": tc.function.name,
                        "input": serde_json::from_str::<Value>(&tc.function.arguments).unwrap_or_default()
                    }));
                }
            }

            if m.role == "tool" {
                content_parts.clear(); // Tool results are handled differently in Anthropic
                return json!({
                    "role": "user",
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                            "content": m.content.as_ref().map(|c| c.to_string()).unwrap_or_default()
                        }
                    ]
                });
            }

            json!({
                "role": role,
                "content": content_parts
            })
        }).collect()
    }
}

#[async_trait]
impl AgentModel for AnthropicModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let system_prompt = request
            .messages
            .iter()
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

        if let Some(tools) = &request.tools {
            body["tools"] = json!(
                tools
                    .iter()
                    .map(|t| {
                        json!({
                            "name": t.function.name,
                            "description": t.function.description,
                            "input_schema": t.function.parameters
                        })
                    })
                    .collect::<Vec<_>>()
            );
        }

        let res = self
            .client
            .post("https://api.anthropic.com/v1/messages")
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

        let mut content_text = String::new();
        let mut tool_calls = Vec::new();

        if let Some(content_array) = json["content"].as_array() {
            for part in content_array {
                match part["type"].as_str() {
                    Some("text") => {
                        if let Some(text) = part["text"].as_str() {
                            content_text.push_str(text);
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCall {
                            id: part["id"].as_str().unwrap_or_default().to_string(),
                            r#type: "function".to_string(),
                            function: FunctionCall {
                                name: part["name"].as_str().unwrap_or_default().to_string(),
                                arguments: part["input"].to_string(),
                                thought_signature: None,
                            },
                        });
                    }
                    _ => {}
                }
            }
        }

        Ok(CompletionResponse {
            content: if content_text.is_empty() {
                None
            } else {
                Some(MessageContent::Text(content_text))
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            usage: None,
        })
    }

    async fn stream_complete(
        &self,
        _request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        // ... (Streaming implementation - simplified for now as it's less critical for tools)
        unimplemented!("Streaming not yet updated for tools")
    }

    fn name(&self) -> &str {
        &self.model_name
    }
}
