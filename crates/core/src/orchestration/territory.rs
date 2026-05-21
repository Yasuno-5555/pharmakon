use anyhow::{Result, anyhow};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct TerritoryManager {
    locked_paths: Mutex<HashMap<String, Instant>>,
    ttl: Duration,
}

impl Default for TerritoryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerritoryManager {
    pub fn new() -> Self {
        Self {
            locked_paths: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(300), // 5 minutes TTL
        }
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            locked_paths: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    async fn clean_expired(&self, locks: &mut HashMap<String, Instant>) {
        let now = Instant::now();
        let ttl = self.ttl;
        locks.retain(|_, &mut acquired_at| now.duration_since(acquired_at) < ttl);
    }

    pub async fn lock_path(&self, path: &str) -> Result<()> {
        let mut locks = self.locked_paths.lock().await;
        self.clean_expired(&mut locks).await;
        if locks.contains_key(path) {
            return Err(anyhow!(
                "Path '{}' is already being worked on by another agent.",
                path
            ));
        }
        locks.insert(path.to_string(), Instant::now());
        Ok(())
    }

    pub async fn unlock_path(&self, path: &str) {
        let mut locks = self.locked_paths.lock().await;
        locks.remove(path);
    }

    pub async fn is_locked(&self, path: &str) -> bool {
        let mut locks = self.locked_paths.lock().await;
        self.clean_expired(&mut locks).await;
        locks.contains_key(path)
    }

    pub async fn get_all_locks(&self) -> Vec<String> {
        let mut locks = self.locked_paths.lock().await;
        self.clean_expired(&mut locks).await;
        locks.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_territory_manager_ttl() {
        let manager = TerritoryManager::with_ttl(Duration::from_millis(50));

        assert!(manager.lock_path("test_file.rs").await.is_ok());
        assert!(manager.is_locked("test_file.rs").await);

        assert!(manager.lock_path("test_file.rs").await.is_err());

        sleep(Duration::from_millis(60)).await;

        assert!(!manager.is_locked("test_file.rs").await);
        assert!(manager.lock_path("test_file.rs").await.is_ok());
    }
}
