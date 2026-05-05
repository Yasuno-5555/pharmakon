use async_trait::async_trait;
use crate::hooks::Hook;
use crate::model::{Message, MessageContent, ContentPart};
use anyhow::Result;
use regex::Regex;
use std::sync::Arc;
use tokio::sync::Mutex;
use pharmakon_tools::link_understanding::LinkUnderstandingTool;
use pharmakon_common::Tool;
use crate::agent::Agent;

pub struct AutoIndexHook {
    agent_ref: Arc<Mutex<Agent>>,
}

impl AutoIndexHook {
    pub fn new(agent_ref: Arc<Mutex<Agent>>) -> Self {
        Self {
            agent_ref,
        }
    }
}

#[async_trait]
impl Hook for AutoIndexHook {
    fn name(&self) -> &str { "auto-index" }

    async fn on_message_received(&self, message: &Message) -> Result<()> {
        if message.role != "user" {
            return Ok(());
        }

        let text_content = match &message.content {
            Some(MessageContent::Text(t)) => t.clone(),
            Some(MessageContent::Multimodal(parts)) => {
                parts.iter().filter_map(|p| {
                    if let ContentPart::Text { text } = p {
                        Some(text.clone())
                    } else {
                        None
                    }
                }).collect::<Vec<_>>().join(" ")
            }
            _ => return Ok(()),
        };

        let url_regex = Regex::new(r"https?://[^\s/$.?#].[^\s]*").unwrap();
        let urls: Vec<_> = url_regex.find_iter(&text_content).map(|m| m.as_str()).collect();

        if urls.is_empty() {
            return Ok(());
        }

        let agent_ref = self.agent_ref.clone();

        for url in urls {
            let url = url.to_string();
            let agent_clone = agent_ref.clone();
            
            tokio::spawn(async move {
                let tool = LinkUnderstandingTool::new();
                log::info!("Auto-indexing background task started for: {}", url);
                if let Ok(analysis) = tool.call(serde_json::json!({ "url": url })).await {
                    let fact = format!("Context from link {}: {}", url, analysis);
                    let agent = agent_clone.lock().await;
                    if let Err(e) = agent.add_fact(&fact).await {
                        log::error!("Failed to add autonomous fact: {}", e);
                    } else {
                        log::info!("Successfully auto-indexed {}", url);
                    }
                }
            });
        }

        Ok(())
    }
}
