use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Fact, ResearchDepth, ResearchNotebook, Tool};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct ResearchSearchTool {
    pub notebook: Arc<Mutex<ResearchNotebook>>,
}

#[async_trait]
impl Tool for ResearchSearchTool {
    fn name(&self) -> &str {
        "research_search"
    }
    fn description(&self) -> &str {
        "Perform a scoped web search for research purposes. Returns snippets and marks URLs in the research tree."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query" },
                "reasoning": { "type": "string", "description": "Why this query is being performed" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"].as_str().unwrap_or_default();

        // In a real implementation, this would call BraveSearchTool or Tavily.
        // For now, we simulate a filtered search result.
        let results = vec![
            json!({"title": format!("Search result for {}", query), "url": "https://example.com/res1", "snippet": "Useful snippet here..."}),
            json!({"title": "Documentation", "url": "https://docs.rs/example", "snippet": "Core concepts and API reference."}),
        ];

        let mut notebook = self.notebook.lock().await;
        let mut report = format!("### Search Results for '{}'\n", query);

        let mut urls = Vec::new();
        for res in results {
            let title = res["title"].as_str().unwrap_or("No Title");
            let url = res["url"].as_str().unwrap_or("");
            let snippet = res["snippet"].as_str().unwrap_or("");

            report.push_str(&format!("- [{}]({}): {}\n", title, url, snippet));
            urls.push(url.to_string());
            notebook
                .visited_urls
                .entry(url.to_string())
                .or_insert(ResearchDepth::Skim);
        }

        notebook.research_tree.insert(query.to_string(), urls);

        Ok(report)
    }
}

pub struct ResearchFetchTool {
    pub notebook: Arc<Mutex<ResearchNotebook>>,
    pub store: Option<Arc<dyn pharmakon_common::ResearchPersistence>>,
}

#[async_trait]
impl Tool for ResearchFetchTool {
    fn name(&self) -> &str {
        "research_fetch"
    }
    fn description(&self) -> &str {
        "Fetch a URL with a specific depth. Skim (fast), Summary (extracted points), or Deep (full text). Checks cache first."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The URL to fetch" },
                "depth": { "type": "string", "enum": ["skim", "summary", "deep"], "default": "summary" }
            },
            "required": ["url"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let url = args["url"].as_str().unwrap_or_default();
        let depth_str = args["depth"].as_str().unwrap_or("summary");

        let depth = match depth_str {
            "skim" => ResearchDepth::Skim,
            "summary" => ResearchDepth::Summary,
            "deep" => ResearchDepth::Deep,
            _ => ResearchDepth::Summary,
        };

        // Check cache
        if let Some(store) = &self.store {
            if let Ok(Some((summary, cached_depth, _))) = store.get_research_cache(url).await {
                // If cached depth is equal or deeper than requested, return cache
                if (cached_depth == "deep")
                    || (cached_depth == "summary" && depth_str != "deep")
                    || (cached_depth == "skim" && depth_str == "skim")
                {
                    return Ok(format!(
                        "(CACHE HIT) Fetched '{}' at {} depth:\n\n{}",
                        url, cached_depth, summary
                    ));
                }
            }
        }

        // Simulate fetching and pre-processing
        let content = match depth {
            ResearchDepth::Skim => "Title and Metadata only (Skimmed)".to_string(),
            ResearchDepth::Summary => {
                "Key finding: This project is evolving rapidly. (Summarized)".to_string()
            }
            ResearchDepth::Deep => {
                "Full content after cleaning script/style tags... (Deep)".to_string()
            }
        };

        // Save to cache
        if let Some(store) = &self.store {
            let _ = store
                .save_research_cache(url, &content, depth_str, &json!({}))
                .await;
        }

        let mut notebook = self.notebook.lock().await;
        notebook.visited_urls.insert(url.to_string(), depth);

        Ok(format!(
            "Fetched '{}' at {} depth:\n\n{}",
            url, depth_str, content
        ))
    }
}

pub struct ResearchConsolidateTool {
    pub notebook: Arc<Mutex<ResearchNotebook>>,
}

#[async_trait]
impl Tool for ResearchConsolidateTool {
    fn name(&self) -> &str {
        "research_consolidate"
    }
    fn description(&self) -> &str {
        "Update the Research Notebook with new facts, pending questions, or dead ends. Use this to maintain state."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "new_facts": { "type": "array", "items": { "type": "object", "properties": { "content": { "type": "string" }, "source": { "type": "string" }, "confidence": { "type": "number" } } } },
                "new_questions": { "type": "array", "items": { "type": "string" } },
                "dead_ends": { "type": "array", "items": { "type": "string" } }
            }
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let mut notebook = self.notebook.lock().await;

        if let Some(facts) = args["new_facts"].as_array() {
            for f in facts {
                notebook.verified_facts.push(Fact {
                    content: f["content"].as_str().unwrap_or_default().to_string(),
                    source_url: f["source"].as_str().unwrap_or_default().to_string(),
                    confidence: f["confidence"].as_f64().unwrap_or(0.8) as f32,
                    timestamp: chrono::Utc::now(),
                });
            }
        }

        if let Some(qs) = args["new_questions"].as_array() {
            for q in qs {
                notebook
                    .pending_questions
                    .push(q.as_str().unwrap_or_default().to_string());
            }
        }

        if let Some(ds) = args["dead_ends"].as_array() {
            for d in ds {
                notebook
                    .dead_ends
                    .push(d.as_str().unwrap_or_default().to_string());
            }
        }

        Ok(notebook.to_summary_string())
    }
}
