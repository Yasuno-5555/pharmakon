use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use pharmakon_channels::Channel;
use pharmakon_common::{Event, Request};
use pharmakon_core::agent::Agent;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
pub mod acp;
pub mod api;
pub mod auth;
pub mod canvas;
pub mod pairing;
pub mod webhooks;

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    version: String,
    name: String,
}

pub struct Gateway {
    pub port: u16,
    pub channels: Vec<Arc<dyn Channel>>,
    pub agent: Arc<Agent>,
    pub canvas_host: Arc<canvas::CanvasHost>,
    pub cron_manager: Arc<pharmakon_core::automation::cron::CronManager>,
    pub config: pharmakon_common::Config,
}

impl Gateway {
    pub fn new(
        port: u16,
        agent: Arc<Agent>,
        cron_manager: Arc<pharmakon_core::automation::cron::CronManager>,
        config: pharmakon_common::Config,
    ) -> Self {
        Self {
            port,
            channels: Vec::new(),
            agent,
            canvas_host: Arc::new(canvas::CanvasHost::new()),
            cron_manager,
            config,
        }
    }

    pub fn add_channel(&mut self, channel: Arc<dyn Channel>) {
        self.channels.push(channel);
    }

    pub async fn run(self) -> Result<()> {
        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any);

        let compression = tower_http::compression::CompressionLayer::new();
        let cache_control = tower_http::set_header::SetResponseHeaderLayer::if_not_present(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("public, max-age=3600"),
        );

        let home = dirs::home_dir().expect("Could not find home directory");
        let ui_dir = home.join(".pharmakon").join("ui");

        let api_v1 = Router::new()
            .route("/tools/execute", axum::routing::post(api::execute_tool))
            .route("/agent/chat", axum::routing::post(api::agent_chat))
            .route("/state", get(api::get_state))
            .layer(axum::middleware::from_fn(auth::auth_middleware));

        let mut app = Router::new()
            .route("/", get(Self::root))
            .route("/status", get(Self::status))
            .route("/health", get(Self::health))
            .nest("/api/v1", api_v1)
            .route("/ws", get(ws_handler))
            .route("/acp", get(acp_handler))
            .route(
                "/webhooks/{id}",
                axum::routing::post(webhooks::webhook_handler),
            );

        if ui_dir.exists() {
            log::info!("Serving UI from {:?}", ui_dir);
            app = app.fallback_service(tower_http::services::ServeDir::new(ui_dir));
        }

        let app = app
            .layer(cors)
            .layer(compression)
            .layer(cache_control)
            .with_state((
                self.agent.clone(),
                self.canvas_host.clone(),
                self.cron_manager.clone(),
                Arc::new(self.config.clone()),
            ));

        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        log::info!("Gateway listening on {}", addr);

        // (CronManager is now managed externally via CronTool)

        // Run channels in background
        for channel in self.channels {
            let channel_agent = self.agent.clone();
            tokio::spawn(async move {
                if let Err(e) = channel.run(channel_agent).await {
                    log::error!("Channel error: {}", e);
                }
            });
        }

        let listener = TcpListener::bind(addr).await?;
        axum::serve(listener, app).await?;

        Ok(())
    }

    async fn root() -> &'static str {
        "Pharmakon Gateway is running."
    }

    async fn status() -> Json<StatusResponse> {
        Json(StatusResponse {
            status: "OK".to_string(),
            version: "0.1.0".to_string(),
            name: "Pharmakon".to_string(),
        })
    }

    async fn health() -> impl IntoResponse {
        axum::http::StatusCode::OK
    }
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State((agent, canvas_host, cron_manager, _config)): State<(
        Arc<Mutex<pharmakon_core::agent::Agent>>,
        Arc<canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
) -> impl IntoResponse {
    tracing::info!("WebSocket upgrade request received!");
    ws.on_upgrade(move |socket| handle_socket(socket, agent, canvas_host, cron_manager))
}

async fn acp_handler(
    ws: WebSocketUpgrade,
    State((agent, _, _, _)): State<(
        Arc<Mutex<pharmakon_core::agent::Agent>>,
        Arc<canvas::CanvasHost>,
        Arc<pharmakon_core::automation::cron::CronManager>,
        Arc<pharmakon_common::Config>,
    )>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| crate::acp::server::handle_acp_socket(socket, agent))
}

async fn handle_socket(
    socket: WebSocket,
    agent: Arc<pharmakon_core::agent::Agent>,
    canvas_host: Arc<canvas::CanvasHost>,
    cron_manager: Arc<pharmakon_core::automation::cron::CronManager>,
) {
    tracing::info!("WebSocket connection established.");
    let mut rx = agent.event_tx.subscribe();

    let (mut sender, mut receiver) = socket.split();

    // Send initial canvas state
    let initial_state = canvas_host.get_state();
    for primitive in initial_state.elements {
        let msg =
            serde_json::to_string(&pharmakon_common::Event::CanvasUpdate { primitive }).unwrap();
        let _ = sender.send(WsMessage::Text(msg.into())).await;
    }

    // Task to send events to client
    let canvas_host_clone = canvas_host.clone();
    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    // Update canvas host state if it's a canvas event
                    canvas_host_clone.handle_event(&event);

                    let msg = serde_json::to_string(&event).unwrap();
                    tracing::info!(target: "gateway", "Sending event: {}", msg);
                    if let Err(e) = sender.send(WsMessage::Text(msg.into())).await {
                        tracing::error!("WebSocket send error: {}", e);
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    tracing::warn!("WebSocket: Broadcast channel lagged, skipping some events");
                    continue;
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    tracing::info!("WebSocket: Broadcast channel closed");
                    break;
                }
            }
        }
    });

    // Task to receive requests from client
    let agent_clone = agent.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(text))) = receiver.next().await {
            tracing::info!(target: "gateway", "Received request: {}", text);
            if let Ok(req) = serde_json::from_str::<Request>(&text) {
                match req {
                    Request::SendMessage { message } => {
                        let agent_spawn = agent_clone.clone();
                        tokio::spawn(async move {
                            if let Err(e) = agent_spawn.chat(&message).await {
                                let _ = agent_spawn.event_tx.send(Event::Error {
                                    message: e.to_string(),
                                });
                            }
                        });
                    }
                    Request::ProvideApproval { id, approved } => {
                        let _ = agent_clone.approval_tx.send((id, approved));
                    }
                    Request::GetStatus => {
                        // Status handled via HTTP
                    }
                    Request::ResetHistory => {
                        agent_clone.reset_history();
                    }
                    Request::InteractiveResponse {
                        element_id,
                        action,
                        value,
                    } => {
                        log::info!(
                            "Interactive response received: id={}, action={}, value={:?}",
                            element_id,
                            action,
                            value
                        );
                    }
                    Request::GetCronJobs => {
                        let jobs = cron_manager.list_jobs().await;
                        let event = Event::CronJobList { jobs };
                        let _ = agent_clone.event_tx.send(event);
                    }
                    Request::CancelCronJob { id } => {
                        if let Err(e) = cron_manager.cancel_job(&id).await {
                            log::error!("Failed to cancel cron job: {}", e);
                        } else {
                            let jobs = cron_manager.list_jobs().await;
                            let event = Event::CronJobList { jobs };
                            let _ = agent_clone.event_tx.send(event);
                        }
                    }
                    Request::GetSessions => {
                        let sessions: Vec<String> = if let Some(store) = &agent_clone.session_store
                        {
                            store.list_sessions().await.unwrap_or_default()
                        } else {
                            vec!["default".to_string()]
                        };
                        let _ = agent_clone.event_tx.send(Event::SessionList { sessions });
                    }
                    Request::SwitchSession { id } => {
                        agent_clone.set_session_id(id.clone());
                        agent_clone.reset_history();

                        // Load history for the new session
                        let history = if let Some(store) = &agent_clone.session_store {
                            store.load_history(&id).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        };

                        agent_clone.replace_history(history.clone());
                        let _ = agent_clone
                            .event_tx
                            .send(Event::HistoryList { messages: history });
                        let _ = agent_clone
                            .event_tx
                            .send(Event::Action("Session switched".to_string()));
                    }
                    Request::GetHistory { session_id } => {
                        let history = if let Some(store) = &agent_clone.session_store {
                            store.load_history(&session_id).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let _ = agent_clone
                            .event_tx
                            .send(Event::HistoryList { messages: history });
                    }
                    Request::SearchSessions { query } => {
                        let sessions = if let Some(store) = &agent_clone.session_store {
                            store.search_sessions(&query).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let _ = agent_clone.event_tx.send(Event::SessionList { sessions });
                    }
                    Request::GetOrchestration => {
                        let event = Event::OrchestrationState {
                            supervisor_active: true,
                            sub_agents: vec![
                                pharmakon_common::SubAgentInfo {
                                    name: "Researcher".to_string(),
                                    role: "Information Retrieval".to_string(),
                                    last_task: Some("Market analysis".to_string()),
                                    status: "Idle".to_string(),
                                },
                                pharmakon_common::SubAgentInfo {
                                    name: "Coder".to_string(),
                                    role: "Software Engineering".to_string(),
                                    last_task: None,
                                    status: "Active".to_string(),
                                },
                            ],
                        };
                        let _ = agent_clone.event_tx.send(event);
                    }
                    Request::GetGatewayStatus => {
                        let event = Event::GatewayStatus {
                            uptime: 3600,                    // Dummy
                            connected_clients: 1,            // Dummy
                            memory_usage: 128 * 1024 * 1024, // Dummy
                        };
                        let _ = agent_clone.event_tx.send(event);
                    }
                    Request::GetMcpStats => {
                        let event = Event::McpStats {
                            stats: vec![pharmakon_common::McpToolStat {
                                name: "brave_search".to_string(),
                                avg_latency_ms: 450,
                                call_count: 12,
                            }],
                        };
                        let _ = agent_clone.event_tx.send(event);
                    }
                    Request::GetVisionFrames => {
                        if let Some(stream) = &agent_clone.vision_stream {
                            let stream_lock = stream.lock().await;
                            let frames = stream_lock
                                .get_recent_frames()
                                .into_iter()
                                .map(|f| pharmakon_common::VisionFrameInfo {
                                    path: f.path.to_string_lossy().to_string(),
                                    captured_at: f.captured_at.to_rfc3339(),
                                    title: f.window_title,
                                })
                                .collect();
                            let _ = agent_clone.event_tx.send(Event::VisionUpdate { frames });
                        }
                    }
                    Request::GetGraphMemory { query } => {
                        if let Some(graph) = &agent_clone.graph_store {
                            if let Ok(relations) = graph.query_relations(&query).await {
                                let _ = agent_clone.event_tx.send(Event::GraphUpdate { relations });
                            }
                        }
                    }
                    Request::GetModels => {
                        let models = pharmakon_core::providers::registry::ModelRegistry::list_available_models();
                        let _ = agent_clone.event_tx.send(Event::ModelList { models });
                    }
                    Request::SwitchModel { model_id } => {
                        if let Some(model) =
                            pharmakon_core::providers::registry::ModelRegistry::get_model(&model_id)
                        {
                            agent_clone.update_model(model);
                            let _ = agent_clone.event_tx.send(Event::ModelSwitched { model_id });
                        } else {
                            let _ = agent_clone.event_tx.send(Event::Error {
                                message: format!("Model not found: {}", model_id),
                            });
                        }
                    }
                }
            }
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
    tracing::info!("WebSocket connection closed.");
}
