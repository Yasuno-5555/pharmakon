use anyhow::{Result, anyhow};
use std::collections::HashSet;
use tokio::sync::Mutex;

pub struct TerritoryManager {
    locked_paths: Mutex<HashSet<String>>,
}

impl Default for TerritoryManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TerritoryManager {
    pub fn new() -> Self {
        Self {
            locked_paths: Mutex::new(HashSet::new()),
        }
    }

    pub async fn lock_path(&self, path: &str) -> Result<()> {
        let mut locks = self.locked_paths.lock().await;
        if locks.contains(path) {
            return Err(anyhow!(
                "Path '{}' is already being worked on by another agent.",
                path
            ));
        }
        locks.insert(path.to_string());
        Ok(())
    }

    pub async fn unlock_path(&self, path: &str) {
        let mut locks = self.locked_paths.lock().await;
        locks.remove(path);
    }

    pub async fn is_locked(&self, path: &str) -> bool {
        let locks = self.locked_paths.lock().await;
        locks.contains(path)
    }

    pub async fn get_all_locks(&self) -> Vec<String> {
        let locks = self.locked_paths.lock().await;
        locks.iter().cloned().collect()
    }
}
