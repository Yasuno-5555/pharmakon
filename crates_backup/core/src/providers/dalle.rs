use crate::providers::media::MediaProvider;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;

pub struct DalleProvider {
    api_key: String,
    model: String,
    client: Client,
}

impl DalleProvider {
    pub fn new(api_key: String, model: Option<String>) -> Self {
        Self {
            api_key,
            model: model.unwrap_or_else(|| "dall-e-3".to_string()),
            client: Client::new(),
        }
    }
}

#[async_trait]
impl MediaProvider for DalleProvider {
    fn name(&self) -> &str {
        "openai-dalle"
    }

    async fn generate_image(&self, prompt: &str, size: &str) -> Result<String> {
        let response = self
            .client
            .post("https://api.openai.com/v1/images/generations")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": self.model,
                "prompt": prompt,
                "n": 1,
                "size": size,
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let err = response.text().await?;
            return Err(anyhow!("OpenAI API error: {}", err));
        }

        let data: serde_json::Value = response.json().await?;
        let url = data["data"][0]["url"]
            .as_str()
            .ok_or_else(|| anyhow!("Failed to parse image URL from response"))?;

        Ok(url.to_string())
    }
}
