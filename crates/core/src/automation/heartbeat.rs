use std::sync::Arc;
use tokio::sync::Mutex;
use std::time::Duration;
use crate::agent::Agent;


pub struct HeartbeatManager {
    agent: Arc<Mutex<Agent>>,
    interval: Duration,
}

impl HeartbeatManager {
    pub fn new(agent: Arc<Mutex<Agent>>, interval_minutes: u64) -> Self {
        Self {
            agent,
            interval: Duration::from_secs(interval_minutes * 60),
        }
    }

    pub async fn start(&self) {
        let agent = self.agent.clone();
        let interval = self.interval;
        let initiative_engine = super::initiative::InitiativeEngineWorker::new(agent.clone());

        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            // First tick fires immediately, we might want to skip it
            timer.tick().await; 

            loop {
                timer.tick().await;
                log::info!("HeartbeatManager: Triggering autonomous check...");
                
                let mut agent_lock = agent.lock().await;
                match agent_lock.heartbeat().await {
                    Ok(response) => {
                        if !response.contains("HEARTBEAT_OK") {
                            log::info!("HeartbeatManager: Agent performed actions: {}", response);
                        }
                    }
                    Err(e) => {
                        log::error!("HeartbeatManager: Error during heartbeat: {}", e);
                    }
                }
                drop(agent_lock);

                // Run Initiative Engine
                if let Err(e) = initiative_engine.run_initiative_cycle().await {
                    log::error!("HeartbeatManager: Initiative Engine error: {}", e);
                }
            }
        });
    }
}
