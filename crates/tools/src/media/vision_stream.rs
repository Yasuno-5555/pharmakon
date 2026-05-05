use std::collections::VecDeque;
use std::path::PathBuf;
use std::fs;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VisionFrame {
    pub path: PathBuf,
    pub captured_at: DateTime<Utc>,
    pub window_title: String,
}

pub struct VisionRingBuffer {
    max_frames: usize,
    frames: VecDeque<VisionFrame>,
    storage_dir: PathBuf,
}

impl VisionRingBuffer {
    pub fn new(max_frames: usize) -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        let storage_dir = home.join(".pharmakon").join("vision_temp");
        fs::create_dir_all(&storage_dir).ok();
        
        Self {
            max_frames,
            frames: VecDeque::new(),
            storage_dir,
        }
    }

    pub async fn push_frame(&mut self, window_title: String) -> anyhow::Result<PathBuf> {
        // Take screenshot using external tool (e.g., screencapture on Mac)
        let timestamp = Utc::now().timestamp_millis();
        let filename = format!("frame_{}.jpg", timestamp);
        let path = self.storage_dir.join(filename);
        
        // Use screencapture -x (silent) -t jpg
        #[cfg(target_os = "macos")]
        {
            let mut child = tokio::process::Command::new("screencapture")
                .arg("-x")
                .arg("-t")
                .arg("jpg")
                .arg(&path)
                .spawn()?;
            child.wait().await?;
        }

        let frame = VisionFrame {
            path: path.clone(),
            captured_at: Utc::now(),
            window_title,
        };

        self.frames.push_back(frame);

        // Strictly enforce capacity and delete old files immediately
        if self.frames.len() > self.max_frames {
            if let Some(old_frame) = self.frames.pop_front() {
                if old_frame.path.exists() {
                    log::debug!("VisionStream: Deleting old frame to save space: {:?}", old_frame.path);
                    let _ = fs::remove_file(old_frame.path);
                }
            }
        }

        Ok(path)
    }

    pub fn get_recent_frames(&self) -> Vec<VisionFrame> {
        self.frames.iter().cloned().collect()
    }

    pub fn cleanup_all(&mut self) {
        while let Some(frame) = self.frames.pop_front() {
            if frame.path.exists() {
                let _ = fs::remove_file(frame.path);
            }
        }
    }
}

impl Drop for VisionRingBuffer {
    fn drop(&mut self) {
        // Emergency cleanup on shutdown
        self.cleanup_all();
    }
}
