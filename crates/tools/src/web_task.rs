use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::time::Duration;

pub struct WebTaskTool;

#[async_trait]
impl Tool for WebTaskTool {
    fn name(&self) -> &str {
        "web_task"
    }

    fn description(&self) -> &str {
        "One-shot web task: search or fetch a page and return a compact summary with sources."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "url": { "type": "string" },
                "max_chars": { "type": "integer", "default": 4000 }
            }
        })
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let max_chars = args["max_chars"].as_u64().unwrap_or(4000) as usize;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(|e| AgentError(e.to_string()))?;
        if let Some(url) = args["url"].as_str() {
            let body = client
                .get(url)
                .send()
                .await
                .map_err(|e| AgentError(e.to_string()))?
                .text()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            let text = scraper::Html::parse_document(&body)
                .root_element()
                .text()
                .collect::<Vec<_>>()
                .join(" ");
            let summary = text
                .split_whitespace()
                .take(max_chars / 6)
                .collect::<Vec<_>>()
                .join(" ");
            return Ok(json!({ "url": url, "summary": summary, "chars": text.len() }).to_string());
        }
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query or url".to_string()))?;
        if let Ok(api_key) = std::env::var("BRAVE_API_KEY") {
            let body: Value = client
                .get("https://api.search.brave.com/res/v1/web/search")
                .header("Accept", "application/json")
                .header("X-Subscription-Token", api_key)
                .query(&[("q", query), ("count", "5")])
                .send()
                .await
                .map_err(|e| AgentError(e.to_string()))?
                .json()
                .await
                .map_err(|e| AgentError(e.to_string()))?;
            return Ok(serde_json::to_string_pretty(&body["web"]["results"]).unwrap_or_default());
        }
        let body: Value = client
            .get("https://api.duckduckgo.com/")
            .query(&[("q", query), ("format", "json"), ("no_html", "1")])
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json!({
            "query": query,
            "abstract": body["AbstractText"],
            "source": body["AbstractURL"],
            "note": "Set BRAVE_API_KEY for richer search results."
        })
        .to_string())
    }
}
