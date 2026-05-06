use axum::{
    http::{HeaderValue, Request, StatusCode, header},
    middleware::Next,
    response::Response,
};
use pharmakon_common::secrets::SecretStore;

pub async fn auth_middleware(
    req: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get("x-api-key")
        .and_then(|h: &HeaderValue| h.to_str().ok());

    if let Some(key) = auth_header {
        let store = SecretStore::new();
        if let Ok(valid_key) = store.get_secret("PHARMAKON_CONTROL_API_KEY") {
            if key == valid_key {
                return Ok(next.run(req).await);
            }
        } else {
            // If no key is set, maybe deny by default for security
            log::warn!("API Access attempted but PHARMAKON_CONTROL_API_KEY is not set.");
        }
    }

    log::error!("Unauthorized API access attempt from {:?}", req.uri());
    Err(StatusCode::UNAUTHORIZED)
}
