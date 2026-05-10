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
                Ok(pharmakon_common::Event::ApprovalRequest { id, tool, args }) => {
                    let _ = tx.send(UiEvent::ApprovalRequest { id, tool, args: args.to_string() });
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
    folder_input: String,
    bottom_tab: BottomTab,
    is_console_open: bool,
}

#[derive(PartialEq, Clone, Copy)]
enum BottomTab { Tools, Logs, Graph, Swarms }

impl PharmakonIde {
    fn new(data: AppData) -> Self {
        Self {
            data,
            folder_input: String::new(),
            bottom_tab: BottomTab::Tools,
            is_console_open: true,
        }
    }
}

impl eframe::App for PharmakonIde {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.data.drain_events();

        // Apply a premium dark theme on the first frame and persist
        let mut visuals = egui::Visuals::dark();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(20, 20, 22); // Background (Cursor Charcoal)
        visuals.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(39, 39, 42)); // Border (Cursor Zinc)
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(28, 28, 30);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 45, 48);
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(139, 92, 246); // Purple accent
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        ctx.set_visuals(visuals);

        // 1. Top bar: status + active model info
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.colored_label(egui::Color32::from_rgb(139, 92, 246), "💊 PHARMAKON COMPANION IDE");
                ui.separator();
                ui.label(format!("Workspace: {}", self.data.workspace_root));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.is_console_open, "📟 Collapsible Drawer");
                    ui.separator();
                    ui.colored_label(egui::Color32::LIGHT_BLUE, format!("Model: {}", self.data.current_model));
                });
            });
        });

        // 2. Left Panel: File explorer (always open)
        egui::SidePanel::left("file_panel").resizable(true).default_width(220.0).show(ctx, |ui| {
            ui.heading("📁 Workspace Files");
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
            ui.horizontal(|ui| {
                if ui.button("📂 Pick Folder").clicked() {
                    if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                        self.data.workspace_root = folder.to_string_lossy().to_string();
                        self.data.refresh_file_tree();
                        self.folder_input.clear();
                    }
                }
                if ui.button("📄 Pick File").clicked() {
                    if let Some(file) = rfd::FileDialog::new().pick_file() {
                        let full = file.to_string_lossy().to_string();
                        self.data.open_file(&full);
                    }
                }
            });
            ui.colored_label(egui::Color32::GRAY, self.data.workspace_root.clone());
            if ui.button("↻ Refresh workspace").clicked() {
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
                            ui.label(&msg.content);
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
                }
            });
        }

        // 5. Central Panel: Integrated Multi-line Code Editor (main panel)
        egui::CentralPanel::default().show(ctx, |ui| {
            let selected_file_path = self.data.selected_file.clone();
            if let Some(path) = selected_file_path {
                ui.horizontal(|ui| {
                    ui.heading(format!("📄 {}", path));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("💾 Save changes").clicked() {
                            if let Err(e) = std::fs::write(&path, &self.data.file_content) {
                                self.data.system_logs.push(format!("ERR: Failed to save {}: {}", path, e));
                            } else {
                                self.data.system_logs.push(format!("SUCCESS: Saved changes to {}", path));
                            }
                        }
                        if ui.button("⏪ Rollback code").clicked() {
                            self.data.rollback();
                        }
                    });
                });
                ui.separator();
                
                // Divvy up vertical workspace space: Upper editor area, lower telemetry runway
                let editor_height = ui.available_height() * 0.55;
                
                ui.group(|ui| {
                    ui.set_height(editor_height);
                    egui::ScrollArea::both().show(ui, |ui| {
                        ui.add_sized(ui.available_size(), egui::TextEdit::multiline(&mut self.data.file_content).font(egui::TextStyle::Monospace).code_editor());
                    });
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

        // Bottom status info
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(if self.data.health_stats.is_alive { "● LIVE" } else { "○ OFFLINE" });
                ui.label(format!(" | {} tokens | ${:.4} | {} tools active",
                    self.data.token_count, self.data.total_cost,
                    self.data.tools.len()));
            });
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}
