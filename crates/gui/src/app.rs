use std::sync::Arc;
use tokio::sync::Mutex;
use std::collections::VecDeque;
use pharmakon_core::agent::Agent;
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::automation::cron::CronManager;

use std::sync::atomic::AtomicBool;

/// The core state of the Pharmakon Premium Dashboard
pub struct AppData {
    pub current_view: ViewType,
    pub input_text: String,
    pub messages: Vec<Message>,
    pub tool_trace: Vec<ToolExecution>,
    pub active_swarms: Vec<SwarmStatus>,
    pub health_stats: HealthStats,
    pub event_log: VecDeque<String>,
    pub agent: Arc<Mutex<Agent>>,
    pub db: Arc<DbSessionStore>,
    pub cron_manager: Arc<CronManager>,
    pub is_window_open: bool,
    pub show_requested: Arc<AtomicBool>,
    pub main_window_id: xilem::WindowId,
}

#[derive(Clone, PartialEq)]
pub struct ToolExecution {
    pub name: String,
    pub status: String,
    pub duration: String,
}

#[derive(Clone, PartialEq)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub thought: Option<String>,
    pub context_used: Vec<String>,
}

#[derive(Clone, Default, PartialEq)]
pub struct HealthStats {
    pub cpu_usage: f32,
    pub memory_usage: u64,
    pub failure_rate: f32,
    pub last_latency: String,
    pub is_healthy: bool,
    pub is_alive: bool,
}

#[derive(Clone, PartialEq)]
pub struct SwarmStatus {
    pub id: String,
    pub role: String,
    pub status: String,
}

#[derive(PartialEq, Clone, Copy, Default)]
pub enum ViewType {
    #[default]
    Chat,
    Settings,
}

impl AppData {
    pub fn new(
        agent: Arc<Mutex<Agent>>,
        db: Arc<DbSessionStore>,
        cron_manager: Arc<CronManager>,
    ) -> Self {
        Self {
            current_view: ViewType::Chat,
            input_text: String::new(),
            messages: Vec::new(),
            tool_trace: Vec::new(),
            active_swarms: Vec::new(),
            health_stats: HealthStats::default(),
            event_log: VecDeque::new(),
            agent,
            db,
            cron_manager,
            is_window_open: true,
            show_requested: Arc::new(AtomicBool::new(false)),
            main_window_id: xilem::WindowId::next(),
        }
    }

    pub fn send_message(&mut self) {
        if self.input_text.trim().is_empty() { return; }
        
        let user_msg = Message {
            role: "user".to_string(),
            content: self.input_text.clone(),
            thought: None,
            context_used: Vec::new(),
        };
        self.messages.push(user_msg);
        let message_to_send = self.input_text.clone();
        self.input_text.clear();
        
        let agent = self.agent.clone();
        tokio::spawn(async move {
            let mut agent_lock = agent.lock().await;
            let _ = agent_lock.chat(&message_to_send).await;
        });
    }
}

pub fn app_logic(data: &mut AppData) -> Vec<xilem::WindowView<AppData>> {
    use xilem::window;
    use std::sync::atomic::Ordering;
    
    if data.show_requested.swap(false, Ordering::SeqCst) {
        data.is_window_open = true;
    }

    let mut windows = Vec::new();
    if data.is_window_open {
        windows.push(window(
            data.main_window_id,
            "Pharmakon Dashboard",
            main_dashboard_view(data)
        ).with_options(|_| {
            xilem::WindowOptions::new("Pharmakon Premium Dashboard")
                .on_close(|data: &mut AppData| {
                    data.is_window_open = false;
                })
        }));
    }
    
    windows
}

pub fn main_dashboard_view(data: &mut AppData) -> impl xilem::WidgetView<AppData> + use<> {
    use xilem::view::{flex_col, flex_row, label, text_button, text_input, sized_box, FlexExt};
    use xilem::style::Style;
    use xilem::FontWeight;

    // Chat Message List
    let messages_view = flex_col(
        data.messages.iter().map(|msg| {
            flex_col((
                label(format!("{}:", msg.role))
                    .text_size(12.0)
                    .weight(FontWeight::BOLD),
                label(msg.content.clone()),
                // Show thoughts if present
                msg.thought.as_ref().map(|thought| {
                    label(format!("Thought: {}", thought))
                        .text_size(11.0)
                        .color(xilem::palette::css::LIGHT_SLATE_GRAY)
                })
            )).padding(10.0)
        }).collect::<Vec<_>>()
    ).flex(1.0);

    // Input Area
    let input_area = flex_row((
        text_input(data.input_text.clone(), |data: &mut AppData, input| {
            data.input_text = input;
        })
        .placeholder("Type a message...")
        .flex(1.0),
        text_button("Send", |data: &mut AppData| {
            data.send_message();
        }),
    )).padding(20.0);

    // Swarm / Autonomy Matrix Sidebar
    use crate::widgets::swarm_visualizer;
    use xilem::masonry::properties::types::AsUnit;
    let swarm_sidebar = sized_box(
        flex_col((
            label("Autonomy Matrix")
                .text_size(16.0)
                .weight(FontWeight::BOLD),
            swarm_visualizer(data.active_swarms.clone()).flex(1.0),
        ))
    ).width(200.px()).padding(10.0);

    // Tool Execution Trace Sidebar (Right)
    let tool_trace_view = sized_box(
        flex_col((
            label("Tool Trace")
                .text_size(14.0)
                .weight(FontWeight::BOLD),
            flex_col(
                data.tool_trace.iter().map(|t| {
                    flex_row((
                        label(format!("⚒ {}", t.name)).text_size(11.0),
                        label(t.status.clone()).text_size(10.0).color(xilem::palette::css::GREEN_YELLOW),
                    )).padding(2.0)
                }).collect::<Vec<_>>()
            ).flex(1.0),
        ))
    ).width(180.px()).padding(10.0);

    // Event Console (Bottom)
    let console_view = sized_box(
        flex_col((
            label("System Console")
                .text_size(12.0)
                .weight(FontWeight::BOLD),
            flex_col(
                data.event_log.iter().rev().take(5).map(|log| {
                    label(format!("> {}", log)).text_size(10.0).color(xilem::palette::css::LIGHT_GRAY)
                }).collect::<Vec<_>>()
            ).flex(1.0),
        ))
    ).height(100.px()).padding(10.0);

    // Health Status Bar
    let health_bar = flex_row((
        label(format!("CPU: {}%", data.health_stats.cpu_usage)).text_size(10.0),
        label(format!("MEM: {}MB", data.health_stats.memory_usage / 1024 / 1024)).text_size(10.0),
        label(if data.health_stats.is_alive { "● ONLINE" } else { "○ OFFLINE" })
            .text_size(10.0)
            .color(if data.health_stats.is_alive { xilem::palette::css::GREEN_YELLOW } else { xilem::palette::css::RED }),
    )).padding(5.0);

    flex_col((
        flex_row((
            swarm_sidebar,
            flex_col((messages_view, input_area)).flex(1.0),
            tool_trace_view,
        )).flex(1.0),
        console_view,
        health_bar,
    ))
}
