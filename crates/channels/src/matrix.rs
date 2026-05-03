use async_trait::async_trait;
use anyhow::Result;
use crate::Channel;
use pharmakon_core::agent::Agent;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct MatrixChannel {
    pub home_server: String,
}

impl MatrixChannel {
    pub fn new(home_server: String) -> Self {
        Self { home_server }
    }
}

#[async_trait]
impl Channel for MatrixChannel {
    async fn run(&self, _agent: Arc<Mutex<Agent>>) -> anyhow::Result<()> {
        log::info!("Matrix channel started for {} (Stub)", self.home_server);
        // Real implementation would use matrix-sdk
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(3600)).await;
        }
    }

    async fn send(&self, target: &str, content: &str) -> anyhow::Result<()> {
        log::info!("Matrix sending to {}: {}", target, content);
        Ok(())
    }

    fn id(&self) -> &str {
        "matrix-stub"
    }
}
