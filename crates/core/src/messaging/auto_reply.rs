use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::agent::Agent;

pub enum AutoReplyResult {
    Handled(String),
    Ignored,
    Passthrough(String),
}

pub struct AutoReplyEngine {
    agent: Arc<Mutex<Agent>>,
}

impl AutoReplyEngine {
    pub fn new(agent: Arc<Mutex<Agent>>) -> Self {
        Self { agent }
    }

    pub async fn handle_message(&self, text: &str, is_group: bool, bot_id: &str) -> Result<AutoReplyResult> {
        let text = text.trim();
        
        // 1. Handle Slash Commands
        if text.starts_with('/') {
            return self.handle_slash_command(text).await;
        }

        // 2. Handle Mentions in Groups
        if is_group {
            let bot_mention = format!("<@{}>", bot_id);
            if text.contains(&bot_mention) {
                let cleaned = text.replace(&bot_mention, "").trim().to_string();
                return Ok(AutoReplyResult::Passthrough(cleaned));
            }
            return Ok(AutoReplyResult::Ignored);
        }

        // 3. Direct Message Passthrough
        Ok(AutoReplyResult::Passthrough(text.to_string()))
    }

    async fn handle_slash_command(&self, text: &str) -> Result<AutoReplyResult> {
        let parts: Vec<&str> = text.split_whitespace().collect();
        let cmd = parts[0];

        match cmd {
            "/status" => {
                Ok(AutoReplyResult::Handled("Pharmakon is active and monitoring channels.".to_string()))
            }
            "/reset" => {
                let mut agent = self.agent.lock().await;
                agent.reset_history();
                Ok(AutoReplyResult::Handled("Session history has been reset.".to_string()))
            }
            "/model" => {
                let agent = self.agent.lock().await;
                Ok(AutoReplyResult::Handled(format!("Current model: {}", agent.model.name())))
            }
            "/help" => {
                Ok(AutoReplyResult::Handled(
                    "Available commands:\n/status - Check bot status\n/reset - Reset conversation history\n/model - Show current model\n/help - Show this help"
                .to_string()))
            }
            _ => Ok(AutoReplyResult::Ignored),
        }
    }
}
