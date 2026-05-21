# Pharmakon Architecture

This document describes the component boundaries, data flow, and design decisions of Pharmakon.

---

## What it is

Pharmakon is a **modular AI agent framework** that coordinates LLMs, tools, and memory. It is not an operating system — it is a Rust application that runs on your machine and provides agentic capabilities through multiple interfaces (CLI, REST API, chat bots).

## Design principles

1. **Local-first**: Tool execution, vector search, and file operations happen on the local machine. LLM calls go to external APIs (or local Ollama).
2. **Observable**: Event-sourced execution (EventLog) with snapshot-based rollback (SnapshotStore) — agent actions are recorded and reversible.
3. **Provider-agnostic**: Unified `AgentModel` trait abstracts over 7+ LLM providers with automatic fallback.
4. **Constitutional safety**: Immutable policy rules prevent self-modification and destructive operations.

---

## Crate architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      External interfaces                     │
│    pharmakon-cli (REPL/TUI)    pharmakon-gateway (REST/WS)  │
│                               Telegram / Discord / Slack     │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│                      pharmakon-core                          │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │Agent loop    │  │ModelRouter   │  │CognitiveScheduler │  │
│  │chat_on_session│  │(fallback,   │  │(task complexity   │  │
│  │entropy monitor│  │ economy)    │  │ classification)   │  │
│  └──────┬───────┘  └──────────────┘  └───────────────────┘  │
│         │                                                    │
│  ┌──────▼───────────────────────────────────────────────┐   │
│  │              Orchestration layer                       │   │
│  │  CodeAct  │  World Model  │  Swarm  │  Skills  │ MCTS │   │
│  │  Planner  │  Speculative  │  Fabric │  Pattern │ AOT  │   │
│  │  Retry    │  Benchmark    │  Replan │  Economy │ etc. │   │
│  └────────────────────────────────────────────────────────┘   │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Tool Scheduler                             │  │
│  │  ExplorationBudget │ ToolPolicyEngine │ AttentionScore │  │
│  │  DirectoryIndexingDaemon │ CodeActGate                  │  │
│  │  IntegratedGovernor (Safety > Quality > Resource)         │  │
│  └────────────────────────────────────────────────────────┘  │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│                     pharmakon-tools                          │
│ 65+ tools in categories: File, Code, Shell, Search, Browser, │
│ Git, LSP, AST, Canvas, Web, Media, Codex (advanced)          │
│ BM25-indexed metadata catalog. Deferred hydration.           │
└───────────────────────────┬─────────────────────────────────┘
                            │
┌───────────────────────────▼─────────────────────────────────┐
│                    pharmakon-memory                           │
│ ┌──────────────────┐  ┌──────────────┐  ┌────────────────┐  │
│ │KnowledgeNexus    │  │GraphStore    │  │CausalGraph     │  │
│ │(LanceDB vectors) │  │(SQLite rels) │  │(DAG edges)     │  │
│ └──────────────────┘  └──────────────┘  └────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Dependency direction

```
common ← memory ← core ← tools ← gateway ← cli
                        ↕
                    plugin-sdk
```

No circular dependencies between crates.

---

## Agent decision loop

The core of Pharmakon is the `Agent::chat_on_session()` method, which runs a loop:

```
User message
    │
    ▼
┌─────────────────┐
│ Dream Mode init │  ← started once per process
│ Context gather  │  ← parallel semantic + nexus search
└────────┬────────┘
         ▼
┌─────────────────┐
│ Classify task   │  ← heuristic (free) or LLM (ambiguous short tasks)
│ complexity      │
└────────┬────────┘
         ▼
┌─────────────────┐
│ Estimate budget │  ← Simple: 8 iters / Standard: progress-based / Deep: lenient
└────────┬────────┘
         ▼
    ┌────┴────┐
    │  LOOP   │◄──────────────────────────────┐
    └────┬────┘                                │
         ▼                                     │
┌─────────────────┐                            │
│ Build prompt    │  ← system rules + skills + │
│ Inject tools    │     BM25 search + working  │
│ (BM25 +         │     memory + playbook      │
│  serendipity)   │                            │
└────────┬────────┘                            │
         ▼                                     │
┌─────────────────┐                            │
│ Model selection │  ← economy-aware or manual │
└────────┬────────┘                            │
         ▼                                     │
┌─────────────────┐                            │
│ Execute model   │  ← with fallback on 429,   │
│ (with fallback) │     MAX_TOKENS, empty      │
└────────┬────────┘                            │
         ▼                                     │
    ┌────┴────┐                                │
    │ Has     │  Yes                           │
    │ tool    ├────► Execute tools ────────────┘
    │ calls?  │     (parallel via tokio::spawn)
    └────┬────┘     record to skill library
         │ No       snapshot before mutation
         ▼          rollback on failure
┌─────────────────┐
│ Process response│  ← extract <think> tags
│ Save to history │     update notebook
│ Index to nexus  │     trigger reflection
└─────────────────┘
    │
    ▼
  Return to user
```

### Budget enforcement

| Policy | Applied to | Behavior |
|---|---|---|
| `FixedIterations(8)` | Simple tasks | Loop terminates after 8 iterations |
| `ProgressBased` | Standard/Deep | Terminates after N consecutive stalled iterations (plus cosine stagnation detection) |
| Hard wall time | All | Depends on complexity (2min simple → 30min deep) |
| Token budget | All | Default 250k tokens per session |
| Multi-tier entropy | All | Tier 1 (>0.50): increase serendipity; Tier 2 (>0.70): strategy prompt; Tier 3 (>0.85): model switch; Tier 4 (>0.95): hard-terminate |
| Cosine stagnation | All | Iteration embedding cosine >0.98 for 2 consecutive iterations → early intervention |

### Model fallback chain

1. Primary model fails (429 / MAX_TOKENS / empty response)
2. Try next model in `fallback_models` list
3. Two consecutive empty responses → switch model
4. All models exhausted → return error

---

## Memory system

### KnowledgeNexus (vector store)
- **Backend**: LanceDB (embedded columnar vector database)
- **Embeddings**: local via `fastembed` (CPU, ONNX runtime)
- **Access-aware decay**: High-access nodes have decay suppression
- **Smart search**: Hybrid BM25 + vector similarity
- **Topic clustering**: Cross-session knowledge sharing via centroid-based clusters (from objeta L3 cache pattern)

### GraphStore (relational)
- **Backend**: SQLite with WAL mode
- **Relations**: Custom edge types between nodes
- **Used for**: Structured fact storage, graph queries

### CausalGraph (DAG)
- **Purpose**: Track causal relationships between actions and outcomes
- **Edge types**: `caused_by`, `fixed_by`, `invalidated_by`
- **Query**: Root cause analysis, counterfactual reasoning

### EventLog & SnapshotStore
- **EventLog**: Append-only JSONL file, typed events (ToolCalled, FileMutated, EntropyAlert, etc.)
- **SnapshotStore**: Content-addressed gzip-compressed file snapshots
- **Separation**: EventLog = causal history, SnapshotStore = state materialization
- **Auto-truncation**: Disk log capped at 50,000 lines (~10MB)

---

## Tool system

### ToolMetaRegistry
- 65+ tools registered with lightweight metadata (~80 bytes/tool)
- BM25-powered semantic search for tool discovery
- Deferred hydration: tool implementations loaded on-demand
- Serendipity injection: 3 random non-core tools injected each turn

### Capability abstraction
Tools are categorized into 10 capabilities: Search, Modify, Execute, Investigate, Orchestrate, Reflect, Validate, Learn, Coordinate, Simulate

### Tool categories
- Core (always loaded): `chat`, `codeact`, `discover_tools`
- FileSystem: `read_file`, `write_file`, `apply_patch`, `grep_search`, `list_dir`
- Network: `web_fetch`, `web_search`, browser
- Media: screenshot, camera, vision, media understanding
- Codex: MCTS, swarm, LSP bridge, AST mutation, time-travel debugger, etc.

---

## Security model

### ConstitutionalPolicy (immutable)
- Agent cannot modify its own source code paths (`crates/core/src/`, etc.)
- Policy engine files are protected
- Destructive shell commands (`rm -rf /`, `sudo`, `chmod 777`) are blocked

### DefaultSecurityPolicy (enforceable)
- Shell commands audited for dangerous patterns
- File paths checked against allow/deny lists
- Can require manual approval for high-risk operations

### ExecutionProfile
Each tool has an execution profile with:
- `SideEffectLevel`: None / Local / Irreversible
- `FilesystemScope`: None / Confined / Unrestricted
- `Reversibility`: Trivial / Possible / Impractical

---

## Multi-provider model routing

The `ModelRouter` handles:
1. **Model selection**: Economy-aware scoring (ROI-based) or manual override. Mock detection via `AgentModel::is_mock()` (type-based, not string comparison).
2. **Token budget tracking**: DSGE-inspired economics with shadow prices
3. **Fallback chain**: On rate limits, MAX_TOKENS, or empty responses
4. **Performance tracking**: Per-model success rates, latency, and cost

Supported providers:
- Anthropic (Claude)
- Google (Gemini)
- OpenAI (GPT-4, o-series)
- DeepSeek
- Groq (Llama, Mixtral)
- Ollama (local models)
- OpenRouter (multi-provider proxy)

---

## Heartbeat & automation

- **HeartbeatManager**: Periodic maintenance cycle (default 30min)
- **Structured probes**: disk_usage, memory_pressure, task_queue_lag, snapshot_quota, LLM success rate, cargo check staleness
- **Hysteresis state machine**: Healthy ↔ Degraded ↔ Critical ↔ Recovering
- **Initiative engine**: Scans trajectory for unresolved tasks, spawns sub-agents
- **Cron**: Schedule agent tasks via cron expressions

---

## Key files

| File | Lines | Purpose |
|---|---|---|
| `crates/core/src/agent.rs` | ~2050 | Agent struct, chat loop, session management |
| `crates/core/src/model_router.rs` | ~200 | Model selection, fallback, economy integration |
| `crates/core/src/orchestration/world.rs` | ~1300 | World model agent, plan AST, verification |
| `crates/core/src/event_log.rs` | ~400 | Append-only typed event log |
| `crates/core/src/snapshot_store.rs` | ~500 | Content-addressed file snapshots |
| `crates/memory/src/weaver.rs` | ~400 | KnowledgeNexus (LanceDB vector store) |
| `crates/core/src/orchestration/tool_scheduler.rs` | ~550 | Tool policy, exploration budget, CodeAct gate, DynamicLambda |
| `crates/core/src/orchestration/governor.rs` | ~450 | ToolGovernor + IntegratedGovernor (3-loop control) |
| `crates/core/src/orchestration/health_monitor.rs` | ~400 | Structured probes, state machine |

---

## Test status

- 81 unit tests in `pharmakon-core`
- All integration tests passing
- 2 tests ignored (require Ollama)
- `cargo check --workspace` passes with 0 errors
