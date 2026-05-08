# Tutorial: Building a Plugin for Pharmakon

This tutorial guides you through creating a Tool using the Pharmakon Plugin SDK.

## 1. Project Setup

Create a new Rust library crate:

```bash
cargo new --lib pharmakon-plugin-weather
cd pharmakon-plugin-weather
```

Add dependencies to `Cargo.toml`:

```toml
[dependencies]
pharmakon-plugin-sdk = { path = "../Pharmakon/crates/plugin-sdk" }
serde_json = "1"
async-trait = "0.1"
```

## 2. Implementing a Tool

```rust
use pharmakon_plugin_sdk::{
    Tool, ToolCategory, ExecutionProfile, SideEffectLevel,
    FilesystemScope, Reversibility, AgentResult, AgentError,
};
use async_trait::async_trait;
use serde_json::{Value, json};

pub struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str { "get_weather" }

    fn description(&self) -> &str {
        "Get the current weather for a specific city."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "location": { "type": "string", "description": "City name" }
            },
            "required": ["location"]
        })
    }

    fn category(&self) -> ToolCategory { ToolCategory::Network }

    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::None,
            network_access: true,
            filesystem_scope: FilesystemScope::None,
            reversibility: Reversibility::Trivial,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let location = args["location"].as_str().unwrap_or("unknown");
        Ok(format!("Weather in {}: Sunny, 25°C", location))
    }
}
```

## 3. Implementing a Plugin Bundle

If your crate provides multiple tools, bundle them:

```rust
use pharmakon_plugin_sdk::{Plugin, PluginHealth, AgentResult};
use std::sync::Arc;

pub struct WeatherPlugin;

impl Plugin for WeatherPlugin {
    fn plugin_id(&self) -> &str { "pharmakon-plugin-weather" }
    fn plugin_version(&self) -> &str { "0.1.0" }

    fn tools(&self) -> Vec<Arc<dyn pharmakon_plugin_sdk::Tool>> {
        vec![Arc::new(WeatherTool)]
    }

    fn initialize(&self) -> AgentResult<()> {
        println!("Weather plugin initialized");
        Ok(())
    }
}
```

## 4. Emitting Events

Tools can emit events back to the agent:

```rust
use pharmakon_plugin_sdk::{PluginEventTx, PluginEvent};

async fn call(&self, args: Value) -> AgentResult<String> {
    // Emit a log event
    if let Some(tx) = &self.event_tx {
        tx.send(PluginEvent::Log {
            level: "info".into(),
            message: "Fetching weather...".into(),
        })?;
    }

    // ... tool logic ...
    Ok("Weather: Sunny".to_string())
}
```

## 5. Registration

In the agent initialization code:

```rust
// Register individual tools
agent.add_tool(Arc::new(WeatherTool)).await;

// Or register a plugin bundle
agent.register_plugin(Arc::new(WeatherPlugin)).await;
```

## 6. Best Practices

1. **Execution Profile**: Always set accurate safety metadata — the governor uses it for approval gating.
2. **Error Handling**: Return `AgentError::new(AgentErrorCode::ToolExecutionFailed, msg)` with clear messages so the LLM can self-correct.
3. **Category**: Use existing categories (`ToolCategory::FileSystem`, `ToolCategory::Network`, etc.) rather than `Custom` unless truly new.
4. **Thread Safety**: Plugins must be `Send + Sync`. Use `Arc<Mutex<T>>` for internal mutable state.
5. **Idempotency**: Tool calls may be retried. Design tools to be safe when called multiple times with the same arguments.
