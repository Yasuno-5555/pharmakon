use crate::model::{
    AgentError, AgentErrorCode, AgentModel, AgentResult, CompletionRequest, CompletionResponse,
    ContentPart, FunctionCall, MessageContent, ToolCall, Usage,
};
use async_trait::async_trait;
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
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "function_declarations"
    )]
    function_declarations: Option<Vec<GeminiFunction>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "google_search")]
    google_search: Option<serde_json::Value>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "google_search_retrieval"
    )]
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
    #[serde(default)]
    parts: Vec<GeminiPart>,
}

#[derive(Serialize, Deserialize)]
struct GeminiPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "inline_data",
        alias = "inlineData"
    )]
    inline_data: Option<GeminiInlineData>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "function_call",
        alias = "functionCall"
    )]
    function_call: Option<GeminiFunctionCall>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "function_response",
        alias = "functionResponse"
    )]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    thought_signature: Option<String>,
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
    content: Option<GeminiContent>,
    #[allow(dead_code)]
    #[serde(rename = "finish_reason", alias = "finishReason")]
    finish_reason: Option<String>,
    #[allow(dead_code)]
    #[serde(rename = "safety_ratings", alias = "safetyRatings")]
    safety_ratings: Option<Vec<SafetyRating>>,
    #[allow(dead_code)]
    #[serde(rename = "grounding_metadata", alias = "groundingMetadata")]
    grounding_metadata: Option<serde_json::Value>,
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
    #[serde(rename = "prompt_token_count", alias = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(rename = "candidates_token_count", alias = "candidatesTokenCount")]
    candidates_token_count: u32,
    #[serde(rename = "total_token_count", alias = "totalTokenCount")]
    total_token_count: u32,
    #[serde(rename = "thoughts_token_count", alias = "thoughtsTokenCount", default)]
    thoughts_token_count: u32,
}

fn map_gemini_finish_reason(fr: Option<&str>) -> Option<pharmakon_common::FinishReason> {
    fr.map(|s| match s.to_uppercase().as_str() {
        "STOP" => pharmakon_common::FinishReason::Stop,
        "MAX_TOKENS" => pharmakon_common::FinishReason::MaxTokens,
        "SAFETY" => pharmakon_common::FinishReason::SafetyFilter,
        "OTHER" => pharmakon_common::FinishReason::Unknown,
        _ => pharmakon_common::FinishReason::Unknown,
    })
}

impl GeminiModel {
    pub fn new(api_key: String, model_id: String) -> Self {
        Self {
            client: Client::builder()
                .connect_timeout(std::time::Duration::from_secs(30))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to build HTTP client"),
            api_key,
            model_id,
        }
    }

    fn map_to_gemini_content(
        &self,
        messages: Vec<crate::model::Message>,
    ) -> (Vec<GeminiContent>, Option<GeminiContent>) {
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

            if let Some(ref content) = m.content
                && m.role != "tool"
            {
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
                            if let ContentPart::Text { text } = part {
                                parts.push(GeminiPart {
                                    text: Some(text.clone()),
                                    inline_data: None,
                                    function_call: None,
                                    function_response: None,
                                    thought: None,
                                })
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
                            args: serde_json::from_str(&tc.function.arguments).unwrap_or_else(
                                |_| serde_json::json!({ "raw_args": tc.function.arguments }),
                            ),
                            thought_signature: tc.function.thought_signature.clone(),
                        }),
                        function_response: None,
                        thought: None,
                    });
                }
            }

            if m.role == "tool"
                && let Some(ref content) = m.content
            {
                let function_name = m.name.clone().filter(|n| !n.is_empty()).or_else(|| {
                        log::debug!(
                            "Tool message is missing the 'name' field, attempting fallback lookup for tool_call_id: {:?}.",
                            m.tool_call_id
                        );
                        // Strategy 1: Look up tool_call_id in previous messages' tool_calls
                        if let Some(id) = &m.tool_call_id {
                            // Strategy 2: Parse tool name from the call_id format: call_<tool_name>_<uuid>
                            if let Some(rest) = id.strip_prefix("call_")
                                && let Some(last_underscore) = rest.rfind('_') {
                                    let extracted = rest[..last_underscore].to_string();
                                    if !extracted.is_empty() {
                                        log::info!("Extracted function name '{}' from tool_call_id", extracted);
                                        return Some(extracted);
                                    }
                                }
                            // Strategy 3: Search previous messages' tool_calls
                            all_messages
                                .iter()
                                .filter_map(|prev| prev.tool_calls.as_ref())
                                .flatten()
                                .find(|tc| tc.id == *id)
                                .map(|tc| tc.function.name.clone())
                        } else {
                            None
                        }
                    }).unwrap_or_default();

                // Pre-flight validation: Ensure the function name is not empty before sending to API
                if function_name.is_empty() {
                    log::error!(
                        "A tool response part has an empty function name for tool_call_id {:?}. Falling back to text part to avoid API rejection.",
                        m.tool_call_id
                    );
                    // Emit the tool result as a text part instead so the model still receives the information
                    // and the request isn't rejected by the Gemini API for an invalid function_response.
                    parts.push(GeminiPart {
                        text: Some(format!("[Tool result (unknown tool)]: {}", content)),
                        inline_data: None,
                        function_call: None,
                        function_response: None,
                        thought: None,
                    });
                } else {
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
            if let Some(last) = cleaned_contents.last_mut()
                && last.role == content.role
            {
                // Merge parts if roles are the same
                last.parts.extend(content.parts);
                continue;
            }
            cleaned_contents.push(content);
        }

        // Ensure strict user/model alternation and starts with user
        let mut final_contents = Vec::new();
        let mut next_expected_role = "user";

        for content in cleaned_contents {
            if content.role == next_expected_role {
                final_contents.push(content);
                next_expected_role = if next_expected_role == "user" {
                    "model"
                } else {
                    "user"
                };
            } else if next_expected_role == "user" && content.role == "model" {
                // If we got a model message but expected user (e.g. at start),
                // prepend an empty user message instead of "Continuing..."
                final_contents.push(GeminiContent {
                    role: "user".to_string(),
                    parts: vec![GeminiPart {
                        text: Some("...".to_string()),
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

        (final_contents, system_instruction)
    }
}

#[async_trait]
impl AgentModel for GeminiModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let (contents, mut system_instruction) = self.map_to_gemini_content(request.messages);

        if let Some(ref sys_inst) = request.system_instruction {
            if let Some(ref mut sys) = system_instruction {
                if let Some(ref mut part) = sys.parts.first_mut() {
                    if let Some(ref mut t) = part.text {
                        t.push_str("\n\n");
                        t.push_str(sys_inst);
                    } else {
                        part.text = Some(sys_inst.clone());
                    }
                }
            } else {
                system_instruction = Some(GeminiContent {
                    role: "system".to_string(),
                    parts: vec![GeminiPart {
                        text: Some(sys_inst.clone()),
                        inline_data: None,
                        function_call: None,
                        function_response: None,
                        thought: None,
                    }],
                });
            }
        }

        let gemini_req = GeminiRequest {
            contents,
            tools: request.tools.as_ref().map(|tools| {
                let mut gemini_tools = Vec::new();

                let functions: Vec<_> = tools
                    .iter()
                    .filter(|t| t.function.name != "google_search" && t.function.name != "gemini_search")
                    .map(|t| {
                        let mut params = t.function.parameters.clone();
                        // Recursively fix missing 'items' in array properties
                        fn fix_array_items(val: &mut serde_json::Value) {
                            if let Some(obj) = val.as_object_mut() {
                                if let Some(type_val) = obj.get("type")
                                    && type_val == "array" && !obj.contains_key("items") {
                                        obj.insert("items".to_string(), serde_json::json!({ "type": "string" }));
                                    }
                                for (_, v) in obj.iter_mut() {
                                    fix_array_items(v);
                                }
                            } else if let Some(arr) = val.as_array_mut() {
                                for v in arr.iter_mut() {
                                    fix_array_items(v);
                                }
                            }
                        }
                        fix_array_items(&mut params);

                        GeminiFunction {
                            name: t.function.name.clone(),
                            description: t.function.description.clone(),
                            parameters: Some(params),
                        }
                    })
                    .collect();

                let has_google_search = tools.iter().any(|t| {
                    t.function.name == "google_search" || t.function.name == "gemini_search"
                });

                if !functions.is_empty() {
                    if has_google_search {
                        log::warn!("Gemini API Limitation: Both function calling and google_search were requested. Prioritizing function calling; google_search will be ignored for this turn.");
                    }
                    gemini_tools.push(GeminiTool {
                        function_declarations: Some(functions),
                        google_search: None,
                        google_search_retrieval: None,
                    });
                } else if has_google_search {
                     gemini_tools.push(GeminiTool {
                        function_declarations: None,
                        google_search: Some(serde_json::json!({})),
                        google_search_retrieval: None,
                    });
                }

                gemini_tools
            }),
            tool_config: if request.tools.is_some() {
                Some(GeminiToolConfig {
                    function_calling_config: GeminiFunctionCallingConfig {
                        mode: "AUTO".to_string(),
                    },
                })
            } else {
                None
            },
            generation_config: Some({
                let complexity = request.complexity.unwrap_or(0.5);
                let thinking_enabled = (self.model_id.contains("2.5")
                    || self.model_id.contains("pro")
                    || self.model_id.contains("reasoner"))
                    && complexity >= 0.3; // Disable thinking for simple tasks to maximize speed/cost savings
                let thinking_budget = if thinking_enabled {
                    if complexity >= 0.7 {
                        if self.model_id.contains("flash") {
                            2048u32
                        } else {
                            4096u32
                        }
                    } else {
                        1024u32 // Light thinking for medium tasks
                    }
                } else {
                    0u32
                };
                // When thinking is enabled, max_output_tokens must leave room
                // for actual output beyond the thinking budget, otherwise Gemini
                // returns MAX_TOKENS with empty candidates.
                let min_output_headroom = if complexity >= 0.7 { 4096u32 } else { 2048u32 };
                let effective_max = request.max_tokens
                    .unwrap_or(8192)
                    .max(thinking_budget + min_output_headroom)
                    .min(8192); // Gemini hard cap

                GeminiConfig {
                    temperature: request.temperature,
                    max_output_tokens: Some(effective_max),
                    thinking_config: if thinking_enabled && thinking_budget > 0 {
                        Some(GeminiThinkingConfig { thinking_budget })
                    } else {
                        None
                    },
                }
            }),
            system_instruction,
            safety_settings: Some(vec![
                SafetySetting {
                    category: "HARM_CATEGORY_HARASSMENT".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_HATE_SPEECH".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_CIVIC_INTEGRITY".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
            ]),
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
            self.model_id
        );

        log::info!("Gemini API URL: {}", url);
        log::info!(
            "Gemini Request Body: {}",
            serde_json::to_string(&gemini_req).unwrap_or_default()
        );

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Gemini API Error Status: {}, Body: {}", status, error_text);

            let code = match status.as_u16() {
                429 => AgentErrorCode::RateLimit,
                401 | 403 => AgentErrorCode::AuthenticationFailed,
                400 => AgentErrorCode::InvalidRequest,
                500..=599 => AgentErrorCode::ModelError,
                _ => AgentErrorCode::NetworkError,
            };

            return Err(AgentError::new(
                code,
                format!("Gemini API error ({}): {}", status, error_text),
            ));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        log::info!("Gemini Response Body: {}", response_text);

        let gemini_resp: GeminiResponse = serde_json::from_str(&response_text).map_err(|e| {
            log::error!(
                "Failed to decode Gemini response: {}. Body: {}",
                e,
                response_text
            );
            AgentError::new(
                AgentErrorCode::InternalError,
                format!("Failed to parse response: {}", e),
            )
        })?;

        log::info!(
            "Successfully decoded Gemini response. Candidates: {}",
            gemini_resp.candidates.len()
        );
        if let Some(usage) = &gemini_resp.usage_metadata {
            log::info!(
                "Usage: prompt={}, candidates={}, total={}",
                usage.prompt_token_count,
                usage.candidates_token_count,
                usage.total_token_count
            );
        }

        let candidate = gemini_resp.candidates.first().ok_or_else(|| {
            AgentError::new(
                AgentErrorCode::ModelError,
                "No candidates returned from Gemini",
            )
        })?;
        log::info!("Candidate 0: finish_reason={:?}", candidate.finish_reason);

        let mut content = None;
        let mut tool_calls = Vec::new();

        let has_parts = if let Some(cand_content) = &candidate.content {
            !cand_content.parts.is_empty()
        } else {
            false
        };

        if !has_parts {
            log::warn!(
                "Gemini returned a candidate with no content or no parts. Finish reason: {:?}",
                candidate.finish_reason
            );
            if let Some(reason) = &candidate.finish_reason {
                if reason == "MAX_TOKENS" {
                    content = Some(MessageContent::Text(
                        "[Model stopped: Max tokens reached]".to_string(),
                    ));
                } else if reason == "STOP" {
                    content = Some(MessageContent::Text(String::new()));
                } else {
                    content = Some(MessageContent::Text(format!("[Model stopped: {}]", reason)));
                }
            } else {
                content = Some(MessageContent::Text(String::new()));
            }
        } else if let Some(cand_content) = &candidate.content {
            for (i, part) in cand_content.parts.iter().enumerate() {
                log::info!(
                    "Processing part {}: text={}, function_call={}, function_response={}, inline_data={}",
                    i,
                    part.text.is_some(),
                    part.function_call.is_some(),
                    part.function_response.is_some(),
                    part.inline_data.is_some()
                );

                if let Some(ref t) = part.text
                    && !t.is_empty()
                {
                    if content.is_none() {
                        content = Some(MessageContent::Text(t.clone()));
                    } else if let Some(MessageContent::Text(ref mut existing)) = content {
                        existing.push_str(t);
                    }
                }
                if let Some(ref th) = part.thought {
                    log::info!("Received native thought from Gemini (length: {})", th.len());
                    if content.is_none() {
                        content = Some(MessageContent::Text(format!("<think>{}</think>", th)));
                    } else if let Some(MessageContent::Text(ref mut existing)) = content {
                        existing.push_str(&format!("\n<think>{}</think>", th));
                    }
                }

                if let Some(ref fc) = part.function_call {
                    tool_calls.push(ToolCall {
                        id: format!(
                            "call_{}_{}",
                            fc.name,
                            uuid::Uuid::new_v4()
                                .to_string()
                                .chars()
                                .take(8)
                                .collect::<String>()
                        ),
                        r#type: "function".to_string(),
                        function: FunctionCall {
                            name: fc.name.clone(),
                            arguments: fc.args.to_string(),
                            thought_signature: fc.thought_signature.clone(),
                        },
                    });
                }
            }
        }

        Ok(CompletionResponse {
            content,
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            usage: gemini_resp.usage_metadata.map(|u| Usage {
                prompt_tokens: u.prompt_token_count,
                completion_tokens: u.candidates_token_count,
                total_tokens: u.total_token_count,
                thoughts_tokens: if u.thoughts_token_count > 0 {
                    Some(u.thoughts_token_count)
                } else {
                    None
                },
            }),
            finish_reason: map_gemini_finish_reason(candidate.finish_reason.as_deref()),
        })
    }

    async fn stream_complete(
        &self,
        request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        let (contents, mut system_instruction) = self.map_to_gemini_content(request.messages);

        if let Some(ref sys_inst) = request.system_instruction {
            if let Some(ref mut sys) = system_instruction {
                if let Some(ref mut part) = sys.parts.first_mut() {
                    if let Some(ref mut t) = part.text {
                        t.push_str("\n\n");
                        t.push_str(sys_inst);
                    } else {
                        part.text = Some(sys_inst.clone());
                    }
                }
            } else {
                system_instruction = Some(GeminiContent {
                    role: "system".to_string(),
                    parts: vec![GeminiPart {
                        text: Some(sys_inst.clone()),
                        inline_data: None,
                        function_call: None,
                        function_response: None,
                        thought: None,
                    }],
                });
            }
        }

        let gemini_req = GeminiRequest {
            contents,
            tools: request.tools.as_ref().map(|tools| {
                let mut gemini_tools = Vec::new();

                let functions: Vec<_> = tools
                    .iter()
                    .filter(|t| t.function.name != "google_search")
                    .map(|t| {
                        let mut params = t.function.parameters.clone();
                        // Recursively fix missing 'items' in array properties
                        fn fix_array_items(val: &mut serde_json::Value) {
                            if let Some(obj) = val.as_object_mut() {
                                if let Some(type_val) = obj.get("type")
                                    && type_val == "array"
                                    && !obj.contains_key("items")
                                {
                                    obj.insert(
                                        "items".to_string(),
                                        serde_json::json!({ "type": "string" }),
                                    );
                                }
                                for (_, v) in obj.iter_mut() {
                                    fix_array_items(v);
                                }
                            } else if let Some(arr) = val.as_array_mut() {
                                for v in arr.iter_mut() {
                                    fix_array_items(v);
                                }
                            }
                        }
                        fix_array_items(&mut params);

                        GeminiFunction {
                            name: t.function.name.clone(),
                            description: t.function.description.clone(),
                            parameters: Some(params),
                        }
                    })
                    .collect();

                let has_other_functions = !functions.is_empty();

                if has_other_functions {
                    gemini_tools.push(GeminiTool {
                        function_declarations: Some(functions),
                        google_search: None,
                        google_search_retrieval: None,
                    });
                }

                if tools.iter().any(|t| t.function.name == "google_search") && !has_other_functions
                {
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
                    function_calling_config: GeminiFunctionCallingConfig {
                        mode: "AUTO".to_string(),
                    },
                })
            } else {
                None
            },
            generation_config: Some({
                let complexity = request.complexity.unwrap_or(0.5);
                let thinking_enabled = (self.model_id.contains("2.5")
                    || self.model_id.contains("pro")
                    || self.model_id.contains("reasoner"))
                    && complexity >= 0.3; // Disable thinking for simple tasks to maximize speed/cost savings
                let thinking_budget = if thinking_enabled {
                    if complexity >= 0.7 {
                        if self.model_id.contains("flash") {
                            2048u32
                        } else {
                            4096u32
                        }
                    } else {
                        1024u32 // Light thinking for medium tasks
                    }
                } else {
                    0u32
                };
                // When thinking is enabled, max_output_tokens must leave room
                // for actual output beyond the thinking budget, otherwise Gemini
                // returns MAX_TOKENS with empty candidates.
                let min_output_headroom = if complexity >= 0.7 { 4096u32 } else { 2048u32 };
                let effective_max = request
                    .max_tokens
                    .unwrap_or(8192)
                    .max(thinking_budget + min_output_headroom)
                    .min(8192); // Gemini hard cap

                GeminiConfig {
                    temperature: request.temperature,
                    max_output_tokens: Some(effective_max),
                    thinking_config: if thinking_enabled && thinking_budget > 0 {
                        Some(GeminiThinkingConfig { thinking_budget })
                    } else {
                        None
                    },
                }
            }),
            system_instruction,
            safety_settings: Some(vec![
                SafetySetting {
                    category: "HARM_CATEGORY_HARASSMENT".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_HATE_SPEECH".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_SEXUALLY_EXPLICIT".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_DANGEROUS_CONTENT".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
                SafetySetting {
                    category: "HARM_CATEGORY_CIVIC_INTEGRITY".to_string(),
                    threshold: "BLOCK_NONE".to_string(),
                },
            ]),
        };

        let url = format!(
            "https://generativelanguage.googleapis.com/v1/models/{}:streamGenerateContent?alt=sse",
            self.model_id
        );
        log::debug!("Gemini Streaming API URL: {}", url);
        log::debug!(
            "Gemini Request Body: {}",
            serde_json::to_string(&gemini_req).unwrap_or_default()
        );

        let response = self
            .client
            .post(&url)
            .header("x-goog-api-key", &self.api_key)
            .json(&gemini_req)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(AgentError::new(
                AgentErrorCode::ModelError,
                format!("Gemini streaming API error: {}", error_text),
            ));
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
                        if let Some(data) = line.strip_prefix("data: ")
                            && let Ok(json) = serde_json::from_str::<serde_json::Value>(data)
                            && let Some(candidates) = json["candidates"].as_array()
                            && let Some(first_candidate) = candidates.first()
                        {
                            let parts = first_candidate["content"]["parts"].as_array();
                            let mut chunk_text = String::new();

                            if let Some(parts_vec) = parts {
                                for part in parts_vec {
                                    if let Some(text) = part["text"].as_str() {
                                        chunk_text.push_str(text);
                                    }
                                    if let Some(thought) = part["thought"].as_str() {
                                        chunk_text.push_str(&format!("<think>{}</think>", thought));
                                    }
                                }
                            }

                            if !chunk_text.is_empty() {
                                return Some((Ok(chunk_text), (byte_stream, buffer)));
                            }
                        }
                        continue;
                    }

                    match byte_stream.try_next().await {
                        Ok(Some(chunk)) => buffer.push_str(&String::from_utf8_lossy(&chunk)),
                        Ok(None) => return None,
                        Err(e) => {
                            return Some((
                                Err(AgentError::new(
                                    AgentErrorCode::NetworkError,
                                    format!("Stream error: {}", e),
                                )),
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

    fn context_window(&self) -> usize {
        1000000
    }

    fn max_output_tokens(&self) -> usize {
        8192
    }
}
