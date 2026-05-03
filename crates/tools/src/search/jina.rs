use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{json, Value};
use reqwest::Client;

pub struct JinaReaderTool {
    client: Client,
}

impl JinaReaderTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for JinaReaderTool {
    fn name(&self) -> &str { "jina_reader" }
    fn description(&self) -> &str { "Read the content of a URL as clean markdown using Jina Reader" }
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
        let url = args["url"].as_str().ok_or_else(|| AgentError("Missing url".to_string()))?;
        let res = self.client.get(format!("https://r.jina.ai/{}", url))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let text = res.text().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(text)
    }
}
