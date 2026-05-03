use async_trait::async_trait;
use anyhow::Result;
use pharmakon_common::voice::TextToSpeech;
use reqwest::Client;
use serde_json::json;

pub struct ElevenLabsProvider {
    api_key: String,
    voice_id: String,
    client: Client,
}

impl ElevenLabsProvider {
    pub fn new(api_key: String, voice_id: String) -> Self {
        Self {
            api_key,
            voice_id,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl TextToSpeech for ElevenLabsProvider {
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>> {
        let url = format!("https://api.elevenlabs.io/v1/text-to-speech/{}", self.voice_id);
        let res = self.client.post(&url)
            .header("xi-api-key", &self.api_key)
            .json(&json!({
                "text": text,
                "model_id": "eleven_monolingual_v1",
                "voice_settings": {
                    "stability": 0.5,
                    "similarity_boost": 0.5
                }
            }))
            .send()
            .await?;

        let bytes = res.bytes().await?;
        Ok(bytes.to_vec())
    }
}
