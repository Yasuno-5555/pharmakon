use anyhow::Result;
use pharmakon_common::{AgentModel, CompletionRequest, Message, MessageContent};
use std::sync::Arc;

pub struct ContextCompactor {
    model: Arc<dyn AgentModel>,
}

impl ContextCompactor {
    pub fn new(model: Arc<dyn AgentModel>) -> Self {
        Self { model }
    }

    pub async fn compact(&self, history: Vec<Message>) -> Result<Vec<Message>> {
        if history.len() < 10 {
            return Ok(history);
        }

        log::info!("Compaction: summarizing conversation");

        let mut new_history = Vec::new();
        if let Some(first) = history.first() {
            new_history.push(first.clone());
        }

        let len = history.len();
        let middle_part = &history[1..len - 5];

        let mut summary_prompt =
            String::from("Summarize the following part of a conversation concisely:\n\n");
        for msg in middle_part {
            let role = &msg.role;
            let content = match &msg.content {
                Some(c) => c.to_string(),
                None => "[No content]".to_string(),
            };
            summary_prompt.push_str(role);
            summary_prompt.push_str(": ");
            summary_prompt.push_str(&content);
            summary_prompt.push_str("\n");
        }

        let summary_res = self
            .model
            .complete(CompletionRequest {
                messages: vec![Message {
                    role: "user".to_string(),
                    content: Option::Some(MessageContent::Text(summary_prompt)),
                    ..Default::default()
                }],
                temperature: Option::Some(0.3f32),
                max_tokens: None,
                tools: None,
            })
            .await
            .map_err(|e| anyhow::Error::new(e))?;

        let summary = match summary_res.content {
            Some(c) => c.to_string(),
            None => "[Summary failed]".to_string(),
        };

        new_history.push(Message {
            role: "system".to_string(),
            content: Option::Some(MessageContent::Text(format!(
                "### CONTEXT ANCHOR ###\nThe following is a compressed semantic summary of the preceding conversation to preserve intent and state:\n{}\n######################", 
                summary
            ))),
            ..Default::default()
        });

        new_history.extend(history[len - 5..].to_vec());

        Ok(new_history)
    }
}
