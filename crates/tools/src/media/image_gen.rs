use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use reqwest::Client;
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

pub struct ImageGenTool {
    client: Client,
}

impl Default for ImageGenTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageGenTool {
    pub fn new() -> Self {
        Self {
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
        "Generate a UI mockup, placeholder, or asset based on a text description and save it locally. Fallback SVG is used if no OpenAI API key is present."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": { "type": "string", "description": "Description of the image/mockup to generate" },
                "filename": { "type": "string", "description": "Filename to save as under frontend public assets (e.g., 'login.svg' or 'dashboard.png')", "default": "generated_asset.svg" }
            },
            "required": ["prompt"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::Media
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let prompt = args["prompt"]
            .as_str()
            .ok_or_else(|| AgentError("Missing prompt".into()))?;
        let filename = args["filename"].as_str().unwrap_or("generated_asset.svg");

        // Ensure output directory exists in frontend public assets if possible
        let mut out_dir = PathBuf::from("frontend/public/assets");
        if !out_dir.exists() {
            out_dir = PathBuf::from(".pharmakon/generated");
            let _ = fs::create_dir_all(&out_dir);
        } else {
            let _ = fs::create_dir_all(&out_dir);
        }

        let out_path = out_dir.join(filename);
        let api_key = std::env::var("OPENAI_API_KEY").ok();

        if let Some(key) = api_key {
            let res = self
                .client
                .post("https://api.openai.com/v1/images/generations")
                .header("Authorization", format!("Bearer {}", key))
                .json(&json!({
                    "model": "dall-e-3",
                    "prompt": prompt,
                    "n": 1,
                    "size": "1024x1024"
                }))
                .send()
                .await
                .map_err(|e| AgentError(format!("OpenAI request failed: {}", e)))?;

            if res.status().is_success() {
                let body: Value = res
                    .json()
                    .await
                    .map_err(|e| AgentError(format!("Failed to parse OpenAI JSON: {}", e)))?;
                if let Some(url) = body["data"][0]["url"].as_str() {
                    let img_data = self
                        .client
                        .get(url)
                        .send()
                        .await
                        .map_err(|e| {
                            AgentError(format!("Failed to download generated image: {}", e))
                        })?
                        .bytes()
                        .await
                        .map_err(|e| AgentError(format!("Failed to read image bytes: {}", e)))?;

                    fs::write(&out_path, img_data)
                        .map_err(|e| AgentError(format!("Failed to write file to disk: {}", e)))?;
                    return Ok(format!(
                        "Successfully generated image via OpenAI DALL-E 3 and saved to {:?}",
                        out_path
                    ));
                }
            }
        }

        let mut final_path = out_path.clone();
        if !filename.ends_with(".svg") {
            final_path = out_path.with_extension("svg");
        }

        let prompt_lower = prompt.to_lowercase();
        let svg_content = if prompt_lower.contains("login") || prompt_lower.contains("auth") {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600" width="100%" height="100%">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#0f172a" />
      <stop offset="50%" stop-color="#1e1b4b" />
      <stop offset="100%" stop-color="#311042" />
    </linearGradient>
    <linearGradient id="card-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="white" stop-opacity="0.1" />
      <stop offset="100%" stop-color="white" stop-opacity="0.03" />
    </linearGradient>
    <linearGradient id="btn-grad" x1="0%" y1="0%" x2="100%" y2="0%">
      <stop offset="0%" stop-color="#6366f1" />
      <stop offset="100%" stop-color="#a855f7" />
    </linearGradient>
    <filter id="glow">
      <feGaussianBlur stdDeviation="8" result="coloredBlur"/>
      <feMerge>
        <feMergeNode in="coloredBlur"/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>
  <rect width="800" height="600" fill="url(#bg)" />
  <circle cx="200" cy="150" r="120" fill="#a855f7" opacity="0.15" filter="url(#glow)" />
  <circle cx="650" cy="450" r="160" fill="#6366f1" opacity="0.1" filter="url(#glow)" />
  <rect x="250" y="100" width="300" height="400" rx="20" fill="url(#card-grad)" stroke="white" stroke-opacity="0.2" stroke-width="1" />
  <text x="400" y="160" font-family="sans-serif" font-size="24" font-weight="bold" fill="white" text-anchor="middle">Welcome Back</text>
  <text x="400" y="185" font-family="sans-serif" font-size="12" fill="#94a3b8" text-anchor="middle">Enter your credentials to continue</text>
  
  <text x="280" y="235" font-family="sans-serif" font-size="11" fill="#94a3b8">Email Address</text>
  <rect x="280" y="245" width="240" height="36" rx="8" fill="#0f172a" stroke="white" stroke-opacity="0.1" stroke-width="1" />
  <text x="295" y="267" font-family="sans-serif" font-size="12" fill="#475569">name@example.com</text>
  
  <text x="280" y="315" font-family="sans-serif" font-size="11" fill="#94a3b8">Password</text>
  <rect x="280" y="325" width="240" height="36" rx="8" fill="#0f172a" stroke="white" stroke-opacity="0.1" stroke-width="1" />
  <text x="295" y="347" font-family="sans-serif" font-size="12" fill="#475569">••••••••••••</text>
  
  <rect x="280" y="390" width="240" height="38" rx="8" fill="url(#btn-grad)" filter="url(#glow)" />
  <text x="400" y="413" font-family="sans-serif" font-size="14" font-weight="bold" fill="white" text-anchor="middle">Sign In</text>
</svg>"##.to_string()
        } else if prompt_lower.contains("dashboard")
            || prompt_lower.contains("chart")
            || prompt_lower.contains("graph")
        {
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600" width="100%" height="100%">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#0b0f19" />
      <stop offset="100%" stop-color="#111827" />
    </linearGradient>
    <linearGradient id="chart-glow" x1="0%" y1="100%" x2="0%" y2="0%">
      <stop offset="0%" stop-color="#6366f1" stop-opacity="0" />
      <stop offset="100%" stop-color="#6366f1" stop-opacity="0.2" />
    </linearGradient>
  </defs>
  <rect width="800" height="600" fill="url(#bg)" />
  <rect x="0" y="0" width="200" height="600" fill="#030712" />
  <text x="30" y="45" font-family="sans-serif" font-size="18" font-weight="bold" fill="#6366f1">Pharmakon OS</text>
  <rect x="20" y="80" width="160" height="32" rx="6" fill="#1e1b4b" />
  <text x="40" y="100" font-family="sans-serif" font-size="13" fill="#818cf8" font-weight="bold">📊 Dashboard</text>
  <text x="40" y="145" font-family="sans-serif" font-size="13" fill="#9ca3af">⚙️ Settings</text>
  <text x="40" y="190" font-family="sans-serif" font-size="13" fill="#9ca3af">🔍 Research</text>
  
  <text x="230" y="45" font-family="sans-serif" font-size="20" font-weight="bold" fill="white">System Analytics</text>
  
  <rect x="230" y="80" width="160" height="80" rx="10" fill="#1f2937" />
  <text x="250" y="110" font-family="sans-serif" font-size="12" fill="#9ca3af">CPU usage</text>
  <text x="250" y="140" font-family="sans-serif" font-size="22" font-weight="bold" fill="#10b981">14.2%</text>
  
  <rect x="410" y="80" width="160" height="80" rx="10" fill="#1f2937" />
  <text x="430" y="110" font-family="sans-serif" font-size="12" fill="#9ca3af">Memory Pool</text>
  <text x="430" y="140" font-family="sans-serif" font-size="22" font-weight="bold" fill="#6366f1">62.8 GB</text>
  
  <rect x="590" y="80" width="180" height="80" rx="10" fill="#1f2937" />
  <text x="610" y="110" font-family="sans-serif" font-size="12" fill="#9ca3af">Network I/O</text>
  <text x="610" y="140" font-family="sans-serif" font-size="22" font-weight="bold" fill="#38bdf8">354 Mb/s</text>
  
  <rect x="230" y="190" width="540" height="370" rx="12" fill="#111827" stroke="#1f2937" stroke-width="1" />
  <text x="260" y="225" font-family="sans-serif" font-size="14" font-weight="bold" fill="white">Telemetry Timeline</text>
  <path d="M 260 500 L 320 420 L 380 460 L 440 330 L 500 380 L 560 270 L 620 290 L 680 220 L 740 240 L 740 500 Z" fill="url(#chart-glow)" />
  <path d="M 260 500 L 320 420 L 380 460 L 440 330 L 500 380 L 560 270 L 620 290 L 680 220 L 740 240" fill="none" stroke="#6366f1" stroke-width="3" />
</svg>"##.to_string()
        } else {
            let escaped_prompt = prompt
                .replace("\"", "&quot;")
                .replace("<", "&lt;")
                .replace(">", "&gt;");
            format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600" width="100%" height="100%">
  <defs>
    <linearGradient id="bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#1e1b4b" />
      <stop offset="100%" stop-color="#0f172a" />
    </linearGradient>
    <linearGradient id="shape-grad" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#3b82f6" />
      <stop offset="50%" stop-color="#8b5cf6" />
      <stop offset="100%" stop-color="#ec4899" />
    </linearGradient>
    <filter id="blur">
      <feGaussianBlur stdDeviation="30" />
    </filter>
  </defs>
  <rect width="800" height="600" fill="url(#bg)" />
  <circle cx="400" cy="300" r="220" fill="url(#shape-grad)" opacity="0.3" filter="url(#blur)" />
  <circle cx="200" cy="400" r="150" fill="#06b6d4" opacity="0.15" filter="url(#blur)" />
  <rect x="150" y="150" width="500" height="300" rx="16" fill="white" fill-opacity="0.03" stroke="white" stroke-opacity="0.1" stroke-width="1" />
  <text x="400" y="270" font-family="sans-serif" font-size="28" font-weight="bold" fill="white" text-anchor="middle">Visual Asset Mockup</text>
  <text x="400" y="320" font-family="sans-serif" font-size="14" fill="#94a3b8" text-anchor="middle">{}</text>
  <text x="400" y="380" font-family="sans-serif" font-size="11" fill="#6366f1" text-anchor="middle" font-weight="bold">PHARMAKON COGNITIVE VISUAL SUBSYSTEM</text>
</svg>"##,
                escaped_prompt
            )
        };

        fs::write(&final_path, svg_content)
            .map_err(|e| AgentError(format!("Failed to write SVG: {}", e)))?;
        Ok(format!(
            "Successfully generated beautiful SVG placeholder mockup for prompt '{}' and saved to {:?}",
            prompt, final_path
        ))
    }
}
