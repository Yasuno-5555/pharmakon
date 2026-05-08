use pharmakon_core::agent::Agent;
use pharmakon_core::model::MockModel;
use std::sync::Arc;

#[tokio::test]
async fn test_agent_basic_chat() {
    let model = Arc::new(MockModel);
    let agent = Agent::new(model, "test-session".to_string());

    let response = agent.chat("Hello from integration test!").await.unwrap();

    assert!(response.contains("Mock stream response"));
    // History should have user message and assistant message
    let history = agent.get_history().await;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].role, "user");
    assert_eq!(history[1].role, "assistant");
}

#[tokio::test]
async fn test_agent_history_reset() {
    let model = Arc::new(MockModel);
    let agent = Agent::new(model, "test-session".to_string());

    let _ = agent.chat("Message 1").await;
    assert_eq!(agent.get_history().await.len(), 2);

    let _ = agent.reset_history().await;
    assert_eq!(agent.get_history().await.len(), 0);
}