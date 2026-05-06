use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct HealthStats {
    total_calls: u64,
    failure_count: u64,
    last_latency: Duration,
}

pub struct HealthMonitor {
    stats: Arc<Mutex<HealthStats>>,
    threshold: f32, // failure rate threshold (0.0 to 1.0)
}

impl HealthMonitor {
    pub fn new(threshold: f32) -> Self {
        Self {
            stats: Arc::new(Mutex::new(HealthStats::default())),
            threshold,
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
}
