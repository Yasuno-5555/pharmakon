use axum::{
    Json,
    extract::{Path, State},
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct WebhookPayload {
    pub message: String,
}

pub async fn webhook_handler(
    Path(webhook_id): Path<String>,
    State((agent, _canvas_host, _cron_manager, config)): State<crate::GatewayState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<WebhookPayload>,
) -> impl IntoResponse {
    // Verify secret if configured
    if let Some(expected_secret) = &config.gateway.webhook_secret {
        let provided_secret = headers
            .get("X-Pharmakon-Secret")
            .and_then(|h| h.to_str().ok());

        if provided_secret != Some(expected_secret) {
            log::warn!("Unauthorized webhook attempt for {}", webhook_id);
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    }

    log::info!(
        "Received authorized webhook {}: {}",
        webhook_id,
        payload.message
    );

    match agent.chat(&payload.message).await {
        Ok(response) => {
            log::info!("Webhook agent response: {}", response);
            axum::http::StatusCode::OK.into_response()
        }
        Err(e) => {
            log::error!("Webhook agent error: {}", e);
            axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
