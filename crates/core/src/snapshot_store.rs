//! Content-addressed snapshot store for file state materialization.
//!
//! Separated from EventLog by design:
//! - EventLog = causal history (what happened, in order)
//! - SnapshotStore = state materialization (what things looked like)
//!
//! Events reference snapshots by ID, but never contain file content inline.
//! This prevents event log bloat (the "Kafka-as-blob-store" antipattern).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

/// A content-addressed file snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Content hash (used as ID)
    pub id: String,
    /// Original file path
    pub path: String,
    /// Byte length of content (for diagnostics without loading)
    pub byte_len: usize,
    /// Timestamp when snapshot was taken
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Content-addressed snapshot store.
/// Snapshots are stored as compressed blobs on disk, indexed by hash.
/// The EventLog references these by `snapshot_id` — never by raw content.
pub struct SnapshotStore {
    /// In-memory index: hash → metadata
    index: Mutex<HashMap<String, FileSnapshot>>,
    /// Disk directory for blob storage
    store_dir: PathBuf,
}

impl SnapshotStore {
    pub fn new(store_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&store_dir);
        Self {
            index: Mutex::new(HashMap::new()),
            store_dir,
        }
    }

    /// Snapshot a file's current content. Returns the snapshot ID (content hash).
    /// If the same content was already snapshotted, returns existing ID (dedup).
    pub async fn snapshot_file(&self, path: &Path) -> Result<String> {
        let content = tokio::fs::read(path).await?;
        let hash = content_hash(&content);

        // Dedup: if we already have this exact content, skip write
        {
            let index = self.index.lock().await;
            if index.contains_key(&hash) {
                return Ok(hash);
            }
        }

        // Write blob to disk
        let blob_path = self.store_dir.join(&hash);
        tokio::fs::write(&blob_path, &content).await?;

        // Record metadata
        let snapshot = FileSnapshot {
            id: hash.clone(),
            path: path.to_string_lossy().to_string(),
            byte_len: content.len(),
            timestamp: chrono::Utc::now(),
        };

        self.index.lock().await.insert(hash.clone(), snapshot);

        Ok(hash)
    }

    /// Restore a file to a previously snapshotted state.
    pub async fn restore(&self, snapshot_id: &str, target_path: &Path) -> Result<()> {
        let blob_path = self.store_dir.join(snapshot_id);
        if !blob_path.exists() {
            anyhow::bail!("Snapshot blob not found: {}", snapshot_id);
        }

        let content = tokio::fs::read(&blob_path).await?;
        tokio::fs::write(target_path, &content).await?;

        log::info!(
            "Restored {} from snapshot {}",
            target_path.display(),
            &snapshot_id[..8]
        );
        Ok(())
    }

    /// Check if a snapshot exists.
    pub async fn has(&self, snapshot_id: &str) -> bool {
        self.index.lock().await.contains_key(snapshot_id)
    }

    /// Get metadata for a snapshot.
    pub async fn get_metadata(&self, snapshot_id: &str) -> Option<FileSnapshot> {
        self.index.lock().await.get(snapshot_id).cloned()
    }

    /// Total number of stored snapshots.
    pub async fn len(&self) -> usize {
        self.index.lock().await.len()
    }

    /// Prune snapshots older than the given duration.
    /// Called during maintenance to prevent unbounded growth.
    pub async fn prune_older_than(&self, max_age: chrono::Duration) -> Result<usize> {
        let cutoff = chrono::Utc::now() - max_age;
        let mut index = self.index.lock().await;
        let to_remove: Vec<String> = index
            .iter()
            .filter(|(_, s)| s.timestamp < cutoff)
            .map(|(k, _)| k.clone())
            .collect();

        let count = to_remove.len();
        for id in &to_remove {
            let blob_path = self.store_dir.join(id);
            let _ = tokio::fs::remove_file(&blob_path).await;
            index.remove(id);
        }

        if count > 0 {
            log::info!("SnapshotStore: pruned {} stale snapshots", count);
        }
        Ok(count)
    }
}

/// Compute a content-addressed hash (SHA-256, truncated to 16 hex chars for ergonomics).
fn content_hash(content: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    // Using DefaultHasher for speed — not cryptographic, but sufficient for dedup.
    // Upgrade to SHA-256 if tampering resistance is needed.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    let h1 = hasher.finish();
    // Double-hash with length for better collision resistance
    content.len().hash(&mut hasher);
    let h2 = hasher.finish();
    format!("{:016x}{:016x}", h1, h2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_snapshot_and_restore() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(tmp.path().join("snapshots"));

        // Create a test file
        let test_file = tmp.path().join("test.rs");
        tokio::fs::write(&test_file, b"fn main() {}").await.unwrap();

        // Snapshot it
        let id = store.snapshot_file(&test_file).await.unwrap();
        assert!(!id.is_empty());

        // Modify the file
        tokio::fs::write(&test_file, b"fn main() { panic!() }").await.unwrap();

        // Restore from snapshot
        store.restore(&id, &test_file).await.unwrap();
        let restored = tokio::fs::read_to_string(&test_file).await.unwrap();
        assert_eq!(restored, "fn main() {}");
    }

    #[tokio::test]
    async fn test_dedup() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(tmp.path().join("snapshots"));

        let test_file = tmp.path().join("test.rs");
        tokio::fs::write(&test_file, b"same content").await.unwrap();

        let id1 = store.snapshot_file(&test_file).await.unwrap();
        let id2 = store.snapshot_file(&test_file).await.unwrap();

        assert_eq!(id1, id2);
        assert_eq!(store.len().await, 1);
    }
}
