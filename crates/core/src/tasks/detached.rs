#![allow(clippy::type_complexity)]

use crate::agent::Agent;
use anyhow::Result;
use pharmakon_common::Event;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};

pub struct DetachedTask {
    pub id: String,
    pub description: String,
}

pub struct DetachedTaskRuntime {
    tasks: Arc<Mutex<Vec<(String, String, tokio::task::JoinHandle<Result<String>>)>>>,
    event_tx: broadcast::Sender<Event>,
    last_report: Arc<Mutex<Option<std::time::Instant>>>,
}

impl Default for DetachedTaskRuntime {
    fn default() -> Self {
        Self::new(broadcast::channel(100).0)
    }
}

impl DetachedTaskRuntime {
    pub fn new(event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            event_tx,
            last_report: Arc::new(Mutex::new(None)),
        }
    }

    /// Spawn a background task with error boundary and result monitoring.
    pub async fn spawn_task(
        &self,
        id: String,
        agent: Arc<Mutex<Agent>>,
        message: String,
        description: String,
    ) {
        let tasks = self.tasks.clone();
        let event_tx = self.event_tx.clone();
        let desc = description.clone();
        let desc_clone1 = desc.clone();
        let id_clone = id.clone();

        let handle = tokio::spawn(async move {
            let agent_lock = agent.lock().await;
            match agent_lock.chat(&message).await {
                Ok(response) => {
                    log::info!("Detached task '{}' completed", desc);
                    let _ = event_tx.send(Event::AgentResponse {
                        content: pharmakon_common::MessageContent::Text(format!(
                            "[Background task: {}]\n{}",
                            desc,
                            response.chars().take(200).collect::<String>()
                        )),
                    });
                    Ok(response)
                }
                Err(e) => {
                    log::error!("Detached task '{}' failed: {}", desc, e);
                    let _ = event_tx.send(Event::Error {
                        message: format!("Background task '{}' failed: {}", desc, e),
                    });
                    Err(e)
                }
            }
        });

        // Register the handle so reap_finished() can monitor it
        {
            let mut tasks_lock = tasks.lock().await;
            tasks_lock.push((id, description, handle));
        }

        // Monitor the spawned task for panics (tokio::spawn returns JoinHandle, JoinError on panic)
        let event_tx_clone = self.event_tx.clone();
        let tasks_clone = self.tasks.clone();
        tokio::spawn(async move {
            // Briefly yield to let the task register itself
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            // Find and extract the handle from our task list
            let handle_opt = {
                let mut tasks_lock = tasks_clone.lock().await;
                let idx = tasks_lock.iter().position(|(id2, _, _)| id2 == &id_clone);
                idx.map(|i| {
                    let (_, _, h) = tasks_lock.remove(i);
                    h
                })
            };

            let Some(handle) = handle_opt else { return };

            match tokio::time::timeout(std::time::Duration::from_secs(300), handle).await {
                Ok(Ok(_)) => {} // completed normally, already handled inside
                Ok(Err(join_err)) => {
                    let msg = if join_err.is_panic() {
                        "panicked"
                    } else {
                        "cancelled"
                    };
                    log::error!("Detached task '{}' {}", desc_clone1, msg);
                    let _ = event_tx_clone.send(Event::Error {
                        message: format!("Background task '{}' {}", desc_clone1, msg),
                    });
                }
                Err(_) => {
                    log::warn!("Detached task '{}' timed out after 300s", desc_clone1);
                    let _ = event_tx_clone.send(Event::Error {
                        message: format!("Background task '{}' timed out", desc_clone1),
                    });
                }
            }
        });
    }

    /// Poll all active handles: remove finished, return running count.
    /// Called periodically by the heartbeat or maintenance cycle.
    pub async fn reap_finished(&self) -> usize {
        let mut tasks_lock = self.tasks.lock().await;
        tasks_lock.retain(|(_, _, _h)| !_h.is_finished());
        tasks_lock.len()
    }

    /// Get list of currently running tasks.
    pub async fn active_tasks(&self) -> Vec<DetachedTask> {
        let tasks_lock = self.tasks.lock().await;
        tasks_lock
            .iter()
            .map(|(id, desc, _h)| DetachedTask {
                id: id.clone(),
                description: desc.clone(),
            })
            .collect()
    }

    pub async fn active_tasks_count(&self) -> usize {
        self.reap_finished().await
    }

    /// Emit telemetry if the running task set has changed since last report.
    pub async fn emit_telemetry_if_changed(&self) {
        let count = self.reap_finished().await;
        let mut last = self.last_report.lock().await;
        let should_report = last.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(60));
        if should_report && count > 0 {
            *last = Some(std::time::Instant::now());
            let _ = self.event_tx.send(Event::SystemLog {
                level: "info".to_string(),
                message: format!("Background tasks running: {}", count),
            });
        }
    }
}
