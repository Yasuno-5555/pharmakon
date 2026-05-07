use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool};
use reqwest::Client;
use serde_json::{Value, json};

pub struct ImageGenTool {
    api_key: String,
    client: Client,
}

impl ImageGenTool {
    pub fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: Client::new(),
        }
    }
}

#[async_trait]
impl Tool for ImageGenTool {
    fn name(&self) -> &str {
        "generate_image"
    }
    fn description(&self) -> &str {
        "Generate an image from a text prompt using DALL-E 3"
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string" }
            },
            "required": ["prompt"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| AgentError("Missing prompt".to_string()))?;
        let res = self
            .client
            .post("https://api.openai.com/v1/images/generations")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "model": "dall-e-3",
                "prompt": prompt,
                "n": 1,
                "size": "1024x1024"
            }))
            .send()
            .await
            .map_err(|e| AgentError(e.to_string()))?;

        let json = res
            .json::<Value>()
            .await
            .map_err(|e| AgentError(e.to_string()))?;
        Ok(json["data"][0]["url"]
            .as_str()
            .unwrap_or("No URL returned")
            .to_string())
    }
}
