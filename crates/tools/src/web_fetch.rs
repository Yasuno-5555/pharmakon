use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, ExecutionProfile, Reversibility, SideEffectLevel, Tool, ToolCategory};
use reqwest::Client;
use serde_json::{Value, json};
use std::time::Duration;

const MAX_BODY_SIZE: u64 = 5 * 1024 * 1024; // 5MB
const DEFAULT_TIMEOUT: u64 = 30;
const USER_AGENT: &str = "Mozilla/5.0 (compatible; Pharmakon/0.1; +https://github.com/yasuno-5555/Pharmakon)";

/// Simple HTML-to-text conversion that extracts readable content.
/// Uses the scraper crate for proper HTML parsing.
fn html_to_text(html: &str, max_length: usize) -> String {
    use scraper::{Html, Selector};
    let document = Html::parse_fragment(html);
    let body_selector = Selector::parse("body, article, main").ok();
    let p_selector = Selector::parse("p, h1, h2, h3, h4, h5, h6, li, td, th, pre, code, blockquote, title, meta[name=description]").ok();
    let a_selector = Selector::parse("a[href]").ok();
    let _img_selector = Selector::parse("img[alt], img[src]").ok();
    let script_selector = Selector::parse("script, style, nav, footer, header, aside, .sidebar, .menu, .nav, .footer, .header, .ad, .advertisement").ok();

    // Remove unwanted elements first — get the root
    let root = if let Some(ref sel) = body_selector {
        if let Some(el) = document.select(sel).next() {
            let mut inner = el.inner_html();
            if let Some(ref remove) = script_selector {
                let remove_doc = Html::parse_fragment(&inner);
                for node in remove_doc.select(remove) {
                    let html_str = node.html();
                    inner = inner.replace(&html_str, "");
                }
            }
            inner
        } else {
            html.to_string()
        }
    } else {
        html.to_string()
    };

    // Re-parse cleaned HTML
    let clean_doc = Html::parse_fragment(&root);

    let mut output = String::new();
    let mut links = Vec::new();
    let mut link_counter = 0;

    // Extract title and description
    if let Ok(sel) = Selector::parse("title")
        && let Some(el) = clean_doc.select(&sel).next() {
            let title = el.text().collect::<String>().trim().to_string();
            if !title.is_empty() {
                output.push_str(&format!("# {}\n\n", title));
            }
        }

    if let Ok(sel) = Selector::parse("meta[name=description]")
        && let Some(el) = clean_doc.select(&sel).next()
            && let Some(content) = el.value().attr("content")
                && !content.is_empty() {
                    output.push_str(&format!("> {}\n\n", content));
                }

    // Extract text content
    if let Some(ref sel) = p_selector {
        for element in clean_doc.select(sel) {
            let tag = element.value().name();
            let text: String = element.text().collect::<Vec<_>>().join(" ").trim().to_string();

            if text.is_empty() {
                continue;
            }

            let prefix = match tag {
                "h1" => "# ",
                "h2" => "## ",
                "h3" => "### ",
                "h4" => "#### ",
                "h5" | "h6" => "##### ",
                "li" => "- ",
                "pre" | "code" => "",
                "blockquote" => "> ",
                "th" => "**",
                _ => "",
            };

            let suffix = match tag {
                "th" => "**",
                _ => "",
            };

            output.push_str(&format!("{}{}{}\n", prefix, text, suffix));

            // Add blank line after headings and blockquotes
            if matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote" | "pre") {
                output.push('\n');
            }

            // Separate list items
            if tag == "li" {
                output.push('\n');
            }

            if output.len() > max_length {
                output.push_str("\n\n... (content truncated)");
                break;
            }
        }
    }

    // Extract links with markers
    if output.len() < max_length / 2
        && let Some(ref sel) = a_selector {
            for element in clean_doc.select(sel) {
                if let Some(href) = element.value().attr("href") {
                    let text: String = element.text().collect();
                    let text = text.trim();
                    if text.is_empty() || href.starts_with('#') || href.starts_with("javascript:") {
                        continue;
                    }
                    link_counter += 1;
                    let href_clean = if href.starts_with('/') {
                        // Relative URL — skip, can't resolve without base
                        continue;
                    } else {
                        href
                    };
                    links.push(format!("[{}] {} — {}", link_counter, text, href_clean));
                    if links.len() >= 20 {
                        break;
                    }
                }
            }
        }

    if !links.is_empty() {
        output.push_str("\n\n---\nLinks:\n");
        for link in &links {
            output.push_str(link);
            output.push('\n');
        }
    }

    output
}

pub struct WebFetchTool {
    client: Client,
}

impl WebFetchTool {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(DEFAULT_TIMEOUT))
                .redirect(reqwest::redirect::Policy::limited(10))
                .danger_accept_invalid_certs(false)
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for WebFetchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }
    fn description(&self) -> &str {
        "Fetch a URL and extract readable text content. Converts HTML to Markdown. 30s timeout, 5MB limit, 10 redirects. Returns structured text with links."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "URL to fetch" },
                "raw": { "type": "boolean", "default": false, "description": "Return raw body instead of converting HTML" },
                "max_chars": { "type": "integer", "default": 10000, "description": "Maximum characters to return" }
            },
            "required": ["url"]
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
        let url = args["url"]
            .as_str()
            .ok_or_else(|| AgentError("Missing url".to_string()))?;
        let raw = args["raw"].as_bool().unwrap_or(false);
        let max_chars = args["max_chars"].as_u64().unwrap_or(10000) as usize;

        // Basic URL validation
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Err(AgentError(format!(
                "Invalid URL: '{}'. URL must start with http:// or https://", url
            )));
        }

        let response = self.client
            .get(url)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AgentError(format!("Request timed out after {}s: {}", DEFAULT_TIMEOUT, url))
                } else if e.is_connect() {
                    AgentError(format!("Connection failed: {} ({})", url, e))
                } else if e.is_redirect() {
                    AgentError(format!("Too many redirects: {}", url))
                } else {
                    AgentError(format!("Request failed: {} ({})", url, e))
                }
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(AgentError(format!(
                "HTTP {}: {}",
                status.as_u16(),
                url
            )));
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        // Read body with size limit
        let body = response
            .text()
            .await
            .map_err(|e| AgentError(format!("Failed to read response body: {}", e)))?;

        if body.len() as u64 > MAX_BODY_SIZE {
            return Ok(format!(
                "Response body is {:.1}MB (max 5MB). Showing first 100KB:\n\n{}",
                body.len() as f64 / (1024.0 * 1024.0),
                &body[..(100 * 1024).min(body.len())]
            ));
        }

        if raw || !content_type.contains("html") {
            // Return raw content (truncated)
            if body.len() > max_chars {
                let truncated: String = body.chars().take(max_chars).collect();
                return Ok(format!(
                    "Content-Type: {}\nSize: {} bytes (showing {} chars)\n\n{}",
                    content_type, body.len(), max_chars, truncated
                ));
            }
            return Ok(format!(
                "Content-Type: {}\nSize: {} bytes\n\n{}",
                content_type, body.len(), body
            ));
        }

        // HTML → Markdown conversion
        let text = html_to_text(&body, max_chars);

        Ok(format!(
            "URL: {}\nFetched: {} bytes → {} chars text\n\n{}",
            url,
            body.len(),
            text.len(),
            text
        ))
    }
}
