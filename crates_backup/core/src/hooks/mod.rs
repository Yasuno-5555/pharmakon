pub mod memory_automation;
pub mod token_economy;

use crate::model::Message;
use anyhow::Result;
use async_trait::async_trait;
use pharmakon_common::Event;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct HookContext {
    pub agent: Arc<crate::agent::Agent>,
}

#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;

    // Generic event hook
    async fn on_event(&self, _ctx: &mut HookContext, _event: &Event) -> Result<()> {
        Ok(())
    }

    // Message hooks
    async fn on_message_received(&self, _message: &Message) -> Result<()> {
        Ok(())
    }
    async fn on_message_sent(&self, _message: &Message) -> Result<()> {
        Ok(())
    }

    // Tool hooks
    async fn before_tool_call(&self, _tool_name: &str, _args: &Value) -> Result<()> {
        Ok(())
    }
    async fn after_tool_call(&self, _tool_name: &str, _result: &str) -> Result<()> {
        Ok(())
    }

    // Agent hooks
    async fn on_agent_thinking(&self, _session_id: &str) -> Result<()> {
        Ok(())
    }
    async fn on_reflection_complete(&self, _insights: &[String]) -> Result<()> {
        Ok(())
    }
    async fn on_context_recovered(&self, _context: &str) -> Result<()> {
        Ok(())
    }
    async fn on_session_switched(&self, _old_id: &str, _new_id: &str) -> Result<()> {
        Ok(())
    }
}

pub struct HookRegistry {
    hooks: Mutex<Vec<Box<dyn Hook>>>,
}

impl Default for HookRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl HookRegistry {
    pub fn new() -> Self {
        Self {
            hooks: Mutex::new(Vec::new()),
        }
    }

    pub fn register(&mut self, hook: Box<dyn Hook>) {
        self.hooks.get_mut().push(hook);
    }

    pub async fn trigger_event(
        &self,
        agent: Arc<crate::agent::Agent>,
        event: &Event,
    ) -> Result<()> {
        let mut ctx = HookContext { agent };
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.on_event(&mut ctx, event).await?;
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

    pub async fn trigger_context_recovered(&self, context: &str) -> Result<()> {
        let hooks = self.hooks.lock().await;
        for hook in hooks.iter() {
            hook.on_context_recovered(context).await?;
        }
        Ok(())
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
}
