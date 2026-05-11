use axum::extract::State;
use axum::{Json, http::StatusCode, response::IntoResponse};
use pharmakon_core::agent::Agent;
use pharmakon_common::{AgentErrorCode, FinishReason};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Instant;
use serde_json::json;

// ── Shared response types ──

#[derive(Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    pub session_id: String,
}

impl<T: Serialize> ApiResponse<T> {
    fn ok(data: T, session_id: &str) -> Self {
        Self { success: true, data: Some(data), error: None, error_code: None, session_id: session_id.to_string() }
    }
    fn err(msg: &str, code: &str, session_id: &str) -> Self {
        Self { success: false, data: None, error: Some(msg.to_string()), error_code: Some(code.to_string()), session_id: session_id.to_string() }
    }
}

fn error_code_from_agent(e: &anyhow::Error) -> String {
    let ae = e.downcast_ref::<pharmakon_common::AgentError>();
    let code = ae.map(|a| a.code());
    match code {
        Some(AgentErrorCode::RateLimit) => "rate_limit".to_string(),
        Some(AgentErrorCode::InvalidRequest) => "invalid_request".to_string(),
        Some(AgentErrorCode::ModelError) => "model_error".to_string(),
        Some(AgentErrorCode::HangDetected) => "loop_detected".to_string(),
        Some(AgentErrorCode::ToolNotFound) => "tool_not_found".to_string(),
        Some(AgentErrorCode::ToolExecutionFailed) => "tool_execution_failed".to_string(),
        Some(AgentErrorCode::AuthenticationFailed) => "auth_failed".to_string(),
        _ => "internal_error".to_string(),
    }
}

// ── Chat endpoint ──

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

#[derive(Serialize)]
pub struct ChatData {
    pub response: String,
    pub finish_reason: Option<String>,
}

pub async fn agent_chat(
    State((agent, _, _, _)): State<(
        Arc<Agent>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    let session_id = agent.session_id.lock().await.clone();
    log::info!("API: Agent chat request (session={}): {}", session_id, req.message);

    match agent.chat(&req.message).await {
        Ok(response) => {
            let data = ChatData { response, finish_reason: None };
            (StatusCode::OK, Json(json!(ApiResponse::ok(data, &session_id)))).into_response()
        }
        Err(e) => {
            let code = error_code_from_agent(&e);
            let msg = e.to_string();
            log::error!("API: Chat failed (session={}): {} [{}]", session_id, msg, code);
            (StatusCode::OK, Json(json!(ApiResponse::<()>::err(&msg, &code, &session_id)))).into_response()
        }
    }
}

// ── Tool execute endpoint ──

#[derive(Deserialize)]
pub struct ExecuteToolRequest {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Serialize)]
pub struct ExecuteToolData {
    pub result: String,
    pub execution_time_ms: u64,
}

pub async fn execute_tool(
    State((agent, _, _, _)): State<(
        Arc<Agent>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
    Json(req): Json<ExecuteToolRequest>,
) -> impl IntoResponse {
    let session_id = agent.session_id.lock().await.clone();
    log::info!("API: Execute tool '{}' (session={})", req.name, session_id);

    let start = Instant::now();
    let mut reg = agent.registry.lock().await;
    match reg.hydrate(&req.name) {
        Some(tool) => {
            match tool.call(req.args).await {
                Ok(result) => {
                    let ms = start.elapsed().as_millis() as u64;
                    let data = ExecuteToolData { result, execution_time_ms: ms };
                    (StatusCode::OK, Json(json!(ApiResponse::ok(data, &session_id)))).into_response()
                }
                Err(e) => {
                    let code = error_code_from_agent(&anyhow::anyhow!("{}", e.0));
                    (StatusCode::OK, Json(json!(ApiResponse::<()>::err(&e.0, &code, &session_id)))).into_response()
                }
            }
        }
        None => {
            let msg = format!("Tool '{}' not found", req.name);
            (StatusCode::NOT_FOUND, Json(json!(ApiResponse::<()>::err(&msg, "tool_not_found", &session_id)))).into_response()
        }
    }
}

// ── State endpoint ──

#[derive(Serialize)]
pub struct StateData {
    pub session_id: String,
    pub trajectory_steps: usize,
    pub history_messages: usize,
    pub model: String,
    pub total_tokens: u64,
    pub tools_count: usize,
}

pub async fn get_state(
    State((agent, _, _, _)): State<(
        Arc<Agent>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
) -> impl IntoResponse {
    let session_id = agent.session_id.lock().await.clone();
    let trajectory = agent.trajectory.lock().await;
    let state_arc = agent.get_current_session_state().await;
    let state = state_arc.lock().await;
    let tools_count = agent.registry.lock().await.all_metadata().len();
    let total_tokens = agent.total_tokens.load(std::sync::atomic::Ordering::Relaxed);

    let data = StateData {
        session_id: session_id.clone(),
        trajectory_steps: trajectory.steps.len(),
        history_messages: state.history.len(),
        model: trajectory.metadata.model.clone(),
        total_tokens,
        tools_count,
    };
    Json(json!(ApiResponse::ok(data, &session_id)))
}

// ── Tools list endpoint ──

pub async fn get_tools(
    State((agent, _, _, _)): State<(
        Arc<Agent>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
) -> impl IntoResponse {
    let session_id = agent.session_id.lock().await.clone();
    let reg = agent.registry.lock().await;
    let tools = reg.all_metadata();
    Json(json!(ApiResponse::ok(tools, &session_id)))
}
