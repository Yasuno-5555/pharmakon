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
        "brave_search"
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
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let secret_store = SecretStore::new();
        let api_key = secret_store
            .get_secret("GOOGLE_SEARCH_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_SEARCH_API_KEY"))
            .map_err(|_| AgentError("GOOGLE_SEARCH_API_KEY not found.".to_string()))?;
        let cx = secret_store
            .get_secret("GOOGLE_SEARCH_CX")
            .or_else(|_| std::env::var("GOOGLE_SEARCH_CX"))
            .map_err(|_| {
                AgentError("GOOGLE_SEARCH_CX (Custom Search Engine ID) not found.".to_string())
            })?;

        let client = reqwest::Client::new();
        let url = "https://www.googleapis.com/customsearch/v1";
        let response = client
            .get(url)
            .query(&[("key", api_key.as_str()), ("cx", cx.as_str()), ("q", query)])
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AgentError(format!(
                "Google API error: {}",
                response.status()
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        let mut results = Vec::new();
        if let Some(items) = body.get("items").and_then(|i| i.as_array()) {
            for item in items {
                let title = item["title"].as_str().unwrap_or("No Title");
                let link = item["link"].as_str().unwrap_or("");
                let snippet = item["snippet"].as_str().unwrap_or("");
                results.push(format!(
                    "### {}\nURL: {}\nSnippet: {}\n",
                    title, link, snippet
                ));
            }
        }

        if results.is_empty() {
            Ok("No Google results found.".to_string())
        } else {
            Ok(results.join("\n"))
        }
    }
}

// ═══════════════════════════════════════════════════════════
// DuckDuckGo Search — zero API key, zero cost
// ═══════════════════════════════════════════════════════════

pub struct DuckDuckGoSearchTool {
    client: reqwest::Client,
}

impl DuckDuckGoSearchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Pharmakon/0.1 (AI assistant; web search)")
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl Tool for DuckDuckGoSearchTool {
    fn name(&self) -> &str {
        "duckduckgo_search"
    }
    fn description(&self) -> &str {
        "Search the web using DuckDuckGo (free, no API key required). Returns titles, URLs, and snippets."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query" },
                "count": { "type": "integer", "default": 8, "description": "Max results (1-20)" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let count = args["count"].as_u64().unwrap_or(8).min(20) as usize;

        // Use DuckDuckGo Lite — minimal HTML, no JS, stable structure
        let url = "https://lite.duckduckgo.com/lite";
        let response = self
            .client
            .get(url)
            .query(&[("q", query)])
            .send()
            .await
            .map_err(|e| AgentError(format!("DuckDuckGo request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError(format!(
                "DuckDuckGo returned HTTP {}",
                response.status()
            )));
        }

        let html = response
            .text()
            .await
            .map_err(|e| AgentError(format!("Failed to read response: {}", e)))?;

        let results = Self::parse_lite_results(&html, count);
        if results.is_empty() {
            Ok("No results found. Try a different query.".to_string())
        } else {
            Ok(results.join("\n\n"))
        }
    }
}

impl DuckDuckGoSearchTool {
    /// Parse DuckDuckGo Lite HTML into structured results.
    /// Lite layout: each result is a table row group with link, title, snippet.
    fn parse_lite_results(html: &str, max_results: usize) -> Vec<String> {
        let mut results = Vec::new();
        let mut current_url = String::new();
        let mut current_title = String::new();
        let mut current_snippet = String::new();

        // State machine: track whether we're inside an <a> tag, and pending href
        let mut in_link = false;
        let mut link_href = String::new();

        for line in html.lines() {
            let trimmed = line.trim();

            // Detect result links: <a rel="nofollow" href="...">
            if trimmed.starts_with("<a ") && trimmed.contains("href=\"") {
                in_link = true;
                link_href = String::new();
                // Extract href
                if let Some(start) = trimmed.find("href=\"") {
                    let after = &trimmed[start + 6..];
                    if let Some(end) = after.find('"') {
                        link_href = after[..end].to_string();
                        // Decode common HTML entities in URLs
                        link_href = link_href.replace("&amp;", "&");
                    }
                }
            }

            if in_link {
                if trimmed.contains("</a>") {
                    in_link = false;
                    // Extract title text between <a ...> and </a>
                    if let Some(start) = trimmed.find('>') {
                        let title_text = &trimmed[start + 1..];
                        if let Some(end) = title_text.find("</a>") {
                            current_title = title_text[..end]
                                .trim()
                                .replace("&amp;", "&")
                                .replace("&lt;", "<")
                                .replace("&gt;", ">")
                                .replace("&quot;", "\"");
                        }
                    }
                } else if current_title.is_empty() && !line.contains('<') {
                    // Continuation of title on next line
                    current_title.push_str(trimmed);
                    current_title = current_title
                        .replace("&amp;", "&")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">");
                }
            }

            // Detect snippet: <td class="result-snippet">
            if trimmed.contains("result-snippet") {
                // Extract text after the td tag
                if let Some(start) = trimmed.find('>') {
                    current_snippet = trimmed[start + 1..]
                        .replace("&amp;", "&")
                        .replace("&lt;", "<")
                        .replace("&gt;", ">");
                }
                // Remove closing tag
                if let Some(end) = current_snippet.find("</td>") {
                    current_snippet = current_snippet[..end].to_string();
                }
            }

            // URL is in a <td> with a link inside, or in a "result-link" class
            if trimmed.contains("result-link") {
                // Just trust the href we extracted from the <a> tag above
            }

            // When we have all three pieces, push result
            if !current_url.is_empty()
                && !current_title.is_empty()
                && !current_snippet.is_empty()
            {
                results.push(format!(
                    "### {}\nURL: {}\n{}",
                    current_title.trim(),
                    current_url.trim(),
                    current_snippet.trim()
                ));
                current_url.clear();
                current_title.clear();
                current_snippet.clear();
                link_href.clear();

                if results.len() >= max_results {
                    break;
                }
            }

            // The URL from link_href becomes the current_url for the next result
            if !link_href.is_empty()
                && current_url.is_empty()
                && !link_href.starts_with("//")
            {
                current_url = link_href.clone();
            }
        }

        results
    }
}
