# 💊 Pharmakon — Personal AI Engineering OS

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Pharmakon** is a high-performance, local-first Rust AI engineering OS. It serves as a unified control plane for autonomous AI engineering, tool orchestration, and multi-channel communication.

**Status:** Phase 0–5 complete (2026-05-09). World Model Agent, Dynamic max_tokens, Codex Serendipity, Skill Library auto-crystallization, Cron scheduling, DB migration (name column).

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
- **CodeAct Hybrid Mode**: Python + Rhai scripting. Rhai tried first; falls back to Python on error for higher LLM fluency. 1 LLM turn = 10+ tool calls. Marked as Core — PRIMARY for compound file/code ops. Regular tools for context/DB/SOUL.
- **Constitutional Engine**: Immutable safety rules preventing self-modification, critical file deletion, and destructive shell commands. Cannot be bypassed.
- **Durable Task Runtime**: `suspend()` / `resume()` with `TaskSnapshot` serialization and EventLog integration.

### Self-Evolving Intelligence (Phase 4)
- **Skill Genome System**: Quantitative metadata per script (capabilities, failure_modes, cost, success_rate). Composite skill merging via `compose_skills()`. Trajectory compression into reusable patterns.
- **Primitive Darwinism**: Lifecycle management (experimental → stable → core → deprecated → removed). Auto-promotion by usage count.
- **AntiPattern Extraction**: Cluster script failures → positive guidance → system prompt injection. Never "don't do X" — always "✅ correct way".
- **Dream Mode**: Background self-play — generates tasks, writes/executes scripts, verifies via cheap LLM, labels and stores. Fully autonomous skill acquisition.
- **Model Auto-Routing**: `ModelMode::Auto` scores all providers by live ROI each turn. `ModelPerformanceTracker` records real-time success/latency. `/model auto` switches to optimal routing.
- **Swarm Economy**: `FractalSwarmTool` uses `GeneralEquilibrium.market_clearing()` to allocate token budgets. Specialization-aware model selection (Deep→accuracy, Fast→latency).
- **Plugin SDK v3**: Updated  trait with , .  trait with , .  event bridge.  with 11 variants.
- **DeepSeek V4**: Models updated to /. Onboarding wizard supports DeepSeek. Default port corrected to 19999.
- **DSGE Economics Engine**: 30+ structures in `cognitive_economics.rs` — `CognitiveBudget` with shadow price, `BellmanPlanner` (iterative DP),  (Markov chain), `ProviderPortfolio` (Markowitz),  (Walrasian tâtonnement). 6 injection points wired into agent loop.
- **Skill Crystallization**: Auto-suggests Rhai→Rust native compilation for stable high-usage skills via `suggest_crystallizations()`.
- **Multi-Provider Fallback**: Auto-switches on API rate limits. Configurable via `~/.pharmakon/config.json`. DeepSeek as first-class provider.
- **Xilem+Vello Desktop GUI**: 8-tab native dashboard (Chat, Stats, Automation, Skills, Research, Graph, Logs, Config). Vello-powered animated SwarmVisualizer. Event bridge with 18+ event types.

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

### Running the Desktop GUI
```bash
cargo run -- gui
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
