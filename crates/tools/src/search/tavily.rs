use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{json, Value};
use reqwest::Client;

pub struct TavilySearchTool {
    api_key: String,
    client: Client,
}

impl TavilySearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for TavilySearchTool {
    fn name(&self) -> &str { "tavily_search" }
    fn description(&self) -> &str { "Search the web using Tavily for LLM-optimized search results" }
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
        let res = self.client.post("https://api.tavily.com/search")
            .json(&json!({ "api_key": self.api_key, "query": query }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let json = res.json::<Value>().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(json.to_string())
    }
}
