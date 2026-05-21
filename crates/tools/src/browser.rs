use async_trait::async_trait;
use headless_chrome::{Browser, LaunchOptions, Tab};
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use reqwest::Client;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct BrowserTool {
    pub cdp_url: Option<String>,
    pub browser: Arc<Mutex<Option<Browser>>>,
    pub tab: Arc<Mutex<Option<Arc<Tab>>>>,
    pub is_mock_mode: Arc<Mutex<bool>>,
    pub mock_url: Arc<Mutex<String>>,
    pub mock_content: Arc<Mutex<String>>,
}

impl Default for BrowserTool {
    fn default() -> Self { Self::new(None) }
}

impl BrowserTool {
    pub fn new(cdp_url: Option<String>) -> Self {
        Self {
            cdp_url,
            browser: Arc::new(Mutex::new(None)),
            tab: Arc::new(Mutex::new(None)),
            is_mock_mode: Arc::new(Mutex::new(false)),
            mock_url: Arc::new(Mutex::new("about:blank".to_string())),
            mock_content: Arc::new(Mutex::new("".to_string())),
        }
    }

    async fn call_mock(&self, action: &str, args: &Value) -> AgentResult<String> {
        let mut mock_url_lock = self.mock_url.lock().await;
        let mut mock_content_lock = self.mock_content.lock().await;

        match action {
            "navigate" => {
                let url = args["url"].as_str().ok_or_else(|| AgentError("Missing URL".into()))?;
                *mock_url_lock = url.to_string();
                
                log::info!("Simulated Browser navigating to {}...", url);
                let client = Client::new();
                let res = client.get(url).send().await;
                match res {
                    Ok(response) => {
                        let html = response.text().await.unwrap_or_default();
                        *mock_content_lock = html;
                        Ok(format!("Mock Browser navigated successfully to {}. Loaded page content ({} bytes).", url, mock_content_lock.len()))
                    }
                    Err(e) => {
                        *mock_content_lock = format!("Failed to fetch: {}", e);
                        Ok(format!("Mock Browser simulated offline load of {} (Request Failed: {})", url, e))
                    }
                }
            }
            "screenshot" => {
                let mut out_dir = PathBuf::from("frontend/public/assets");
                if !out_dir.exists() {
                    out_dir = PathBuf::from(".pharmakon/screenshots");
                    let _ = fs::create_dir_all(&out_dir);
                } else {
                    let _ = fs::create_dir_all(&out_dir);
                }
                let out_path = out_dir.join("mock_browser_screenshot.svg");

                let mock_svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 768" width="100%" height="100%">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#1e1b4b" />
      <stop offset="100%" stop-color="#0f172a" />
    </linearGradient>
  </defs>
  <rect width="1024" height="768" fill="url(#bg)" />
  
  <!-- Browser Chrome Header Bar -->
  <rect x="0" y="0" width="1024" height="70" fill="#0f172a" stroke="#1f2937" stroke-width="1" />
  <circle cx="20" cy="35" r="6" fill="#ef4444" />
  <circle cx="40" cy="35" r="6" fill="#f59e0b" />
  <circle cx="60" cy="35" r="6" fill="#10b981" />
  
  <rect x="100" y="20" width="750" height="30" rx="6" fill="#1f2937" stroke="#374151" stroke-width="1" />
  <text x="120" y="40" font-family="sans-serif" font-size="12" fill="#9ca3af">{}</text>
  <text x="880" y="40" font-family="sans-serif" font-size="12" fill="#f59e0b" font-weight="bold">[EMULATED MODE]</text>

  <!-- Page Content Viewport Mock -->
  <rect x="50" y="100" width="924" height="600" rx="12" fill="#111827" stroke="#1f2937" stroke-width="1" />
  <text x="100" y="150" font-family="sans-serif" font-size="24" font-weight="bold" fill="white">Loaded Mock Viewport</text>
  <text x="100" y="190" font-family="sans-serif" font-size="14" fill="#9ca3af">Content length: {} bytes</text>
  
  <rect x="100" y="230" width="824" height="1" fill="#1f2937" />
  <text x="100" y="270" font-family="sans-serif" font-size="14" fill="#818cf8">Active simulated elements and anchor links have been parsed and annotated.</text>
</svg>"##, mock_url_lock, mock_content_lock.len());

                fs::write(&out_path, mock_svg).map_err(|e| AgentError(format!("Failed to write mock SVG: {}", e)))?;
                Ok(format!("Mock screenshot successfully saved to {:?}", out_path))
            }
            "extract" => {
                Ok(mock_content_lock.clone())
            }
            "click" | "type" => {
                let selector = args["selector"].as_str().unwrap_or("unknown");
                Ok(format!("Simulated mock browser action '{}' on selector '{}' successfully.", action, selector))
            }
            "wait" => {
                let seconds = args["seconds"].as_u64().unwrap_or(1);
                tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
                Ok(format!("Waited {} seconds (simulated).", seconds))
            }
            "annotate" => {
                let html = mock_content_lock.clone();
                let mut elements = Vec::new();
                let mut id = 1;
                
                // Parse out potential link and input elements
                for cap in html.split('<').skip(1) {
                    if cap.starts_with("a ") && cap.contains("href=") {
                        let text = cap.split('>').nth(1).unwrap_or("").split('<').next().unwrap_or("").trim();
                        let display_text = if text.is_empty() { "Anchor Link" } else { text };
                        let truncated = &display_text[..std::cmp::min(display_text.len(), 40)];
                        elements.push(json!({
                            "id": id,
                            "tag": "a",
                            "text": truncated
                        }));
                        id += 1;
                    } else if cap.starts_with("button") || cap.starts_with("input") {
                        elements.push(json!({
                            "id": id,
                            "tag": "input",
                            "text": "Interactive Element"
                        }));
                        id += 1;
                    }
                    if id > 20 { break; }
                }

                let mut out_dir = PathBuf::from("frontend/public/assets");
                if !out_dir.exists() {
                    out_dir = PathBuf::from(".pharmakon/screenshots");
                    let _ = fs::create_dir_all(&out_dir);
                } else {
                    let _ = fs::create_dir_all(&out_dir);
                }
                let out_path = out_dir.join("mock_annotated_screenshot.svg");
                
                let mock_svg = format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1024 768" width="100%" height="100%">
  <rect width="1024" height="768" fill="#0f172a" />
  <rect x="0" y="0" width="1024" height="50" fill="#1e1b4b" />
  <text x="30" y="30" font-family="sans-serif" font-size="14" fill="white">Annotated Mock Screen - {}</text>
  <text x="100" y="100" font-family="sans-serif" font-size="18" fill="#10b981">Successfully annotated {} interactive elements.</text>
</svg>"##, mock_url_lock, id - 1);
                
                fs::write(&out_path, mock_svg).map_err(|e| AgentError(format!("Failed to write mock SVG: {}", e)))?;
                Ok(format!("Annotated mock screen saved to: {:?}\nElements mapped:\n{}", out_path, serde_json::to_string_pretty(&elements).unwrap_or_default()))
            }
            _ => Err(AgentError("Unsupported mock browser action".into()))
        }
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }
    fn description(&self) -> &str {
        "Navigate to a URL, click elements, type text, or take screenshots. Reuses the active tab session to preserve navigation history."
    }
    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["navigate", "screenshot", "extract", "click", "type", "wait", "annotate"] },
                "url": { "type": "string", "description": "URL to navigate to (required for 'navigate')" },
                "selector": { "type": "string", "description": "CSS selector to target (required for 'click', 'type')" },
                "text": { "type": "string", "description": "Text to type (required for 'type')" },
                "seconds": { "type": "integer", "description": "Number of seconds to wait (required for 'wait')", "default": 1 }
            },
            "required": ["action"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Media
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| AgentError("Missing action".to_string()))?
            .to_string();

        let is_mock = *self.is_mock_mode.lock().await;
        if is_mock {
            return self.call_mock(&action, &args).await;
        }

        let mut browser_lock = self.browser.lock().await;
        let mut tab_lock = self.tab.lock().await;

        if browser_lock.is_none() {
            log::info!("Initializing browser instance (CDP: {:?})...", self.cdp_url);
            let browser_res = if let Some(url) = &self.cdp_url {
                Browser::connect(url.clone())
            } else {
                Browser::new(LaunchOptions {
                    headless: true,
                    ..Default::default()
                })
            };

            match browser_res {
                Ok(browser) => {
                    *browser_lock = Some(browser);
                }
                Err(e) => {
                    log::warn!("Failed to launch headless chrome ({}). Falling back to mock HTTP browser emulator...", e);
                    *self.is_mock_mode.lock().await = true;
                    return self.call_mock(&action, &args).await;
                }
            }
        }

        if tab_lock.is_none() {
            let browser = browser_lock.as_ref().ok_or_else(|| AgentError("Browser not initialized".to_string()))?;
            let tab = browser.new_tab().map_err(|e| AgentError(e.to_string()))?;
            *tab_lock = Some(tab);
        }

        let tab = tab_lock.as_ref().ok_or_else(|| AgentError("Tab session not initialized".to_string()))?;

        match action.as_str() {
            "navigate" => {
                let url = args["url"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing URL".to_string()))?;
                tab.navigate_to(url)
                    .map_err(|e| AgentError(e.to_string()))?;
                tab.wait_until_navigated()
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Successfully navigated to {}", url))
            }
            "click" => {
                let selector = args["selector"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing selector".to_string()))?;
                tab.find_element(selector)
                    .map_err(|e| AgentError(e.to_string()))?
                    .click()
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Clicked element: {}", selector))
            }
            "type" => {
                let selector = args["selector"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing selector".to_string()))?;
                let text = args["text"]
                    .as_str()
                    .ok_or_else(|| AgentError("Missing text".to_string()))?;
                tab.find_element(selector)
                    .map_err(|e| AgentError(e.to_string()))?
                    .type_into(text)
                    .map_err(|e| AgentError(e.to_string()))?;
                Ok(format!("Typed into {}: {}", selector, text))
            }
            "wait" => {
                let seconds = args["seconds"].as_u64().unwrap_or(1);
                tokio::time::sleep(std::time::Duration::from_secs(seconds)).await;
                Ok(format!("Waited for {} seconds", seconds))
            }
            "screenshot" => {
                let png_data = tab
                    .capture_screenshot(
                        headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                        None,
                        None,
                        true,
                    )
                    .map_err(|e| AgentError(e.to_string()))?;
                
                let mut out_dir = PathBuf::from("frontend/public/assets");
                if !out_dir.exists() {
                    out_dir = PathBuf::from(".pharmakon/screenshots");
                    let _ = fs::create_dir_all(&out_dir);
                } else {
                    let _ = fs::create_dir_all(&out_dir);
                }
                let out_path = out_dir.join("browser_screenshot.png");
                fs::write(&out_path, &png_data).map_err(|e| AgentError(format!("Failed to write screenshot: {}", e)))?;
                
                Ok(format!("Captured screenshot successfully and saved to {:?}", out_path))
            }
            "extract" => {
                let content = tab.get_content().map_err(|e| AgentError(e.to_string()))?;
                Ok(content)
            }
            "annotate" => {
                let js = r#"
                (function() {
                    let interactiveSelector = 'a, button, input, select, textarea, [role="button"], [tabindex]:not([tabindex="-1"])';
                    let elements = document.querySelectorAll(interactiveSelector);
                    let annotations = [];
                    let id = 1;
                    
                    for (let el of elements) {
                        let rect = el.getBoundingClientRect();
                        if (rect.width === 0 || rect.height === 0 || window.getComputedStyle(el).visibility === 'hidden') continue;
                        
                        let div = document.createElement('div');
                        div.style.position = 'absolute';
                        div.style.border = '2px solid red';
                        div.style.backgroundColor = 'rgba(255,0,0,0.1)';
                        div.style.left = (rect.left + window.scrollX) + 'px';
                        div.style.top = (rect.top + window.scrollY) + 'px';
                        div.style.width = rect.width + 'px';
                        div.style.height = rect.height + 'px';
                        div.style.zIndex = 10000;
                        div.style.pointerEvents = 'none';
                        
                        let label = document.createElement('span');
                        label.innerText = id;
                        label.style.position = 'absolute';
                        label.style.backgroundColor = 'red';
                        label.style.color = 'white';
                        label.style.fontSize = '12px';
                        label.style.top = '-14px';
                        label.style.left = '0';
                        div.appendChild(label);
                        
                        document.body.appendChild(div);
                        
                        let text = el.innerText || el.value || el.placeholder || el.getAttribute('aria-label') || '';
                        annotations.push({
                            id: id,
                            tag: el.tagName.toLowerCase(),
                            text: text.substring(0, 50).replace(/\n/g, ' '),
                        });
                        
                        el.setAttribute('data-pharmakon-id', id);
                        id++;
                    }
                    return JSON.stringify(annotations);
                })();
                "#;
                
                let res = tab.evaluate(js, false).map_err(|e| AgentError(e.to_string()))?;
                let annotations_json = res.value.unwrap_or_default().to_string();
                
                let png_data = tab
                    .capture_screenshot(
                        headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                        None,
                        None,
                        true,
                    )
                    .map_err(|e| AgentError(e.to_string()))?;
                    
                let mut out_dir = PathBuf::from("frontend/public/assets");
                if !out_dir.exists() {
                    out_dir = PathBuf::from(".pharmakon/screenshots");
                    let _ = fs::create_dir_all(&out_dir);
                } else {
                    let _ = fs::create_dir_all(&out_dir);
                }
                let out_path = out_dir.join("browser_annotated.png");
                fs::write(&out_path, &png_data).map_err(|e| AgentError(format!("Failed to write annotated screenshot: {}", e)))?;
                
                Ok(format!("Annotated screen saved to: {:?}\nElements mapped:\n{}", out_path, annotations_json))
            }
            _ => Err(AgentError("Unsupported browser action".to_string())),
        }
    }
}
