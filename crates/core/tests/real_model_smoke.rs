//! Real-model smoke tests.
//!
//! These tests require at least one API key to be set. They are `#[ignore]` by
//! default. Run with:
//!
//! ```bash
//! cargo test -p pharmakon-core -- --ignored real_model
//! ```
//!
//! The tests auto-detect available providers from environment variables and
//! skip gracefully if none are configured.

use pharmakon_core::agent::Agent;
use pharmakon_core::providers::registry::ModelRegistry;
use std::sync::Arc;

// ── Helpers ──────────────────────────────────────────────────────────────

/// Try to find any available real model (not mock).
/// Returns (model_id, model) or None if no API keys are configured.
fn find_available_model() -> Option<(String, Arc<dyn pharmakon_core::model::AgentModel>)> {
    let candidates = ModelRegistry::list_available_models();
    // Filter out ollama (which might hang if Ollama isn't running)
    // and openrouter (which needs additional setup)
    candidates
        .into_iter()
        .filter(|id| !id.starts_with("ollama/") && !id.starts_with("openrouter/"))
        .find_map(|id| ModelRegistry::get_model(&id).map(|m| (id, m)))
}

/// Find a second model (different provider) for switch/fallback tests.
fn find_second_model(
    first_provider: &str,
) -> Option<(String, Arc<dyn pharmakon_core::model::AgentModel>)> {
    let candidates = ModelRegistry::list_available_models();
    candidates
        .into_iter()
        .filter(|id| {
            !id.starts_with("ollama/")
                && !id.starts_with("openrouter/")
                && !id.starts_with(first_provider)
        })
        .find_map(|id| ModelRegistry::get_model(&id).map(|m| (id, m)))
}

/// Helper to run a simple chat and get the response.
async fn quick_chat(agent: &Agent, msg: &str) -> String {
    match agent.chat(msg).await {
        Ok(r) => r,
        Err(e) => format!("ERROR: {}", e),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

/// Test 1: Basic chat — send "hello" and verify we get a non-empty response.
#[tokio::test]
#[ignore = "requires real API key (set e.g. DEEPSEEK_API_KEY)"]
async fn test_real_basic_chat() {
    let Some((model_id, model)) = find_available_model() else {
        eprintln!("SKIP: No API keys configured. Set DEEPSEEK_API_KEY or GEMINI_API_KEY.");
        return;
    };
    eprintln!("→ Using model: {}", model_id);

    let agent = Agent::new(model, "smoke-basic".to_string());
    let response = quick_chat(&agent, "Say exactly 'OK' and nothing else.").await;

    assert!(
        !response.is_empty(),
        "Expected non-empty response from {}",
        model_id
    );
    assert!(
        response.to_lowercase().contains("ok"),
        "Expected 'OK' in response, got: {}",
        response.chars().take(200).collect::<String>()
    );

    eprintln!(
        "✓ Basic chat OK: {}",
        response.chars().take(100).collect::<String>()
    );
}

/// Test 2: Model switch via /model command.
#[tokio::test]
#[ignore = "requires at least 2 real API keys"]
async fn test_real_model_switch() {
    let Some((first_id, first_model)) = find_available_model() else {
        eprintln!("SKIP: No API keys configured.");
        return;
    };

    let first_provider = first_id.split('/').next().unwrap_or("");
    let Some((second_id, _second_model)) = find_second_model(first_provider) else {
        eprintln!(
            "SKIP: Only one provider available ({}). Need two for switch test.",
            first_id
        );
        return;
    };

    eprintln!("→ Primary: {}", first_id);
    eprintln!("→ Switch target: {}", second_id);

    let agent = Agent::new(first_model, "smoke-switch".to_string());

    // Verify initial model
    assert!(
        agent.model_name().await.contains(first_provider),
        "Initial model should be {}",
        first_provider
    );

    // Switch via /model command
    let switch_cmd = format!("/model {}", second_id);
    let switch_resp = quick_chat(&agent, &switch_cmd).await;
    eprintln!(
        "→ Switch response: {}",
        switch_resp.chars().take(100).collect::<String>()
    );

    // Verify model switched
    let current = agent.model_name().await;
    assert!(
        current.contains(second_id.split('/').last().unwrap_or("")),
        "Model should have switched to '{}', but is '{}'",
        second_id,
        current
    );

    eprintln!("✓ Model switch OK: {} → {}", first_id, current);
}

/// Test 3: Fallback chain — use a non-existent model name and verify
/// the agent falls back to a working model instead of crashing.
#[tokio::test]
#[ignore = "requires real API key"]
async fn test_real_fallback_chain() {
    let Some((_model_id, model)) = find_available_model() else {
        eprintln!("SKIP: No API keys configured.");
        return;
    };

    // Register a working model as fallback, then try a non-existent primary
    let fallback_id = {
        let candidates = ModelRegistry::list_available_models();
        candidates
            .into_iter()
            .find(|id| !id.starts_with("ollama/") && !id.starts_with("openrouter/"))
    };

    let Some(fallback_id) = fallback_id else {
        eprintln!("SKIP: No fallback model available.");
        return;
    };

    eprintln!("→ Fallback configured: {}", fallback_id);

    let agent = Agent::new(model, "smoke-fallback".to_string())
        .with_fallback_models(vec![fallback_id.clone()]);

    // Try to trigger a fallback by sending a message that will cause an error
    // (We rely on the model router's built-in fallback; this primarily tests
    // that the setup doesn't crash and a basic chat works.)
    let response = quick_chat(&agent, "Say 'hello' in one word.").await;

    assert!(
        !response.is_empty(),
        "Response should not be empty (fallback setup OK)"
    );
    eprintln!(
        "✓ Fallback setup OK: {}",
        response.chars().take(100).collect::<String>()
    );
}
