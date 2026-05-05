use pharmakon_core::agent::Agent;
use pharmakon_core::model::MockModel;
use std::sync::Arc;

#[tokio::test]
async fn test_agent_basic_chat() {
    let model = Arc::new(MockModel);
    let mut agent = Agent::new(model, "test-session".to_string());
    
    let response = agent.chat("Hello from integration test!").await.unwrap();
    
    assert!(response.contains("Mock stream response"));
    // History should have user message and assistant message
    assert_eq!(agent.history.len(), 2);
    assert_eq!(agent.history[0].role, "user");
    assert_eq!(agent.history[1].role, "assistant");
}

#[tokio::test]
async fn test_agent_history_reset() {
    let model = Arc::new(MockModel);
    let mut agent = Agent::new(model, "test-session".to_string());
    
    agent.chat("Message 1").await.unwrap();
    assert_eq!(agent.history.len(), 2);
    
    agent.reset_history();
    assert_eq!(agent.history.len(), 0);
}
