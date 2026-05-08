use async_trait::async_trait;
use pharmakon_common::{AgentResult, Tool};
use serde_json::{Value, json};

pub struct DiscoverToolsTool {
    catalog: crate::tool_meta_catalog::ToolMetaCatalog,
}

impl Default for DiscoverToolsTool {
    fn default() -> Self {
        Self::new()
    }
}

impl DiscoverToolsTool {
    pub fn new() -> Self {
        Self {
            catalog: crate::tool_meta_catalog::build_default_catalog(),
        }
    }
}

#[async_trait]
impl Tool for DiscoverToolsTool {
    fn name(&self) -> &str {
        "discover_tools"
    }
    fn description(&self) -> &str {
        "Search for available tools based on a query using BM25 ranking. Use this when you are not sure which tool to use for a specific task."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Keyword or task description to search for relevant tools" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"].as_str().unwrap_or("");
        let results = self.catalog.search(query, 10);

        if results.is_empty() {
            Ok(format!(
                "No tools found matching '{}'. Try a broader search or different keywords.",
                query
            ))
        } else {
            let mut output = format!("### Discovered Tools for '{}' (BM25 Ranked):\n\n", query);
            for res in results {
                output.push_str(&format!(
                    "- **{}** (Score: {:.2}): {}\n",
                    res.meta.name, res.score, res.meta.description
                ));
            }
            Ok(output)
        }
    }
}
