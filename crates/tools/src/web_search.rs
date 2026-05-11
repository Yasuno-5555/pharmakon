use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, ExecutionProfile, Reversibility, SideEffectLevel, SecretStore, Tool, ToolCategory};
use serde_json::{Value, json};

// ═══════════════════════════════════════════════════════════
// BraveSearchTool
// ═══════════════════════════════════════════════════════════

pub struct BraveSearchTool {
    api_key: String,
}

impl BraveSearchTool {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }
}

#[async_trait]
impl Tool for BraveSearchTool {
    fn name(&self) -> &str {
        "brave_search"
    }
    fn description(&self) -> &str {
        "Search the web via Brave Search API. Returns titles, URLs, and snippets."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "default": 5, "description": "Results to return (max 20)" }
            },
            "required": ["query"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::None,
            network_access: true,
            filesystem_scope: pharmakon_common::FilesystemScope::None,
            reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let count = args["count"].as_u64().unwrap_or(5).min(20);

        let client = reqwest::Client::new();
        let response = client
            .get("https://api.search.brave.com/res/v1/web/search")
            .header("Accept", "application/json")
            .header("Accept-Encoding", "gzip")
            .header("X-Subscription-Token", &self.api_key)
            .query(&[("q", query), ("count", &count.to_string())])
            .send()
            .await
            .map_err(|e| AgentError(format!("Brave Search request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(AgentError(format!(
                "Brave Search API error: HTTP {}",
                response.status()
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| AgentError(format!("Failed to parse Brave response: {}", e)))?;

        let mut results = Vec::new();
        if let Some(web_results) = body
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
        {
            for (i, res) in web_results.iter().enumerate() {
                let title = res["title"].as_str().unwrap_or("No Title");
                let url = res["url"].as_str().unwrap_or("");
                let snippet = res["description"].as_str().unwrap_or("");
                results.push(format!(
                    "{}. **{}**\n   URL: {}\n   {}\n",
                    i + 1, title, url, snippet
                ));
            }
        }

        if results.is_empty() {
            Ok(format!("No results found for '{}'. Try a different query.", query))
        } else {
            let joined = results.join("\n");
            Ok(format!("Search results for '{}':\n\n{}", query, joined))
        }
    }
}

// ═══════════════════════════════════════════════════════════
// GoogleSearchTool
// ═══════════════════════════════════════════════════════════

pub struct GoogleSearchTool;

#[async_trait]
impl Tool for GoogleSearchTool {
    fn name(&self) -> &str {
        "google_search"
    }
    fn description(&self) -> &str {
        "Search Google via Custom Search API. Requires GOOGLE_SEARCH_API_KEY and GOOGLE_SEARCH_CX in environment or secrets."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "default": 5, "description": "Results (max 10)" }
            },
            "required": ["query"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::None,
            network_access: true,
            filesystem_scope: pharmakon_common::FilesystemScope::None,
            reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let _count = args["count"].as_u64().unwrap_or(5).min(10);
        let secret_store = SecretStore::new();

        let api_key = secret_store
            .get_secret("GOOGLE_SEARCH_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_SEARCH_API_KEY"))
            .map_err(|_| AgentError("GOOGLE_SEARCH_API_KEY not set in secrets or env".to_string()))?;
        let cx = secret_store
            .get_secret("GOOGLE_SEARCH_CX")
            .or_else(|_| std::env::var("GOOGLE_SEARCH_CX"))
            .map_err(|_| AgentError("GOOGLE_SEARCH_CX not set".to_string()))?;

        let client = reqwest::Client::new();
        let response = client
            .get("https://www.googleapis.com/customsearch/v1")
            .query(&[("key", api_key.as_str()), ("cx", cx.as_str()), ("q", query)])
            .send()
            .await
            .map_err(|e| AgentError(format!("Google search failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AgentError(format!("Google API error: HTTP {} - {}", status, body)));
        }

        let body: Value = response.json().await.map_err(|e| AgentError(e.to_string()))?;
        let mut results = Vec::new();

        if let Some(items) = body.get("items").and_then(|i| i.as_array()) {
            for (i, item) in items.iter().enumerate() {
                let title = item["title"].as_str().unwrap_or("No Title");
                let link = item["link"].as_str().unwrap_or("");
                let snippet = item["snippet"].as_str().unwrap_or("");
                results.push(format!(
                    "{}. **{}**\n   URL: {}\n   {}\n",
                    i + 1, title, link, snippet
                ));
            }
        }

        if results.is_empty() {
            Ok(format!("No results found for '{}'.", query))
        } else {
            Ok(format!("Google results for '{}':\n\n{}", query, results.join("\n")))
        }
    }
}

// ═══════════════════════════════════════════════════════════
// DuckDuckGoSearchTool — scraper-crate based HTML parsing
// ═══════════════════════════════════════════════════════════

pub struct DuckDuckGoSearchTool {
    client: reqwest::Client,
}

impl DuckDuckGoSearchTool {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .user_agent("Mozilla/5.0 (compatible; Pharmakon/0.1)")
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Parse DuckDuckGo Lite HTML using the scraper crate.
    fn parse_lite_results(html: &str, max_results: usize) -> Vec<String> {
        use scraper::{Html, Selector};

        let doc = Html::parse_document(html);
        let link_sel = match Selector::parse("a[rel=nofollow]") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let snippet_sel = match Selector::parse(".result-snippet") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        // Collect links (title + URL)
        let mut links: Vec<(String, String)> = Vec::new();
        for el in doc.select(&link_sel) {
            let href = el.value().attr("href").unwrap_or("");
            if href.is_empty() || href.starts_with("//") || href.starts_with("javascript:") {
                continue;
            }
            let title: String = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if title.is_empty() {
                continue;
            }
            links.push((title, decode_html_entities(href)));
            if links.len() >= max_results {
                break;
            }
        }

        // Collect snippets
        let mut snippets: Vec<String> = Vec::new();
        for el in doc.select(&snippet_sel) {
            let text: String = el.text().collect::<Vec<_>>().join(" ").trim().to_string();
            if !text.is_empty() {
                snippets.push(decode_html_entities(&text));
            }
            if snippets.len() >= max_results {
                break;
            }
        }

        // Zip links with snippets
        let mut results = Vec::new();
        for (i, (title, url)) in links.iter().enumerate() {
            let snippet = snippets.get(i).map(|s| s.as_str()).unwrap_or("");
            results.push(format!(
                "{}. **{}**\n   URL: {}\n   {}",
                i + 1,
                decode_html_entities(title),
                url,
                snippet
            ));
        }

        results
    }
}

fn decode_html_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            for c in chars.by_ref() {
                if c == ';' {
                    break;
                }
                entity.push(c);
            }
            let decoded = match entity.as_str() {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "apos" => "'",
                "nbsp" => " ",
                "#39" => "'",
                _ => {
                    // Try numeric
                    if let Some(num) = entity.strip_prefix('#') {
                        if let Ok(n) = num.parse::<u32>() {
                            if let Some(ch) = char::from_u32(n) {
                                out.push(ch);
                                continue;
                            }
                        }
                    }
                    // Fallback: keep entity as-is
                    out.push('&');
                    out.push_str(&entity);
                    out.push(';');
                    continue;
                }
            };
            out.push_str(decoded);
        } else {
            out.push(c);
        }
    }
    out
}

#[async_trait]
impl Tool for DuckDuckGoSearchTool {
    fn name(&self) -> &str {
        "duckduckgo_search"
    }
    fn description(&self) -> &str {
        "Search the web using DuckDuckGo (free, no API key). Returns numbered results with titles, URLs, and snippets."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "default": 8, "description": "Max results (1-20)" }
            },
            "required": ["query"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::None,
            network_access: true,
            filesystem_scope: pharmakon_common::FilesystemScope::None,
            reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let count = args["count"].as_u64().unwrap_or(8).min(20) as usize;

        let url = "https://lite.duckduckgo.com/lite";
        let response = self.client
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

        let html = response.text().await
            .map_err(|e| AgentError(format!("Failed to read response: {}", e)))?;

        let results = Self::parse_lite_results(&html, count);

        if results.is_empty() {
            Ok(format!("No results found for '{}'. Try a different query.", query))
        } else {
            Ok(format!("DuckDuckGo results for '{}':\n\n{}", query, results.join("\n\n")))
        }
    }
}

// ═══════════════════════════════════════════════════════════
// Search Dispatcher
// ═══════════════════════════════════════════════════════════

pub struct SearchDispatcherTool {
    duckduckgo: DuckDuckGoSearchTool,
}

impl SearchDispatcherTool {
    pub fn new() -> Self {
        Self {
            duckduckgo: DuckDuckGoSearchTool::new(),
        }
    }
}

#[async_trait]
impl Tool for SearchDispatcherTool {
    fn name(&self) -> &str {
        "search"
    }
    fn description(&self) -> &str {
        "Universal web search. Routes to DuckDuckGo (free) by default. Returns structured results with titles, URLs, and snippets."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "count": { "type": "integer", "default": 8, "description": "Max results (1-20)" }
            },
            "required": ["query"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Network
    }
    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::None,
            network_access: true,
            filesystem_scope: pharmakon_common::FilesystemScope::None,
            reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        self.duckduckgo.call(args).await
    }
}
