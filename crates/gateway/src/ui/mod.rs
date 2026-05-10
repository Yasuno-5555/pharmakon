pub mod app;
pub mod tray;
pub mod widgets;

pub use app::{AppData, UiEvent};
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
                    let _ = tx.send(UiEvent::ToolCall { name, args: args_str });
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
                Ok(pharmakon_common::Event::TokenUsageUpdate { total_tokens, total_cost }) => {
                    let _ = tx.send(UiEvent::TokenUsage { tokens: total_tokens, cost: total_cost });
                }
                Ok(pharmakon_common::Event::GatewayStatus { uptime, memory_usage, .. }) => {
                    let _ = tx.send(UiEvent::GatewayStatus { uptime, memory: memory_usage });
                }
                Ok(pharmakon_common::Event::McpStats { stats }) => {
                    let mapped: Vec<(String, u32)> = stats.into_iter().map(|s| (s.name, s.call_count)).collect();
                    let _ = tx.send(UiEvent::McpStats(mapped));
                }
                Ok(pharmakon_common::Event::ToolList { tools }) => {
                    let mapped: Vec<app::ToolInfo> = tools.into_iter().map(|t| app::ToolInfo {
                        name: t.name,
                        description: t.description,
                    }).collect();
                    let _ = tx.send(UiEvent::ToolList(mapped));
                }
                Ok(pharmakon_common::Event::SystemLog { level, message }) => {
                    let _ = tx.send(UiEvent::SystemLog { level, message });
                }
                Ok(pharmakon_common::Event::OrchestrationState { sub_agents, .. }) => {
                    let mapped: Vec<app::SwarmStatus> = sub_agents.into_iter().map(|s| app::SwarmStatus {
                        id: s.name,
                        role: s.role,
                        status: s.status,
                    }).collect();
                    let _ = tx.send(UiEvent::OrchestrationState(mapped));
                }
                Ok(pharmakon_common::Event::GraphUpdate { relations }) => {
                    let _ = tx.send(UiEvent::GraphUpdate(relations));
                }
                Ok(pharmakon_common::Event::SettingsUpdate { settings }) => {
                    let _ = tx.send(UiEvent::SettingsUpdate(settings));
                }
                Ok(pharmakon_common::Event::CronJobList { jobs }) => {
                    let mapped: Vec<app::CronJobInfoData> = jobs.into_iter().map(|j| app::CronJobInfoData {
                        id: j.id, schedule_type: j.schedule_type, expr: j.expr, message: j.message,
                    }).collect();
                    let _ = tx.send(UiEvent::CronJobList(mapped));
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
    spawn_event_bridge(agent.clone(), tx);

    let app_data = AppData::new(agent, db, cron_manager, rx);

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1400.0, 900.0])
            .with_title("💊 Pharmakon IDE"),
        ..Default::default()
    };

    eframe::run_native(
        "Pharmakon IDE",
        options,
        Box::new(move |_cc| Ok(Box::new(PharmakonIde::new(app_data)))),
    ).map_err(|e| e.into())
}

/// Main egui application.
struct PharmakonIde {
    data: AppData,
    active_tab: IdeTab,
    folder_input: String,
}

#[derive(PartialEq, Clone, Copy)]
enum IdeTab { Chat, Files, Logs, Tools, Graph }

impl PharmakonIde {
    fn new(data: AppData) -> Self {
        Self { data, active_tab: IdeTab::Chat, folder_input: String::new() }
    }
}

impl eframe::App for PharmakonIde {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.data.drain_events();

        // Top bar: tab selector + status
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, IdeTab::Chat, "💬 Chat");
                ui.selectable_value(&mut self.active_tab, IdeTab::Files, "📁 Files");
                ui.selectable_value(&mut self.active_tab, IdeTab::Logs, "📋 Logs");
                ui.selectable_value(&mut self.active_tab, IdeTab::Tools, "🔧 Tools");
                ui.selectable_value(&mut self.active_tab, IdeTab::Graph, "🗄 Graph");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} tokens | ${:.4} | {}h | {}MB | {}: {}",
                        self.data.token_count, self.data.total_cost,
                        self.data.uptime_secs / 3600, self.data.memory_mb,
                        if self.data.health_stats.is_alive { "●" } else { "○" },
                        self.data.current_model));
                });
            });
        });

        // Sidebar: file tree
        egui::SidePanel::left("file_panel").resizable(true).default_width(220.0).show(ctx, |ui| {
            ui.heading("📁 Files");
            ui.horizontal(|ui| {
                let resp = ui.add_sized([ui.available_width() - 50.0, 20.0], egui::TextEdit::singleline(&mut self.folder_input).hint_text("/path/to/project..."));
                if ui.button("📂").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    if !self.folder_input.is_empty() {
                        let p = std::path::PathBuf::from(&self.folder_input);
                        if p.is_dir() {
                            self.data.workspace_root = p.to_string_lossy().to_string();
                            self.data.refresh_file_tree();
                            self.folder_input.clear();
                        }
                    }
                }
            });
            ui.colored_label(egui::Color32::GRAY, self.data.workspace_root.clone());
            // no-op for compatibility
            if ui.button("↻ Refresh").clicked() {
                self.data.refresh_file_tree();
            }
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                for file in self.data.file_tree.clone() {
                    let is_dir = file.starts_with("📁");
                    let path: String = file.chars().skip(2).collect(); let path = path.trim().to_string();
                    if ui.selectable_label(
                        self.data.selected_file.as_deref() == Some(&path),
                        &file,
                    ).clicked() && !is_dir {
                        let full = format!("{}/{}", self.data.workspace_root, path);
                        self.data.open_file(&full);
                    }
                }
            });
        });

        // Right panel: tool trace
        egui::SidePanel::right("trace_panel").resizable(true).default_width(250.0).show(ctx, |ui| {
            ui.heading("⚡ Tool Trace");
            ui.separator();
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for trace in self.data.tool_trace.iter().rev().take(40) {
                    ui.label(trace);
                }
                if self.data.tool_trace.is_empty() {
                    ui.label("Awaiting first tool call...");
                }
            });
        });

        // Main content area
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                IdeTab::Chat => {
                    // Chat messages
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for msg in &self.data.messages {
                            let (prefix, color) = match msg.role.as_str() {
                                "user" => ("🧑 You", egui::Color32::LIGHT_GREEN),
                                "agent" => ("💊 Agent", egui::Color32::LIGHT_BLUE),
                                "tool" => ("🔧 Tool", egui::Color32::GRAY),
                                _ => ("⚙ Sys", egui::Color32::YELLOW),
                            };
                            ui.colored_label(color, format!("{}: ", prefix));
                            if let Some(thought) = &msg.thought {
                                ui.colored_label(egui::Color32::from_rgb(139, 92, 246), format!("  [{}]", thought));
                            }
                            ui.label(&msg.content);
                            if let Some(tool) = &msg.tool_name {
                                ui.label(format!("  ⚡ {}", tool));
                            }
                            ui.separator();
                        }
                    });
                    // Input bar
                    ui.separator();
                    ui.horizontal(|ui| {
                        let resp = ui.add_sized([ui.available_width() - 140.0, 24.0], egui::TextEdit::singleline(&mut self.data.input_text).hint_text("Deploy instruction..."));
                        if ui.button("Send").clicked() || (resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                            self.data.send_message();
                            ui.ctx().request_repaint();
                        }
                    });
                }
                IdeTab::Files => {
                    // Code viewer
                    if let Some(ref path) = self.data.selected_file {
                        ui.heading(path);
                        ui.separator();
                        egui::ScrollArea::both().show(ui, |ui| {
                            let mut content = self.data.file_content.clone();
                            ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut content).font(egui::TextStyle::Monospace).code_editor());
                        });
                    } else {
                        ui.centered_and_justified(|ui| {
                            ui.heading("📁 Select a file from the left panel");
                        });
                    }
                }
                IdeTab::Logs => {
                    egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                        for log in &self.data.system_logs {
                            let color = if log.contains("ERR") { egui::Color32::RED }
                                else if log.contains("WARN") { egui::Color32::YELLOW }
                                else { egui::Color32::GRAY };
                            ui.colored_label(color, log);
                        }
                    });
                }
                IdeTab::Tools => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading(format!("🔧 Tools ({} registered)", self.data.tools.len()));
                        ui.separator();
                        for tool in &self.data.tools {
                            ui.label(format!("{}: {}", tool.name, tool.description));
                        }
                    });
                }
                IdeTab::Graph => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.heading("🗄 Knowledge Nexus");
                        ui.separator();
                        if self.data.graph_relations.is_empty() {
                            ui.label("No graph relations loaded.");
                        } else {
                            for rel in &self.data.graph_relations {
                                ui.label(rel);
                            }
                        }
                    });
                }
            }
        });

        // Bottom status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(if self.data.health_stats.is_alive { "● LIVE" } else { "○ OFFLINE" });
                ui.label(format!(" | {} tokens | ${:.4} | {} tools | {} active swarms",
                    self.data.token_count, self.data.total_cost,
                    self.data.tools.len(), self.data.active_swarms.len()));
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}
