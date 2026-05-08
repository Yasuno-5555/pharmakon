use pharmakon_common::Config;
use pharmakon_common::agent::AgentConfig;
use pharmakon_core::agent_router::AgentRouter;
use pharmakon_core::model::MockModel;
use pharmakon_core::persistence::DbSessionStore;
use std::sync::Arc;

async fn setup_test_router(config: Config) -> AgentRouter {
    let store = Arc::new(
        DbSessionStore::new("sqlite::memory:")
            .await
            .expect("Failed to create in-memory store"),
    );
    let default_model = Arc::new(MockModel);

    AgentRouter::new(default_model, store, config, None, None)
}

#[tokio::test]
async fn test_router_fallback_to_default_model() {
    let mut config = Config::default();
    config.agents.insert(
        "bad-model-agent".to_string(),
        AgentConfig {
            model_id: Some("nonexistent/provider".to_string()),
            allowed_tools: None,
            soul_path: None,
        },
    );

    let mut router = setup_test_router(config).await;
    let agent_handle = router.get_agent("bad-model-agent").await.unwrap();
    let agent = agent_handle.lock().await;

    // The agent's model should be the default MockModel, not the non-existent one.
    assert_eq!(
        agent.model.lock().await.name(),
        "mock-model",
        "Model should fall back to default"
    );
}

#[tokio::test]
async fn test_router_tool_instantiation_failure() {
    let mut config = Config::default();
    config.agents.insert(
        "bad-tools-agent".to_string(),
        AgentConfig {
            model_id: None, // Use default model
            // Request tools that will fail to instantiate
            allowed_tools: Some(vec![
                "brave_search".to_string(), // Fails because of missing API key
                "fact_tool".to_string(),    // Fails because it's not supported yet
            ]),
            soul_path: None,
        },
    );

    let mut router = setup_test_router(config).await;
    let agent_handle = router.get_agent("bad-tools-agent").await.unwrap();
    let agent = agent_handle.lock().await;

    // No tools should have been added because they all failed to instantiate.
    let reg = agent.registry.lock().await;
    assert!(
        reg.get_loaded().is_empty(),
        "No tools should be added if instantiation fails"
    );
}