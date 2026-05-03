use async_trait::async_trait;
use crate::model::{CompletionRequest, CompletionResponse, Message, AgentModel, MessageContent, ToolCall, FunctionCall, ContentPart, AgentResult, AgentError};
use reqwest::Client;
use serde_json::{json, Value};

pub struct GeminiModel {
    api_key: String,
    model_name: String,
    client: Client,
}

impl GeminiModel {
    pub fn new(api_key: String, model_name: String) -> Self {
        Self {
            api_key,
            model_name,
            client: Client::new(),
        }
    }

    fn map_to_gemini_contents(&self, messages: Vec<Message>) -> Vec<Value> {
        messages.into_iter()
            .filter(|m| m.role != "system")
            .map(|m| {
                let role = match m.role.as_str() {
                    "user" => "user",
                    "assistant" => "model",
                    "tool" => "function",
                    _ => "user",
                };

                let mut parts = Vec::new();
                
                if let Some(content) = &m.content {
                    match content {
                        MessageContent::Text(t) => {
                            parts.push(json!({ "text": t }));
                        }
                        MessageContent::Multimodal(content_parts) => {
                            for part in content_parts {
                                match part {
                                    ContentPart::Text { text } => {
                                        parts.push(json!({ "text": text }));
                                    }
                                    ContentPart::Image { image_url } => {
                                        if image_url.url.starts_with("data:") {
                                            if let Some((mime, data)) = parse_data_url(&image_url.url) {
                                                parts.push(json!({
                                                    "inline_data": {
                                                        "mime_type": mime,
                                                        "data": data
                                                    }
                                                }));
                                            }
                                        } else {
                                            parts.push(json!({ "text": format!("[Image: {}]", image_url.url) }));
                                        }
                                    }
                                    ContentPart::Audio { input_audio } => {
                                        parts.push(json!({
                                            "inline_data": {
                                                "mime_type": format!("audio/{}", input_audio.format),
                                                "data": input_audio.data
                                            }
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }

                if let Some(tool_calls) = &m.tool_calls {
                    for tc in tool_calls {
                        parts.push(json!({
                            "functionCall": {
                                "name": tc.function.name,
                                "args": serde_json::from_str::<Value>(&tc.function.arguments).unwrap_or_default()
                            }
                        }));
                    }
                }

                if m.role == "tool" {
                    parts.push(json!({
                        "functionResponse": {
                            "name": m.tool_call_id.clone().unwrap_or_default(),
                            "response": {
                                "name": m.tool_call_id.clone().unwrap_or_default(),
                                "content": { "result": m.content.as_ref().map(|c| c.to_string()).unwrap_or_default() }
                            }
                        }
                    }));
                }

                json!({
                    "role": role,
                    "parts": parts
                })
            }).collect()
    }
}

#[async_trait]
impl AgentModel for GeminiModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model_name, self.api_key
        );

        let system_instruction = request.messages.iter()
            .find(|m| m.role == "system")
            .and_then(|m| m.content.as_ref())
            .map(|c| json!({ "parts": [{ "text": c.to_string() }] }));

        let contents = self.map_to_gemini_contents(request.messages);
        
        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(0.7),
            }
        });

        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }

        if let Some(tools) = request.tools {
            let gemini_tools = tools.into_iter().map(|t| {
                json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "parameters": t.function.parameters,
                })
            }).collect::<Vec<_>>();
            body["tools"] = json!([{ "functionDeclarations": gemini_tools }]);
        }

        let res = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !res.status().is_success() {
            let err = res.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Gemini API error: {}", err)));
        }

        let json: Value = res.json().await.map_err(|e| AgentError(e.to_string()))?;
        
        let first_candidate = &json["candidates"][0];
        let content_parts = &first_candidate["content"]["parts"];
        
        let mut text_content = String::new();
        let mut tool_calls = Vec::new();

        if let Some(parts) = content_parts.as_array() {
            for part in parts {
                if let Some(text) = part["text"].as_str() {
                    text_content.push_str(text);
                }
                if let Some(fc) = part["functionCall"].as_object() {
                    let name = fc["name"].as_str().unwrap_or_default().to_string();
                    let args = fc["args"].to_string();
                    tool_calls.push(ToolCall {
                        id: name.clone(),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name,
                            arguments: args,
                        },
                    });
                }
            }
        }

        Ok(CompletionResponse {
            content: if text_content.is_empty() { None } else { Some(MessageContent::Text(text_content)) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            usage: None,
        })
    }

    async fn stream_complete(&self, request: CompletionRequest) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>> {
        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
            self.model_name, self.api_key
        );

        let system_instruction = request.messages.iter()
            .find(|m| m.role == "system")
            .and_then(|m| m.content.as_ref())
            .map(|c| json!({ "parts": [{ "text": c.to_string() }] }));

        let contents = self.map_to_gemini_contents(request.messages);
        let mut body = json!({
            "contents": contents,
            "generationConfig": {
                "temperature": request.temperature.unwrap_or(0.7),
            }
        });

        if let Some(si) = system_instruction {
            body["systemInstruction"] = si;
        }

        let response = self.client.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.map_err(|e| AgentError(e.to_string()))?;
            return Err(AgentError(format!("Gemini streaming API error: {}", error_text)));
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
                        if let Some(data) = line.strip_prefix("data: ") {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(text) = json["candidates"][0]["content"]["parts"][0]["text"].as_str() {
                                    if !text.is_empty() {
                                        return Some((Ok(text.to_string()), (byte_stream, buffer)));
                                    }
                                }
                            }
                        }
                        continue;
                    }

                    match byte_stream.try_next().await {
                        Ok(Some(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                        Ok(None) => return None,
                        Err(e) => return Some((Err(AgentError(format!("Gemini stream error: {}", e))), (byte_stream, buffer))),
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

fn parse_data_url(url: &str) -> Option<(String, String)> {
    if !url.starts_with("data:") {
        return None;
    }
    let parts: Vec<&str> = url.splitn(2, ',').collect();
    if parts.len() != 2 {
        return None;
    }
    let header = parts[0];
    let data = parts[1];
    
    let mime = header.strip_prefix("data:")?
        .split(';')
        .next()?
        .to_string();
        
    Some((mime, data.to_string()))
}
