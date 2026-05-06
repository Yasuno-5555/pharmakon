use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemSample {
    pub timestamp: DateTime<Utc>,
    pub cpu_usage: f32,
    pub mem_usage_percent: f32,
    pub disk_usage_percent: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct TokenUsageStats {
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_cost_est: f32, // Estimated cost in USD
}

pub struct SystemTelemetry {
    sys: System,
    history: Vec<SystemSample>,
    max_history: usize,
}

impl SystemTelemetry {
    pub fn new(max_history: usize) -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();

        Self {
            sys,
            history: Vec::new(),
            max_history,
        }
    }

    pub fn sample(&mut self) -> SystemSample {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();

        let cpu_usage = self.sys.global_cpu_usage();
        let total_mem = self.sys.total_memory() as f32;
        let used_mem = self.sys.used_memory() as f32;
        let mem_usage_percent = (used_mem / total_mem) * 100.0;

        // Simplified disk usage (root partition)
        let disk_usage_percent = 0.0; // Placeholder for efficiency

        let sample = SystemSample {
            timestamp: Utc::now(),
            cpu_usage,
            mem_usage_percent,
            disk_usage_percent,
        };

        self.history.push(sample.clone());
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }

        sample
    }

    pub fn get_history(&self) -> Vec<SystemSample> {
        self.history.clone()
    }

    pub fn get_summary_24h(&self) -> String {
        if self.history.is_empty() {
            return "No data collected.".to_string();
        }

        let avg_cpu: f32 =
            self.history.iter().map(|s| s.cpu_usage).sum::<f32>() / self.history.len() as f32;
        let max_cpu: f32 = self.history.iter().map(|s| s.cpu_usage).fold(0.0, f32::max);

        format!(
            "Last 24h Summary: Avg CPU {:.1}%, Max CPU {:.1}%. Sample count: {}",
            avg_cpu,
            max_cpu,
            self.history.len()
        )
    }
}
