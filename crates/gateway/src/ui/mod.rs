#![allow(clippy::collapsible_if, clippy::needless_borrows_for_generic_args)]

pub mod app;
pub mod tray;
pub mod widgets;

pub use app::{AppData, DiffLine, FileNode, TerminalLine, UiEvent};
use pharmakon_core::agent::Agent;
use pharmakon_core::automation::cron::CronManager;
use pharmakon_core::persistence::DbSessionStore;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Bridge agent broadcast events to the UI mpsc channel.
fn spawn_event_bridge(agent: Arc<Agent>, tx: mpsc::UnboundedSender<UiEvent>) {
    tokio::spawn(async move {
        let mut rx = agent.event_tx.subscribe();
        loop {
            match rx.recv().await {
                Ok(pharmakon_common::Event::AgentThought { content }) => {
                    let _ = tx.send(UiEvent::AgentThought(content.to_string()));
                }
                Ok(pharmakon_common::Event::AgentThoughtChunk { chunk, .. }) => {
                    let _ = tx.send(UiEvent::AgentThought(chunk));
                }
                Ok(pharmakon_common::Event::AgentResponse { content }) => {
                    let _ = tx.send(UiEvent::AgentResponse(content.to_string()));
                }
                Ok(pharmakon_common::Event::AgentResponseChunk { chunk, .. }) => {
                    let _ = tx.send(UiEvent::AgentResponse(chunk));
                }
                Ok(pharmakon_common::Event::ToolCall { name, args }) => {
                    let args_str = serde_json::to_string(&args).unwrap_or_default();
                    let _ = tx.send(UiEvent::ToolCall {
                        name,
                        args: args_str,
                    });
                }
                Ok(pharmakon_common::Event::ToolResult { result }) => {
                    let _ = tx.send(UiEvent::ToolResult(result));
                }
                Ok(pharmakon_common::Event::Error { message }) => {
                    let _ = tx.send(UiEvent::Error(message));
                }
                Ok(pharmakon_common::Event::ModelList { models }) => {
                    let _ = tx.send(UiEvent::ModelList(models));
                }
                Ok(pharmakon_common::Event::ModelSwitched { model_id }) => {
                    let _ = tx.send(UiEvent::ModelSwitched(model_id));
                }
                Ok(pharmakon_common::Event::TokenUsageUpdate {
                    total_tokens,
                    total_cost,
                }) => {
                    let _ = tx.send(UiEvent::TokenUsage {
                        tokens: total_tokens,
                        cost: total_cost,
                    });
                }
                Ok(pharmakon_common::Event::GatewayStatus {
                    uptime,
                    memory_usage,
                    ..
                }) => {
                    let _ = tx.send(UiEvent::GatewayStatus {
                        uptime,
                        memory: memory_usage,
                    });
                }
                Ok(pharmakon_common::Event::McpStats { stats }) => {
                    let mapped: Vec<(String, u32)> =
                        stats.into_iter().map(|s| (s.name, s.call_count)).collect();
                    let _ = tx.send(UiEvent::McpStats(mapped));
                }
                Ok(pharmakon_common::Event::ToolList { tools }) => {
                    let mapped: Vec<app::ToolInfo> = tools
                        .into_iter()
                        .map(|t| app::ToolInfo {
                            name: t.name,
                            description: t.description,
                        })
                        .collect();
                    let _ = tx.send(UiEvent::ToolList(mapped));
                }
                Ok(pharmakon_common::Event::SystemLog { level, message }) => {
                    let _ = tx.send(UiEvent::SystemLog { level, message });
                }
                Ok(pharmakon_common::Event::OrchestrationState { sub_agents, .. }) => {
                    let mapped: Vec<app::SwarmStatus> = sub_agents
                        .into_iter()
                        .map(|s| app::SwarmStatus {
                            id: s.name,
                            role: s.role,
                            status: s.status,
                        })
                        .collect();
                    let _ = tx.send(UiEvent::OrchestrationState(mapped));
                }
                Ok(pharmakon_common::Event::GraphUpdate { relations }) => {
                    let _ = tx.send(UiEvent::GraphUpdate(relations));
                }
                Ok(pharmakon_common::Event::SettingsUpdate { settings }) => {
                    let _ = tx.send(UiEvent::SettingsUpdate(settings));
                }
                Ok(pharmakon_common::Event::CronJobList { jobs }) => {
                    let mapped: Vec<app::CronJobInfoData> = jobs
                        .into_iter()
                        .map(|j| app::CronJobInfoData {
                            id: j.id,
                            schedule_type: j.schedule_type,
                            expr: j.expr,
                            message: j.message,
                        })
                        .collect();
                    let _ = tx.send(UiEvent::CronJobList(mapped));
                }
                Ok(pharmakon_common::Event::ApprovalRequest { id, tool, args }) => {
                    let _ = tx.send(UiEvent::ApprovalRequest {
                        id,
                        tool,
                        args: args.to_string(),
                    });
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("GUI event bridge lagged by {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::info!("GUI event bridge: closed");
                    break;
                }
            }
        }
    });
}

/// Run the egui-based IDE.
pub fn run_app(
    agent: Arc<Agent>,
    db: Arc<DbSessionStore>,
    cron_manager: Arc<CronManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_event_bridge(agent.clone(), tx.clone());

    let app_data = AppData::new(agent, db, cron_manager, rx, tx.clone());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("💊 Pharmakon IDE"),
        ..Default::default()
    };

    eframe::run_native(
        "Pharmakon IDE",
        options,
        Box::new(move |_cc| Ok(Box::new(PharmakonIde::new(app_data, tx)))),
    )
    .map_err(|e| e.into())
}

#[derive(Debug, Clone)]
enum MessageSegment {
    Text(String),
    CodeBlock { language: String, code: String },
}

fn parse_message(content: &str) -> Vec<MessageSegment> {
    let mut segments = Vec::new();
    let mut current_text = String::new();
    let mut in_code_block = false;
    let mut current_lang = String::new();
    let mut current_code = String::new();

    for line in content.lines() {
        if line.trim().starts_with("```") {
            if in_code_block {
                segments.push(MessageSegment::CodeBlock {
                    language: current_lang.clone(),
                    code: current_code.clone(),
                });
                current_code.clear();
                current_lang.clear();
                in_code_block = false;
            } else {
                if !current_text.is_empty() {
                    segments.push(MessageSegment::Text(current_text.clone()));
                    current_text.clear();
                }
                current_lang = line.trim().trim_start_matches("```").to_string();
                in_code_block = true;
            }
        } else if in_code_block {
            current_code.push_str(line);
            current_code.push('\n');
        } else {
            current_text.push_str(line);
            current_text.push('\n');
        }
    }

    if in_code_block {
        segments.push(MessageSegment::CodeBlock {
            language: current_lang,
            code: current_code,
        });
    } else if !current_text.is_empty() {
        segments.push(MessageSegment::Text(current_text));
    }

    segments
}

fn syntax_highlight(
    _ctx: &egui::Context,
    syntax_set: &syntect::parsing::SyntaxSet,
    theme_set: &syntect::highlighting::ThemeSet,
    text: &str,
    extension: &str,
    inline_suggestion: Option<&str>,
) -> egui::text::LayoutJob {
    use egui::TextFormat;
    use egui::text::LayoutJob;
    use syntect::easy::HighlightLines;

    let syntax = syntax_set
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| syntax_set.find_syntax_plain_text());

    let theme = &theme_set.themes["base16-ocean.dark"];
    let mut highlighter = HighlightLines::new(syntax, theme);

    let mut job = LayoutJob::default();

    for line in text.split_inclusive('\n') {
        let regions = highlighter
            .highlight_line(line, syntax_set)
            .unwrap_or_default();
        for (style, r_text) in regions {
            let color =
                egui::Color32::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);
            job.append(
                r_text,
                0.0,
                TextFormat {
                    font_id: egui::FontId::monospace(12.0),
                    color,
                    ..Default::default()
                },
            );
        }
    }

    if let Some(ghost) = inline_suggestion {
        job.append(
            ghost,
            0.0,
            TextFormat {
                font_id: egui::FontId::monospace(12.0),
                color: egui::Color32::from_gray(100),
                italics: true,
                ..Default::default()
            },
        );
    }

    job
}

fn draw_file_tree(
    ui: &mut egui::Ui,
    nodes: &mut [FileNode],
    selected_file: Option<&str>,
) -> Option<String> {
    let mut clicked_file = None;
    for node in nodes {
        if node.is_dir {
            let icon = if node.expanded { "📂 " } else { "📁 " };
            let label = format!("{}{}", icon, node.name);

            let id = ui.make_persistent_id(&node.path);
            let header = egui::CollapsingHeader::new(label)
                .id_salt(id)
                .default_open(false);

            let res = header.show(ui, |ui| {
                if let Some(f) = draw_file_tree(ui, &mut node.children, selected_file) {
                    clicked_file = Some(f);
                }
            });

            if res.header_response.clicked() {
                node.expanded = !node.expanded;
            }
        } else {
            let ext = node.path.extension().and_then(|s| s.to_str()).unwrap_or("");
            let icon = match ext {
                "rs" => "🦀 ",
                "toml" => "📋 ",
                "md" => "📝 ",
                _ => "📄 ",
            };
            let label = format!("{}{}", icon, node.name);
            let is_selected = selected_file == Some(node.path.to_str().unwrap_or(""));
            if ui.selectable_label(is_selected, label).clicked() {
                clicked_file = Some(node.path.to_str().unwrap_or("").to_string());
            }
        }
    }
    clicked_file
}

/// Main egui application.
struct PharmakonIde {
    data: AppData,
    folder_input: String,
    bottom_tab: BottomTab,
    is_console_open: bool,
    event_tx: mpsc::UnboundedSender<UiEvent>,
}

#[derive(PartialEq, Clone, Copy)]
enum BottomTab {
    Tools,
    Logs,
    Graph,
    Swarms,
    Terminal,
}

impl PharmakonIde {
    fn new(data: AppData, event_tx: mpsc::UnboundedSender<UiEvent>) -> Self {
        Self {
            data,
            folder_input: String::new(),
            bottom_tab: BottomTab::Tools,
            is_console_open: true,
            event_tx,
        }
    }
}

impl eframe::App for PharmakonIde {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.data.drain_events();

        // Apply a premium dark theme on the first frame and persist
        let mut visuals = egui::Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 20, 22); // Background (Cursor Charcoal)
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(39, 39, 42)); // Border (Cursor Zinc)
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(28, 28, 30);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 45, 48);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(139, 92, 246); // Purple accent
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        ctx.set_visuals(visuals);

        // 1. Top bar: status + active model info
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(139, 92, 246),
                    "💊 PHARMAKON COMPANION IDE",
                );
                ui.separator();
                ui.label(format!("Workspace: {}", self.data.workspace_root));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.is_console_open, "📟 Collapsible Drawer");
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::LIGHT_BLUE,
                        format!("Model: {}", self.data.current_model),
                    );
                });
            });
        });

        // 2. Left Panel: File explorer (always open)
        egui::SidePanel::left("file_panel")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.heading("📁 Workspace Files");
                ui.horizontal(|ui| {
                    let resp = ui.add_sized(
                        [ui.available_width() - 50.0, 20.0],
                        egui::TextEdit::singleline(&mut self.folder_input)
                            .hint_text("/path/to/project..."),
                    );
                    if (ui.button("📂").clicked()
                        || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                        && !self.folder_input.is_empty()
                    {
                        let p = std::path::PathBuf::from(&self.folder_input);
                        if p.is_dir() {
                            self.data.workspace_root = p.to_string_lossy().to_string();
                            self.data.refresh_file_tree();
                            self.folder_input.clear();
                        }
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("📂 Pick Folder").clicked()
                        && let Some(folder) = rfd::FileDialog::new().pick_folder()
                    {
                        self.data.workspace_root = folder.to_string_lossy().to_string();
                        self.data.refresh_file_tree();
                        self.folder_input.clear();
                    }
                    if ui.button("📄 Pick File").clicked()
                        && let Some(file) = rfd::FileDialog::new().pick_file()
                    {
                        let full = file.to_string_lossy().to_string();
                        self.data.open_file(&full);
                    }
                });
                ui.colored_label(egui::Color32::GRAY, self.data.workspace_root.clone());
                if ui.button("↻ Refresh workspace").clicked() {
                    self.data.refresh_file_tree();
                }
                ui.separator();

                // Recursive hierarchical file tree
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(clicked_p) = draw_file_tree(
                        ui,
                        &mut self.data.file_tree_nodes,
                        self.data.selected_file.as_deref(),
                    ) {
                        self.data.open_file(&clicked_p);
                    }
                });
            });

        // 3. Right Panel: Chat/AI Assistant panel (always open, Cursor-style!)
        egui::SidePanel::right("chat_panel").resizable(true).default_width(380.0).show(ctx, |ui| {
            ui.heading("🤖 Pharmakon Copilot");
            ui.separator();

            if let Some((_id, tool, args)) = self.data.pending_approval.clone() {
                ui.group(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(245, 158, 11), "🛡️ TOOL APPROVAL REQUIRED");
                    ui.label(format!("Tool: {}", tool));
                    ui.label(format!("Arguments: {}", args));
                    ui.horizontal(|ui| {
                        if ui.button("✓ Approve & Run").clicked() {
                            self.data.resolve_approval(true);
                        }
                        if ui.button("✗ Reject").clicked() {
                            self.data.resolve_approval(false);
                        }
                    });
                });
                ui.separator();
            }

            // Layout Chat body & inputs
            let body_height = ui.available_height() - 75.0;

            // Scrollable Chat history
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), body_height),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for msg in &self.data.messages {
                            let (prefix, color) = match msg.role.as_str() {
                                "user" => ("🧑 You", egui::Color32::from_rgb(52, 211, 153)),
                                "agent" => ("💊 Copilot", egui::Color32::from_rgb(129, 140, 248)),
                                "tool" => ("🔧 Tool Exec", egui::Color32::LIGHT_GRAY),
                                _ => ("⚙ System", egui::Color32::from_rgb(251, 191, 36)),
                            };
                            ui.colored_label(color, prefix);
                            if let Some(thought) = &msg.thought {
                                ui.colored_label(egui::Color32::from_rgb(139, 92, 246), format!("  🧠 thought: {}", thought));
                            }

                            // Parse and render segments (including code blocks with copy/apply buttons)
                            let segments = parse_message(&msg.content);
                            for segment in segments {
                                match segment {
                                    MessageSegment::Text(text) => {
                                        ui.label(text);
                                    }
                                    MessageSegment::CodeBlock { language, code } => {
                                        ui.group(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.colored_label(egui::Color32::from_rgb(139, 92, 246), format!("💻 {}", language));
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.button("△ Apply to Editor").clicked() {
                                                        self.data.file_content = code.clone();
                                                        self.data.system_logs.push("Applied code block to editor".into());
                                                    }
                                                });
                                            });
                                            ui.separator();
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(&code)
                                                    .font(egui::FontId::monospace(11.0))
                                                    .color(egui::Color32::from_rgb(220, 220, 220))
                                            ));
                                        });
                                    }
                                }
                            }

                            if let Some(tool) = &msg.tool_name {
                                ui.label(format!("  ⚡ {}", tool));
                            }
                            ui.separator();
                        }
                    });
                }
            );

            ui.separator();

            // Text Entry and Send Command Area
            ui.horizontal(|ui| {
                let resp = ui.add_sized([ui.available_width() - 65.0, 45.0], egui::TextEdit::multiline(&mut self.data.input_text).hint_text("Ask Copilot or request code modification..."));
                if ui.button("Send 🚀").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)) {
                    self.data.send_message();
                    ui.ctx().request_repaint();
                }
            });
        });

        // 4. Bottom Panel: Collapsible Terminal drawer for execution logs, tools, graph, swarms
        if self.is_console_open {
            egui::TopBottomPanel::bottom("terminal_panel").resizable(true).default_height(200.0).show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Tools, "⚡ Live Tools");
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Logs, "📋 Logs");
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Graph, "🗄 Graph relations");
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Swarms, "⚙ Swarms list");
                    ui.selectable_value(&mut self.bottom_tab, BottomTab::Terminal, "📟 Terminal");
                });
                ui.separator();

                match self.bottom_tab {
                    BottomTab::Tools => {
                        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                            for trace in self.data.tool_trace.iter().rev().take(40) {
                                ui.label(trace);
                            }
                            if self.data.tool_trace.is_empty() {
                                ui.label("Awaiting first tool call...");
                            }
                        });
                    }
                    BottomTab::Logs => {
                        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                            for log in &self.data.system_logs {
                                let color = if log.contains("ERR") { egui::Color32::RED }
                                    else if log.contains("WARN") { egui::Color32::YELLOW }
                                    else { egui::Color32::GRAY };
                                ui.colored_label(color, log);
                            }
                        });
                    }
                    BottomTab::Graph => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if self.data.graph_relations.is_empty() {
                                ui.label("No graph relations loaded.");
                            } else {
                                for rel in &self.data.graph_relations {
                                    ui.label(rel);
                                }
                            }
                        });
                    }
                    BottomTab::Swarms => {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if self.data.active_swarms.is_empty() {
                                ui.label("No active swarms.");
                            } else {
                                for swarm in &self.data.active_swarms {
                                    ui.label(format!("Swarm ID: {} | Role: {} | Status: {}", swarm.id, swarm.role, swarm.status));
                                }
                            }
                        });
                    }
                    BottomTab::Terminal => {
                        ui.vertical(|ui| {
                            let available_height = ui.available_height() - 40.0;
                            egui::ScrollArea::vertical().stick_to_bottom(true).max_height(available_height).show(ui, |ui| {
                                for line in &self.data.terminal_lines {
                                    let color = if line.is_input {
                                        egui::Color32::from_rgb(139, 92, 246) // Input purple
                                    } else {
                                        egui::Color32::from_rgb(220, 220, 220) // Output white
                                    };
                                    let prefix = if line.is_input { "$ " } else { "" };
                                    ui.horizontal(|ui| {
                                        ui.colored_label(egui::Color32::from_gray(120), format!("[{}]", line.timestamp));
                                        ui.colored_label(color, format!("{}{}", prefix, line.text));
                                    });
                                }
                                if self.data.terminal_lines.is_empty() {
                                    ui.colored_label(egui::Color32::GRAY, "Embedded Terminal Ready. Type command and hit Enter.");
                                }
                            });

                            ui.separator();

                            ui.horizontal(|ui| {
                                ui.label("$");
                                let resp = ui.add_sized(
                                    [ui.available_width() - 80.0, 25.0],
                                    egui::TextEdit::singleline(&mut self.data.terminal_input)
                                        .hint_text("cargo build / git status / echo hello...")
                                );

                                let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                                if ui.button("Run").clicked() || enter_pressed {
                                    let cmd = self.data.terminal_input.trim().to_string();
                                    if !cmd.is_empty() {
                                        // Self-insert prompt input locally immediately
                                        let _ = self.event_tx.send(UiEvent::TerminalOutput {
                                            text: cmd.clone(),
                                            is_input: true,
                                        });

                                        let tx = self.event_tx.clone();
                                        tokio::spawn(async move {
                                            let output = if cfg!(target_os = "windows") {
                                                std::process::Command::new("cmd")
                                                    .args(&["/C", &cmd])
                                                    .output()
                                            } else {
                                                std::process::Command::new("sh")
                                                    .args(&["-c", &cmd])
                                                    .output()
                                            };

                                            match output {
                                                Ok(out) => {
                                                    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                                                    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
                                                    if !stdout.is_empty() {
                                                        let _ = tx.send(UiEvent::TerminalOutput {
                                                            text: stdout,
                                                            is_input: false,
                                                        });
                                                    }
                                                    if !stderr.is_empty() {
                                                        let _ = tx.send(UiEvent::TerminalOutput {
                                                            text: stderr,
                                                            is_input: false,
                                                        });
                                                    }
                                                }
                                                Err(e) => {
                                                    let _ = tx.send(UiEvent::TerminalOutput {
                                                        text: format!("Execution Error: {}", e),
                                                        is_input: false,
                                                    });
                                                }
                                            }
                                        });
                                        self.data.terminal_input.clear();
                                    }
                                }
                            });
                        });
                    }
                }
            });
        }

        // 5. Central Panel: Integrated Multi-line Code Editor with file tabs, line numbers, highlighting and diffing
        egui::CentralPanel::default().show(ctx, |ui| {
            // Render tabs at the top of Central Panel
            if !self.data.open_tabs.is_empty() {
                ui.horizontal(|ui| {
                    for (idx, path) in self.data.open_tabs.clone().iter().enumerate() {
                        let filename = std::path::Path::new(path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or(path);
                        let ext = std::path::Path::new(path)
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("");
                        let icon = match ext {
                            "rs" => "🦀 ",
                            "toml" => "📋 ",
                            "md" => "📝 ",
                            _ => "📄 ",
                        };

                        let label = format!("{}{}", icon, filename);
                        let is_active = self.data.active_tab_index == Some(idx);

                        let tab_btn = ui.selectable_label(is_active, label);
                        if tab_btn.clicked() {
                            self.data.open_file(path);
                        }
                        if ui.button("✕").clicked() {
                            self.data.close_tab(idx);
                        }
                        ui.separator();
                    }
                });
                ui.separator();
            }

            let selected_file_path = self.data.selected_file.clone();
            if let Some(path) = selected_file_path {
                ui.horizontal(|ui| {
                    ui.heading(format!("📄 {}", path));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("💾 Save changes").clicked() {
                            if self.data.original_content != self.data.file_content {
                                self.data.show_save_confirm_dialog = true;
                            } else {
                                self.data.system_logs.push("No changes to save".into());
                            }
                        }
                        if ui.button("⏪ Rollback code").clicked() {
                            self.data.rollback();
                        }
                        let diff_btn_text = if self.data.diff_preview_mode { "👁️ Edit Mode" } else { "🔍 Preview Diff" };
                        if ui.button(diff_btn_text).clicked() {
                            self.data.diff_preview_mode = !self.data.diff_preview_mode;
                            if self.data.diff_preview_mode {
                                self.data.compute_diff();
                            }
                        }
                    });
                });
                ui.separator();

                // Divvy up vertical workspace space: Upper editor area, lower telemetry runway
                let editor_height = ui.available_height() * 0.55;

                ui.group(|ui| {
                    ui.set_height(editor_height);

                    if self.data.diff_preview_mode {
                        // Render Diff View
                        egui::ScrollArea::both().show(ui, |ui| {
                            for diff_line in &self.data.diff_lines {
                                match diff_line {
                                    DiffLine::Unchanged { text, line_no } => {
                                        ui.horizontal(|ui| {
                                            ui.add_sized([35.0, 18.0], egui::Label::new(
                                                egui::RichText::new(format!("{:>3}", line_no))
                                                    .color(egui::Color32::from_gray(100))
                                                    .font(egui::FontId::monospace(12.0))
                                            ));
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(text)
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::from_rgb(220, 220, 220))
                                            ));
                                        });
                                    }
                                    DiffLine::Added { text, line_no } => {
                                        ui.horizontal(|ui| {
                                            ui.add_sized([35.0, 18.0], egui::Label::new(
                                                egui::RichText::new(format!("{:>3}", line_no))
                                                    .color(egui::Color32::from_rgb(16, 185, 129))
                                                    .font(egui::FontId::monospace(12.0))
                                            ));
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(format!("+ {}", text))
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::from_rgb(52, 211, 153))
                                            ));
                                        });
                                    }
                                    DiffLine::Removed { text, line_no } => {
                                        ui.horizontal(|ui| {
                                            ui.add_sized([35.0, 18.0], egui::Label::new(
                                                egui::RichText::new(format!("{:>3}", line_no))
                                                    .color(egui::Color32::from_rgb(239, 68, 68))
                                                    .font(egui::FontId::monospace(12.0))
                                            ));
                                            ui.add(egui::Label::new(
                                                egui::RichText::new(format!("- {}", text))
                                                    .font(egui::FontId::monospace(12.0))
                                                    .color(egui::Color32::from_rgb(248, 113, 113))
                                            ));
                                        });
                                    }
                                }
                            }
                        });
                    } else {
                        // Render Code Editor with Line Numbers and Syntax Highlighting
                        let ext = std::path::Path::new(&path)
                            .extension()
                            .and_then(|s| s.to_str())
                            .unwrap_or("");

                        let syntax_set = &self.data.syntax_set;
                        let theme_set = &self.data.theme_set;

                        // Suggest brace completion ghost text as prototype suggestion
                        if self.data.inline_suggestion.is_none() && self.data.file_content.trim_end().ends_with('{') {
                            self.data.inline_suggestion = Some(app::InlineSuggestion {
                                ghost_text: "\n    // Suggestion: implement reflection triggers\n}".into(),
                                position: self.data.file_content.len(),
                            });
                        }

                        let suggestion_text = self.data.inline_suggestion.as_ref().map(|s| s.ghost_text.clone());
                        let suggestion_text_ref = suggestion_text.as_deref();

                        let mut layouter = |ui: &egui::Ui, string: &str, _wrap_width: f32| {
                            let job = syntax_highlight(ui.ctx(), syntax_set, theme_set, string, ext, suggestion_text_ref);
                            ui.fonts(|f| f.layout_job(job))
                        };

                        let total_lines = self.data.file_content.lines().count().max(1);
                        let mut apply_autocomplete = false;
                        let mut suggestion_to_apply = String::new();

                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Draw Line Numbers
                                ui.vertical(|ui| {
                                    ui.set_width(35.0);
                                    for i in 1..=total_lines {
                                        ui.add_sized([35.0, 16.0], egui::Label::new(
                                            egui::RichText::new(format!("{:>3}", i))
                                                .color(egui::Color32::from_gray(100))
                                                .font(egui::FontId::monospace(12.0))
                                        ));
                                    }
                                });

                                // Draw Editor
                                ui.vertical(|ui| {
                                    let resp = ui.add_sized(
                                        ui.available_size(),
                                        egui::TextEdit::multiline(&mut self.data.file_content)
                                            .font(egui::FontId::monospace(12.0))
                                            .code_editor()
                                            .desired_width(f32::INFINITY)
                                            .desired_rows(total_lines)
                                            .layouter(&mut layouter)
                                    );

                                    // Handle autocomplete selection with TAB
                                    if resp.has_focus() && suggestion_text.is_some() {
                                        if ui.input(|i| i.key_pressed(egui::Key::Tab)) {
                                            apply_autocomplete = true;
                                            if let Some(ref sugg) = suggestion_text {
                                                suggestion_to_apply = sugg.clone();
                                            }
                                        }
                                    }
                                });
                            });
                        });

                        if apply_autocomplete {
                            self.data.inline_suggestion = None;
                            self.data.file_content.push_str(&suggestion_to_apply);
                        }
                    }
                });

                ui.separator();

                // Lower double-column runway displaying AI decision trees & cognitive tracing timeline
                ui.columns(2, |cols| {
                    // Left Column: Plan Execution DAG (Decision Nodes)
                    cols[0].vertical(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(139, 92, 246), "📋 Plan AST — Cognitive Runway");
                        ui.separator();
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if self.data.plan_dag.is_empty() {
                                ui.colored_label(egui::Color32::GRAY, "Awaiting instruction flow to model Plan AST...");
                            } else {
                                for node in &self.data.plan_dag {
                                    let (sym, color) = match node.status {
                                        app::DagStatus::Pending => ("⬡ Pending", egui::Color32::GRAY),
                                        app::DagStatus::Running => ("▶ Running", egui::Color32::from_rgb(139, 92, 246)),
                                        app::DagStatus::Success => ("✓ Success", egui::Color32::from_rgb(16, 185, 129)),
                                        app::DagStatus::Failed => ("✗ Failed", egui::Color32::from_rgb(239, 68, 68)),
                                        app::DagStatus::Gated => ("🛡️ Gated", egui::Color32::from_rgb(245, 158, 11)),
                                    };
                                    ui.horizontal(|ui| {
                                        ui.colored_label(color, sym);
                                        ui.label(&node.label);
                                    });
                                }
                            }
                        });
                    });

                    // Right Column: Timeline / Cognitive Tracing commits
                    cols[1].vertical(|ui| {
                        ui.colored_label(egui::Color32::from_rgb(59, 130, 246), "⏳ Cognitive Timeline — Git for Cognition");
                        ui.separator();
                        egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                            if self.data.cognitive_timeline.is_empty() {
                                ui.colored_label(egui::Color32::GRAY, "Awaiting cognitive events tracing...");
                            } else {
                                for ev in &self.data.cognitive_timeline {
                                    let badge_color = match ev.kind {
                                        app::TimelineKind::Plan => egui::Color32::from_rgb(139, 92, 246),
                                        app::TimelineKind::Verify => egui::Color32::from_rgb(16, 185, 129),
                                        app::TimelineKind::Execute => egui::Color32::from_rgb(59, 130, 246),
                                        app::TimelineKind::Fail => egui::Color32::from_rgb(239, 68, 68),
                                        app::TimelineKind::Rollback => egui::Color32::from_rgb(244, 63, 94),
                                        app::TimelineKind::Gate => egui::Color32::from_rgb(245, 158, 11),
                                    };
                                    ui.horizontal(|ui| {
                                        ui.colored_label(egui::Color32::GRAY, format!("[{}]", ev.timestamp));
                                        ui.colored_label(badge_color, format!("{:?}", ev.kind));
                                        ui.label(&ev.event);
                                    });
                                }
                            }
                        });
                    });
                });
            } else {
                // Return a stunning, premium "Cursor Welcome Dashboard" inside the central editor viewport!
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.heading("💊 Pharmakon - Autonomous Cognitive Environment");
                    ui.colored_label(egui::Color32::GRAY, "Lightweight multi-agent compiler, reflection system, and local editor.");
                    ui.add_space(30.0);

                    ui.group(|ui| {
                        ui.set_width(450.0);
                        ui.heading("🚀 Quick Workspace Setup");
                        ui.separator();
                        ui.label("1. Select a Rust file from the left workspace tree.");
                        ui.label("2. The central editor will render the code.");
                        ui.label("3. Chat with Pharmakon Copilot on the right panel to automatically rewrite code, run speculative execution compilations, and verify AST constructs!");
                        ui.add_space(10.0);
                    });

                    ui.add_space(20.0);
                    ui.group(|ui| {
                        ui.set_width(450.0);
                        ui.heading("📊 Live Token Economy");
                        ui.separator();
                        ui.label(format!("Token Count: {} tokens", self.data.token_count));
                        ui.label(format!("Accrued Cost: ${:.5}", self.data.total_cost));
                        ui.label(format!("Active Swarms: {} agents", self.data.active_swarms.len()));
                        ui.label(format!("Current Session State: {}", if self.data.health_stats.is_alive { "HEALTHY" } else { "STALLED" }));
                    });
                });
            }
        });

        // Safe Confirmation Save Dialog Window Popup
        if self.data.show_save_confirm_dialog {
            let mut open = true;
            let current_path = self.data.selected_file.clone().unwrap_or_default();
            egui::Window::new("💾 Confirm Saving Changes?")
                .open(&mut open)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label("Are you sure you want to save changes to this file?");
                    ui.colored_label(egui::Color32::YELLOW, format!("Path: {}", current_path));
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui.button("Save ✅").clicked() {
                            if let Err(e) = std::fs::write(&current_path, &self.data.file_content) {
                                self.data
                                    .system_logs
                                    .push(format!("ERR: Failed to save: {}", e));
                            } else {
                                self.data
                                    .system_logs
                                    .push(format!("SUCCESS: Saved changes to {}", current_path));
                                self.data.original_content = self.data.file_content.clone();
                            }
                            self.data.show_save_confirm_dialog = false;
                        }
                        if ui.button("Cancel ❌").clicked() {
                            self.data.show_save_confirm_dialog = false;
                        }
                    });
                });
            if !open {
                self.data.show_save_confirm_dialog = false;
            }
        }

        // Bottom status info
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(if self.data.health_stats.is_alive {
                    "● LIVE"
                } else {
                    "○ OFFLINE"
                });
                ui.label(format!(
                    " | {} tokens | ${:.4} | {} tools active",
                    self.data.token_count,
                    self.data.total_cost,
                    self.data.tools.len()
                ));
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}
