use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, KnowledgeConnector, Tool};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct ContextConnectorTool {
    connectors: Vec<Arc<dyn KnowledgeConnector>>,
}

impl Default for ContextConnectorTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextConnectorTool {
    pub fn new() -> Self {
        Self {
            connectors: Vec::new(),
        }
    }

    pub fn add_connector(&mut self, connector: Arc<dyn KnowledgeConnector>) {
        self.connectors.push(connector);
    }
}

#[async_trait]
impl Tool for ContextConnectorTool {
    fn name(&self) -> &str {
        "search_knowledge"
    }
    fn description(&self) -> &str {
        "Search for information in connected external sources (Notion, Slack, etc.)"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "The search query" },
                "source": { "type": "string", "description": "Optional specific source to search" }
            },
            "required": ["query"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let query = args["query"]
            .as_str()
            .ok_or_else(|| AgentError("Missing query".to_string()))?;
        let specific_source = args["source"].as_str();

        let mut all_context = Vec::new();

        for connector in &self.connectors {
            if let Some(s) = specific_source
                && connector.name() != s {
                    continue;
                }

            match connector.fetch_context(query).await {
                Ok(context) => {
                    for item in context {
                        all_context.push(format!("[{}]: {}", connector.name(), item));
                    }
                }
                Err(e) => {
                    log::error!("Connector {} failed: {}", connector.name(), e);
                }
            }
        }

        if all_context.is_empty() {
            Ok("No relevant information found in external knowledge bases.".to_string())
        } else {
            Ok(all_context.join("\n---\n"))
        }
    }
}

pub struct SlackConnector {
    pub token: String,
}

#[async_trait]
impl KnowledgeConnector for SlackConnector {
    fn name(&self) -> &str {
        "slack"
    }
    async fn fetch_context(&self, _query: &str) -> anyhow::Result<Vec<String>> {
        // Placeholder for real Slack API call
        Ok(vec![
            "Recent Slack message: Project Pharmakon stabilization is in progress.".to_string(),
        ])
    }
}

pub struct NotionConnector {
    pub token: String,
}

#[async_trait]
impl KnowledgeConnector for NotionConnector {
    fn name(&self) -> &str {
        "notion"
    }
    async fn fetch_context(&self, _query: &str) -> anyhow::Result<Vec<String>> {
        // Placeholder for real Notion API call
        Ok(vec![
            "Notion Page: Pharmakon Architecture Overview (v1.0)".to_string(),
        ])
    }
}
