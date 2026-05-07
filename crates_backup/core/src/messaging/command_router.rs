use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::agent::Agent;

pub struct CommandRouter;

impl CommandRouter {
    pub async fn handle_command(agent: &mut Agent, text: &str) -> Result<Option<String>> {
        if !text.starts_with('/') {
            return Ok(None);
        }

        let parts: Vec<&str> = text.split_whitespace().collect();
        let command = parts[0];
        let args = &parts[1..];

        match command {
            "/status" => {
                let status = format!(
                    "🦀 **Pharmakon Status**\n- Agent: {}\n- Tools: {}\n- Memory size: {}\n- Session: {}",
                    agent.id,
                    agent.tools.len(),
                    agent.fact_memory.lock().await.facts.len(),
                    agent.session_id
                );
                Ok(Some(status))
            }
            "/help" => {
                let help = "Available commands:\n/status - Show system status\n/clear - Clear current session memory\n/help - Show this message";
                Ok(Some(help.to_string()))
            }
            "/clear" => {
                agent.clear_memory().await?;
                Ok(Some("Memory cleared successfully.".to_string()))
            }
            _ => Ok(Some(format!("Unknown command: {}. Type /help for available commands.", command))),
        }
    }
}
