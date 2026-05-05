use anyhow::Result;
use crate::model::Message;
use std::sync::Arc;
use crate::agent::Agent;
use tokio::sync::Mutex;

pub struct Crestodian;

impl Crestodian {
    pub async fn rescue(agent: Arc<Mutex<Agent>>, error_message: &str) -> Result<String> {
        log::warn!("Crestodian attempting rescue for error: {}", error_message);
        
        let mut agent_lock = agent.lock().await;
        
        // Strategy: Add a system message explaining the failure and asking the agent to try a different approach
        let rescue_prompt = format!(
            "CRITICAL SYSTEM NOTE: The previous action failed with the following error: '{}'. \
            Please analyze why it failed and suggest or attempt an alternative approach to fulfill the user's request.",
            error_message
        );
        
        let rescue_msg = Message {
            role: "system".to_string(),
            content: Some(::pharmakon_common::MessageContent::Text(rescue_prompt)),
            ..Default::default()
        };
        
        agent_lock.history.push(rescue_msg);
        
        // Re-trigger the chat loop
        agent_lock.chat("Please continue based on the rescue note.").await
    }
}
