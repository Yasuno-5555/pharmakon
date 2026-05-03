pub mod memory_automation;
use async_trait::async_trait;
use crate::model::Message;
use serde_json::Value;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    
    // Message hooks
    async fn on_message_received(&self, _message: &Message) -> Result<()> { Ok(()) }
    async fn on_message_sent(&self, _message: &Message) -> Result<()> { Ok(()) }
    
    // Tool hooks
    async fn before_tool_call(&self, _tool_name: &str, _args: &Value) -> Result<()> { Ok(()) }
    async fn after_tool_call(&self, _tool_name: &str, _result: &str) -> Result<()> { Ok(()) }
    
    // Agent hooks
    async fn on_agent_thinking(&self, _session_id: &str) -> Result<()> { Ok(()) }
}

pub struct HookRegistry {
    hooks: Mutex<Vec<Arc<dyn Hook>>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Mutex::new(Vec::new()) }
    }

    pub async fn register(&self, hook: Arc<dyn Hook>) {
        self.hooks.lock().await.push(hook);
    }

    pub async fn trigger_message_received(&self, message: &Message) -> Result<()> {
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.on_message_received(message).await?;
        }
        Ok(())
    }

    pub async fn trigger_message_sent(&self, message: &Message) -> Result<()> {
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.on_message_sent(message).await?;
        }
        Ok(())
    }

    pub async fn trigger_before_tool_call(&self, tool_name: &str, args: &Value) -> Result<()> {
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.before_tool_call(tool_name, args).await?;
        }
        Ok(())
    }

    pub async fn trigger_after_tool_call(&self, tool_name: &str, result: &str) -> Result<()> {
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.after_tool_call(tool_name, result).await?;
        }
        Ok(())
    }

    pub async fn trigger_agent_thinking(&self, session_id: &str) -> Result<()> {
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.on_agent_thinking(session_id).await?;
        }
        Ok(())
    }
}
