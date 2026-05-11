use crate::agent::Agent;
use std::sync::Arc;
use std::time::Duration;

pub struct HeartbeatManager {
    agent: Arc<Agent>,
    interval: Duration,
}

impl HeartbeatManager {
    pub fn new(agent: Arc<Agent>, interval_minutes: u64) -> Self {
        Self {
            agent,
            interval: Duration::from_secs(interval_minutes * 60),
        }
    }

    pub async fn start(&self) {
        let agent = self.agent.clone();
        let interval = self.interval;
        let initiative_engine = super::initiative::InitiativeEngineWorker::new(agent.clone());
        let shutdown_token = self.agent.shutdown_token.clone();

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            // First tick fires immediately, we might want to skip it
            timer.tick().await;

            loop {
                timer.tick().await;
                if shutdown_token.load(std::sync::atomic::Ordering::SeqCst) {
                    log::info!("HeartbeatManager: shutdown requested, stopping");
                    break;
                }
                log::info!("HeartbeatManager: Triggering autonomous check...");

                match agent.heartbeat().await {
                    Ok(response) => {
                        if !response.contains("HEARTBEAT_OK") {
                            log::info!("HeartbeatManager: Agent performed actions: {}", response);
                        }
                    }
                    Err(e) => {
                        log::error!("HeartbeatManager: Error during heartbeat: {}", e);
                    }
                }

                // Maintenance (Memory Decay, etc.)
                if let Err(e) = agent.perform_maintenance().await {
                    log::error!("HeartbeatManager: Maintenance error: {}", e);
                }

                // Run Initiative Engine
                if let Err(e) = initiative_engine.run_initiative_cycle().await {
                    log::error!("HeartbeatManager: Initiative Engine error: {}", e);
                }
            }
        });
    }
}
