use crate::Channel;
use anyhow::Result;
use async_trait::async_trait;
use pharmakon_core::agent::Agent;
use std::sync::Arc;

pub struct WhatsAppChannel {
    // In a real implementation, we would use a Go-bridge (whatsmeow)
    // or a dedicated Rust library.
}

impl Default for WhatsAppChannel {
    fn default() -> Self {
        Self::new()
    }
}

impl WhatsAppChannel {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    async fn run(&self, _agent: Arc<Agent>) -> Result<()> {
        log::info!("WhatsApp channel started (Stub)");
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    }

    async fn send(&self, target: &str, content: &str) -> Result<()> {
        log::info!("WhatsApp sending to {}: {}", target, content);
        Ok(())
    }

    fn id(&self) -> &str {
        "whatsapp-stub"
    }
}
