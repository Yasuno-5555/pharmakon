use anyhow::Result;
use async_trait::async_trait;
use pharmakon_common::voice::{SpeechToText, TextToSpeech};
use reqwest::Client;
use serde_json::json;

pub struct OpenAiVoice {
    api_key: String,
    client: Client,
}

impl OpenAiVoice {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl SpeechToText for OpenAiVoice {
    async fn transcribe(&self, audio_data: Vec<u8>) -> Result<String> {
        // Whisper API implementation
        // For brevity, we'll implement the request structure
        let form = reqwest::multipart::Form::new()
            .part(
                "file",
                reqwest::multipart::Part::bytes(audio_data).file_name("audio.webm"),
            )
            .text("model", "whisper-1");

        let response: reqwest::Response = self
            .client
            .post("https://api.openai.com/v1/audio/transcriptions")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await?;

        let result: serde_json::Value = response.json().await?;
        Ok(result["text"].as_str().unwrap_or_default().to_string())
    }
}

#[async_trait]
impl TextToSpeech for OpenAiVoice {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>> {
        let response = self
            .client
            .post("https://api.openai.com/v1/audio/speech")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": "tts-1",
                "input": text,
                "voice": "alloy"
            }))
            .send()
            .await?;

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
