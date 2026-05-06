use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, SecretStore, Tool};
use serde_json::{Value, json};

pub struct BraveSearchTool;

impl BraveSearchTool {
    pub fn new(_api_key: String) -> Self {
        Self
    }
}

#[async_trait]
impl Tool for BraveSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }
    fn description(&self) -> &str {
        "Search the web for current information. Returns a list of results with snippets."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query" },
                "count": { "type": "integer", "default": 5, "description": "Number of results to return" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let count = args["count"].as_u64().unwrap_or(5);

        let secret_store = SecretStore::new();
        let api_key = secret_store
            .get_secret("BRAVE_API_KEY")
            .or_else(|_| std::env::var("BRAVE_API_KEY"))
            .map_err(|_| {
                AgentError(
                    "BRAVE_API_KEY not found. Please set it in secrets or environment.".to_string(),
                )
            })?;

        let client = reqwest::Client::new();
        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentError(format!(
                "Brave Search API error: {}",
                response.status()
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let mut results = Vec::new();

        if let Some(web_results) = body
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
        {
            for res in web_results {
                let title = res["title"].as_str().unwrap_or("No Title");
                let url = res["url"].as_str().unwrap_or("");
                let snippet = res["description"].as_str().unwrap_or("");
                results.push(format!(
                    "### {}\nURL: {}\nSnippet: {}\n",
                    title, url, snippet
                ));
            }
        }

        if results.is_empty() {
            Ok("No results found.".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}
pub struct GoogleSearchTool;

#[async_trait]
impl Tool for GoogleSearchTool {
    fn name(&self) -> &str {
        "google_search"
    }
    fn description(&self) -> &str {
        "Search Google for high-precision results using the Google Custom Search API. Best for broad technical queries."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query" },
                "count": { "type": "integer", "default": 5 }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"].as_str().ok_or_else(|| AgentError("Missing query".to_string()))?;
        let secret_store = SecretStore::new();
        let api_key = secret_store.get_secret("GOOGLE_SEARCH_API_KEY").or_else(|_| std::env::var("GOOGLE_SEARCH_API_KEY"))
            .map_err(|_| AgentError("GOOGLE_SEARCH_API_KEY not found.".to_string()))?;
        let cx = secret_store.get_secret("GOOGLE_SEARCH_CX").or_else(|_| std::env::var("GOOGLE_SEARCH_CX"))
            .map_err(|_| AgentError("GOOGLE_SEARCH_CX (Custom Search Engine ID) not found.".to_string()))?;

        let client = reqwest::Client::new();
        let url = "https://www.googleapis.com/customsearch/v1";
        let response = client.get(url)
            .query(&[("key", api_key.as_str()), ("cx", cx.as_str()), ("q", query)])
            .send().await.map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
             return Err(AgentError(format!("Google API error: {}", response.status())));
        }

        let body: Value = response.json().await.map_err(|e| AgentError(e.to_string()))?;
        let mut results = Vec::new();
        if let Some(items) = body.get("items").and_then(|i| i.as_array()) {
            for item in items {
                let title = item["title"].as_str().unwrap_or("No Title");
                let link = item["link"].as_str().unwrap_or("");
                let snippet = item["snippet"].as_str().unwrap_or("");
                results.push(format!("### {}\nURL: {}\nSnippet: {}\n", title, link, snippet));
            }
        }

        if results.is_empty() {
            Ok("No Google results found.".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}
