pub mod app;
pub mod tray;
pub mod widgets;

pub use app::{AppData, UiEvent, ViewType};
use pharmakon_core::agent::Agent;
use pharmakon_core::automation::cron::CronManager;
use pharmakon_core::persistence::DbSessionStore;
use std::sync::Arc;
use tokio::sync::mpsc;
use tray::TrayHandler;

use masonry_winit::app::MasonryUserEvent;
use std::sync::atomic::Ordering;
use tray_icon::menu::MenuEvent;
use xilem::{EventLoop, Xilem};

fn app_logic_wrapper(data: &mut AppData) -> std::vec::IntoIter<xilem::WindowView<AppData>> {
    app::app_logic(data).into_iter()
}

impl xilem::AppState for AppData {
    fn keep_running(&self) -> bool {
        true
    }
}

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
                Ok(pharmakon_common::Event::SessionList { sessions }) => {
                    let _ = tx.send(UiEvent::SessionList(sessions));
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
                Ok(pharmakon_common::Event::ResearchNotebookUpdate { notebook }) => {
                    let mapped = app::ResearchNotebookData {
                        current_goal: notebook.current_goal,
                        verified_facts: notebook.verified_facts.into_iter().map(|f| app::FactData {
                            content: f.content,
                            source_url: f.source_url,
                            confidence: f.confidence,
                        }).collect(),
                        pending_questions: notebook.pending_questions,
                        research_tree: notebook.research_tree.into_iter().collect(),
                        dead_ends: notebook.dead_ends,
                    };
                    let _ = tx.send(UiEvent::ResearchNotebookUpdate(mapped));
                }
                Ok(pharmakon_common::Event::GraphUpdate { relations }) => {
                    let _ = tx.send(UiEvent::GraphUpdate(relations));
                }
                Ok(pharmakon_common::Event::SettingsUpdate { settings }) => {
                    let _ = tx.send(UiEvent::SettingsUpdate(settings));
                }
                Ok(pharmakon_common::Event::CronJobList { jobs }) => {
                    let mapped: Vec<app::CronJobInfoData> = jobs.into_iter().map(|j| app::CronJobInfoData {
                        id: j.id,
                        schedule_type: j.schedule_type,
                        expr: j.expr,
                        message: j.message,
                    }).collect();
                    let _ = tx.send(UiEvent::CronJobList(mapped));
                }
                Ok(pharmakon_common::Event::UsageHistory { history }) => {
                    let mapped: Vec<app::UsageEntry> = history.into_iter().map(|h| app::UsageEntry {
                        timestamp: h.timestamp,
                        tokens: h.tokens,
                        cost: h.cost,
                    }).collect();
                    let _ = tx.send(UiEvent::UsageHistory(mapped));
                }
                Ok(pharmakon_common::Event::ForensicLog { id, action, hypothesis, observation }) => {
                    let _ = tx.send(UiEvent::ForensicLog {
                        action: format!("{}/{}", action, id),
                        hypothesis,
                        observation,
                    });
                }
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("GUI event bridge lagged by {} messages", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    log::info!("GUI event bridge: broadcast channel closed");
                    break;
                }
            }
        }
    });
}

pub fn run_app(
    agent: Arc<Agent>,
    db: Arc<DbSessionStore>,
    cron_manager: Arc<CronManager>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_event_bridge(agent.clone(), tx);

    let app_data = AppData::new(agent, db, cron_manager, rx);

    let event_loop = EventLoop::with_user_event().build()?;
    let proxy = event_loop.create_proxy();

    let _tray = TrayHandler::new();

    let proxy_clone = proxy.clone();
    let show_requested = app_data.show_requested.clone();
    tokio::spawn(async move {
        let receiver = MenuEvent::receiver();
        while let Ok(_event) = receiver.recv() {
            show_requested.store(true, Ordering::SeqCst);
            let _ = proxy_clone.send_event(MasonryUserEvent::Action(
                xilem::WindowId::next(),
                Box::new(()),
                xilem::masonry::core::WidgetId::reserved(0),
            ));
        }
    });

    let app = Xilem::new(app_data, app_logic_wrapper);

    let (driver, windows) =
        app.into_driver_and_windows(move |event| proxy.send_event(event).map_err(|err| err.0));

    masonry_winit::app::run_with(
        event_loop,
        windows,
        driver,
        xilem::masonry::theme::default_property_set(),
    )?;

    Ok(())
}
