use async_trait::async_trait;
use serde_json::{Value, json};
use pharmakon_common::{Tool, AgentResult, AgentError, AgentModel, Message, MessageContent, ContentPart, ImageUrl, CompletionRequest};
use std::fs;
use std::sync::Arc;

pub struct MediaUnderstandingTool {
    pub model: Arc<dyn AgentModel>,
}

impl MediaUnderstandingTool {
    pub fn new(model: Arc<dyn AgentModel>) -> Self {
        Self { model }
    }
}

#[async_trait]
impl Tool for MediaUnderstandingTool {
    fn name(&self) -> &str { "understand_media" }
    fn description(&self) -> &str { "Describe the content of an image file. Use this to 'see' what is in a media file." }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Local path to the image file" },
                "query": { "type": "string", "description": "What to look for or ask about the media (optional)" }
            },
            "required": ["path"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let path = args["path"].as_str().ok_or_else(|| AgentError("Missing path".to_string()))?;
        let query = args["query"].as_str().unwrap_or("Describe this image in detail.");
        
        let bytes = fs::read(path).map_err(|e| AgentError(format!("Failed to read file {}: {}", path, e)))?;
        use base64::Engine as _;
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let mime_type = if path.ends_with(".png") { "image/png" } else { "image/jpeg" };
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);
  
        let request = CompletionRequest {
            messages: vec![
                Message {
                    role: "user".to_string(),
                    content: Some(MessageContent::Multimodal(vec![
                        ContentPart::Text { text: query.to_string() },
                        ContentPart::Image { image_url: ImageUrl { url: data_url, detail: None } }
                    ])),
                    ..Default::default()
                }
            ],
            temperature: Some(0.0),
            max_tokens: Some(500),
            tools: None,
        };

        log::info!("MediaUnderstandingTool: Sending vision request for {}...", path);
        let response = self.model.complete(request).await?;
        
        let result = response.content.as_ref()
            .and_then(|c| c.as_text())
            .unwrap_or("Failed to get textual description from vision model.");

        Ok(result.to_string())
    }
}
