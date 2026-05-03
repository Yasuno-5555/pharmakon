use async_trait::async_trait;
use anyhow::anyhow;
use serde_json::Value;
use headless_chrome::{Browser, LaunchOptions};
use pharmakon_common::{Tool, AgentResult, AgentError};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct BrowserTool {
    pub cdp_url: Option<String>,
    pub browser: Arc<Mutex<Option<Browser>>>,
}

impl BrowserTool {
    pub fn new(cdp_url: Option<String>) -> Self {
        Self {
            cdp_url,
            browser: Arc::new(Mutex::new(None)),
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str { "browser" }
    fn description(&self) -> &str { "Navigate to a URL, click elements, or take screenshots. Uses an isolated browser instance." }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["navigate", "screenshot", "extract", "click", "type", "wait"] },
                "url": { "type": "string" },
                "selector": { "type": "string" },
                "text": { "type": "string" },
                "seconds": { "type": "integer" }
            },
            "required": ["action"]
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().ok_or_else(|| AgentError("Missing action".to_string()))?;
        
        let mut browser_lock = self.browser.lock().await;
        if browser_lock.is_none() {
            log::info!("Initializing browser instance (CDP: {:?})...", self.cdp_url);
            let browser = if let Some(url) = &self.cdp_url {
                Browser::connect(url.clone()).map_err(|e| AgentError(e.to_string()))?
            } else {
                Browser::new(LaunchOptions {
                    headless: true,
                    ..Default::default()
                }).map_err(|e| AgentError(e.to_string()))?
            };
            *browser_lock = Some(browser);
        }
        
        let browser = browser_lock.as_ref().unwrap();
        let tab = browser.new_tab().map_err(|e| AgentError(e.to_string()))?;

        match action {
            "navigate" => {
                let url = args["url"].as_str().ok_or_else(|| AgentError("Missing URL".to_string()))?;
                tab.navigate_to(url).map_err(|e| AgentError(e.to_string()))?;
                tab.wait_until_navigated().map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Successfully navigated to {}", url))
            }
            "click" => {
                let selector = args["selector"].as_str().ok_or_else(|| AgentError("Missing selector".to_string()))?;
                tab.find_element(selector).map_err(|e| AgentError(e.to_string()))?
                    .click().map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Clicked element: {}", selector))
            }
            "type" => {
                let selector = args["selector"].as_str().ok_or_else(|| AgentError("Missing selector".to_string()))?;
                let text = args["text"].as_str().ok_or_else(|| AgentError("Missing text".to_string()))?;
                tab.find_element(selector).map_err(|e| AgentError(e.to_string()))?
                    .type_into(text).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Typed into {}: {}", selector, text))
            }
            "wait" => {
                let seconds = args["seconds"].as_u64().unwrap_or(1);
                tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
                Ok(format!("Waited for {} seconds", seconds))
            }
            "screenshot" => {
                let png_data = tab.capture_screenshot(
                    headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                    None,
                    None,
                    true
                ).map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Captured screenshot ({} bytes)", png_data.len()))
            }
            "extract" => {
                let content = tab.get_content().map_err(|e| AgentError(e.to_string()))?;
                Ok(content)
            }
            _ => Err(AgentError("Unsupported browser action".to_string())),
        }
    }
}
