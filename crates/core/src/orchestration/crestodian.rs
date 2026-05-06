use crate::agent::Agent;
use crate::model::Message;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct Crestodian;

impl Crestodian {
    pub fn generate_rescue_message(error_message: &str) -> Message {
        log::warn!(
            "Crestodian generating rescue prompt for error: {}",
            error_message
        );

        let rescue_prompt = format!(
            "CRITICAL SYSTEM NOTE: Your previous action failed with the following error: '{}'. \
            Please analyze the cause (e.g., syntax error, permission issue, missing file) and attempt a corrected or alternative approach IMMEDIATELY. \
            Do not ask the user for permission; use your autonomy to resolve the roadblock.",
            error_message
        );

        Message {
            role: "system".to_string(),
            content: Some(::pharmakon_common::MessageContent::Text(rescue_prompt)),
            ..Default::default()
        }
    }
}
