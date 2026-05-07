use anyhow::{Result, anyhow};
use std::process::Command;

pub struct FfmpegSidecar;

impl FfmpegSidecar {
    pub fn convert_to_wav(input_path: &str, output_path: &str) -> Result<()> {
        let status = Command::new("ffmpeg")
            .arg("-i")
            .arg(input_path)
            .arg("-ar")
            .arg("16000")
            .arg("-ac")
            .arg("1")
            .arg("-y")
            .arg(output_path)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("FFmpeg conversion failed"))
        }
    }

    pub fn extract_audio(video_path: &str, audio_path: &str) -> Result<()> {
        let status = Command::new("ffmpeg")
            .arg("-i")
            .arg(video_path)
            .arg("-vn")
            .arg("-acodec")
            .arg("pcm_s16le")
            .arg("-y")
            .arg(audio_path)
            .status()?;

        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("FFmpeg audio extraction failed"))
        }
    }
}
