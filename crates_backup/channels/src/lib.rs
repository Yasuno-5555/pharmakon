pub mod discord;
pub mod telegram;
// pub mod slack;
pub mod whatsapp;
// pub mod matrix;
use async_trait::async_trait;

use pharmakon_core::agent::Agent;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub sender: String,
    pub content: String,
    pub channel_id: String,
}

#[async_trait]
pub trait Channel: Send + Sync {
    /// Start listening for messages on this channel
    async fn run(&self, agent: Arc<Agent>) -> anyhow::Result<()>;

    /// Send a message out through this channel
    async fn send(&self, target: &str, content: &str) -> anyhow::Result<()>;

    /// Unique identifier for this channel instance
    fn id(&self) -> &str;
}

pub struct MockChannel {
    pub id: String,
}

impl MockChannel {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string() }
    }
}

#[async_trait]
impl Channel for MockChannel {
    async fn run(&self, _agent: Arc<Agent>) -> anyhow::Result<()> {
        log::info!("MockChannel {} started.", self.id);

        // Simulating some periodic activity or just staying alive
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            log::debug!("MockChannel {} is alive.", self.id);
        }
    }

    async fn send(&self, target: &str, content: &str) -> anyhow::Result<()> {
        log::info!("MockChannel {} sending to {}: {}", self.id, target, content);
        Ok(())
    }

    fn id(&self) -> &str {
        &self.id
    }
}
