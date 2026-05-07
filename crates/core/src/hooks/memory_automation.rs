use crate::agent::Agent;
use crate::hooks::Hook;
use crate::model::{ContentPart, Message, MessageContent};
use anyhow::Result;
use async_trait::async_trait;
use regex::Regex;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct AutoIndexHook {
    agent_ref: Arc<Mutex<Agent>>,
}

impl AutoIndexHook {
    pub fn new(agent_ref: Arc<Mutex<Agent>>) -> Self {
        Self { agent_ref }
    }
}

#[async_trait]
impl Hook for AutoIndexHook {
    fn name(&self) -> &str {
        "auto-index"
    }

    async fn on_message_received(&self, message: &Message) -> Result<()> {
        if message.role != "user" {
            return Ok(());
        }

        let text_content = match &message.content {
            Some(MessageContent::Text(t)) => t.clone(),
            Some(MessageContent::Multimodal(parts)) => parts
                .iter()
                .filter_map(|p| {
                    if let ContentPart::Text { text } = p {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join(" "),
            _ => return Ok(()),
        };

        let url_regex = Regex::new(r"https?://[^\s/$.?#].[^\s]*").unwrap();
        let urls: Vec<_> = url_regex
            .find_iter(&text_content)
            .map(|m| m.as_str())
            .collect();

        if urls.is_empty() {
            return Ok(());
        }

        let agent_ref = self.agent_ref.clone();

        for url in urls {
            let url = url.to_string();
            let agent_clone = agent_ref.clone();

            tokio::spawn(async move {
                log::info!("Auto-indexing background task started for: {}", url);
                // Tool call logic removed for now to break dependency.
                // This needs to be re-introduced by getting the tool from the agent's registry.
                log::warn!("LinkUnderstandingTool call removed due to dependency refactoring. Auto-indexing of URLs is currently disabled.");
            });
        }

        Ok(())
    }
}
