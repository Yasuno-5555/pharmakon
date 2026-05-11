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
    let store = SecretStore::new();
    match store.get_secret("PHARMAKON_CONTROL_API_KEY") {
        Ok(valid_key) => {
            let auth_header = req
                .headers()
                .get("x-api-key")
                .and_then(|h: &HeaderValue| h.to_str().ok());

            if let Some(key) = auth_header {
                if key == valid_key {
                    return Ok(next.run(req).await);
                }
            }
            log::error!("Unauthorized API access attempt from {:?}", req.uri());
            Err(StatusCode::UNAUTHORIZED)
        }
        Err(_) => {
            // PHARMAKON_CONTROL_API_KEY is not set, bypass authentication for dev environment
            log::warn!("PHARMAKON_CONTROL_API_KEY is not set. Bypassing API key authentication.");
            Ok(next.run(req).await)
        }
    }
}
