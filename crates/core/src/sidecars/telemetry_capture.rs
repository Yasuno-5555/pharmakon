use chrono::Utc;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::sleep;
use xcap::Monitor;

pub struct TelemetryCaptureWorker {
    interval: Duration,
    output_dir: PathBuf,
}

impl TelemetryCaptureWorker {
    pub fn new(interval_secs: u64) -> anyhow::Result<Self> {
        let home =
            dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Could not find home directory"))?;
        let output_dir = home.join(".pharmakon").join("trajectory").join("visuals");

        if !output_dir.exists() {
            std::fs::create_dir_all(&output_dir)?;
        }

        Ok(Self {
            interval: Duration::from_secs(interval_secs),
            output_dir,
        })
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        log::info!("Starting Telemetry Capture Worker (Photographic Trajectory)...");

        // Initial monitor detection
        let monitors =
            Monitor::all().map_err(|e| anyhow::anyhow!("Failed to detect monitors: {}", e))?;

        if monitors.is_empty() {
            log::warn!("No monitors detected. Telemetry capture will be inactive.");
            return Ok(());
        }

        loop {
            for (i, monitor) in monitors.iter().enumerate() {
                match monitor.capture_image() {
                    Ok(image) => {
                        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
                        let filename = format!("capture_{}_m{}.png", timestamp, i);
                        let path = self.output_dir.join(filename);

                        // In a real implementation, we would compress to WebP here
                        if let Err(e) = image.save(&path) {
                            log::error!("Failed to save telemetry capture: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to capture monitor {}: {}", i, e);
                    }
                }
            }
            sleep(self.interval).await;
        }
    }
}
