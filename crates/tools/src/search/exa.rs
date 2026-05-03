use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{json, Value};
use reqwest::Client;

pub struct ExaSearchTool {
    api_key: String,
    client: Client,
}

impl ExaSearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for ExaSearchTool {
    fn name(&self) -> &str { "exa_search" }
    fn description(&self) -> &str { "Search the web using Exa (formerly Metaphor) for high-quality, neural search results" }
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
        let res = self.client.post("https://api.exa.ai/search")
            .header("x-api-key", &self.api_key)
            .json(&json!({ "query": query, "useAutoprompt": true }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let json = res.json::<Value>().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(json.to_string())
    }
}
