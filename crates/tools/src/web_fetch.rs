use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use reqwest::Client;
use serde_json::{Value, json};

pub struct WebFetchTool {
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }
    fn description(&self) -> &str {
        "Fetch the raw content of a URL (HTML, JSON, etc.)"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string" }
            },
            "required": ["url"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| AgentError("Missing url".to_string()))?;
        let res = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let text = res.text().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(text)
    }
}
