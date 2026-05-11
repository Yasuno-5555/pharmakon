# Plugin development

Plugins are Rust crates that implement traits from `pharmakon-plugin-sdk`. They are compiled as part of the workspace — not dynamically loaded.

## Implementing a tool

```rust
use async_trait::async_trait;
use pharmakon_common::{Tool, ToolCategory, AgentResult, ExecutionProfile};

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Does something useful" }
    fn category(&self) -> ToolCategory { ToolCategory::Custom("my_category".into()) }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            },
            "required": ["input"]
        })
    }

    async fn call(&self, args: serde_json::Value) -> AgentResult<String> {
        let input = args["input"].as_str().unwrap_or("");
        Ok(format!("Processed: {}", input))
    }

    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: pharmakon_common::SideEffectLevel::None,
            filesystem_scope: pharmakon_common::FilesystemScope::None,
            reversibility: pharmakon_common::Reversibility::Trivial,
            ..Default::default()
        }
    }
}
```

## Registering a tool

Tools are registered with the agent at startup:

```rust
agent.add_tool(Arc::new(MyTool)).await;
```

## Plugin SDK

The `pharmakon-plugin-sdk` crate re-exports the traits and types needed:

- `Tool` trait: implement this to add a new tool
- `ToolCategory`: categorize your tool (affects when it's loaded)
- `AgentResult` / `AgentError`: return types
- `ExecutionProfile`: declare safety characteristics

## Event hooks

Tools can emit events via the broadcast channel. The gateway forwards events to WebSocket clients and the TUI dashboard.
