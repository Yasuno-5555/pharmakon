use crate::acp::AcpMessage;
use axum::extract::ws::{Message as WsMessage, WebSocket};
use futures::{SinkExt, StreamExt};
use pharmakon_common::Event;
use pharmakon_core::agent::Agent;
use std::sync::Arc;

pub async fn handle_acp_socket(socket: WebSocket, agent: Arc<Agent>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = agent.event_tx.subscribe();

    let (tx, mut internal_rx) = tokio::sync::mpsc::channel::<AcpMessage>(32);

    let agent_clone = agent.clone();
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(event) = rx.recv() => {
                    let acp_msg = match event {
                        Event::ApprovalRequest { id, tool, args } => {
                            AcpMessage::ApprovalRequest { id, tool, args }
                        }
                        _ => {
                            AcpMessage::Event {
                                session_id: agent_clone.session_id.lock().await.clone(),
                                event
                            }
                        }
                    };
                    if let Ok(json) = serde_json::to_string(&acp_msg)
                        && sender.send(WsMessage::Text(json.into())).await.is_err() { break; }
                }
                Some(acp_msg) = internal_rx.recv() => {
                    if let Ok(json) = serde_json::to_string(&acp_msg)
                        && sender.send(WsMessage::Text(json.into())).await.is_err() { break; }
                }
            }
        }
    });

    let agent_clone = agent.clone();
    let tx_clone = tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(WsMessage::Text(text))) = receiver.next().await {
            if let Ok(acp_msg) = serde_json::from_str::<AcpMessage>(&text) {
                match acp_msg {
                    AcpMessage::ApprovalResponse { id, approved } => {
                        let _ = agent_clone.approval_tx.send((id, approved));
                    }
                    AcpMessage::Initialize { .. } => {
                        let _ = tx_clone
                            .send(AcpMessage::Initialized {
                                server_version: "0.1.0".to_string(),
                                capabilities: vec![
                                    "streaming".to_string(),
                                    "approvals".to_string(),
                                    "soul_control".to_string(),
                                ],
                            })
                            .await;
                    }
                    AcpMessage::UpdateSoul {
                        traits,
                        system_prompt,
                    } => {
                        let mut soul = agent_clone.prompt_manager.lock().await.soul().clone();
                        if let Some(t) = traits {
                            soul.traits = t;
                        }
                        if let Some(p) = system_prompt {
                            soul.system_prompt = p;
                        }
                        agent_clone.set_soul(soul).await;
                    }
                    AcpMessage::Prompt {
                        session_id: _,
                        message,
                    } => {
                        let agent_inner = agent_clone.clone();
                        tokio::spawn(async move {
                            if let Err(e) = agent_inner.chat(&message).await {
                                let _ = agent_inner.event_tx.send(Event::Error {
                                    message: e.to_string(),
                                });
                            }
                        });
                    }
                    AcpMessage::Cancel { .. } => {
                        log::warn!(
                            "ACP Cancel requested but not fully implemented in core agent loop"
                        );
                    }
                    AcpMessage::ListSessions => {
                        let _ = tx_clone.send(AcpMessage::Sessions {
                            sessions: vec![serde_json::json!({"id": "cli-default", "label": "Default Session"})]
                        }).await;
                    }
                    AcpMessage::GetConfig => {
                        if let Ok(config) = pharmakon_common::Config::load() {
                            let _ = tx_clone
                                .send(AcpMessage::Config {
                                    data: serde_json::to_value(config).unwrap_or_default(),
                                })
                                .await;
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
