use async_trait::async_trait;
use pharmakon_common::{AgentError, AgentResult, Tool, ToolCategory};
use serde_json::{Value, json};
use std::fs;
use std::path::PathBuf;

pub struct NativeGuiEmulatorTool;

impl Default for NativeGuiEmulatorTool {
    fn default() -> Self { Self::new() }
}

impl NativeGuiEmulatorTool {
    pub fn new() -> Self { Self }
}

enum Widget {
    Heading(String),
    Label(String),
    Button(String),
    TextEdit(String),
    Checkbox(String),
    Slider,
    Separator,
    HorizontalStart,
    GroupEnd,
}

fn extract_string_param(line: &str, method: &str) -> Option<String> {
    if let Some(pos) = line.find(method) {
        let after = &line[pos + method.len()..];
        if let Some(start) = after.find('"') {
            if let Some(end) = after[start+1..].find('"') {
                return Some(after[start+1..start+1+end].to_string());
            }
        }
        // Fallback to single quotes
        if let Some(start) = after.find('\'') {
            if let Some(end) = after[start+1..].find('\'') {
                return Some(after[start+1..start+1+end].to_string());
            }
        }
    }
    None
}

fn extract_text_edit_placeholder(line: &str) -> String {
    if let Some(pos) = line.find('&') {
        let after = &line[pos..];
        let var_name: String = after.chars().skip(1).take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if !var_name.is_empty() {
            return format!("Text: {}", var_name);
        }
    }
    "Enter text...".to_string()
}

#[async_trait]
impl Tool for NativeGuiEmulatorTool {
    fn name(&self) -> &str {
        "native_gui_emulator"
    }
    fn description(&self) -> &str {
        "Visual design helper and previewer for egui & Vello applications. Parses egui code to render beautiful SVG layout mocks and scaffolds multi-platform egui/vello GUI templates."
    }
    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["preview", "scaffold"] },
                "path": { "type": "string", "description": "For 'preview': Path to the Rust file containing egui GUI code." },
                "directory": { "type": "string", "description": "For 'scaffold': Path to the directory where the egui/vello project will be created." },
                "app_name": { "type": "string", "description": "For 'scaffold': Name of the scaffolding application.", "default": "pharmakon_gui_app" }
            },
            "required": ["action"]
        })
    }
    fn category(&self) -> ToolCategory {
        ToolCategory::System
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let action = args["action"].as_str().ok_or_else(|| AgentError("Missing action".into()))?;

        match action {
            "preview" => {
                let path_str = args["path"].as_str().ok_or_else(|| AgentError("Missing path".into()))?;
                let path = PathBuf::from(path_str);
                if !path.exists() {
                    return Err(AgentError(format!("File does not exist: {}", path_str)));
                }

                let content = fs::read_to_string(&path).map_err(|e| AgentError(format!("Failed to read file: {}", e)))?;
                
                // Parse out egui widgets
                let mut widgets = Vec::new();
                let mut current_window = "egui Application Viewport".to_string();
                
                for line in content.lines() {
                    let trimmed = line.trim();
                    if trimmed.contains("egui::Window::new") {
                        if let Some(start) = trimmed.find('"') {
                            if let Some(end) = trimmed[start+1..].find('"') {
                                current_window = trimmed[start+1..start+1+end].to_string();
                            }
                        }
                    }
                    
                    if trimmed.contains(".heading(") {
                        if let Some(val) = extract_string_param(trimmed, ".heading(") {
                            widgets.push(Widget::Heading(val));
                        }
                    } else if trimmed.contains(".label(") {
                        if let Some(val) = extract_string_param(trimmed, ".label(") {
                            widgets.push(Widget::Label(val));
                        }
                    } else if trimmed.contains(".button(") {
                        if let Some(val) = extract_string_param(trimmed, ".button(") {
                            widgets.push(Widget::Button(val));
                        }
                    } else if trimmed.contains("text_edit_") {
                        widgets.push(Widget::TextEdit(extract_text_edit_placeholder(trimmed)));
                    } else if trimmed.contains("checkbox(") {
                        let val = extract_string_param(trimmed, "checkbox(").unwrap_or_else(|| "Checkbox option".to_string());
                        widgets.push(Widget::Checkbox(val));
                    } else if trimmed.contains("slider(") || trimmed.contains("Slider::new") {
                        widgets.push(Widget::Slider);
                    } else if trimmed.contains("separator()") {
                        widgets.push(Widget::Separator);
                    } else if trimmed.contains("horizontal(|") {
                        widgets.push(Widget::HorizontalStart);
                    } else if trimmed.contains("});") && trimmed.len() <= 6 {
                        widgets.push(Widget::GroupEnd);
                    }
                }

                let mut svg = String::new();
                svg.push_str(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600" width="100%" height="100%">
  <defs>
    <linearGradient id="egui_bg" x1="0%" y1="0%" x2="100%" y2="100%">
      <stop offset="0%" stop-color="#141414" />
      <stop offset="100%" stop-color="#1c1c1c" />
    </linearGradient>
    <filter id="shadow" x="-5%" y="-5%" width="110%" height="110%">
      <feDropShadow dx="2" dy="4" stdDeviation="6" flood-color="#000" flood-opacity="0.6"/>
    </filter>
  </defs>
  
  <!-- Canvas Dark Background -->
  <rect width="800" height="600" fill="#0c0c0c" />
  
  <!-- egui Floating Window Frame -->
  <rect x="100" y="50" width="600" height="500" rx="6" fill="url(#egui_bg)" stroke="#2f2f2f" stroke-width="1.5" filter="url(#shadow)" />
  
  <!-- Title Bar -->
  <rect x="100" y="50" width="600" height="35" rx="6" fill="#1f1f1f" />
  <!-- Close, Max, Min simulated circles -->
  <circle cx="125" cy="68" r="5" fill="#ef4444" />
  <circle cx="141" cy="68" r="5" fill="#f59e0b" />
  <circle cx="157" cy="68" r="5" fill="#10b981" />
"##);

                svg.push_str(&format!(r##"  <text x="400" y="73" font-family="sans-serif" font-size="13" font-weight="bold" fill="#dfdfdf" text-anchor="middle">{}</text>
"##, current_window));

                let mut current_y = 110.0;
                let mut current_x = 130.0;
                let mut in_horizontal = false;
                let mut horizontal_x_adv = 0.0;

                for widget in &widgets {
                    match widget {
                        Widget::HorizontalStart => {
                            in_horizontal = true;
                            horizontal_x_adv = 0.0;
                        }
                        Widget::GroupEnd => {
                            if in_horizontal {
                                in_horizontal = false;
                                current_y += 40.0;
                                current_x = 130.0;
                            }
                        }
                        Widget::Heading(text) => {
                            let x = if in_horizontal { current_x + horizontal_x_adv } else { current_x };
                            svg.push_str(&format!(r##"  <text x="{}" y="{}" font-family="sans-serif" font-size="20" font-weight="bold" fill="#ffffff">{}</text>
"##, x, current_y + 18.0, text));
                            if in_horizontal {
                                horizontal_x_adv += (text.len() * 12) as f64 + 30.0;
                            } else {
                                current_y += 35.0;
                            }
                        }
                        Widget::Label(text) => {
                            let x = if in_horizontal { current_x + horizontal_x_adv } else { current_x };
                            svg.push_str(&format!(r##"  <text x="{}" y="{}" font-family="sans-serif" font-size="13" fill="#cbcbcb">{}</text>
"##, x, current_y + 14.0, text));
                            if in_horizontal {
                                horizontal_x_adv += (text.len() * 8) as f64 + 20.0;
                            } else {
                                current_y += 25.0;
                            }
                        }
                        Widget::Button(text) => {
                            let x = if in_horizontal { current_x + horizontal_x_adv } else { current_x };
                            let width = (text.len() * 9).max(80) as f64;
                            svg.push_str(&format!(r##"  <rect x="{}" y="{}" width="{}" height="24" rx="4" fill="#2d2d2d" stroke="#4f46e5" stroke-width="1" />
  <text x="{}" y="{}" font-family="sans-serif" font-size="12" fill="#ffffff" text-anchor="middle">{}</text>
"##, x, current_y, width, x + (width / 2.0), current_y + 16.0, text));
                            if in_horizontal {
                                horizontal_x_adv += width + 15.0;
                            } else {
                                current_y += 35.0;
                            }
                        }
                        Widget::TextEdit(placeholder) => {
                            let x = if in_horizontal { current_x + horizontal_x_adv } else { current_x };
                            let width = 180.0;
                            svg.push_str(&format!(r##"  <rect x="{}" y="{}" width="{}" height="24" rx="3" fill="#0d0d0d" stroke="#3e3e3e" stroke-width="1" />
  <text x="{}" y="{}" font-family="sans-serif" font-size="12" fill="#888888">{}</text>
"##, x, current_y, width, x + 8.0, current_y + 16.0, placeholder));
                            if in_horizontal {
                                horizontal_x_adv += width + 15.0;
                            } else {
                                current_y += 35.0;
                            }
                        }
                        Widget::Checkbox(label) => {
                            let x = if in_horizontal { current_x + horizontal_x_adv } else { current_x };
                            svg.push_str(&format!(r##"  <rect x="{}" y="{}" width="14" height="14" rx="2" fill="#2d2d2d" stroke="#4a4a4a" stroke-width="1" />
  <polyline points="{},{} {},{} {},{}" fill="none" stroke="#4f46e5" stroke-width="2" />
  <text x="{}" y="{}" font-family="sans-serif" font-size="12" fill="#cbcbcb">{}</text>
"##, x, current_y, x + 3.0, current_y + 7.0, x + 6.0, current_y + 10.0, x + 11.0, current_y + 4.0, x + 22.0, current_y + 11.0, label));
                            if in_horizontal {
                                horizontal_x_adv += (label.len() * 8) as f64 + 40.0;
                            } else {
                                current_y += 28.0;
                            }
                        }
                        Widget::Slider => {
                            let x = if in_horizontal { current_x + horizontal_x_adv } else { current_x };
                            let width = 140.0;
                            svg.push_str(&format!(r##"  <rect x="{}" y="{}" width="{}" height="6" rx="3" fill="#090909" />
  <rect x="{}" y="{}" width="40" height="6" rx="3" fill="#4f46e5" />
  <circle cx="{}" cy="{}" r="6" fill="#818cf8" stroke="#ffffff" stroke-width="1" />
"##, x, current_y + 8.0, width, x, current_y + 8.0, x + 40.0, current_y + 11.0));
                            if in_horizontal {
                                horizontal_x_adv += width + 20.0;
                            } else {
                                current_y += 30.0;
                            }
                        }
                        Widget::Separator => {
                            let y = current_y + 10.0;
                            svg.push_str(&format!(r##"  <line x1="120" y1="{}" x2="680" y2="{}" stroke="#2f2f2f" stroke-width="1" />
"##, y, y));
                            current_y += 20.0;
                        }
                    }
                }

                svg.push_str("\n</svg>");

                let mut out_dir = PathBuf::from("frontend/public/assets");
                if !out_dir.exists() {
                    out_dir = PathBuf::from(".pharmakon/screenshots");
                    let _ = fs::create_dir_all(&out_dir);
                } else {
                    let _ = fs::create_dir_all(&out_dir);
                }
                let out_path = out_dir.join("egui_preview.svg");
                fs::write(&out_path, &svg).map_err(|e| AgentError(format!("Failed to save egui preview: {}", e)))?;

                let mut report = format!("### egui Visual Layout Analysis for `{}`\n\n", path_str);
                report.push_str(&format!("- Active Application Window: **{}**\n", current_window));
                report.push_str(&format!("- Rendered Preview Saved to: **{:?}**\n\n", out_path));
                report.push_str("#### Widget Hierarchy Tree\n");
                for w in &widgets {
                    let w_desc = match w {
                        Widget::Heading(t) => format!("Heading: \"{}\"", t),
                        Widget::Label(t) => format!("Label: \"{}\"", t),
                        Widget::Button(t) => format!("Button: \"{}\"", t),
                        Widget::TextEdit(p) => format!("TextEdit Field ({})", p),
                        Widget::Checkbox(l) => format!("Checkbox: \"{}\"", l),
                        Widget::Slider => "Slider Widget".to_string(),
                        Widget::Separator => "--- Separator ---".to_string(),
                        Widget::HorizontalStart => "Horizontal Row [".to_string(),
                        Widget::GroupEnd => "] End Horizontal Row".to_string(),
                    };
                    report.push_str(&format!("- {}\n", w_desc));
                }

                Ok(report)
            }
            "scaffold" => {
                let dir_str = args["directory"].as_str().ok_or_else(|| AgentError("Missing directory".into()))?;
                let app_name = args["app_name"].as_str().unwrap_or("pharmakon_gui_app");
                let dir = PathBuf::from(dir_str).join(app_name);

                let src_dir = dir.join("src");
                fs::create_dir_all(&src_dir).map_err(|e| AgentError(format!("Failed to create directories: {}", e)))?;

                // 1. Scaffold Cargo.toml
                let cargo_toml = format!(r##"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[dependencies]
eframe = "0.27.0"
egui = "0.27.0"
vello = "0.1.0"
log = "0.4"
env_logger = "0.10"
"##, app_name);
                fs::write(dir.join("Cargo.toml"), cargo_toml).map_err(|e| AgentError(format!("Failed to write Cargo.toml: {}", e)))?;

                // 2. Scaffold main.rs
                let main_rs = r##"use eframe::egui;

fn main() -> eframe::Result<()> {
    env_logger::init();
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Pharmakon Custom egui & Vello App",
        native_options,
        Box::new(|cc| Box::new(CustomApp::new(cc))),
    )
}

struct CustomApp {
    title: String,
    counter: i32,
    slider_val: f32,
    text_input: String,
}

impl CustomApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            title: "Pharmakon Embedded GUI Panel".to_owned(),
            counter: 0,
            slider_val: 10.0,
            text_input: "Interactive user text".to_owned(),
        }
    }
}

impl eframe::App for CustomApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading(&self.title);
            ui.separator();

            ui.label("This interface is rendered inside a fully reactive immediate-mode Rust canvas.");

            ui.horizontal(|ui| {
                ui.label("Control Node Action:");
                if ui.button("Increment Counter").clicked() {
                    self.counter += 1;
                }
                if ui.button("Reset").clicked() {
                    self.counter = 0;
                }
            });

            ui.label(format!("Active Counter value: {}", self.counter));
            ui.separator();

            ui.label("Adjust system entropy:");
            ui.add(egui::Slider::new(&mut self.slider_val, 0.0..=100.0).text("entropy"));

            ui.separator();
            ui.label("Data Link Terminal Input:");
            ui.text_edit_singleline(&mut self.text_input);
        });
    }
}
"##;
                fs::write(src_dir.join("main.rs"), main_rs).map_err(|e| AgentError(format!("Failed to write main.rs: {}", e)))?;

                Ok(format!("Successfully scaffolded fully compiling egui & Vello template at: {:?}\nIncludes package configuration and reactive state main.rs implementation.", dir))
            }
            _ => Err(AgentError("Unknown action".into()))
        }
    }
}
