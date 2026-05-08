# Plugin Development Guide

Pharmakon plugins are Rust crates that implement the `Tool` and `Plugin` traits from `pharmakon-plugin-sdk`. Plugins are compiled natively (no WASM), loaded dynamically, and executed with full access to the host system's capabilities.

## Project Setup

```bash
cargo new --lib pharmakon-plugin-mytool
cd pharmakon-plugin-mytool
```

Add to `Cargo.toml`:

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
pharmakon-plugin-sdk = { path = "../Pharmakon/crates/plugin-sdk" }
serde_json = "1"
async-trait = "0.1"
```

## Tool Implementation

```rust
use pharmakon_plugin_sdk::{
    Tool, ToolCategory, ExecutionProfile, SideEffectLevel,
    FilesystemScope, Reversibility, AgentResult, Plugin, PluginHealth,
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct MyTool;

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Description of what this tool does." }

    fn parameters(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            },
            "required": ["input"]
        })
    }

    fn category(&self) -> ToolCategory { ToolCategory::FileSystem }

    fn execution_profile(&self) -> ExecutionProfile {
        ExecutionProfile {
            side_effect_level: SideEffectLevel::Local,
            network_access: false,
            filesystem_scope: FilesystemScope::Confined,
            reversibility: Reversibility::Possible,
            requires_human_approval: false,
        }
    }

    async fn call(&self, args: serde_json::Value) -> AgentResult<String> {
        let input = args["input"].as_str().unwrap_or_default();
        Ok(format!("Processed: {}", input))
    }
}

// Bundle as a Plugin
pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn plugin_id(&self) -> &str { "pharmakon-plugin-mytool" }
    fn tools(&self) -> Vec<Arc<dyn Tool>> { vec![Arc::new(MyTool)] }
}
```

## Registration

```rust
// In your application's agent initialization:
agent.add_tool(Arc::new(MyTool)).await;
// Or register the full plugin:
agent.register_plugin(Arc::new(MyPlugin)).await;
```

## Safety Classification

Each tool MUST declare its `ExecutionProfile` accurately. The `ToolGovernor` uses this for:

| Field | Purpose |
|-------|---------|
| `side_effect_level` | `None` (read-only), `Local` (writes within workspace), `Irreversible` (git push, deploy) |
| `network_access` | Whether the tool makes external HTTP calls |
| `filesystem_scope` | `None`, `Confined` (workspace only), `Unrestricted` |
| `reversibility` | `Trivial` (undo with git), `Possible` (with rollback), `Impractical` |
| `requires_human_approval` | Set `true` for destructive operations |

## Tool Discovery

The agent discovers tools via `ToolMetaRegistry`:
- Tools in `ToolCategory::Core` are always injected into every completion request
- Other tools are lazily loaded via `reg.search(query)` — semantic matching against the task
- `ToolMeta` (~80 bytes) stays in memory; full `Tool` implementation is hydrated on first call

## Event Emission

Plugins can emit events back to the agent via `PluginEventTx`:

```rust
pub enum PluginEvent {
    Log { level: String, message: String },
    StatusChange { tool: String, status: String },
    Error { message: String },
}
```

## Best Practices

1. **Accurate profiles**: The governor gates destructive operations based on your `ExecutionProfile`. Be honest.
2. **Clear error messages**: Return structured errors so the LLM can self-correct.
3. **Use standard categories**: Prefer `ToolCategory::FileSystem`, `::Network`, `::System` over `Custom`.
4. **Thread safety**: All tool methods are called from async contexts; avoid blocking.
5. **Snapshot before mutation**: For file tools, snapshot before writing to enable rollback.
