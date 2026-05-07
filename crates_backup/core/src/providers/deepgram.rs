use anyhow::Result;
use async_trait::async_trait;
use pharmakon_common::voice::SpeechToText;
use reqwest::Client;
use serde_json::Value;

pub struct DeepgramProvider {
    api_key: String,
    client: Client,
}

impl DeepgramProvider {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl SpeechToText for DeepgramProvider {
    async fn transcribe(&self, audio_data: Vec<u8>) -> Result<String> {
        let res = self
            .client
            .post("https://api.deepgram.com/v1/listen")
            .header("Authorization", format!("Token {}", self.api_key))
            .header("Content-Type", "audio/wav")
            .body(audio_data)
            .send()
            .await?;

        let json: Value = res.json().await?;
        let transcript = json["results"]["channels"][0]["alternatives"][0]["transcript"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        Ok(transcript)
    }
}
