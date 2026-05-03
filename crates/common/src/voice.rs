use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait SpeechToText: Send + Sync {
    async fn transcribe(&self, audio_data: Vec<u8>) -> Result<String>;
}

#[async_trait]
pub trait TextToSpeech: Send + Sync {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>>;
}

#[async_trait]
pub trait StreamedSpeechToText: Send + Sync {
    async fn transcribe_stream(&self, audio_rx: tokio::sync::mpsc::Receiver<Vec<u8>>) -> Result<tokio::sync::mpsc::Receiver<String>>;
}

#[async_trait]
pub trait StreamedTextToSpeech: Send + Sync {
    async fn synthesize_stream(&self, text_rx: tokio::sync::mpsc::Receiver<String>) -> Result<tokio::sync::mpsc::Receiver<Vec<u8>>>;
}
