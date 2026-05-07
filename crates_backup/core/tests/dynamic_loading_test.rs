use pharmakon_common::Config;
use pharmakon_common::agent::AgentConfig;
use pharmakon_core::agent_router::AgentRouter;
use pharmakon_core::persistence::DbSessionStore;
use pharmakon_core::providers::gemini::GeminiModel;
use std::sync::Arc;

fn create_test_config() -> Config {
    let mut config = Config::default();

    // Manually insert agent configurations
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

// Helper function to create a default model for testing
fn create_default_model() -> Arc<dyn pharmakon_core::model::AgentModel> {
    let api_key =
        std::env::var("GEMINI_API_KEY").unwrap_or_else(|_| "dummy_key_for_default".to_string());
    Arc::new(GeminiModel::new(api_key, "gemini-2.5-flash".to_string()))
}

#[tokio::test]
async fn test_dynamic_agent_loading() {
    // 1. Create a self-contained config for the test
    let config = create_test_config();

    // Ensure the API key is set for the default model as well
    if let Ok(key) = std::env::var("GEMINI_API_KEY") {
        unsafe {
            std::env::set_var("GEMINI_API_KEY", key);
        }
    } else {
        println!(
            "Warning: GEMINI_API_KEY not set. Test might fail if it needs to contact the real API."
        );
    }

    // 2. Initialize necessary components
    let store = Arc::new(
        DbSessionStore::new("sqlite::memory:")
            .await
            .expect("Failed to create in-memory store"),
    );
    let default_model = create_default_model();

    let mut router = AgentRouter::new(default_model, store, config, None, None);

    // 3. Test the Gemini agent
    let gemini_agent_handle = router
        .get_agent("gemini_researcher")
        .await
        .expect("Failed to get gemini_researcher agent");
    let gemini_agent = gemini_agent_handle.lock().await;

    // Check model
    assert!(
        gemini_agent
            .model
            .lock()
            .await
            .name()
            .contains("gemini-2.5-flash"),
        "Gemini agent should have the specified Gemini model. Found: {}",
        gemini_agent.model.lock().await.name()
    );

    // Check tools
    let gemini_tools_guard = gemini_agent.tools.lock().await;
    let gemini_tools: Vec<String> = gemini_tools_guard
        .iter()
        .map(|t| t.name().to_string())
        .collect();
    assert_eq!(
        gemini_tools.len(),
        1,
        "Gemini agent should have 1 tool, but found {}. Tools: {:?}",
        gemini_tools.len(),
        gemini_tools
    );
    assert!(
        gemini_tools.contains(&"web_fetch".to_string()),
        "Gemini agent should have the 'web_fetch' tool"
    );
    println!("'gemini_researcher' agent loaded correctly.");

    // 4. Test the OpenAI agent (which should fall back to the default model)
    let openai_agent_handle = router
        .get_agent("openai_coder_fallback")
        .await
        .expect("Failed to get openai_coder_fallback agent");
    let openai_agent = openai_agent_handle.lock().await;

    // Check model (should be the default model)
    assert!(
        openai_agent
            .model
            .lock()
            .await
            .name()
            .contains("gemini-2.5-flash"),
        "OpenAI agent should fall back to the default Gemini model. Found: {}",
        openai_agent.model.lock().await.name()
    );

    // Check tools
    let openai_tools_guard = openai_agent.tools.lock().await;
    let openai_tools: Vec<String> = openai_tools_guard
        .iter()
        .map(|t| t.name().to_string())
        .collect();
    assert_eq!(openai_tools.len(), 2, "OpenAI agent should have 2 tools");
    assert!(
        openai_tools.contains(&"shell".to_string()),
        "OpenAI agent should have the 'shell' tool"
    );
    assert!(
        openai_tools.contains(&"read_file".to_string()),
        "OpenAI agent should have the 'read_file' tool"
    );
    println!("'openai_coder_fallback' agent loaded correctly and fell back to default model.");

    println!(
        "
Dynamic agent loading test passed successfully!"
    );
}
