use std::sync::Arc;
use crate::persistence::DbSessionStore;
use anyhow::Result;
use reqwest::{Client, Request, Response};
use serde_json::Value;

pub struct ForensicClient {
    client: Client,
    store: Arc<DbSessionStore>,
    session_id: String,
}

impl ForensicClient {
    pub fn new(store: Arc<DbSessionStore>, session_id: String) -> Self {
        Self {
            client: Client::new(),
            store,
            session_id,
        }
    }

    pub async fn execute_logged(&self, req: Request) -> Result<Response> {
        let url = req.url().to_string();
        let method = req.method().to_string();
        let req_body = format!("{:?}", req.body()); // Simplified for now

        let res = self.client.execute(req).await?;

        let status = res.status().as_u16();
        let res_text = "(Binary or Large body captured)"; // Full capture logic would go here

        let _ = self.store.log_traffic(
            &self.session_id,
            &url,
            &method,
            status,
            &req_body,
            res_text
        ).await;

        Ok(res)
    }
}
