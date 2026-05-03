use crate::persistence::DbSessionStore;
use std::sync::Arc;
use anyhow::Result;
use tokio::time::{sleep, Duration};

pub struct DeliveryQueue {
    store: Arc<DbSessionStore>,
}

impl DeliveryQueue {
    pub fn new(store: Arc<DbSessionStore>) -> Self {
        Self { store }
    }

    pub async fn run_worker<F, Fut>(&self, deliver_fn: F)
    where
        F: Fn(String, String) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<()>> + Send,
    {
        let deliver_fn = Arc::new(deliver_fn);
        loop {
            if let Ok(pending) = self.store.get_pending_deliveries().await {
                for (id, session_id, payload) in pending {
                    log::info!("Attempting redelivery for session {}: {}", session_id, id);
                    if (deliver_fn)(session_id.clone(), payload).await.is_ok() {
                        let _ = self.store.mark_delivered(id).await;
                    }
                }
            }
            sleep(Duration::from_secs(30)).await;
        }
    }
}
