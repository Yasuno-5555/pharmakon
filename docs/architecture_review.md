# Pharmakon Architecture Review

Date: 2026-05-07

## Current Workspace Truth

The Cargo workspace is the single source of truth for crate layout. The active crates are:

- `crates/common`: shared agent types, tool traits, events, config, secrets.
- `crates/core`: agent loop, providers, prompts, orchestration, persistence, automation, security policy.
- `crates/memory`: Knowledge Nexus, LanceDB/FastEmbed embeddings, SQLite graph, semantic search, compaction.
- `crates/tools`: standard and Codex-style tool implementations.
- `crates/gateway`: Axum HTTP/WebSocket gateway, ACP bridge, webhooks.
- `crates/channels`: messaging adapters.
- `crates/cli`: installable `pharmakon` binary.
- `crates/gui`: native GUI/system tray scaffolding.
- `crates/mcp`: MCP integration.
- `crates/plugin-sdk`: WASM plugin SDK.
- `crates/audio`: audio capability crate.

Earlier docs omitted `memory`, `gui`, `mcp`, `plugin-sdk`, and `audio` in different places. README and the system spec now align with the workspace.

## Findings

1. Crate structure drift was real.
   The implementation already had more crates than README described. The fix is documentation alignment plus treating `Cargo.toml` workspace membership as the authoritative structure.

2. Knowledge Nexus is partially implemented, not merely aspirational.
   `crates/memory/src/weaver.rs` uses LanceDB and FastEmbed, and `crates/tools/src/ast_ingest.rs` uses tree-sitter-rust. The graph/vector path exists, but visualization, semantic conflict workflows, and actor-style event sourcing were missing. Tooling has been added to expose those workflows.

3. Reflection existed but was unsafe.
   The prior `reflect` tool appended arbitrary text to `PHARMAKON.md`. It now validates lesson length, filters dangerous self-modification phrases and secret markers, deduplicates bullets, and writes timestamped reflection logs.

4. Decay policy was too aggressive in the docs.
   Runtime decay now clamps aggressive factors, and the docs describe access-aware ranking instead of a naive 5% daily loss model.

5. Agent type ownership had split.
   Gateway/channels/heartbeat mixed `Arc<Agent>` and `Arc<Mutex<Agent>>`. The install path now consistently uses `Arc<Agent>` where the agent already owns internal mutexes.

6. Tool surface was under-registered.
   CLI startup registered only a few tools manually. The standard agent setup now registers filesystem, terminal, browser, web, git, project, Knowledge Nexus, and Codex-style engineering tools with duplicate-name protection.

## Added Tooling

The new Codex-style tool layer lives in `crates/tools/src/codex.rs` and includes:

- Trace and replay: `execution_trace`, `deterministic_replay`, `time_travel_debugger`.
- Reliability and decision support: `tool_reliability`, `context_budget_optimizer`, `cognitive_mirror`, `intent_compiler`, `failure_prediction`, `regret_minimization`, `counterfactual_simulator`.
- Safe execution: `dry_run`, `diff_security_auditor`, `workspace_snapshot`, `spec_first_test`.
- Code intelligence: `semantic_grep`, `mutate_ast`, `ast_lsp_bridge`.
- Knowledge Nexus operations: `semantic_conflict_resolution`, `nexus_visualizer`, `memory_actor_status`, `graph_prefetch`.
- Autonomy and evolution: `autonomy_dial`, `skill_composition`, `failure_memory`, `proactive_intervention`, `proactive_self_optimization`, `mcts_simulator`, `rlfc`, `ephemeral_red_team`, `fractal_swarm`.
- Local utility ports: `node_repl`, `automation`, `current_time`, `weather_lookup`, `finance_lookup`, `sports_lookup`, `local_model_router`, `web_task`, `codex_tool_catalog`.

## Recommended Next Work

1. Replace planning-only advanced tools with deeper runtime integrations one by one.
   Start with `memory_actor_status` becoming the real single-writer memory actor, then route all Knowledge Nexus writes through it.

2. Harden patch application.
   Place `diff_security_auditor` and `dry_run` in the agent tool execution path before risky file and shell tools, not only as callable tools.

3. Make `mutate_ast` truly parser-backed.
   The current implementation performs structured Rust function replacement with brace matching and rustfmt. The next step is a syn/tree-sitter-backed node replacement engine.

4. Move multi-agent failure policy into core orchestration.
   Add explicit timeout, cancellation, parent-child propagation, and cycle detection to swarm/supervisor messaging.

5. Add focused integration tests for the new tool pack.
   The build passes, but trace/replay/snapshot/security tools should get small deterministic tests before treating them as stable APIs.
