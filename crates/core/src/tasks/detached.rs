use crate::agent::Agent;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub struct DetachedTask {
    pub id: String,
    pub handle: JoinHandle<Result<String>>,
}

pub struct DetachedTaskRuntime {
    tasks: Arc<Mutex<Vec<DetachedTask>>>,
}

impl DetachedTaskRuntime {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn spawn_task(&self, id: String, agent: Arc<Mutex<Agent>>, message: String) {
        let tasks = self.tasks.clone();
        let handle = tokio::spawn(async move {
            let mut agent_lock = agent.lock().await;
            agent_lock.chat(&message).await
        });

        let mut tasks_lock = tasks.lock().await;
        tasks_lock.push(DetachedTask { id, handle });
    }
}
