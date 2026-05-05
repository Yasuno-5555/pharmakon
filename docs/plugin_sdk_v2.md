# Pharmakon Plugin SDK V2 Specification

This document describes the specification for building Tools and Hooks for the Pharmakon agent system.

## Overview
The Plugin SDK V2 is designed for high-performance, autonomous agent orchestration. It supports parallel hook execution, tiered reasoning, and deep context awareness.

## 1. Tool Trait
Tools allow the agent to interact with the external world.

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> AgentResult<String>;
    
    // Metadata for UI and Discovery
    fn category(&self) -> ToolCategory { ToolCategory::Custom("generic".to_string()) }
    fn metadata(&self) -> HashMap<String, String>;
    
    // Safety
    fn requires_approval(&self, _args: &Value) -> bool { false }
}
```

### Tool Categories
- `FileSystem`: Local file operations.
- `Network`: Web search, API calls.
- `Media`: Image recognition, OCR.
- `Autonomous`: Sub-agent spawning, swarm management.
- `System`: OS-level commands, terminal.

## 2. Hook Trait
Hooks allow plugins to monitor and intervene in the agent's internal lifecycle.

```rust
#[async_trait]
pub trait Hook: Send + Sync {
    fn name(&self) -> &str;
    
    // Message Lifecycle
    async fn on_message_received(&self, message: &Message) -> Result<()>;
    async fn on_message_sent(&self, message: &Message) -> Result<()>;
    
    // Tool Lifecycle
    async fn before_tool_call(&self, name: &str, args: &Value) -> Result<()>;
    async fn after_tool_call(&self, name: &str, result: &str) -> Result<()>;
    
    // Autonomous Lifecycle (New in V2)
    async fn on_agent_thinking(&self, session_id: &str) -> Result<()>;
    async fn on_reflection_complete(&self, insights: &[String]) -> Result<()>;
    async fn on_context_recovered(&self, context: &str) -> Result<()>;
    async fn on_session_switched(&self, old_id: &str, new_id: &str) -> Result<()>;
}
```

## 3. Concurrency Model
Pharmakon uses a **Parallel Hook Engine**. All registered hooks for a specific trigger are executed concurrently using `futures::future::join_all`.
- **Constraint**: Hooks should be non-blocking and use `async/await`.
- **Performance**: Heavy computations should be offloaded to `tokio::task::spawn_blocking`.

## 4. Contextual Awareness (PluginContext)
Plugins can access agent resources through the `PluginContext`:
- `session_id`: The current active session.
- `event_tx`: Broadcast channel for real-time UI updates.
- `weaver`: Access to the long-term memory vector store.
- `store`: Access to the SQL-based session and commitment store.

## 5. Vision RAG Support
Any tool implementing image analysis should index descriptions into the `MemoryWeaver` with the prefix `[VISUAL MEMORY]`. This ensures the agent can semantically search through visual history.

## 6. Context Anchoring
The system automatically compresses long histories into "Context Anchors". Hooks can listen to `on_context_recovered` to see the resulting compressed state.
