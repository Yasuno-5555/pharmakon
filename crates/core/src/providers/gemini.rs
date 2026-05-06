use async_trait::async_trait;
use crate::model::{CompletionRequest, CompletionResponse, AgentModel, Usage, ToolCall, FunctionCall, MessageContent, ContentPart, AgentResult, AgentError};
use reqwest::Client;
use serde::{Deserialize, Serialize};

pub struct GeminiModel {
    pub client: Client,
    pub api_key: String,
    pub model_id: String,
}

#[derive(Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTool>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "tool_config")]
    tool_config: Option<GeminiToolConfig>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "generation_config")]
    generation_config: Option<GeminiConfig>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "system_instruction")]
    system_instruction: Option<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "safety_settings")]
    safety_settings: Option<Vec<SafetySetting>>,
}

#[derive(Serialize, Deserialize)]
struct SafetySetting {
    category: String,
    threshold: String,
}

#[derive(Serialize)]
struct GeminiToolConfig {
    #[serde(rename = "function_calling_config")]
    function_calling_config: GeminiFunctionCallingConfig,
}

#[derive(Serialize)]
struct GeminiFunctionCallingConfig {
    mode: String, // "AUTO", "ANY", "NONE"
}

#[derive(Serialize, Deserialize)]
struct GeminiConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "max_output_tokens")]
    max_output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinking_config")]
    thinking_config: Option<GeminiThinkingConfig>,
}

#[derive(Serialize, Deserialize)]
struct GeminiThinkingConfig {
    #[serde(rename = "thinking_budget")]
    thinking_budget: u32,
}

#[derive(Serialize, Deserialize)]
struct GeminiTool {
    #[serde(skip_serializing_if = "Option::is_none", rename = "function_declarations")]
    function_declarations: Option<Vec<GeminiFunction>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "google_search")]
    google_search: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "google_search_retrieval")]
    google_search_retrieval: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct GeminiFunction {
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
struct GeminiContent {
    #[serde(skip_serializing_if = "String::is_empty")]
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "inline_data")]
    inline_data: Option<GeminiInlineData>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "function_call")]
    function_call: Option<GeminiFunctionCall>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "function_response")]
    function_response: Option<GeminiFunctionResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thought: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct GeminiInlineData {
    mime_type: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
}

#[derive(Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<Candidate>,
    #[serde(rename = "usage_metadata")]
    usage_metadata: Option<GeminiUsage>,
}

#[derive(Deserialize)]
struct Candidate {
    content: GeminiContent,
    #[allow(dead_code)]
    #[serde(rename = "finish_reason")]
    finish_reason: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "safety_ratings")]
    safety_ratings: Option<Vec<SafetyRating>>,
}

#[derive(Deserialize)]
#[allow(dead_code)]
struct SafetyRating {
    category: String,
    probability: String,
    blocked: Option<bool>,
}

#[derive(Deserialize)]
struct GeminiUsage {
    #[serde(rename = "prompt_token_count")]
    prompt_token_count: u32,
    #[serde(rename = "candidates_token_count")]
    candidates_token_count: u32,
    #[serde(rename = "total_token_count")]
    total_token_count: u32,
    #[serde(rename = "thoughts_token_count", default)]
    thoughts_token_count: u32,
}

impl GeminiModel {
    pub fn new(api_key: String, model_id: String) -> Self {
        Self {
            client: Client::new(),
            api_key,
            model_id,
        }
    }

    fn map_to_gemini_content(&self, messages: Vec<crate::model::Message>) -> (Vec<GeminiContent>, Option<GeminiContent>) {
        let mut raw_contents = Vec::new();
        let mut system_instruction = None;

        let all_messages = messages.clone();

        for m in messages {
            let role = match m.role.as_str() {
                "user" => "user",
                "assistant" => "model",
                "system" => {
                    if let Some(content) = m.content {
                        system_instruction = Some(GeminiContent {
                            role: "system".to_string(),
                            parts: vec![GeminiPart {
                                text: Some(content.to_string()),
                                inline_data: None,
                                function_call: None,
                                function_response: None,
                                thought: None,
                            }],
                        });
                    }
                    continue;
                }
                "tool" => "user",
                _ => "user",
            };

            let mut parts = Vec::new();
            
            if let Some(ref content) = m.content {
                match content {
                    MessageContent::Text(t) => parts.push(GeminiPart {
                        text: Some(t.clone()),
                        inline_data: None,
                        function_call: None,
                        function_response: None,
                        thought: None,
                    }),
                    MessageContent::Multimodal(p) => {
                        for part in p {
                            match part {
                                ContentPart::Text { text } => parts.push(GeminiPart {
                                    text: Some(text.clone()),
                                    inline_data: None,
                                    function_call: None,
                                    function_response: None,
                                    thought: None,
                                }),
                                _ => {}
                            }
                        }
                    }
                }
            }

            if let Some(ref tool_calls) = m.tool_calls {
                for tc in tool_calls {
                    parts.push(GeminiPart {
                        text: None,
                        inline_data: None,
                        function_call: Some(GeminiFunctionCall {
                            name: tc.function.name.clone(),
                            args: serde_json::from_str(&tc.function.arguments).unwrap_or_else(|_| serde_json::json!({ "raw_args": tc.function.arguments })),
                        }),
                        function_response: None,
                        thought: None,
                    });
                }
            }

            if m.role == "tool" {
                if let Some(ref content) = m.content {
                    let function_name = if let Some(id) = &m.tool_call_id {
                        all_messages.iter()
                            .filter_map(|prev| prev.tool_calls.as_ref())
                            .flatten()
                            .find(|tc| tc.id == *id)
                            .map(|tc| tc.function.name.clone())
                            .unwrap_or_default()
                    } else {
                        "".to_string()
                    };

                    parts.push(GeminiPart {
                        text: None,
                        inline_data: None,
                        function_call: None,
                        function_response: Some(GeminiFunctionResponse {
                            name: function_name,
                            response: serde_json::json!({ "result": content.to_string() }),
                        }),
                        thought: None,
                    });
                }
            }

            if !parts.is_empty() {
                raw_contents.push(GeminiContent {
                    role: role.to_string(),
                    parts,
                });
            }
        }

        // History Cleaning Logic
        let mut cleaned_contents: Vec<GeminiContent> = Vec::new();
        for content in raw_contents {
            if let Some(last) = cleaned_contents.last_mut() {
                if last.role == content.role {
                    // Merge parts if roles are the same
                    last.parts.extend(content.parts);
                    continue;
                }
            }
            cleaned_contents.push(content);
        }

        // Ensure strict user/model alternation and starts with user
        let mut final_contents = Vec::new();
        let mut next_expected_role = "user";

        for content in cleaned_contents {
            if content.role == next_expected_role {
                final_contents.push(content);
                next_expected_role = if next_expected_role == "user" { "model" } else { "user" };
            } else {
                if next_expected_role == "user" && content.role == "model" {
                    // Insert a dummy user message to maintain alternation
                    final_contents.push(GeminiContent {
                        role: "user".to_string(),
                        parts: vec![GeminiPart {
                            text: Some("Continuing...".to_string()),
                            inline_data: None,
                            function_call: None,
                            function_response: None,
                            thought: None,
                        }],
                    });
                    final_contents.push(content);
                    next_expected_role = "user";
                } else if next_expected_role == "model" && content.role == "user" {
                    // Merge consecutive user messages if model was skipped
                    if let Some(last) = final_contents.last_mut() {
                        last.parts.extend(content.parts);
                    }
                }
            }
        }

        (final_contents, system_instruction)
    }
}

#[async_trait]
impl AgentModel for GeminiModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let (contents, system_instruction) = self.map_to_gemini_content(request.messages);
        
        let gemini_req = GeminiRequest {
            contents,
            tools: request.tools.as_ref().map(|tools| {
                let mut gemini_tools = Vec::new();
                
                let functions: Vec<_> = tools.iter()
                    .filter(|t| t.function.name != "google_search")
                    .map(|t| GeminiFunction {
                        name: t.function.name.clone(),
                        description: t.function.description.clone(),
                        parameters: Some(t.function.parameters.clone()),
                    }).collect();
                
                let has_other_functions = !functions.is_empty();

                if has_other_functions {
                    gemini_tools.push(GeminiTool {
                        function_declarations: Some(functions),
                        google_search: None,
                        google_search_retrieval: None,
                    });
                }

                if tools.iter().any(|t| t.function.name == "google_search") && !has_other_functions {
                    if self.model_id.contains("1.5") {
                        gemini_tools.push(GeminiTool {
                            function_declarations: None,
                            google_search: None,
                            google_search_retrieval: Some(serde_json::json!({
                                "dynamic_retrieval_config": {
                                    "mode": "MODE_DYNAMIC",
                                    "dynamic_threshold": 0.3
                                }
                            })),
                        });
                    } else {
                        gemini_tools.push(GeminiTool {
                            function_declarations: None,
                            google_search: Some(serde_json::json!({})),
                            google_search_retrieval: None,
                        });
                    }
                }

                gemini_tools
            }),
            tool_config: if request.tools.is_some() {
                Some(GeminiToolConfig {
                    function_calling_config: GeminiFunctionCallingConfig { mode: "AUTO".to_string() }
                })
            } else { None },
            generation_config: Some(GeminiConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                // Only enable Thinking Mode for high-end Pro models to balance performance and cost
                thinking_config: if self.model_id.contains("pro") && !self.model_id.contains("flash") {
                    Some(GeminiThinkingConfig { thinking_budget: 4096 })
                } else { None },
            }),
            system_instruction,
            safety_settings: Some(vec![
                SafetySetting { category: "HARM_CATEGORY_HARASSMENT".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_HATE_SPEECH".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_CIVIC_INTEGRITY".to_string(), threshold: "BLOCK_NONE".to_string() },
            ]),
        };

        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", self.model_id);

        log::debug!("Gemini API URL: {}", url);
        log::debug!("Gemini Request Body: {}", serde_json::to_string(&gemini_req).unwrap_or_default());

        let response = self.client.post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Gemini API Error Status: {}, Body: {}", status, error_text);
            return Err(AgentError(format!("Gemini API error ({}): {}", status, error_text)));
        }

        let response_text = response.text().await.map_err(|e| AgentError(e.to_string()))?;
        log::debug!("Gemini Response Body: {}", response_text);

        let gemini_resp: GeminiResponse = serde_json::from_str(&response_text).map_err(|e| {
            log::error!("Failed to decode Gemini response: {}. Body: {}", e, response_text);
            AgentError(e.to_string())
        })?;
        let candidate = gemini_resp.candidates.get(0).ok_or_else(|| AgentError("No candidates returned from Gemini".to_string()))?;

        let mut content = None;
        let mut tool_calls = Vec::new();

        for part in &candidate.content.parts {
            if let Some(ref t) = part.text {
                if content.is_none() {
                    content = Some(MessageContent::Text(t.clone()));
                } else if let Some(MessageContent::Text(ref mut existing)) = content {
                    existing.push_str(t);
                }
            }
            if let Some(ref fc) = part.function_call {
                tool_calls.push(ToolCall {
                    id: format!("call_{}_{}", fc.name, uuid::Uuid::new_v4().to_string().chars().take(8).collect::<String>()),
                    r#type: "function".to_string(),
                    function: FunctionCall {
                        name: fc.name.clone(),
                        arguments: fc.args.to_string(),
                    }
                });
            }
        }

        Ok(CompletionResponse {
            content,
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
            usage: gemini_resp.usage_metadata.map(|u| Usage {
                prompt_tokens: u.prompt_token_count,
                completion_tokens: u.candidates_token_count,
                total_tokens: u.total_token_count,
                thoughts_tokens: if u.thoughts_token_count > 0 { Some(u.thoughts_token_count) } else { None },
            }),
        })
    }

    async fn stream_complete(&self, request: CompletionRequest) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>> {
        let (contents, system_instruction) = self.map_to_gemini_content(request.messages);
        
        let gemini_req = GeminiRequest {
            contents,
            tools: request.tools.as_ref().map(|tools| {
                let mut gemini_tools = Vec::new();
                
                let functions: Vec<_> = tools.iter()
                    .filter(|t| t.function.name != "google_search")
                    .map(|t| GeminiFunction {
                        name: t.function.name.clone(),
                        description: t.function.description.clone(),
                        parameters: Some(t.function.parameters.clone()),
                    }).collect();
                
                let has_other_functions = !functions.is_empty();

                if has_other_functions {
                    gemini_tools.push(GeminiTool {
                        function_declarations: Some(functions),
                        google_search: None,
                        google_search_retrieval: None,
                    });
                }

                if tools.iter().any(|t| t.function.name == "google_search") && !has_other_functions {
                    if self.model_id.contains("1.5") {
                        gemini_tools.push(GeminiTool {
                            function_declarations: None,
                            google_search: None,
                            google_search_retrieval: Some(serde_json::json!({
                                "dynamic_retrieval_config": {
                                    "mode": "MODE_DYNAMIC",
                                    "dynamic_threshold": 0.3
                                }
                            })),
                        });
                    } else {
                        gemini_tools.push(GeminiTool {
                            function_declarations: None,
                            google_search: Some(serde_json::json!({})),
                            google_search_retrieval: None,
                        });
                    }
                }

                gemini_tools
            }),
            tool_config: if request.tools.is_some() {
                Some(GeminiToolConfig {
                    function_calling_config: GeminiFunctionCallingConfig { mode: "AUTO".to_string() }
                })
            } else { None },
            generation_config: Some(GeminiConfig {
                temperature: request.temperature,
                max_output_tokens: request.max_tokens,
                // Only enable Thinking Mode for high-end Pro models to balance performance and cost
                thinking_config: if self.model_id.contains("pro") && !self.model_id.contains("flash") {
                    Some(GeminiThinkingConfig { thinking_budget: 4096 })
                } else { None },
            }),
            system_instruction,
            safety_settings: Some(vec![
                SafetySetting { category: "HARM_CATEGORY_HARASSMENT".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_HATE_SPEECH".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(), threshold: "BLOCK_NONE".to_string() },
                SafetySetting { category: "HARM_CATEGORY_CIVIC_INTEGRITY".to_string(), threshold: "BLOCK_NONE".to_string() },
            ]),
        };

        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse", self.model_id);
        log::debug!("Gemini Streaming API URL: {}", url);
        log::debug!("Gemini Request Body: {}", serde_json::to_string(&gemini_req).unwrap_or_default());

        let response = self.client.post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
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
                                if let Some(candidates) = json["candidates"].as_array() {
                                    if let Some(first_candidate) = candidates.get(0) {
                                        if let Some(parts) = first_candidate["content"]["parts"].as_array() {
                                            for part in parts {
                                                if let Some(text) = part["text"].as_str() {
                                                    if !text.is_empty() {
                                                        return Some((Ok(text.to_string()), (byte_stream, buffer)));
                                                    }
                                                }
                                            }
                                        }
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
