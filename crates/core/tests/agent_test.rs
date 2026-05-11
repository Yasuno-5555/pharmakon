#![allow(unused_imports)]

use pharmakon_core::agent::Agent;
use pharmakon_core::model::{AgentModel, AgentResult, CompletionRequest, CompletionResponse};

use async_trait::async_trait;
use std::sync::Arc;

struct MockModel;

#[async_trait]
impl AgentModel for MockModel {
    async fn complete(&self, request: CompletionRequest) -> AgentResult<CompletionResponse> {
        let last_msg = request.messages.last().unwrap();
        let content_str = last_msg
            .content
            .as_ref()
            .map(|c| c.to_string())
            .unwrap_or_default();
        let content = if content_str.contains("hello") {
            "Hi there! I am your AI assistant."
        } else {
            "I heard you."
        };

        Ok(CompletionResponse {
            content: Some(pharmakon_common::MessageContent::Text(content.to_string())),
            tool_calls: None,
            usage: None,
            finish_reason: None,
        })
    }

    fn name(&self) -> &str {
        "mock-model"
    }

    fn context_window(&self) -> usize {
        8192
    }

    fn max_output_tokens(&self) -> usize {
        4096
    }

    async fn stream_complete(
        &self,
        _request: CompletionRequest,
    ) -> AgentResult<
        std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send + 'static>>,
    > {
        let stream = futures::stream::iter(vec![
            Ok("Hi there! I am ".to_string()),
            Ok("your AI assistant.".to_string()),
        ]);
        Ok(Box::pin(stream))
    }
}

#[tokio::test]
async fn test_agent_chat_basic() {
    let model = Arc::new(MockModel);
    let agent = Agent::new(model, "test-session".to_string());

    let response = agent.chat("hello agent").await.unwrap();
    assert!(response.contains("AI assistant"));
    assert_eq!(agent.get_history().await.len(), 2); // User + Assistant
}

#[tokio::test]
async fn test_agent_reset_history() {
    let model = Arc::new(MockModel);
    let agent = Agent::new(model, "test-session".to_string());

    agent.chat("msg 1").await.unwrap();
    assert_eq!(agent.get_history().await.len(), 2);

    agent.reset_history().await.unwrap();
    assert_eq!(agent.get_history().await.len(), 0);
}
