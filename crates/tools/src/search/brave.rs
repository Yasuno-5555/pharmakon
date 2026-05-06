use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use reqwest::Client;
use serde_json::{Value, json};

pub struct BraveSearchTool {
    api_key: String,
    client: Client,
}

impl BraveSearchTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for BraveSearchTool {
    fn name(&self) -> &str {
        "brave_search"
    }
    fn description(&self) -> &str {
        "Search the web using Brave Search for privacy-focused results"
    }
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
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let res = self
            .client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let json = res
            .json::<Value>()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json.to_string())
    }
}
