use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SessionMetadata {
    pub session_id: String,
    pub name: Option<String>,
    pub created_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub model_override: Option<String>,
    pub extra: HashMap<String, String>,
}

pub struct SessionManager {
    locks: Arc<Mutex<HashMap<String, Arc<Mutex<()>>>>>,
    metadata_cache: Arc<Mutex<HashMap<String, SessionMetadata>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(Mutex::new(HashMap::new())),
            metadata_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn get_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(session_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn get_metadata(&self, session_id: &str) -> Option<SessionMetadata> {
        let cache = self.metadata_cache.lock().await;
        cache.get(session_id).cloned()
    }

    pub async fn update_active(&self, session_id: &str) {
        let mut cache = self.metadata_cache.lock().await;
        if let Some(meta) = cache.get_mut(session_id) {
            meta.last_active_at = Utc::now();
        } else {
            cache.insert(
                session_id.to_string(),
                SessionMetadata {
                    session_id: session_id.to_string(),
                    name: None,
                    created_at: Utc::now(),
                    last_active_at: Utc::now(),
                    model_override: None,
                    extra: HashMap::new(),
                },
            );
        }
    }

    pub fn generate_slug(&self, goal: &str) -> String {
        // Simple slug generation: lowercase, remove non-alphanumeric, replace spaces with hyphens
        goal.to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .take(5)
            .collect::<Vec<_>>()
            .join("-")
    }
}
