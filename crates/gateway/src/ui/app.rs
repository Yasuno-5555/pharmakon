use pharmakon_core::agent::Agent;
use pharmakon_core::automation::cron::CronManager;
use pharmakon_core::persistence::DbSessionStore;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::Mutex;

use std::sync::atomic::AtomicBool;
use chrono::Utc;

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
    pub sessions: Vec<String>,
    pub current_session: String,
    pub search_query: String,
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
            sessions: Vec::new(),
            current_session: "default".to_string(),
            search_query: String::new(),
        }
    }

    pub fn send_message(&mut self) {
        if self.input_text.trim().is_empty() {
            return;
        }

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
        let session_id = self.current_session.clone();
        tokio::spawn(async move {
            let agent_lock = agent.lock().await;
            agent_lock.set_session_id(session_id).await;
            let _ = agent_lock.chat(&message_to_send).await;
        });
    }

    pub fn switch_session(&mut self, session_id: String) {
        self.current_session = session_id.clone();
        self.messages.clear();

        let agent = self.agent.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            let history = db.load_history(&session_id).await.unwrap_or_default();
            let agent_lock = agent.lock().await;
            agent_lock.set_session_id(session_id).await;
            agent_lock.replace_history(history).await;
            // Note: We'd need a way to send this back to the UI thread if not polling
        });
    }

    pub fn search_sessions(&mut self, query: String) {
        self.search_query = query.clone();
        let db = self.db.clone();
        tokio::spawn(async move {
            let _results = db.search_sessions(&query).await.unwrap_or_default();
            // Similar to switch_session, needs UI update mechanism
        });
    }

    pub fn start_new_session(&mut self) {
        let new_id = format!("gui-session-{}", Utc::now().timestamp() % 1000000);
        self.sessions.push(new_id.clone());
        self.switch_session(new_id);
    }
}

pub fn app_logic(data: &mut AppData) -> Vec<xilem::WindowView<AppData>> {
    use std::sync::atomic::Ordering;
    use xilem::window;

    if data.show_requested.swap(false, Ordering::SeqCst) {
        data.is_window_open = true;
    }

    let mut windows = Vec::new();
    if data.is_window_open {
        windows.push(
            window(
                data.main_window_id,
                "Pharmakon Dashboard",
                main_dashboard_view(data),
            )
            .with_options(|_| {
                xilem::WindowOptions::new("Pharmakon Premium Dashboard").on_close(
                    |data: &mut AppData| {
                        data.is_window_open = false;
                    },
                )
            }),
        );
    }

    windows
}

pub fn main_dashboard_view(data: &mut AppData) -> impl xilem::WidgetView<AppData> + use<> {
    use xilem::FontWeight;
    use xilem::style::Style;
    use xilem::view::{FlexExt, flex_col, flex_row, label, sized_box, text_button, text_input};

    // Chat Message List
    let mut msg_views = Vec::new();
    for msg in &data.messages {
        let content = if let Some(thought) = &msg.thought {
            format!("Thought: {}\n\n{}", thought, msg.content)
        } else {
            msg.content.clone()
        };

        msg_views.push(
            flex_col((
                label(format!("{}:", msg.role))
                    .text_size(12.0)
                    .weight(FontWeight::BOLD),
                label(content),
            ))
            .padding(10.0),
        );
    }
    let messages_view = flex_col(msg_views).flex(1.0);

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
    ))
    .padding(20.0);

    // Swarm / Autonomy Matrix Sidebar
    use super::widgets::swarm_visualizer;
    use xilem::masonry::properties::types::AsUnit;
    let swarm_sidebar = sized_box(flex_col((
        label("Autonomy Matrix")
            .text_size(16.0)
            .weight(FontWeight::BOLD),
        swarm_visualizer(data.active_swarms.clone()).flex(1.0),
    )))
    .width(200.px())
    .padding(10.0);

    // Tool Execution Trace Sidebar (Right)
    let tool_trace_view = sized_box(flex_col((
        label("Tool Trace").text_size(14.0).weight(FontWeight::BOLD),
        flex_col(
            data.tool_trace
                .iter()
                .map(|t| {
                    flex_row((
                        label(format!("⚒ {}", t.name)).text_size(11.0),
                        label(t.status.clone())
                            .text_size(10.0)
                            .color(xilem::palette::css::GREEN_YELLOW),
                    ))
                    .padding(2.0)
                })
                .collect::<Vec<_>>(),
        )
        .flex(1.0),
    )))
    .width(180.px())
    .padding(10.0);

    // Event Console (Bottom)
    let console_view = sized_box(flex_col((
        label("System Console")
            .text_size(12.0)
            .weight(FontWeight::BOLD),
        flex_col(
            data.event_log
                .iter()
                .rev()
                .take(5)
                .map(|log| {
                    label(format!("> {}", log))
                        .text_size(10.0)
                        .color(xilem::palette::css::LIGHT_GRAY)
                })
                .collect::<Vec<_>>(),
        )
        .flex(1.0),
    )))
    .height(100.px())
    .padding(10.0);

    // Health Status Bar
    let health_bar = flex_row((
        label(format!("CPU: {}%", data.health_stats.cpu_usage)).text_size(10.0),
        label(format!(
            "MEM: {}MB",
            data.health_stats.memory_usage / 1024 / 1024
        ))
        .text_size(10.0),
        label(if data.health_stats.is_alive {
            "● ONLINE"
        } else {
            "○ OFFLINE"
        })
        .text_size(10.0)
        .color(if data.health_stats.is_alive {
            xilem::palette::css::GREEN_YELLOW
        } else {
            xilem::palette::css::RED
        }),
    ))
    .padding(5.0);

    // Session Sidebar (Left)
    let session_sidebar = sized_box(flex_col((
        flex_row((
            label("SESSIONS").text_size(14.0).weight(FontWeight::BOLD),
            text_button("+", |data: &mut AppData| {
                data.start_new_session();
            }),
        ))
        .padding(5.0),
        text_input(data.search_query.clone(), |data: &mut AppData, q| {
            data.search_sessions(q);
        })
        .placeholder("Search..."),
        flex_col(
            data.sessions
                .iter()
                .cloned()
                .map(|s| {
                    let s_id = s.clone();
                    text_button(s, move |data: &mut AppData| {
                        data.switch_session(s_id.clone());
                    })
                })
                .collect::<Vec<_>>(),
        )
        .flex(1.0),
    )))
    .width(160.px())
    .padding(10.0);

    flex_col((
        flex_row((
            session_sidebar,
            swarm_sidebar,
            flex_col((messages_view, input_area)).flex(1.0),
            tool_trace_view,
        ))
        .flex(1.0),
        console_view,
        health_bar,
    ))
}
