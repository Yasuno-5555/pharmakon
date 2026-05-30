#![allow(clippy::collapsible_if)]

use pharmakon_common::Event;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{Disks, System};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum HealthState {
    Healthy,
    Degraded,
    Critical,
    Recovering,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProbeResult {
    pub ok: bool,
    pub probe_name: String,
    pub value: f64,
    pub severity: Severity,
    pub message: String,
    pub ttl: Duration,
}

struct HealthStats {
    total_calls: u64,
    failure_count: u64,
    last_latency: Duration,
    last_state_change: Instant,
    action_cooldown: Option<Instant>,
    current_state: HealthState,
    consecutive_healthy: u32,
    last_cargo_check: Instant,
}

impl Default for HealthStats {
    fn default() -> Self {
        Self {
            total_calls: 0,
            failure_count: 0,
            last_latency: Duration::from_secs(0),
            last_state_change: Instant::now(),
            action_cooldown: None,
            current_state: HealthState::Healthy,
            consecutive_healthy: 3, // Start as healthy
            last_cargo_check: Instant::now(),
        }
    }
}

#[derive(Clone)]
pub struct HealthMonitor {
    stats: Arc<Mutex<HealthStats>>,
    event_tx: Option<broadcast::Sender<Event>>,
    threshold: f32, // failure rate threshold (0.0 to 1.0)
    pub test_mode: bool,
}

impl HealthMonitor {
    pub fn new(threshold: f32) -> Self {
        Self {
            stats: Arc::new(Mutex::new(HealthStats::default())),
            event_tx: None,
            threshold,
            test_mode: false,
        }
    }

    pub fn with_event_tx(threshold: f32, event_tx: broadcast::Sender<Event>) -> Self {
        Self {
            stats: Arc::new(Mutex::new(HealthStats::default())),
            event_tx: Some(event_tx),
            threshold,
            test_mode: false,
        }
    }

    pub fn record_success(&self, latency: Duration) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_calls += 1;
        stats.last_latency = latency;
    }

    pub fn record_failure(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.total_calls += 1;
        stats.failure_count += 1;
    }

    pub fn record_cargo_check(&self) {
        let mut stats = self.stats.lock().unwrap();
        stats.last_cargo_check = Instant::now();
    }

    pub fn is_healthy(&self) -> bool {
        let stats = self.stats.lock().unwrap();
        if stats.total_calls < 5 {
            return true;
        }
        let failure_rate = stats.failure_count as f32 / stats.total_calls as f32;
        failure_rate < self.threshold
    }

    pub fn status_report(&self) -> String {
        let stats = self.stats.lock().unwrap();
        format!(
            "Calls: {}, Failures: {}, Failure Rate: {:.1}%, Last Latency: {:?}",
            stats.total_calls,
            stats.failure_count,
            (stats.failure_count as f32 / stats.total_calls as f32) * 100.0,
            stats.last_latency
        )
    }

    // --- Heartbeat 2.0 Probers ---

    pub fn check_disk_usage(&self) -> ProbeResult {
        if self.test_mode {
            return ProbeResult {
                ok: true,
                probe_name: "disk_usage".to_string(),
                value: 100.0,
                severity: Severity::Info,
                message: "Disk space ok (mocked)".to_string(),
                ttl: Duration::from_secs(60),
            };
        }

        let disks = Disks::new_with_refreshed_list();
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".pharmakon");
        if !path.exists() {
            let _ = std::fs::create_dir_all(&path);
        }

        let mut available = 0;
        let mut total = 0;

        for disk in &disks {
            if path.starts_with(disk.mount_point()) {
                available = disk.available_space();
                total = disk.total_space();
                break;
            }
        }

        if total == 0 {
            if let Some(disk) = disks.first() {
                available = disk.available_space();
                total = disk.total_space();
            }
        }

        let pct = if total > 0 {
            (available as f64 / total as f64) * 100.0
        } else {
            100.0
        };

        let (ok, severity, msg) = if pct < 8.0 {
            (
                false,
                Severity::Critical,
                format!("Disk space critical: {:.1}% free", pct),
            )
        } else if pct < 15.0 {
            (
                false,
                Severity::Warning,
                format!("Disk space low: {:.1}% free", pct),
            )
        } else {
            (
                true,
                Severity::Info,
                format!("Disk space ok: {:.1}% free", pct),
            )
        };

        ProbeResult {
            ok,
            probe_name: "disk_usage".to_string(),
            value: pct,
            severity,
            message: msg,
            ttl: Duration::from_secs(60),
        }
    }

    pub fn check_memory_pressure(&self) -> ProbeResult {
        if self.test_mode {
            return ProbeResult {
                ok: true,
                probe_name: "memory_pressure".to_string(),
                value: 50.0,
                severity: Severity::Info,
                message: "Memory pressure ok (mocked)".to_string(),
                ttl: Duration::from_secs(30),
            };
        }

        let mut sys = System::new_all();
        sys.refresh_all();
        let pid = sysinfo::Pid::from(std::process::id() as usize);

        let rss_bytes = if let Some(proc) = sys.process(pid) {
            proc.memory()
        } else {
            0
        };
        let rss_mb = rss_bytes as f64 / (1024.0 * 1024.0);

        let (ok, severity, msg) = if rss_mb > 500.0 {
            (
                false,
                Severity::Critical,
                format!("Memory pressure critical: {:.1} MB RSS", rss_mb),
            )
        } else if rss_mb > 200.0 {
            (
                false,
                Severity::Warning,
                format!("Memory pressure warning: {:.1} MB RSS", rss_mb),
            )
        } else {
            (
                true,
                Severity::Info,
                format!("Memory pressure ok: {:.1} MB RSS", rss_mb),
            )
        };

        ProbeResult {
            ok,
            probe_name: "memory_pressure".to_string(),
            value: rss_mb,
            severity,
            message: msg,
            ttl: Duration::from_secs(30),
        }
    }

    pub fn check_task_queue_lag(&self, active_tasks: usize) -> ProbeResult {
        let val = active_tasks as f64;
        let (ok, severity, msg) = if val > 50.0 {
            (
                false,
                Severity::Critical,
                format!("Task queue lag critical: {} active tasks", active_tasks),
            )
        } else if val > 10.0 {
            (
                false,
                Severity::Warning,
                format!("Task queue lag warning: {} active tasks", active_tasks),
            )
        } else {
            (
                true,
                Severity::Info,
                format!("Task queue lag ok: {} active tasks", active_tasks),
            )
        };

        ProbeResult {
            ok,
            probe_name: "task_queue_lag".to_string(),
            value: val,
            severity,
            message: msg,
            ttl: Duration::from_secs(30),
        }
    }

    pub fn check_snapshot_quota(&self) -> ProbeResult {
        if self.test_mode {
            return ProbeResult {
                ok: true,
                probe_name: "snapshot_quota".to_string(),
                value: 0.0,
                severity: Severity::Info,
                message: "Snapshot quota ok (mocked)".to_string(),
                ttl: Duration::from_secs(60),
            };
        }

        let mut total_size = 0u64;
        let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push(".pharmakon");
        path.push("snapshots");

        if let Ok(entries) = std::fs::read_dir(&path) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    total_size += meta.len();
                }
            }
        }

        let quota_bytes = 500 * 1024 * 1024; // 500 MB default
        let pct = (total_size as f64 / quota_bytes as f64) * 100.0;

        let (ok, severity, msg) = if pct > 85.0 {
            (
                false,
                Severity::Critical,
                format!("Snapshot quota critical: {:.1}%", pct),
            )
        } else if pct > 60.0 {
            (
                false,
                Severity::Warning,
                format!("Snapshot quota warning: {:.1}%", pct),
            )
        } else {
            (
                true,
                Severity::Info,
                format!("Snapshot quota ok: {:.1}%", pct),
            )
        };

        ProbeResult {
            ok,
            probe_name: "snapshot_quota".to_string(),
            value: pct,
            severity,
            message: msg,
            ttl: Duration::from_secs(60),
        }
    }

    pub fn check_llm_success(&self) -> ProbeResult {
        if self.test_mode {
            return ProbeResult {
                ok: true,
                probe_name: "last_llm_success".to_string(),
                value: 100.0,
                severity: Severity::Info,
                message: "LLM success rate ok (mocked)".to_string(),
                ttl: Duration::from_secs(30),
            };
        }

        let stats = self.stats.lock().unwrap();
        let rate = if stats.total_calls > 0 {
            ((stats.total_calls - stats.failure_count) as f64 / stats.total_calls as f64) * 100.0
        } else {
            100.0
        };

        let (ok, severity, msg) = if rate < 50.0 && stats.total_calls >= 5 {
            (
                false,
                Severity::Critical,
                format!("LLM success rate critical: {:.1}%", rate),
            )
        } else if rate < 80.0 && stats.total_calls >= 5 {
            (
                false,
                Severity::Warning,
                format!("LLM success rate warning: {:.1}%", rate),
            )
        } else {
            (
                true,
                Severity::Info,
                format!("LLM success rate ok: {:.1}%", rate),
            )
        };

        ProbeResult {
            ok,
            probe_name: "last_llm_success".to_string(),
            value: rate,
            severity,
            message: msg,
            ttl: Duration::from_secs(30),
        }
    }

    pub fn check_cargo_check_stale(&self) -> ProbeResult {
        if self.test_mode {
            return ProbeResult {
                ok: true,
                probe_name: "cargo_check_stale".to_string(),
                value: 0.0,
                severity: Severity::Info,
                message: "Cargo check stale ok (mocked)".to_string(),
                ttl: Duration::from_secs(60),
            };
        }

        let elapsed = self.stats.lock().unwrap().last_cargo_check.elapsed();
        let minutes = elapsed.as_secs() as f64 / 60.0;

        let (ok, severity, msg) = if minutes > 120.0 {
            (
                false,
                Severity::Critical,
                format!(
                    "Cargo check stale critical: {:.1} minutes since last check",
                    minutes
                ),
            )
        } else if minutes > 30.0 {
            (
                false,
                Severity::Warning,
                format!(
                    "Cargo check stale warning: {:.1} minutes since last check",
                    minutes
                ),
            )
        } else {
            (
                true,
                Severity::Info,
                format!(
                    "Cargo check stale ok: {:.1} minutes since last check",
                    minutes
                ),
            )
        };

        ProbeResult {
            ok,
            probe_name: "cargo_check_stale".to_string(),
            value: minutes,
            severity,
            message: msg,
            ttl: Duration::from_secs(60),
        }
    }

    // --- State Machine & Auto-reremedy with Cooldown ---

    pub fn update_state(&self, active_tasks: usize) -> HealthState {
        let probes = vec![
            self.check_disk_usage(),
            self.check_memory_pressure(),
            self.check_task_queue_lag(active_tasks),
            self.check_snapshot_quota(),
            self.check_llm_success(),
            self.check_cargo_check_stale(),
        ];

        let mut max_severity = Severity::Info;
        for probe in &probes {
            match probe.severity {
                Severity::Critical => max_severity = Severity::Critical,
                Severity::Warning if max_severity != Severity::Critical => {
                    max_severity = Severity::Warning
                }
                _ => {}
            }
        }

        let mut stats = self.stats.lock().unwrap();
        let prev_state = stats.current_state;

        let next_state = match max_severity {
            Severity::Critical => {
                stats.consecutive_healthy = 0;
                HealthState::Critical
            }
            Severity::Warning => {
                stats.consecutive_healthy = 0;
                match prev_state {
                    HealthState::Critical => HealthState::Recovering,
                    _ => HealthState::Degraded,
                }
            }
            Severity::Info => match prev_state {
                HealthState::Healthy => HealthState::Healthy,
                _ => {
                    stats.consecutive_healthy += 1;
                    if stats.consecutive_healthy >= 3 {
                        HealthState::Healthy
                    } else {
                        HealthState::Recovering
                    }
                }
            },
        };

        if next_state != prev_state {
            stats.current_state = next_state;
            stats.last_state_change = Instant::now();
            let msg = format!("Health transition: {:?} -> {:?}", prev_state, next_state);
            log::info!("{}", msg);
            if let Some(tx) = &self.event_tx {
                let _ = tx.send(Event::SystemLog {
                    level: "WARN".to_string(),
                    message: msg,
                });
            }
        }

        next_state
    }

    pub fn get_current_state(&self) -> HealthState {
        self.stats.lock().unwrap().current_state
    }

    pub fn trigger_auto_remedy_if_needed(&self) {
        let current_state = self.get_current_state();
        if current_state == HealthState::Critical || current_state == HealthState::Degraded {
            let mut stats = self.stats.lock().unwrap();
            let now = Instant::now();
            if let Some(cooldown) = stats.action_cooldown {
                if now.duration_since(cooldown) < Duration::from_secs(300) {
                    return;
                }
            }

            stats.action_cooldown = Some(now);
            log::warn!("HealthMonitor: Triggering auto-remedy cooldown action...");
            tokio::spawn(async move {
                let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                path.push(".pharmakon");
                path.push("snapshots");
                if let Ok(entries) = std::fs::read_dir(&path) {
                    let mut files: Vec<_> = entries.flatten().collect();
                    if files.len() > 10 {
                        files.sort_by_key(|f| f.metadata().and_then(|m| m.modified()).ok());
                        for f in files.iter().take(5) {
                            let _ = std::fs::remove_file(f.path());
                        }
                        log::info!(
                            "HealthMonitor Auto-Remedy: Pruned 5 oldest snapshots to free space."
                        );
                    }
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_probers_execution() {
        let mut monitor = HealthMonitor::new(0.3);
        monitor.test_mode = true;
        assert_eq!(monitor.check_disk_usage().probe_name, "disk_usage");
        assert_eq!(
            monitor.check_memory_pressure().probe_name,
            "memory_pressure"
        );
        assert_eq!(monitor.check_task_queue_lag(5).probe_name, "task_queue_lag");
        assert_eq!(monitor.check_snapshot_quota().probe_name, "snapshot_quota");
        assert_eq!(monitor.check_llm_success().probe_name, "last_llm_success");
        assert_eq!(
            monitor.check_cargo_check_stale().probe_name,
            "cargo_check_stale"
        );
    }

    #[test]
    fn test_hysteresis_and_state_transition() {
        let mut monitor = HealthMonitor::new(0.3);
        monitor.test_mode = true;
        assert_eq!(monitor.get_current_state(), HealthState::Healthy);

        // Warning trigger: Degraded
        let state = monitor.update_state(15);
        assert_eq!(state, HealthState::Degraded);
        assert_eq!(monitor.get_current_state(), HealthState::Degraded);

        // Healthy input: Needs 3 consecutive healthy samples to transition back to Healthy
        let state = monitor.update_state(0);
        assert_eq!(state, HealthState::Recovering);

        let state = monitor.update_state(0);
        assert_eq!(state, HealthState::Recovering);

        let state = monitor.update_state(0);
        assert_eq!(state, HealthState::Healthy);
    }
}
