use pharmakon_common::Config;
use pharmakon_common::agent::AgentConfig;
use pharmakon_core::agent_router::AgentRouter;
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::providers::gemini::GeminiModel;
use std::sync::Arc;

fn create_test_config() -> Config {
    let mut config = Config::default();

    config.agents.insert(
        "gemini_researcher".to_string(),
        AgentConfig {
            model_id: Some("gemini/gemini-2.5-flash".to_string()),
            allowed_tools: Some(vec!["web_fetch".to_string()]),
            soul_path: None,
        },
    );
    config.agents.insert(
        "openai_coder_fallback".to_string(),
        AgentConfig {
            model_id: Some("openai/gpt-4o".to_string()),
            allowed_tools: Some(vec!["shell".to_string(), "read_file".to_string()]),
            soul_path: None,
        },
    );

    config
}

fn create_default_model() -> Arc<dyn pharmakon_core::model::AgentModel> {
    let api_key =
        std::env::var("GEMINI_API_KEY").unwrap_or_else(|_| "dummy_key_for_default".to_string());
    Arc::new(GeminiModel::new(api_key, "gemini-2.5-flash".to_string()))
}

#[tokio::test]
async fn test_dynamic_agent_loading() {
    let config = create_test_config();

    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        unsafe {
            std::env::set_var("GEMINI_API_KEY", key);
        }
    }

    let store = Arc::new(
        DbSessionStore::new("sqlite::memory:")
            .await
            .expect("Failed to create in-memory store"),
    );
    let default_model = create_default_model();

    let mut router = AgentRouter::new(default_model, store, config, None, None);

    // Test the Gemini agent — should use the specified model
    let gemini_agent_handle = router
        .get_agent("gemini_researcher")
        .await
        .expect("Failed to get gemini_researcher agent");
    let gemini_agent = gemini_agent_handle.lock().await;

    assert!(
        gemini_agent
            .model
            .lock()
            .await
            .name()
            .contains("gemini-2.5-flash"),
        "Gemini agent should use gemini-2.5-flash model. Found: {}",
        gemini_agent.model.lock().await.name()
    );

    // Tool registry exists and has metadata (catalog is built at construction time)
    let gemini_reg = gemini_agent.registry.lock().await;
    let gemini_tools: Vec<String> = gemini_reg
        .all_metadata()
        .iter()
        .map(|m| m.name.clone())
        .collect();
    assert!(
        !gemini_tools.is_empty(),
        "Gemini agent should have tool metadata in its registry"
    );
    println!("'gemini_researcher' agent loaded with {} tool metadata entries.", gemini_tools.len());

    // Test the OpenAI agent — should fall back to the default model
    let openai_agent_handle = router
        .get_agent("openai_coder_fallback")
        .await
        .expect("Failed to get openai_coder_fallback agent");
    let openai_agent = openai_agent_handle.lock().await;

    assert!(
        openai_agent
            .model
            .lock()
            .await
            .name()
            .contains("gemini-2.5-flash"),
        "OpenAI agent should fall back to default Gemini model. Found: {}",
        openai_agent.model.lock().await.name()
    );

    println!("Dynamic agent loading test passed successfully!");
}
