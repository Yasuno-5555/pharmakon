use crate::agent::Agent;
use anyhow::{Result, anyhow};
use pharmakon_common::CronJobInfo;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;
use tokio_cron_scheduler::{Job, JobScheduler};
use uuid::Uuid;

pub struct CronJobData {
    pub id: Uuid,
    pub schedule_type: String,
    pub expr: String,
    pub message: String,
}

pub struct CronManager {
    scheduler: JobScheduler,
    jobs: Arc<Mutex<HashMap<Uuid, CronJobData>>>,
}

impl CronManager {
    pub async fn new() -> Result<Self> {
        let scheduler = JobScheduler::new().await?;
        scheduler.start().await?;
        Ok(Self {
            scheduler,
            jobs: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn add_agent_job(
        &self,
        schedule: &str,
        agent: Weak<Agent>,
        message: String,
    ) -> Result<Uuid> {
        let jobs_arc = self.jobs.clone();
        let closure_message = message.clone();

        let job = Job::new_async(schedule, move |uuid, _l| {
            let agent = agent.clone();
            let msg = closure_message.clone();
            let jobs_clone = jobs_arc.clone();
            Box::pin(async move {
                if let Some(agent_arc) = agent.upgrade() {
                    log::info!("Cron job triggered: {}", msg);
                    if let Err(e) = agent_arc.chat(&msg).await {
                        log::error!("Error in cron agent job: {}", e);
                    }
                } else {
                    log::warn!("Agent dropped, skipping cron job: {}", msg);
                    // Ideally we should remove the job from the scheduler here if the agent is dead
                    let mut jobs_lock = jobs_clone.lock().await;
                    jobs_lock.remove(&uuid);
                }
            })
        })?;

        let id = self.scheduler.add(job).await?;

        self.jobs.lock().await.insert(
            id,
            CronJobData {
                id,
                schedule_type: "cron".to_string(),
                expr: schedule.to_string(),
                message: message.clone(),
            },
        );

        Ok(id)
    }

    pub async fn add_one_shot(
        &self,
        delay_secs: u64,
        agent: Weak<Agent>,
        message: String,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let jobs_arc = self.jobs.clone();

        self.jobs.lock().await.insert(
            id,
            CronJobData {
                id,
                schedule_type: "delay".to_string(),
                expr: delay_secs.to_string(),
                message: message.clone(),
            },
        );

        tokio::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
            if let Some(agent_arc) = agent.upgrade() {
                // Check if job wasn't cancelled
                let exists = jobs_arc.lock().await.contains_key(&id);
                if exists {
                    log::info!("One-shot cron job triggered: {}", message);
                    let _ = agent_arc.chat(&message).await;
                    // Remove after execution
                    jobs_arc.lock().await.remove(&id);
                } else {
                    log::info!("One-shot job {} was cancelled, skipping execution.", id);
                }
            } else {
                log::warn!("Agent dropped, skipping one-shot job: {}", message);
                jobs_arc.lock().await.remove(&id);
            }
        });

        Ok(id)
    }

    pub async fn list_jobs(&self) -> Vec<CronJobInfo> {
        let jobs_lock = self.jobs.lock().await;
        jobs_lock
            .values()
            .map(|data| CronJobInfo {
                id: data.id.to_string(),
                schedule_type: data.schedule_type.clone(),
                expr: data.expr.clone(),
                message: data.message.clone(),
            })
            .collect()
    }

    pub async fn cancel_job(&self, id_str: &str) -> Result<()> {
        let id = Uuid::parse_str(id_str).map_err(|e| anyhow!("Invalid UUID: {}", e))?;

        let mut jobs_lock = self.jobs.lock().await;
        if let Some(data) = jobs_lock.remove(&id) {
            if data.schedule_type == "cron" {
                self.scheduler.remove(&id).await?;
            }
            // For delay tasks, removing it from the map is enough to prevent execution
            // since the spawned task checks the map before running.
            Ok(())
        } else {
            Err(anyhow!("Job {} not found", id))
        }
    }
}
