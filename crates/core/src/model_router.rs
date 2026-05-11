//! ModelRouter — Model selection, fallback, and economy integration.
//!
//! Extracted from Agent's God Object to consolidate model routing concerns:
//! - Model selection with economy-aware scoring
//! - Fallback on rate limits/MAX_TOKENS/empty responses
//! - Token accounting for the DSGE economy layer

use crate::model::{CompletionRequest, ToolDefinition, AgentModel, Message};
use crate::orchestration::dsge_integration::AgentEconomy;
use pharmakon_common::Event;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};
use anyhow::Result;
use tokio::sync::broadcast;

pub struct ModelRouter {
    pub economy: Arc<StdMutex<AgentEconomy>>,
    pub event_tx: broadcast::Sender<Event>,
    pub total_tokens: Arc<AtomicU64>,
    pub token_budget: u64,
    pub fallback_models: Arc<StdMutex<Vec<String>>>,
}

fn try_send_event(tx: &broadcast::Sender<Event>, event: Event) {
    if let Err(e) = tx.send(event) {
        log::warn!("ModelRouter event bus error: {}", e);
    }
}

impl ModelRouter {
    pub fn new(
        economy: Arc<StdMutex<AgentEconomy>>,
        event_tx: broadcast::Sender<Event>,
        total_tokens: Arc<AtomicU64>,
        token_budget: u64,
        fallback_models: Arc<StdMutex<Vec<String>>>,
    ) -> Self {
        Self { economy, event_tx, total_tokens, token_budget, fallback_models }
    }

    pub fn recommend_max_tokens(&self, model_name: &str) -> u64 {
        self.economy.lock().unwrap().recommend_max_tokens(model_name) as u64
    }

    pub fn select_model(
        &self,
        user_message: &str,
        default_model: Arc<dyn AgentModel>,
        complexity: f64,
    ) -> Arc<dyn AgentModel> {
        if default_model.name() == "mock-model" || default_model.name() == "test" {
            return default_model;
        }
        self.economy.lock().unwrap()
            .select_model(user_message, complexity)
            .unwrap_or(default_model)
    }

    pub fn record_model_result(&self, name: &str, latency_ms: u64, success: bool, is_rate_limit: bool) {
        self.economy.lock().unwrap().record_model_result(name, latency_ms, success, is_rate_limit);
    }

    pub fn observe_latency(&self, latency_ms: u64) {
        self.economy.lock().unwrap().observe_latency(latency_ms);
    }

    pub fn record_token_usage(&self, prompt: u64, completion: u64) {
        self.economy.lock().unwrap().record_token_usage(prompt, completion);
    }

    pub fn budget_used_pct(&self) -> f64 {
        let used = self.total_tokens.load(Ordering::SeqCst);
        used as f64 / self.token_budget as f64
    }

    /// Execute a model completion with automatic fallback on rate limits, MAX_TOKENS, and empty responses.
    /// Returns the (response, model_used) tuple.
    pub async fn execute_with_fallback(
        &self,
        messages: Vec<Message>,
        tools: Option<Vec<ToolDefinition>>,
        mut target_model: Arc<dyn AgentModel>,
        _session_id: &str,
        start_time: std::time::Instant,
        complexity: f64,
    ) -> Result<(pharmakon_common::agent_types::CompletionResponse, Arc<dyn AgentModel>)> {
        let mut request = CompletionRequest {
            messages,
            temperature: Some(0.2),
            max_tokens: Some(self.recommend_max_tokens(target_model.name()) as u32),
            tools,
            complexity: Some(complexity),
            system_instruction: None,
        };

        let mut response_result: Option<Result<pharmakon_common::agent_types::CompletionResponse>> = None;
        let mut current_fallback_index = 0;
        let mut consecutive_empty_responses = 0;
        let fallback_models = self.fallback_models.clone();

        while response_result.is_none() {
            let model_lock = target_model.clone();
            let request_clone = request.clone();
            // Run in a spawned task so provider panics are caught (JoinError on panic)
            response_result = Some(
                match tokio::task::spawn(async move { model_lock.complete(request_clone).await }).await {
                    Ok(Ok(resp)) => Ok(resp),
                    Ok(Err(e)) => Err(anyhow::anyhow!("{}", e)),
                    Err(join_err) => {
                        let msg = if join_err.is_panic() { "model provider panicked" } else { "model task cancelled" };
                        log::error!("{} for model '{}'", msg, target_model.name());
                        Err(anyhow::anyhow!("{}", msg))
                    }
                }
            );

            match &response_result {
                Some(Ok(res)) => {
                    let is_max_tokens = res.finish_reason.as_ref()
                        .map(|fr| matches!(fr, pharmakon_common::FinishReason::MaxTokens)).unwrap_or(false)
                        || res.content.as_ref()
                            .map(|c| c.to_string().contains("[Model stopped: Max tokens reached]")).unwrap_or(false);

                    if is_max_tokens {
                        let fallback_list = fallback_models.lock().unwrap();
                        if current_fallback_index < fallback_list.len() {
                            let fallback_id = &fallback_list[current_fallback_index];
                            log::warn!("Output token limit (MAX_TOKENS) for {}. Escalating to fallback: {}", target_model.name(), fallback_id);
                            try_send_event(&self.event_tx, Event::Error {
                                message: format!("Output token limit reached (MAX_TOKENS) for {}. Escalating to fallback: {}", target_model.name(), fallback_id),
                            });
                            if let Some(new_model) = crate::providers::registry::ModelRegistry::get_model(fallback_id) {
                                target_model = new_model;
                                current_fallback_index += 1;
                                consecutive_empty_responses = 0;
                                request.max_tokens = Some(self.recommend_max_tokens(target_model.name()) as u32);
                                response_result = None;
                                continue;
                            }
                        }
                    }

                    let is_empty = res.content.as_ref()
                        .map(|c| c.to_string().trim().is_empty()).unwrap_or(true)
                        && res.tool_calls.is_none();
                    if is_empty {
                        consecutive_empty_responses += 1;
                        log::warn!("Empty response from {} (consecutive: {})", target_model.name(), consecutive_empty_responses);

                        if consecutive_empty_responses >= 2 {
                            let fallback_list = fallback_models.lock().unwrap();
                            if current_fallback_index < fallback_list.len() {
                                let fallback_id = &fallback_list[current_fallback_index];
                                log::warn!("Two empty responses from {}. Switching to fallback: {}", target_model.name(), fallback_id);
                                try_send_event(&self.event_tx, Event::Error {
                                    message: format!("Two consecutive empty responses from {}. Switching to fallback: {}", target_model.name(), fallback_id),
                                });
                                if let Some(new_model) = crate::providers::registry::ModelRegistry::get_model(fallback_id) {
                                    target_model = new_model;
                                    current_fallback_index += 1;
                                    consecutive_empty_responses = 0;
                                    response_result = None;
                                    continue;
                                }
                            }
                        } else {
                            let used = self.total_tokens.load(Ordering::SeqCst);
                            if used > self.token_budget * 8 / 10 {
                                log::warn!("Skipping empty-response retry: budget at {:.0}%", (used as f64 / self.token_budget as f64) * 100.0);
                                break;
                            }
                            log::info!("Retrying same model once for empty response...");
                            response_result = None;
                            continue;
                        }
                    } else {
                        consecutive_empty_responses = 0;
                    }
                }
                Some(Err(e)) => {
                    let is_rate_limit = e.to_string().to_lowercase().contains("429")
                        || e.to_string().to_lowercase().contains("too many requests")
                        || e.to_string().to_lowercase().contains("quota");

                    let fallback_list = fallback_models.lock().unwrap();
                    if is_rate_limit && current_fallback_index < fallback_list.len() {
                        let fallback_id = &fallback_list[current_fallback_index];
                        log::warn!("Rate limit for {}. Falling back to: {}", target_model.name(), fallback_id);
                        try_send_event(&self.event_tx, Event::Error {
                            message: format!("API Rate limit reached for {}. Switching to fallback: {}", target_model.name(), fallback_id),
                        });
                        if let Some(new_model) = crate::providers::registry::ModelRegistry::get_model(fallback_id) {
                            target_model = new_model;
                            current_fallback_index += 1;
                            response_result = None;
                            continue;
                        } else {
                            log::error!("Fallback model {} not found.", fallback_id);
                            current_fallback_index += 1;
                            response_result = None;
                            continue;
                        }
                    }

                    self.record_model_result(target_model.name(), 0, false, is_rate_limit);
                    try_send_event(&self.event_tx, Event::Error { message: format!("Model error: {}", e) });
                    return Err(anyhow::anyhow!("All fallback models exhausted. Final error: {}", e));
                }
                None => {
                    return Err(anyhow::anyhow!("[InternalError] Model response loop terminated without setting a result."));
                }
            }
        }

        let response = response_result
            .ok_or_else(|| anyhow::anyhow!("[InternalError] Missing model response"))?
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Token accounting into DSGE economy layer
        if let Some(ref usage) = response.usage {
            self.record_token_usage(usage.prompt_tokens as u64, usage.completion_tokens as u64);
            self.total_tokens.fetch_add(usage.total_tokens as u64, Ordering::SeqCst);
            let quality_proxy = if response.content.is_some() { 0.8 } else { 0.3 };
            let mut economy = self.economy.lock().unwrap();
            economy.record_observation(crate::orchestration::dsge_integration::CallObservation {
                tokens_spent: usage.total_tokens as u64,
                latency_ms: start_time.elapsed().as_millis() as u64,
                success: response.content.is_some() || response.tool_calls.is_some(),
                model_id: target_model.name().to_string(),
                quality_proxy,
            });
            if economy.observations.len().is_multiple_of(8) {
                economy.update_production_from_observations();
            }
        }
        self.record_model_result(target_model.name(), start_time.elapsed().as_millis() as u64, true, false);
        self.observe_latency(start_time.elapsed().as_millis() as u64);

        Ok((response, target_model))
    }
}
