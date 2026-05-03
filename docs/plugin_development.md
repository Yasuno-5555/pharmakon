# Plugin Development Guide

Pharmakon supports a dynamic plugin system via WebAssembly (WASM). By compiling your tools to WASM using the `pharmakon-plugin-sdk`, you can securely load and execute external logic at runtime inside a sandboxed environment.

## Overview
Plugins in Pharmakon are implemented as WASM modules that adhere to specific WIT (WebAssembly Interface Types) bindings. This allows plugins to define expected inputs (JSON schemas) and securely return string or JSON outputs.

## Prerequisites
- Rust with the `wasm32-wasip1` target installed:
  ```bash
  rustup target add wasm32-wasip1
  ```
- `cargo-component` (recommended for WIT bindings).

## Creating a Plugin

### 1. Setup
Create a new library project using the SDK:
```bash
cargo new --lib my_custom_tool
cd my_custom_tool
```

Add the SDK to your dependencies in `Cargo.toml`:
```toml
[dependencies]
pharmakon-plugin-sdk = { path = "../Pharmakon/crates/plugin-sdk" }
```

### 2. Implementation
Implement the `ToolPlugin` trait for your logic.

```rust
use pharmakon_plugin_sdk::{ToolPlugin, export_plugin};
use serde_json::Value;

struct MyCustomTool;

impl ToolPlugin for MyCustomTool {
    fn name() -> String {
        "my_custom_tool".to_string()
    }

    fn description() -> String {
        "Executes my custom logic.".to_string()
    }

    fn parameters() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "input_data": { "type": "string", "description": "Data to process" }
            },
            "required": ["input_data"]
        })
    }

    fn call(args: Value) -> Result<String, String> {
        let input = args.get("input_data")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        
        Ok(format!("Processed: {}", input))
    }
}

// Export the plugin for WASM bindings
export_plugin!(MyCustomTool);
```

### 3. Build the WASM Binary
Compile your project to the WASI target:
```bash
cargo build --target wasm32-wasip1 --release
```

The resulting `.wasm` file can be found in `target/wasm32-wasip1/release/my_custom_tool.wasm`.

## Integrating with Pharmakon
To load your custom plugin into a Pharmakon Agent, register it via the `WasmTool` adapter:

```rust
use pharmakon_tools::wasm_tool::WasmTool;

let my_tool = WasmTool::load("path/to/my_custom_tool.wasm").expect("Failed to load WASM");
agent.add_tool(Arc::new(my_tool));
```

## Security
All plugins run under Wasmtime's strict sandboxing rules. They cannot access the filesystem, network, or environment variables unless explicitly permitted by the Pharmakon engine's Wasmtime configuration.
