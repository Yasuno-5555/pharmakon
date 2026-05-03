use async_trait::async_trait;
use pharmakon_common::{Tool, AgentResult, AgentError};
use serde_json::{Value, json};
use wasmtime::*;
use std::sync::Arc;

pub struct WasmTool {
    name: String,
    wasm_bytes: Vec<u8>,
}

impl WasmTool {
    pub fn new(name: String, wasm_bytes: Vec<u8>) -> Self {
        Self { name, wasm_bytes }
    }
}

#[async_trait]
impl Tool for WasmTool {
    fn name(&self) -> &str { &self.name }
    fn description(&self) -> &str { "Execute a WASM-based tool in a secure sandbox." }
    fn parameters(&self) -> Value {
        // In a more complete implementation, we would extract parameters from the WASM metadata
        json!({
            "type": "object",
            "properties": {
                "input": { "type": "string" }
            }
        })
    }

    async fn call(&self, args: Value) -> AgentResult<String> {
        let engine = Engine::default();
        let module = Module::new(&engine, &self.wasm_bytes)
            .map_err(|e| AgentError(format!("Failed to create WASM module: {}", e)))?;
        
        let mut store = Store::new(&engine, ());
        let linker = Linker::new(&engine);
        
        // Add basic imports if needed (e.g. log, get_time)
        
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| AgentError(format!("Failed to instantiate WASM: {}", e)))?;

        let call_fn = instance.get_typed_func::<(i32, i32), i64>(&mut store, "call")
            .map_err(|_| AgentError("WASM module does not export 'call(i32, i32) -> i64'".to_string()))?;

        let alloc_fn = instance.get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|_| AgentError("WASM module does not export 'alloc(i32) -> i32'".to_string()))?;

        let memory = instance.get_memory(&mut store, "memory")
            .ok_or_else(|| AgentError("WASM module does not export 'memory'".to_string()))?;

        let input_str = args.to_string();
        let input_bytes = input_str.as_bytes();
        let input_len = input_bytes.len() as i32;

        let input_ptr = alloc_fn.call(&mut store, input_len)
            .map_err(|e| AgentError(format!("Failed to allocate memory in WASM: {}", e)))?;

        memory.write(&mut store, input_ptr as usize, input_bytes)
            .map_err(|e| AgentError(format!("Failed to write to WASM memory: {}", e)))?;

        let result_packed = call_fn.call(&mut store, (input_ptr, input_len))
            .map_err(|e| AgentError(format!("WASM execution error: {}", e)))?;

        let result_ptr = (result_packed >> 32) as i32;
        let result_len = (result_packed & 0xFFFFFFFF) as i32;

        let mut result_bytes = vec![0u8; result_len as usize];
        memory.read(&mut store, result_ptr as usize, &mut result_bytes)
            .map_err(|e| AgentError(format!("Failed to read from WASM memory: {}", e)))?;

        let result_str = String::from_utf8(result_bytes)
            .map_err(|e| AgentError(format!("WASM returned invalid UTF-8: {}", e)))?;

        Ok(result_str)
    }
}
