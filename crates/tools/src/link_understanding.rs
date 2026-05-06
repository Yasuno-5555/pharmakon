use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use reqwest::Client;
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct LinkUnderstandingTool {
    client: Client,
    pub model: Option<Arc<dyn pharmakon_common::AgentModel>>,
}

impl LinkUnderstandingTool {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            model: None,
        }
    }
}

#[async_trait]
impl Tool for LinkUnderstandingTool {
    fn name(&self) -> &str {
        "understand_link"
    }
    fn description(&self) -> &str {
        "Extract rich metadata, title, and a summary from a URL to understand its content"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The URL to analyze" }
            },
            "required": ["url"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| AgentError("Missing url".to_string()))?;

        log::info!("Analyzing link: {}", url);

        let res = self
            .client
            .get(url)
            .header("User-Agent", "Pharmakon/0.1.0")
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let html_content = res.text().await.map_err(|e| AgentError(e.to_string()))?;
        let document = Html::parse_document(&html_content);

        let title_selector = Selector::parse("title").unwrap();
        let title = document
            .select(&title_selector)
            .next()
            .map(|e| e.inner_html())
            .unwrap_or_else(|| "No title found".to_string());

        let meta_desc_selector = Selector::parse("meta[name='description']").unwrap();
        let description = document
            .select(&meta_desc_selector)
            .next()
            .and_then(|e| e.value().attr("content"))
            .unwrap_or("No description found");

        // OpenGraph extraction
        let mut og_data = std::collections::HashMap::new();
        let og_selector = Selector::parse("meta[property^='og:']").unwrap();
        for element in document.select(&og_selector) {
            if let (Some(property), Some(content)) = (
                element.value().attr("property"),
                element.value().attr("content"),
            ) {
                og_data.insert(property.to_string(), content.to_string());
            }
        }

        // JSON-LD extraction
        let json_ld_selector = Selector::parse("script[type='application/ld+json']").unwrap();
        let mut structured_data = Vec::new();
        for element in document.select(&json_ld_selector) {
            let json_text = element.inner_html();
            if let Ok(json_val) = serde_json::from_str::<Value>(&json_text) {
                structured_data.push(json_val);
            }
        }

        // Basic summary: first few paragraphs
        let p_selector = Selector::parse("p").unwrap();
        let summary: String = document
            .select(&p_selector)
            .take(5)
            .map(|e| e.text().collect::<String>())
            .collect::<Vec<_>>()
            .join("\n\n");

        let result = json!({
            "url": url,
            "title": title,
            "description": description,
            "open_graph": og_data,
            "structured_data_count": structured_data.len(),
            "summary_preview": summary
        });

        Ok(result.to_string())
    }
}
