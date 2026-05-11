use anyhow::Result;
use axum::{
    Json, Router,
    extract::State,
    extract::ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
};
use futures::{SinkExt, StreamExt};
use pharmakon_common::{Event, Request};
use pharmakon_core::agent::Agent;
use serde::Serialize;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use crate::channels::Channel;

pub mod acp;
pub mod api;
pub mod auth;
pub mod canvas;
pub mod pairing;
pub mod webhooks;
pub mod channels;
pub mod ui;

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    version: String,
    name: String,
}

pub struct Gateway {
    pub port: u16,
    pub channels: Vec<Arc<dyn channels::Channel + Send + Sync>>,
    pub agent: Arc<Agent>,
    pub canvas_host: Arc<canvas::CanvasHost>,
    pub cron_manager: Arc<pharmakon_core::automation::cron::CronManager>,
    pub config: pharmakon_common::Config,
}

use axum::response::Html;

async fn serve_ui() -> impl IntoResponse {
    let home = dirs::home_dir().expect("Could not find home directory");
    let ui_path = home.join(".pharmakon").join("ui").join("index.html");
    if let Ok(contents) = tokio::fs::read_to_string(ui_path).await {
        Html(contents)
    } else {
        Html("<h1>UI not found</h1><p>Please run the frontend build and place the output in ~/.pharmakon/ui</p>".to_string())
    }
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

    pub async fn init_tools(&self) -> Result<()> {
        pharmakon_core::tool_init::init_all_agent_tools(&self.agent).await?;

        // Gateway-specific: Brave search with API key, if available
        if let Ok(key) = std::env::var("BRAVE_SEARCH_API_KEY") {
            use pharmakon_tools::search::BraveSearchTool;
            self.agent.add_tool(Arc::new(BraveSearchTool::new(key))).await;
        }

        Ok(())
    }

    pub fn add_channel(&mut self, channel: Arc<dyn channels::Channel + Send + Sync>) {
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
            .route("/tools", get(api::get_tools))
            .route("/agent/chat", axum::routing::post(api::agent_chat))
            .route("/state", get(api::get_state))
            .layer(axum::middleware::from_fn(auth::auth_middleware));

        let mut app = Router::new()
            .route("/", get(serve_ui))
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
        log::info!("Gateway listening on http://{}", addr);
        log::warn!("Gateway is using plain HTTP. Set PHARMAKON_GATEWAY_TLS_CERT and PHARMAKON_GATEWAY_TLS_KEY env vars for HTTPS.");
        log::warn!("For production, place a reverse proxy (nginx/caddy) in front of the gateway for TLS termination.");

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
        Arc<pharmakon_core::agent::Agent>,
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
        Arc<pharmakon_core::agent::Agent>,
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
                let agent_clone_inner = agent_clone.clone();
                match req {
                    Request::SendMessage { message } => {
                        tokio::spawn(async move {
                            let agent_lock = agent_clone_inner;
                            if let Err(e) = agent_lock.chat(&message).await {
                                let _ = agent_lock.event_tx.send(Event::Error {
                                    message: e.to_string(),
                                });
                            }
                        });
                    }
                    Request::ProvideApproval { id, approved } => {
                        let agent_lock = agent_clone_inner;
                        let _ = agent_lock.approval_tx.send((id, approved));
                    }
                    Request::GetStatus => {
                        // Status handled via HTTP
                    }
                    Request::ResetHistory => {
                        let _ = agent_clone_inner.reset_history().await;
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
                        let agent_lock = agent_clone_inner;
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::CancelCronJob { id } => {
                        if let Err(e) = cron_manager.cancel_job(&id).await {
                            log::error!("Failed to cancel cron job: {}", e);
                        } else {
                            let jobs = cron_manager.list_jobs().await;
                            let event = Event::CronJobList { jobs };
                            let agent_lock = agent_clone_inner;
                            let _ = agent_lock.event_tx.send(event);
                        }
                    }
                    Request::GetSessions => {
                        let agent_lock = agent_clone_inner;
                        let sessions: Vec<String> = if let Some(store) = &agent_lock.session_store {
                            store.list_sessions().await.unwrap_or_default()
                        } else {
                            vec!["default".to_string()]
                        };
                        let _ = agent_lock.event_tx.send(Event::SessionList { sessions });
                    }
                    Request::SwitchSession { id } => {
                        let agent_lock = agent_clone_inner;
                        agent_lock.set_session_id(id.clone()).await;
                        let _ = agent_lock.reset_history().await;

                        // Load history for the new session
                        let history = if let Some(store) = &agent_lock.session_store {
                            store.load_history(&id).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let _ = agent_lock.replace_history(history.clone()).await;
                        let _ = agent_lock
                            .event_tx
                            .send(Event::HistoryList { messages: history });
                        let _ = agent_lock
                            .event_tx
                            .send(Event::Action("Session switched".to_string()));
                    }
                    Request::GetHistory { session_id } => {
                        let agent_lock = agent_clone_inner;
                        let history = if let Some(store) = &agent_lock.session_store {
                            store.load_history(&session_id).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let _ = agent_lock
                            .event_tx
                            .send(Event::HistoryList { messages: history });
                    }
                    Request::SearchSessions { query } => {
                        let agent_lock = agent_clone_inner;
                        let sessions = if let Some(store) = &agent_lock.session_store {
                            store.search_sessions(&query).await.unwrap_or_default()
                        } else {
                            Vec::new()
                        };
                        let _ = agent_lock.event_tx.send(Event::SessionList { sessions });
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
                        let agent_lock = agent_clone_inner;
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetGatewayStatus => {
                        let agent_lock = agent_clone_inner;
                        let uptime = agent_lock.start_time.elapsed().as_secs();

                        use sysinfo::System;
                        let mut sys = System::new_all();
                        sys.refresh_all();
                        let memory_usage = sys.used_memory();

                        let event = Event::GatewayStatus {
                            uptime,
                            connected_clients: 1,
                            memory_usage,
                        };
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetMcpStats => {
                        let agent_lock = agent_clone_inner;
                        let counts = agent_lock.tool_call_counts.lock().await;
                        let stats = counts
                            .iter()
                            .map(|(name, count)| pharmakon_common::McpStatEntry {
                                name: name.clone(),
                                call_count: *count,
                            })
                            .collect();
                        let event = Event::McpStats { stats };
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetTools => {
                        let agent_lock = agent_clone_inner;
                        let reg = agent_lock.registry.lock().await;
                        let tools = reg.all_metadata()
                            .iter()
                            .map(|t| pharmakon_common::ToolInfo {
                                name: t.name.clone(),
                                description: t.description.clone(),
                                parameters: serde_json::Value::Null, 
                            })
                            .collect();
                        let event = Event::ToolList { tools };
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetUsageHistory => {
                        let agent_lock = agent_clone_inner;
                        let history_lock = agent_lock.usage_history.lock().await;
                        let history = history_lock
                            .iter()
                            .map(|(ts, tokens, cost)| pharmakon_common::UsageEntry {
                                timestamp: ts.format("%H:%M").to_string(),
                                tokens: *tokens,
                                cost: *cost,
                            })
                            .collect();
                        let event = Event::UsageHistory { history };
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetResearchNotebook => {
                        let agent_lock = agent_clone_inner;
                        let notebook = agent_lock.research_notebook.lock().await.clone();
                        let event = Event::ResearchNotebookUpdate { notebook };
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetSettings => {
                        // For now, return basic settings
                        let event = Event::SettingsUpdate {
                            settings: serde_json::json!({
                                "model": "gemini-2.0-flash",
                                "temperature": 0.7,
                                "auto_approval": false,
                                "max_tokens": 100000
                            }),
                        };
                        let agent_lock = agent_clone_inner;
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::UpdateSettings { settings } => {
                        log::info!("Settings updated: {:?}", settings);
                        let event = Event::Action("Settings updated successfully".to_string());
                        let agent_lock = agent_clone_inner;
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetVisionFrames => {
                        // vision_stream removed from Agent
                        let _ = agent_clone_inner.event_tx.send(Event::Error {
                            message: "Vision stream is currently disabled.".to_string(),
                        });
                    }
                    Request::GetGraphMemory { query } => {
                        let agent_lock = agent_clone_inner;
                        if let Some(graph) = &agent_lock.graph_store
                            && let Ok(relations) = graph.query_relations(&query).await {
                                let relations_str = relations
                                    .into_iter()
                                    .map(|(n, e)| {
                                        format!("{} -> {} ({})", e.from_id, n.label, e.relation)
                                    })
                                    .collect();
                                let _ = agent_lock.event_tx.send(Event::GraphUpdate {
                                    relations: relations_str,
                                });
                            }
                    }
                    Request::GetModels => {
                        let models = pharmakon_core::providers::registry::ModelRegistry::list_available_models();
                        let agent_lock = agent_clone_inner;
                        let _ = agent_lock.event_tx.send(Event::ModelList { models });
                    }
                    Request::SwitchModel { model_id } => {
                        let agent_lock = agent_clone_inner;
                        if let Some(model) =
                            pharmakon_core::providers::registry::ModelRegistry::get_model(&model_id)
                        {
                            let _ = agent_lock.update_model(model).await;
                            let _ = agent_lock.event_tx.send(Event::ModelSwitched { model_id });
                        } else {
                            let _ = agent_lock.event_tx.send(Event::Error {
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