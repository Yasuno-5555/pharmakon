use pharmakon_core::agent::Agent;
use pharmakon_core::model::{AgentModel, CompletionRequest, CompletionResponse, MessageContent, AgentResult};
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_common::Config;
use pharmakon_tools::ShellTool;
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use async_trait::async_trait;

// A more advanced MockModel for detailed testing
struct InspectableMockModel {
    was_complete_called: AtomicBool,
    was_stream_complete_called: AtomicBool,
}

impl InspectableMockModel {
    fn new() -> Self {
        Self {
            was_complete_called: AtomicBool::new(false),
            was_stream_complete_called: AtomicBool::new(false),
        }
    }
}

#[async_trait]
impl AgentModel for InspectableMockModel {
    async fn complete(&self, _request: CompletionRequest) -> AgentResult<CompletionResponse> {
        self.was_complete_called.store(true, Ordering::SeqCst);
        Ok(CompletionResponse {
            content: Some(MessageContent::Text("complete called".to_string())),
            tool_calls: None,
            usage: None,
        })
    }

    fn name(&self) -> &str { "inspectable-mock-model" }

    async fn stream_complete(&self, _request: CompletionRequest) -> AgentResult<std::pin::Pin<Box<dyn futures::Stream<Item = AgentResult<String>> + Send>>> {
        self.was_stream_complete_called.store(true, Ordering::SeqCst);
        let stream = futures::stream::iter(vec![Ok("stream complete called".to_string())]);
        Ok(Box::pin(stream))
    }
}

async fn setup_test_agent_with_model(model: Arc<dyn AgentModel>) -> tokio::sync::MutexGuard<'static, Agent> {
    let config = Config::default();
    let store = Arc::new(DbSessionStore::new("sqlite::memory:").await.expect("Failed to create in-memory store"));
    
    // Use model, store, and config directly
    let router = Box::leak(Box::new(tokio::sync::Mutex::new(pharmakon_core::agent_router::AgentRouter::new(model, store, config))));
    
    let agent_handle = router.lock().await.get_agent("test-agent-logic").await.unwrap();
    
    // We also need to leak the handle to get a 'static MutexGuard
    let agent_handle_leaked = Box::leak(Box::new(agent_handle));

    agent_handle_leaked.lock().await
}

#[tokio::test]
async fn test_chat_calls_stream_when_no_tools() {
    let model = Arc::new(InspectableMockModel::new());
    let mut agent = setup_test_agent_with_model(model.clone()).await;

    let _ = agent.chat("test message").await;

    assert!(!model.was_complete_called.load(Ordering::SeqCst), "complete() should NOT have been called");
    assert!(model.was_stream_complete_called.load(Ordering::SeqCst), "stream_complete() SHOULD have been called");
}

#[tokio::test]
async fn test_chat_calls_complete_when_tools_are_present() {
    let model = Arc::new(InspectableMockModel::new());
    let mut agent = setup_test_agent_with_model(model.clone()).await;

    agent.add_tool(Arc::new(ShellTool));

    let _ = agent.chat("test message").await;

    assert!(model.was_complete_called.load(Ordering::SeqCst), "complete() SHOULD have been called");
    assert!(!model.was_stream_complete_called.load(Ordering::SeqCst), "stream_complete() should NOT have been called");
}
