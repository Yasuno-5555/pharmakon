# 🦞 Pharmakon — Personal AI Engineering OS

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Pharmakon** is a high-performance, local-first Rust AI engineering OS. It serves as a unified control plane for autonomous AI engineering, tool orchestration, and multi-channel communication.

**Status:** Phase 0–3 complete (2026-05-08). Foundation, Control Plane, Intelligence Layer, and Advanced Features all implemented.

## Key Features

### Foundation (Phase 0)
- **ToolMetaRegistry + BM25 Search**: 65+ tools indexed by lightweight metadata (~80 bytes/tool). Deferred hydration — tool implementations loaded on-demand. BM25-powered semantic tool discovery.
- **EventLog & SnapshotStore**: Append-only JSONL event log with structured event kinds. Content-addressed file snapshots enabling atomic rollback. Separated causal history from state materialization.
- **ExecutionProfile Classification**: Risk assessment via `SideEffectLevel` (None/Local/Irreversible), `FilesystemScope` (None/Confined/Unrestricted), and `Reversibility` (Trivial/Possible/Impractical).

### Control Plane (Phase 1)
- **Entropy Monitor**: Four-factor inline entropy scoring (stagnation 0.4, repetition 0.25, failure 0.2, token_drift 0.15). Hard-terminates pathological loops at >0.95 entropy.
- **Atomic Rollback**: `rollback_to_snapshot()` and `rollback_to_event()` — reverse any file mutation to its pre-mutation state via SnapshotStore.
- **Cognitive Scheduler**: LLM-based task complexity classification (Simple/Standard/Deep) with heuristic fallback. `ManagedTask` with cognitive economics (`priority_score`, `expected_information_gain`, `retry_cost`).

### Intelligence Layer (Phase 2)
- **Capability Abstraction**: 65 tools mapped to 10 semantic capabilities (`Search`, `Modify`, `Execute`, `Investigate`, `Orchestrate`, `Reflect`, `Validate`, `Learn`, `Coordinate`, `Simulate`). ~90% token reduction in prompt injection.
- **Causal Memory Edges**: Knowledge Nexus tracks `caused_by`, `fixed_by`, `invalidated_by` relationships. Auto-recorded by RLFC on success/failure.
- **Swarm Return Channel**: `SpawnHandle` with `oneshot::Receiver` — sub-agent results are verified, not hallucinated. `FractalSwarmTool` awaits all handles.

### Advanced Features (Phase 3)
- **CodeAct Hybrid Mode**: Rhai scripting engine for compound tool execution. 1 LLM turn = 10+ tool calls via control flow in scripts. Sandboxed with registered functions only.
- **Constitutional Engine**: Immutable safety rules preventing self-modification, critical file deletion, and destructive shell commands. Cannot be bypassed.
- **Durable Task Runtime**: `suspend()` / `resume()` with `TaskSnapshot` serialization and EventLog integration.

## Project Structure

- **`pharmakon-core`**: The agent's brain — decision loop, entropy monitor, atomic rollback, cognitive scheduler, CodeAct engine, constitutional policy engine.
- **`pharmakon-memory`**: Long-term memory — Knowledge Nexus (LanceDB + SQLite Graph), causal edges, semantic search, access-aware decay.
- **`pharmakon-tools`**: The agent's hands — ToolMetaRegistry (BM25), capability abstraction, Codex OS tools, AST mutation, LSP bridge, browser, shell.
- **`pharmakon-gateway`**: Senses and voice — multi-channel (Telegram, Discord, Slack), real-time dashboard, tool orchestration.
- **`pharmakon-cli`**: Primary UI — onboarding, agent interaction, service management.
- **`pharmakon-common`**: Shared foundation — `ToolMetaCatalog`, `EventLog`, `SpawnHandle`, `ExecutionProfile`, core traits.

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): Full system architecture, component diagram, interaction flow.
- [PHARMAKON.md](PHARMAKON.md): Engineering mandates, safety constitution, tool discipline.
- [実装計画書.md](実装計画書.md): Implementation plan with all Phase 0–3 tasks.
- [docs/](docs/): User guide, plugin development, channel setup.

## Getting Started

### Prerequisites
- Rust 1.75+
- Docker (for sandboxed execution)
- SQLite

### Installation
```bash
git clone https://github.com/Yasuno-5555/Pharmakon.git
cd Pharmakon
cargo build --release
```

### Setup
```bash
cargo run -- onboard
```

### Running the Gateway
```bash
cargo run -- gateway --port 19999
```

## Security

- **Constitutional PolicyEngine**: Immutable rules — no self-modification, no critical file deletion, no destructive commands.
- **Atomic Rollback**: Any file mutation reversible via `rollback_to_event()`.
- **Entropy Overflow Protection**: Pathological loops hard-terminated at >0.95 entropy.
- **SpawnHandle Verification**: Sub-agent results verified via `oneshot` channels.

## License

MIT License.
