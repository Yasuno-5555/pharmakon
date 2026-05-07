use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{json, Value};
use reqwest::Client;

pub struct FirecrawlTool {
    api_key: String,
    client: Client,
}

impl FirecrawlTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for FirecrawlTool {
    fn name(&self) -> &str { "firecrawl" }
    fn description(&self) -> &str { "Crawl websites and convert them to clean markdown for LLM consumption" }
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
        let res = self.client.post("https://api.firecrawl.dev/v0/scrape")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({ "url": url }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let json = res.json::<Value>().await.map_err(|e| AgentError(e.to_string()))?;
        Ok(json.to_string())
    }
}
