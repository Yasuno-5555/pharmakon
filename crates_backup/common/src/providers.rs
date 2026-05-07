use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait MediaProvider: Send + Sync {
    fn name(&self) -> &str;

    /// Generate an image from a prompt. Returns a URL or base64 data.
    async fn generate_image(&self, prompt: &str, size: &str) -> Result<String>;

    /// Generate music from a prompt. Returns a URL.
    async fn generate_music(&self, _prompt: &str) -> Result<String> {
        Err(anyhow::anyhow!(
            "Music generation not supported by this provider"
        ))
    }

    /// Generate video from a prompt. Returns a URL.
    async fn generate_video(&self, _prompt: &str) -> Result<String> {
        Err(anyhow::anyhow!(
            "Video generation not supported by this provider"
        ))
    }
}
