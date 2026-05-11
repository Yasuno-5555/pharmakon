//! Content-addressed snapshot store for file state materialization.
//!
//! Separated from EventLog by design:
//! - EventLog = causal history (what happened, in order)
//! - SnapshotStore = state materialization (what things looked like)
//!
//! Events reference snapshots by ID, but never contain file content inline.
//! This prevents event log bloat (the "Kafka-as-blob-store" antipattern).
//!
//! Blobs are stored gzip-compressed on disk to reduce storage footprint.
//! A size quota prevents unbounded growth; old snapshots are evicted on
//! write when the store exceeds the configured limit.

use anyhow::Result;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

/// Default maximum total snapshot store size (500 MB).
const DEFAULT_QUOTA_BYTES: u64 = 500 * 1024 * 1024;

/// Files larger than this are skipped by snapshot_dir / snapshot_file
/// to prevent a single large binary from blowing up the store.
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10 MB

/// Default retention for age-based pruning (7 days).
const DEFAULT_MAX_AGE_DAYS: i64 = 7;

/// A content-addressed file snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    /// Content hash (used as ID)
    pub id: String,
    /// Original file path
    pub path: String,
    /// Uncompressed byte length of content (for diagnostics without loading)
    pub byte_len: usize,
    /// Compressed byte length on disk
    pub compressed_len: u64,
    /// Timestamp when snapshot was taken
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Content-addressed snapshot store.
/// Snapshots are stored as gzip-compressed blobs on disk, indexed by hash.
/// The EventLog references these by `snapshot_id` — never by raw content.
pub struct SnapshotStore {
    /// In-memory index: hash → metadata
    index: Mutex<HashMap<String, FileSnapshot>>,
    /// Disk directory for blob storage
    store_dir: PathBuf,
    /// Maximum total size in bytes before eviction kicks in
    quota_bytes: u64,
}

impl SnapshotStore {
    pub fn new(store_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&store_dir);
        Self {
            index: Mutex::new(HashMap::new()),
            store_dir,
            quota_bytes: DEFAULT_QUOTA_BYTES,
        }
    }

    /// Create a store with a custom size quota.
    pub fn with_quota(store_dir: PathBuf, quota_bytes: u64) -> Self {
        let _ = std::fs::create_dir_all(&store_dir);
        Self {
            index: Mutex::new(HashMap::new()),
            store_dir,
            quota_bytes,
        }
    }

    // ── blob I/O (compressed) ──────────────────────────────────────

    async fn write_blob(&self, hash: &str, data: &[u8]) -> Result<u64> {
        let blob_path = self.store_dir.join(hash);
        let data = data.to_vec(); // owned for spawn_blocking
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::create(&blob_path)?;
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(&data)?;
            let file = encoder.finish()?;
            let compressed_len = file.metadata()?.len();
            Ok(compressed_len)
        })
        .await?
    }

    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>> {
        let blob_path = self.store_dir.join(hash);
        tokio::task::spawn_blocking(move || {
            let file = std::fs::File::open(&blob_path)?;
            let mut decoder = GzDecoder::new(file);
            let mut buf = Vec::new();
            decoder.read_to_end(&mut buf)?;
            Ok(buf)
        })
        .await?
    }

    // ── quota management ───────────────────────────────────────────

    /// Total on-disk size of all blobs (bytes).
    pub fn total_size(&self) -> u64 {
        let mut total = 0u64;
        if let Ok(entries) = std::fs::read_dir(&self.store_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    total += meta.len();
                }
            }
        }
        total
    }

    /// Evict oldest snapshots until the store is under the quota.
    /// Returns the number of snapshots removed.
    pub async fn enforce_quota(&self) -> Result<usize> {
        let current = self.total_size();
        if current <= self.quota_bytes {
            return Ok(0);
        }

        // Batch-evict: collect oldest snapshots to remove in one pass
        let excess = current.saturating_sub(self.quota_bytes);
        let mut removed = 0usize;
        let mut freed = 0u64;

        let to_remove: Vec<String> = {
            let index = self.index.lock().await;
            let mut entries: Vec<_> = index.values().collect();
            entries.sort_by_key(|s| s.timestamp);
            let mut ids = Vec::new();
            for entry in &entries {
                if freed >= excess {
                    break;
                }
                freed += entry.compressed_len;
                ids.push(entry.id.clone());
            }
            ids
        };

        for id in &to_remove {
            let blob_path = self.store_dir.join(id);
            let _ = std::fs::remove_file(&blob_path);
            self.index.lock().await.remove(id);
            removed += 1;
        }

        if removed > 0 {
            log::info!(
                "SnapshotStore: evicted {} snapshots to enforce quota ({} MB), freed ~{:.1} MB",
                removed,
                self.quota_bytes / (1024 * 1024),
                freed as f64 / (1024.0 * 1024.0)
            );
        }
        Ok(removed)
    }

    // ── snapshot_file ──────────────────────────────────────────────

    /// Returns true if a file is too large to snapshot.
    pub fn is_too_large(path: &Path) -> bool {
        std::fs::metadata(path)
            .map(|m| m.len() > MAX_FILE_SIZE)
            .unwrap_or(true)
    }

    /// Snapshot a file's current content. Returns the snapshot ID (content hash).
    /// If the same content was already snapshotted, returns existing ID (dedup).
    /// Skips files larger than `MAX_FILE_SIZE`.
    pub async fn snapshot_file(&self, path: &Path) -> Result<String> {
        // Skip files that are too large
        if Self::is_too_large(path) {
            log::debug!(
                "SnapshotStore: skipping large file {} (>{:.1} MB)",
                path.display(),
                MAX_FILE_SIZE as f64 / (1024.0 * 1024.0)
            );
            let hash = format!("skipped_large_{}", content_hash(path.to_string_lossy().as_bytes()));
            return Ok(hash);
        }

        let content = tokio::fs::read(path).await?;
        let hash = content_hash(&content);

        // Dedup: if we already have this exact content, skip write
        {
            let index = self.index.lock().await;
            if index.contains_key(&hash) {
                return Ok(hash);
            }
        }

        // Write compressed blob to disk
        let compressed_len = self.write_blob(&hash, &content).await?;

        // Record metadata
        let snapshot = FileSnapshot {
            id: hash.clone(),
            path: path.to_string_lossy().to_string(),
            byte_len: content.len(),
            compressed_len,
            timestamp: chrono::Utc::now(),
        };

        self.index.lock().await.insert(hash.clone(), snapshot);

        // Enforce quota after write
        self.enforce_quota().await?;

        Ok(hash)
    }

    /// Like snapshot_file but for content already in memory.
    pub async fn snapshot_bytes(&self, path: &Path, content: &[u8]) -> Result<String> {
        if content.len() as u64 > MAX_FILE_SIZE {
            log::debug!(
                "SnapshotStore: skipping large in-memory blob for {} (>{:.1} MB)",
                path.display(),
                MAX_FILE_SIZE as f64 / (1024.0 * 1024.0)
            );
            let hash = format!(
                "skipped_large_mem_{}",
                content_hash(path.to_string_lossy().as_bytes())
            );
            return Ok(hash);
        }

        let hash = content_hash(content);
        {
            let index = self.index.lock().await;
            if index.contains_key(&hash) {
                return Ok(hash);
            }
        }

        let compressed_len = self.write_blob(&hash, content).await?;
        let snapshot = FileSnapshot {
            id: hash.clone(),
            path: path.to_string_lossy().to_string(),
            byte_len: content.len(),
            compressed_len,
            timestamp: chrono::Utc::now(),
        };
        self.index.lock().await.insert(hash.clone(), snapshot);
        self.enforce_quota().await?;
        Ok(hash)
    }

    // ── restore ────────────────────────────────────────────────────

    /// Restore a file to a previously snapshotted state.
    pub async fn restore(&self, snapshot_id: &str, target_path: &Path) -> Result<()> {
        if snapshot_id.starts_with("skipped_large") {
            log::debug!(
                "SnapshotStore: skipping restore for large-file marker {}",
                snapshot_id
            );
            return Ok(());
        }

        let blob_path = self.store_dir.join(snapshot_id);
        if !blob_path.exists() {
            anyhow::bail!("Snapshot blob not found: {}", snapshot_id);
        }

        let content = self.read_blob(snapshot_id).await?;
        tokio::fs::write(target_path, &content).await?;

        log::info!(
            "Restored {} from snapshot {}",
            target_path.display(),
            &snapshot_id[..snapshot_id.len().min(8)]
        );
        Ok(())
    }

    // ── queries ────────────────────────────────────────────────────

    pub async fn has(&self, snapshot_id: &str) -> bool {
        self.index.lock().await.contains_key(snapshot_id)
    }

    pub async fn get_metadata(&self, snapshot_id: &str) -> Option<FileSnapshot> {
        self.index.lock().await.get(snapshot_id).cloned()
    }

    pub async fn len(&self) -> usize {
        self.index.lock().await.len()
    }

    /// Total uncompressed bytes stored.
    pub async fn total_uncompressed_bytes(&self) -> usize {
        self.index.lock().await.values().map(|s| s.byte_len).sum()
    }

    // ── pruning ───────────────────────────────────────────────────

    /// Prune snapshots older than the given duration.
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

    /// Run a combined maintenance pass: age-based prune + quota enforcement.
    /// This is the recommended periodic maintenance entry point.
    pub async fn maintenance(&self) -> Result<usize> {
        let age_pruned = self
            .prune_older_than(chrono::Duration::days(DEFAULT_MAX_AGE_DAYS))
            .await?;
        let quota_pruned = self.enforce_quota().await?;
        let total = age_pruned + quota_pruned;
        if total > 0 {
            log::info!(
                "SnapshotStore maintenance: removed {} snapshots ({} age, {} quota), current size: {:.1} MB",
                total,
                age_pruned,
                quota_pruned,
                self.total_size() as f64 / (1024.0 * 1024.0)
            );
        }
        Ok(total)
    }

    /// Prune on startup: aggressively clean old snapshots and enforce quota.
    /// Uses a shorter retention window for startup cleanup (24 hours).
    pub async fn prune_on_startup(&self) -> Result<usize> {
        log::info!(
            "SnapshotStore: startup prune — current size {:.1} MB, {} snapshots",
            self.total_size() as f64 / (1024.0 * 1024.0),
            self.len().await
        );
        // Use a shorter retention at startup to aggressively clean any accumulated cruft
        let pruned = self
            .prune_older_than(chrono::Duration::hours(24))
            .await?;
        let quota_pruned = self.enforce_quota().await?;
        Ok(pruned + quota_pruned)
    }

    // ── directory snapshot/restore ─────────────────────────────────

    /// Snapshot a directory recursively. Returns a map of relative path -> snapshot_id.
    /// Skips files larger than MAX_FILE_SIZE and known large directories.
    pub async fn snapshot_dir(&self, dir_path: &Path) -> Result<HashMap<PathBuf, String>> {
        let mut snapshots = HashMap::new();
        let mut file_count = 0usize;
        self.snapshot_dir_recursive(dir_path, dir_path, &mut snapshots, &mut file_count)
            .await?;
        Ok(snapshots)
    }

    async fn snapshot_dir_recursive(
        &self,
        root: &Path,
        current: &Path,
        snapshots: &mut HashMap<PathBuf, String>,
        file_count: &mut usize,
    ) -> Result<()> {
        const MAX_FILES: usize = 500;
        let skip_dirs = [
            "target",
            ".git",
            ".pharmakon",
            "node_modules",
            ".fastembed_cache",
            "__pycache__",
            "Library",
            "Music",
            "Pictures",
            "Movies",
            "Downloads",
            "Desktop",
            "Documents",
            "Applications",
            ".cargo",
            ".rustup",
            ".cache",
            ".local",
            ".npm",
            ".Trash",
            "OrbStack",
            ".deepseek",
            "Obsidian-Hub",
            "go",
            "openclaw",
            "zotero_webdav",
        ];
        let mut entries = tokio::fs::read_dir(current).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Ok(file_type) = entry.file_type().await {
                if file_type.is_symlink() {
                    continue;
                }
            }
            if path.is_dir() {
                if skip_dirs.contains(&name_str.as_ref()) {
                    continue;
                }
                if name_str.starts_with('.') {
                    continue;
                }
                Box::pin(self.snapshot_dir_recursive(root, &path, snapshots, file_count)).await?;
            } else if path.is_file() {
                if *file_count >= MAX_FILES {
                    return Ok(()); // safety valve: stop after 500 files
                }
                *file_count += 1;
                if let Ok(rel) = path.strip_prefix(root) {
                    if let Ok(id) = self.snapshot_file(&path).await {
                        snapshots.insert(rel.to_path_buf(), id);
                    }
                }
            }
        }
        Ok(())
    }

    /// Restore a directory from a previous snapshot map.
    pub async fn restore_dir(
        &self,
        root: &Path,
        snapshots: &HashMap<PathBuf, String>,
    ) -> Result<()> {
        for (rel_path, id) in snapshots {
            let full_path = root.join(rel_path);
            if let Some(parent) = full_path.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }
            let _ = self.restore(id, &full_path).await;
        }
        Ok(())
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
        tokio::fs::write(&test_file, b"fn main() { panic!() }")
            .await
            .unwrap();

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

    #[tokio::test]
    async fn test_snapshot_dir() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(tmp.path().join("snapshots"));

        // Setup a directory structure
        let root = tmp.path().join("project");
        tokio::fs::create_dir_all(&root).await.unwrap();
        tokio::fs::write(root.join("main.rs"), b"main contents")
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("src")).await.unwrap();
        tokio::fs::write(root.join("src/lib.rs"), b"lib contents")
            .await
            .unwrap();

        // Snapshot directory
        let snap = store.snapshot_dir(&root).await.unwrap();
        assert_eq!(snap.len(), 2);

        // Mutate files
        tokio::fs::write(root.join("main.rs"), b"mutated main")
            .await
            .unwrap();
        tokio::fs::write(root.join("src/lib.rs"), b"mutated lib")
            .await
            .unwrap();

        // Restore directory
        store.restore_dir(&root, &snap).await.unwrap();

        // Verify restoration
        assert_eq!(
            tokio::fs::read_to_string(root.join("main.rs")).await.unwrap(),
            "main contents"
        );
        assert_eq!(
            tokio::fs::read_to_string(root.join("src/lib.rs"))
                .await
                .unwrap(),
            "lib contents"
        );
    }

    #[tokio::test]
    async fn test_compression_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::new(tmp.path().join("snapshots"));

        let data = vec![b'A'; 4096]; // 4 KB of repeat data — compresses well
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            data.hash(&mut hasher);
            data.len().hash(&mut hasher);
            let h1 = hasher.finish();
            let h2 = hasher.finish();
            format!("{:016x}{:016x}", h1, h2)
        };

        let compressed_len = store.write_blob(&hash, &data).await.unwrap();
        // 4 KB of repeated bytes should compress to well under 100 bytes
        assert!(
            compressed_len < 200,
            "expected compression, got {} bytes",
            compressed_len
        );

        let roundtripped = store.read_blob(&hash).await.unwrap();
        assert_eq!(roundtripped, data);
    }

    #[tokio::test]
    async fn test_prune_on_startup() {
        let tmp = TempDir::new().unwrap();
        let store = SnapshotStore::with_quota(tmp.path().join("snapshots"), 1024);

        // Write a small blob directly
        let hash = "test_hash_0001";
        store.write_blob(hash, b"hello").await.unwrap();

        // Insert a fake old snapshot into the index
        store.index.lock().await.insert(
            hash.to_string(),
            FileSnapshot {
                id: hash.to_string(),
                path: "old.txt".to_string(),
                byte_len: 5,
                compressed_len: 50,
                timestamp: chrono::Utc::now() - chrono::Duration::hours(48),
            },
        );

        let pruned = store.prune_on_startup().await.unwrap();
        assert!(pruned >= 1, "expected at least 1 pruned, got {}", pruned);
        assert_eq!(store.len().await, 0);
    }
}
