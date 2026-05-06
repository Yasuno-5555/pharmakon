use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AcpMessage {
    // Handshake
    Initialize {
        client_id: String,
        version: String,
    },
    Initialized {
        server_version: String,
        capabilities: Vec<String>,
    },

    // Core Bridge Flow
    Prompt {
        session_id: String,
        message: String,
    },
    Cancel {
        session_id: String,
    },
    ListSessions,
    Sessions {
        sessions: Vec<Value>,
    },

    // Control
    StartSession {
        session_id: String,
        model: Option<String>,
    },
    StopSession {
        session_id: String,
    },
    UpdateSoul {
        traits: Option<Vec<String>>,
        system_prompt: Option<String>,
    },
    GetConfig,
    Config {
        data: Value,
    },

    // Approval
    ApprovalRequest {
        id: String,
        tool: String,
        args: Value,
    },
    ApprovalResponse {
        id: String,
        approved: bool,
    },

    // Events
    Event {
        session_id: String,
        event: pharmakon_common::Event,
    },

    // Error
    Error {
        code: i32,
        message: String,
    },
}

pub mod server;
