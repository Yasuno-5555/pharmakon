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
        // ... (existing code remains same) ...
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

        let mut summary_prompt = String::from(
            "Compress the following conversation into a HIGH-DENSITY semantic summary. \
                          Focus on key facts, user preferences, current goals, and finalized decisions. \
                          Use a structural format if possible. Avoid conversational filler.\n\n",
        );
        for msg in middle_part {
            let role = &msg.role;
            let content = match &msg.content {
                Some(c) => c.to_string(),
                None => "[No content]".to_string(),
            };
            summary_prompt.push_str(role);
            summary_prompt.push_str(": ");
            summary_prompt.push_str(&content);
            summary_prompt.push('\n');
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
                complexity: None,
                system_instruction: None,
            })
            .await
            .map_err(anyhow::Error::new)?;

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

    /// Sparse Encoding: Compresses a large block of text into a high-density structural summary.
    pub async fn compact_block(&self, content: &str, target_density: f32) -> Result<String> {
        let prompt = format!(
            "REDUCE the following content to a HIGH-DENSITY structural representation (Sparse Encoding). \
            Identify key symbols, inputs, outputs, risks, and core logic. Use a key-value style. \
            Target Compression Ratio: {:.1}:1. \n\nCONTENT:\n{}",
            1.0 / target_density.max(0.1),
            content
        );

        let res = self
            .model
            .complete(CompletionRequest {
                messages: vec![Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Text(prompt)),
                    ..Default::default()
                }],
                temperature: Some(0.2),
                max_tokens: None,
                tools: None,
                complexity: None,
                system_instruction: None,
            })
            .await
            .map_err(anyhow::Error::new)?;

        Ok(res
            .content
            .map(|c| c.to_string())
            .unwrap_or_else(|| "[Compression failed]".to_string()))
    }
}
