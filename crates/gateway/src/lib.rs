use axum::{
    routing::get,
    Router,
    Json,
    extract::ws::{WebSocketUpgrade, WebSocket, Message as WsMessage},
    extract::State,
    response::IntoResponse,
};
use serde::Serialize;
use pharmakon_common::{Event, Request};
use std::net::SocketAddr;
use anyhow::Result;
use tokio::net::TcpListener;
use pharmakon_channels::Channel;
use pharmakon_core::agent::Agent;
use std::sync::Arc;
use tokio::sync::Mutex;
use futures::{StreamExt, SinkExt};
pub mod pairing;
pub mod webhooks;
pub mod canvas;
pub mod acp;

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    version: String,
    name: String,
}

pub struct Gateway {
    pub port: u16,
    pub channels: Vec<Arc<dyn Channel>>,
    pub agent: Arc<Mutex<Agent>>,
    pub canvas_host: Arc<canvas::CanvasHost>,
    pub cron_manager: Arc<pharmakon_core::automation::cron::CronManager>,
    pub config: pharmakon_common::Config,
}

impl Gateway {
    pub fn new(port: u16, agent: Arc<Mutex<Agent>>, cron_manager: Arc<pharmakon_core::automation::cron::CronManager>, config: pharmakon_common::Config) -> Self {
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

        let home = dirs::home_dir().expect("Could not find home directory");
        let ui_dir = home.join(".pharmakon").join("ui");
        
        let mut app = Router::new()
            .route("/", get(Self::root))
            .route("/status", get(Self::status))
            .route("/health", get(Self::health))
            .route("/ws", get(ws_handler))
            .route("/acp", get(acp_handler))
            .route("/webhooks/{id}", axum::routing::post(webhooks::webhook_handler));

        if ui_dir.exists() {
            log::info!("Serving UI from {:?}", ui_dir);
            app = app.fallback_service(tower_http::services::ServeDir::new(ui_dir));
        }

        let app = app.layer(cors)
            .with_state((self.agent.clone(), self.canvas_host.clone(), self.cron_manager.clone(), Arc::new(self.config.clone())));

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
    State((agent, canvas_host, cron_manager, _config)): State<(Arc<Mutex<pharmakon_core::agent::Agent>>, Arc<canvas::CanvasHost>, Arc<pharmakon_core::automation::cron::CronManager>, Arc<pharmakon_common::Config>)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, agent, canvas_host, cron_manager))
}

async fn acp_handler(
    ws: WebSocketUpgrade,
    State((agent, _, _, _)): State<(Arc<Mutex<pharmakon_core::agent::Agent>>, Arc<canvas::CanvasHost>, Arc<pharmakon_core::automation::cron::CronManager>, Arc<pharmakon_common::Config>)>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| crate::acp::server::handle_acp_socket(socket, agent))
}

async fn handle_socket(
    socket: WebSocket, 
    agent: Arc<Mutex<pharmakon_core::agent::Agent>>, 
    canvas_host: Arc<canvas::CanvasHost>,
    cron_manager: Arc<pharmakon_core::automation::cron::CronManager>
) {
    tracing::debug!("WebSocket connection established.");
    let mut rx = {
        let agent_lock = agent.lock().await;
        agent_lock.event_tx.subscribe()
    };

    let (mut sender, mut receiver) = socket.split();

    // Send initial canvas state
    let initial_state = canvas_host.get_state();
    for primitive in initial_state.elements {
        let msg = serde_json::to_string(&pharmakon_common::Event::CanvasUpdate { primitive }).unwrap();
        let _ = sender.send(WsMessage::Text(msg.into())).await;
    }

    // Task to send events to client
    let canvas_host_clone = canvas_host.clone();
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            // Update canvas host state if it's a canvas event
            canvas_host_clone.handle_event(&event);
            
            let msg = serde_json::to_string(&event).unwrap();
            tracing::debug!(target: "gateway", "Sending event: {}", msg);
            if let Err(e) = sender.send(WsMessage::Text(msg.into())).await {
                tracing::error!("WebSocket send error: {}", e);
                break;
            }
        }
    });

    // Task to receive requests from client
    let agent_clone = agent.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(text))) = receiver.next().await {
            tracing::debug!(target: "gateway", "Received request: {}", text);
            if let Ok(req) = serde_json::from_str::<Request>(&text) {
                match req {
                    Request::SendMessage { message } => {
                        let mut agent_lock = agent_clone.lock().await;
                        if let Err(e) = agent_lock.chat(&message).await {
                            let _ = agent_lock.event_tx.send(Event::Error { message: e.to_string() });
                        }
                    }
                    Request::ProvideApproval { id, approved } => {
                        let agent_lock = agent_clone.lock().await;
                        let _ = agent_lock.approval_tx.send((id, approved));
                    }
                    Request::GetStatus => {
                        // Status handled via HTTP but could add WS status here
                    }
                    Request::ResetHistory => {
                        let mut agent_lock = agent_clone.lock().await;
                        agent_lock.reset_history();
                    }
                    Request::InteractiveResponse { element_id, action, value } => {
                        log::info!("Interactive response received: id={}, action={}, value={:?}", element_id, action, value);
                    }
                    Request::GetCronJobs => {
                        let jobs = cron_manager.list_jobs().await;
                        let event = Event::CronJobList { jobs };
                        let agent_lock = agent_clone.lock().await;
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::CancelCronJob { id } => {
                        if let Err(e) = cron_manager.cancel_job(&id).await {
                            log::error!("Failed to cancel cron job: {}", e);
                        } else {
                            let jobs = cron_manager.list_jobs().await;
                            let event = Event::CronJobList { jobs };
                            let agent_lock = agent_clone.lock().await;
                            let _ = agent_lock.event_tx.send(event);
                        }
                    }
                    Request::GetSessions => {
                        let agent_lock = agent_clone.lock().await;
                        let sessions: Vec<String> = if let Some(store) = &agent_lock.session_store {
                            store.list_sessions().await.unwrap_or_default()
                        } else {
                            vec!["default".to_string()]
                        };
                        let _ = agent_lock.event_tx.send(Event::SessionList { sessions });
                    }
                    Request::SwitchSession { id } => {
                        let mut agent_lock = agent_clone.lock().await;
                        agent_lock.session_id = id;
                        let _ = agent_lock.event_tx.send(Event::Action("Session switched".to_string()));
                    }
                    Request::GetOrchestration => {
                        let agent_lock = agent_clone.lock().await;
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
                                }
                            ]
                        };
                        let _ = agent_lock.event_tx.send(event);
                    }
                    Request::GetGatewayStatus => {
                        let agent_lock = agent_clone.lock().await;
                        let event = Event::GatewayStatus {
                            uptime: 3600, // Dummy
                            connected_clients: 1, // Dummy
                            memory_usage: 128 * 1024 * 1024, // Dummy
                        };
                        let _ = agent_lock.event_tx.send(event);
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
    tracing::debug!("WebSocket connection closed.");
}
