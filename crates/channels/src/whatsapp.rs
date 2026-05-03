use async_trait::async_trait;
use anyhow::Result;
use crate::Channel;
use pharmakon_core::agent::Agent;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct WhatsAppChannel {
    // In a real implementation, we would use a Go-bridge (whatsmeow) 
    // or a dedicated Rust library.
}

impl WhatsAppChannel {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Channel for WhatsAppChannel {
    async fn run(&self, _agent: Arc<Mutex<Agent>>) -> Result<()> {
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
