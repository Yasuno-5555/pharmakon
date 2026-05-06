use axum::extract::State;
use axum::{Json, http::StatusCode, response::IntoResponse};
use pharmakon_core::agent::Agent;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Deserialize)]
pub struct ExecuteToolRequest {
    pub name: String,
    pub args: serde_json::Value,
}

#[derive(Serialize)]
pub struct ExecuteToolResponse {
    pub result: String,
}

pub async fn execute_tool(
    State((agent, _, _, _)): State<(
        Arc<Mutex<Agent>>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
    Json(req): Json<ExecuteToolRequest>,
) -> impl IntoResponse {
    log::info!(
        "API: Executing tool '{}' with args: {:?}",
        req.name,
        req.args
    );

    let agent_lock = agent.lock().await;
    let tools = agent_lock.tools.lock().await;
    if let Some(tool) = tools.iter().find(|t| t.name() == req.name) {
        match tool.call(req.args).await {
            Ok(result) => (StatusCode::OK, Json(ExecuteToolResponse { result })).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
                .into_response(),
        }
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("Tool '{}' not found", req.name) })),
        )
            .into_response()
    }
}

#[derive(Deserialize)]
pub struct ChatRequest {
    pub message: String,
}

pub async fn agent_chat(
    State((agent, _, _, _)): State<(
        Arc<Mutex<Agent>>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    log::info!("API: Agent chat request: {}", req.message);
    let agent_lock = agent.lock().await;
    match agent_lock.chat(&req.message).await {
        Ok(response) => (StatusCode::OK, Json(json!({ "response": response }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

pub async fn get_state(
    State((agent, _, _, _)): State<(
        Arc<Mutex<Agent>>,
        Arc<crate::canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
) -> impl IntoResponse {
    let agent_lock = agent.lock().await;
    let trajectory = agent_lock.trajectory.lock().await;
    let state_arc = agent_lock.get_current_session_state().await;
    let state = state_arc.lock().await;

    Json(json!({
        "session_id": agent_lock.session_id.lock().await.clone(),
        "trajectory_steps": trajectory.steps.len(),
        "history_messages": state.history.len(),
        "model": trajectory.metadata.model.clone(),
    }))
}

use serde_json::json;
