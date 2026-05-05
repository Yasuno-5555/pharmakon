# Tutorial: Building Your First Plugin for Pharmakon

This tutorial will guide you through creating a simple **Tool** (external action) and a **Hook** (lifecycle monitor) using the Pharmakon SDK V2.

## 1. Creating a Custom Tool

We'll build a `WeatherTool` that "simulates" fetching weather data.

### Implementation

```rust
use async_trait::async_trait;
use serde_json::{Value, json};
use pharmakon_common::{Tool, ToolCategory, AgentResult, AgentError};

pub struct WeatherTool;

#[async_trait]
impl Tool for WeatherTool {
    fn name(&self) -> &str { "get_weather" }
    
    fn description(&self) -> &str { 
        "Get the current weather for a specific location." 
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

    fn category(&self) -> ToolCategory { 
        ToolCategory::Network 
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let location = args["location"].as_str().unwrap_or("unknown");
        // In a real plugin, you would call an API here.
        Ok(format!("The weather in {} is Sunny, 25°C.", location))
    }
}
```

## 2. Creating a Custom Hook

We'll build an `InsightLogger` that saves the agent's autonomous reflections to a local file.

### Implementation

```rust
use async_trait::async_trait;
use pharmakon_core::hooks::Hook;
use anyhow::Result;
use std::fs::OpenOptions;
use std::io::Write;

pub struct InsightLogger {
    log_path: String,
}

#[async_trait]
impl Hook for InsightLogger {
    fn name(&self) -> &str { "insight_logger" }

    async fn on_reflection_complete(&self, insights: &[String]) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        for insight in insights {
            writeln!(file, "[INSIGHT] {}", insight)?;
        }
        
        println!("✅ InsightLogger: Saved {} insights to {}", insights.len(), self.log_path);
        Ok(())
    }
}
```

## 3. Registering Your Plugin

To enable your plugins, add them during agent initialization in `main.rs`.

```rust
// In main.rs
let agent = Agent::new(model, session_id);

// Register Tool
agent.add_tool(Arc::new(WeatherTool));

// Register Hook
agent.hooks.register(Arc::new(InsightLogger { 
    log_path: "insights.log".to_string() 
})).await;
```

## 4. Best Practices

1.  **Use Async Everywhere**: Always use `.await` for I/O to avoid blocking the parallel engine.
2.  **Thread Safety**: Plugins must be `Send + Sync`. Use `Arc<Mutex<T>>` if you need internal state.
3.  **Error Handling**: Return `AgentError` for tools to give the LLM clear feedback on why a command failed.
4.  **Batching**: If your hook processes data, consider batching if the trigger frequency is high (e.g., `on_message_received`).

## Next Steps
Check the [Plugin SDK V2 Specification](./plugin_sdk_v2.md) for deeper details on `PluginContext` and advanced lifecycle events.
