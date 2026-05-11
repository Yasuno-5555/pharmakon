use axum::{
    http::{HeaderValue, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use pharmakon_common::secrets::SecretStore;

pub async fn auth_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // 1. Check environment variable first
    let env_key = std::env::var("PHARMAKON_CONTROL_API_KEY").ok();

    // 2. Fallback to SecretStore
    let store = SecretStore::new();
    let valid_key = env_key.or_else(|| store.get_secret("PHARMAKON_CONTROL_API_KEY").ok());

    match valid_key {
        Some(key) if !key.is_empty() => {
            let auth_header = req
                .headers()
                .get("x-api-key")
                .and_then(|h: &HeaderValue| h.to_str().ok());

            if let Some(req_key) = auth_header
                && req_key == key {
                    return Ok(next.run(req).await);
                }
            log::error!("Unauthorized API access attempt from {:?}", req.uri());
            Err(StatusCode::UNAUTHORIZED)
        }
        _ => {
            // PHARMAKON_CONTROL_API_KEY is not set or empty, bypass authentication for dev environment
            log::warn!("PHARMAKON_CONTROL_API_KEY is not set or empty. Bypassing API key authentication.");
            Ok(next.run(req).await)
        }
    }
}
