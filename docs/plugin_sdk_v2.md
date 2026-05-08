# Pharmakon Plugin SDK Specification

The Plugin SDK enables external crates to implement tools that the Pharmakon agent discovers and invokes at runtime. Located at `crates/plugin-sdk/`.

## Tool Trait

Aligns with `pharmakon_common::Tool`. Full trait signature:

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> serde_json::Value;
    async fn call(&self, args: serde_json::Value) -> AgentResult<String>;

    fn category(&self) -> ToolCategory { ToolCategory::Custom("generic".to_string()) }
    fn execution_profile(&self) -> ExecutionProfile { ExecutionProfile::default() }
    fn metadata(&self) -> HashMap<String, String> { HashMap::new() }
    fn requires_approval(&self, _args: &Value) -> bool { false }
    fn approval_description(&self, _args: &Value) -> String { String::new() }

    fn to_meta(&self) -> ToolMeta { ... }
}
```

### New in current version

- **`execution_profile()`** — safety classification (side_effect_level, network_access, filesystem_scope, reversibility)
- **`to_meta()`** — lightweight ToolMeta for deferred hydration (only ~80 bytes kept in memory)
- **`requires_approval()` / `approval_description()`** — fine-grained human-in-the-loop gating

## Tool Categories

```rust
pub enum ToolCategory {
    Core,           // Always injected (CodeAct, reflection)
    FileSystem,     // File I/O
    Network,        // Web, API calls
    Media,          // Vision, images
    Autonomous,     // Sub-agent spawning
    System,         // Shell, terminal
    Orchestration,  // Swarm, MCTS, tool routing
    Coding,         // AST, linter, quality
    Custom(String), // User-defined
}
```

## Execution Profile

```rust
pub enum SideEffectLevel { None, Local, Irreversible }
pub enum FilesystemScope { None, Confined, Unrestricted }
pub enum Reversibility { Trivial, Possible, Impractical }

pub struct ExecutionProfile {
    pub side_effect_level: SideEffectLevel,
    pub network_access: bool,
    pub filesystem_scope: FilesystemScope,
    pub reversibility: Reversibility,
    pub requires_human_approval: bool,
}
```

## Plugin Trait

Plugin-level lifecycle for tool bundles:

```rust
pub trait Plugin: Send + Sync {
    fn initialize(&self) -> AgentResult<()> { Ok(()) }
    fn tools(&self) -> Vec<Arc<dyn Tool>>;
    fn shutdown(&self) -> AgentResult<()> { Ok(()) }
    fn health_check(&self) -> AgentResult<PluginHealth> { ... }
    fn plugin_id(&self) -> &str;
    fn plugin_version(&self) -> &str { "0.1.0" }
}
```

## Plugin Events

Plugins can emit events back to the agent via `PluginEventTx`:

```rust
pub enum PluginEvent {
    Log { level: String, message: String },
    StatusChange { tool: String, status: String },
    Error { message: String },
}
```

## Error Model

```rust
pub enum AgentErrorCode {
    RateLimit, InvalidRequest, AuthenticationFailed,
    ContextExceeded, ModelError, ToolNotFound,
    ToolExecutionFailed, HangDetected, NetworkError,
    InternalError, EnvironmentError,
}

pub type AgentResult<T> = Result<T, AgentError>;
```

## Lightweight Metadata (ToolMeta)

Only ~80 bytes per tool — kept in memory permanently. Full `Tool` is hydrated on demand when the LLM calls it.

```rust
pub struct ToolMeta {
    pub name: String,
    pub description: String,
    pub category: ToolCategory,
    pub profile: ExecutionProfile,
}
```

## Tool Routing & Capability Abstraction

The agent's `ToolMetaRegistry` maps LLM intent to concrete tools:
- `reg.search(query, top_k)` — semantic search for relevant tools
- `reg.all_metadata()` — lightweight ToolMeta listing
- `reg.hydrate(name)` — lazily loads the full Tool implementation

On-demand hydration keeps ~60 tools registered at <5KB memory overhead.
