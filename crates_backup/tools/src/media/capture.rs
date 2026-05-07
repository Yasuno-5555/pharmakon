use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use screenshots::Screen;
use serde_json::{Value, json};
use std::fs;

use chrono::Local;
use nokhwa::Camera;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{CameraIndex, RequestedFormat, RequestedFormatType};

pub struct ScreenshotTool;

#[async_trait]
impl Tool for ScreenshotTool {
    fn name(&self) -> &str {
        "screenshot"
    }
    fn description(&self) -> &str {
        "Capture a screenshot of the main display."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "display_id": { "type": "integer", "description": "Display ID to capture (default: 0 for main)" }
            }
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let display_id = args["display_id"].as_u64().unwrap_or(0) as usize;
        let screens =
            Screen::all().map_err(|e| AgentError(format!("Failed to get screens: {}", e)))?;

        let screen = screens
            .get(display_id)
            .or_else(|| screens.first())
            .ok_or_else(|| AgentError("No screens found".to_string()))?;

        let image = screen
            .capture()
            .map_err(|e| AgentError(format!("Failed to capture screen: {}", e)))?;

        let home = dirs::home_dir()
            .ok_or_else(|| AgentError("Could not find home directory".to_string()))?;
        let media_dir = home.join(".pharmakon").join("media");
        fs::create_dir_all(&media_dir).map_err(|e| AgentError(e.to_string()))?;

        let filename = format!("screenshot_{}.png", Local::now().format("%Y%m%d_%H%M%S"));
        let path = media_dir.join(&filename);

        image
            .save(&path)
            .map_err(|e| AgentError(format!("Failed to save screenshot: {}", e)))?;

        Ok(format!("Screenshot saved to: {}", path.to_string_lossy()))
    }
}

pub struct CameraTool;

#[async_trait]
impl Tool for CameraTool {
    fn name(&self) -> &str {
        "camera_capture"
    }
    fn description(&self) -> &str {
        "Capture a frame from the default camera."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "camera_index": { "type": "integer", "description": "Camera index to use (default: 0)" }
            }
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let index = args["camera_index"].as_u64().unwrap_or(0) as u32;

        let mut camera = Camera::new(
            CameraIndex::Index(index),
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate),
        )
        .map_err(|e| AgentError(format!("Failed to open camera: {}", e)))?;

        camera
            .open_stream()
            .map_err(|e| AgentError(format!("Failed to open camera stream: {}", e)))?;
        let frame = camera
            .frame()
            .map_err(|e| AgentError(format!("Failed to capture camera frame: {}", e)))?;
        let decoded = frame
            .decode_image::<RgbFormat>()
            .map_err(|e| AgentError(format!("Failed to decode frame: {}", e)))?;

        let home = dirs::home_dir()
            .ok_or_else(|| AgentError("Could not find home directory".to_string()))?;
        let media_dir = home.join(".pharmakon").join("media");
        fs::create_dir_all(&media_dir).map_err(|e| AgentError(e.to_string()))?;

        let filename = format!("camera_{}.jpg", Local::now().format("%Y%m%d_%H%M%S"));
        let path = media_dir.join(&filename);

        decoded
            .save(&path)
            .map_err(|e| AgentError(format!("Failed to save camera frame: {}", e)))?;

        Ok(format!("Camera frame saved to: {}", path.to_string_lossy()))
    }
}
