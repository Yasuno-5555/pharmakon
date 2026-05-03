use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{json, Value};
use reqwest::Client;

pub struct DuckDuckGoSearchTool {
    client: Client,
}

impl DuckDuckGoSearchTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for DuckDuckGoSearchTool {
    fn name(&self) -> &str { "ddg_search" }
    fn description(&self) -> &str { "Search the web using DuckDuckGo for instant answers and facts" }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"].as_str().ok_or_else(|| AgentError("Missing query".to_string()))?;
        let res = self.client.get("https://api.duckduckgo.com/")
            .query(&[("q", query), ("format", "json")])
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let json = res.json::<Value>().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(json.to_string())
    }
}
