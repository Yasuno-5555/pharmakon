# Pharmakon Engineering Guide

This document records engineering patterns, constraints, and conventions learned during development. It is intended for contributors and advanced users.

---

## Code conventions

- **Rust stable only**: No `#![feature(...)]`, no nightly.
- **Strict typing**: Avoid `any`; prefer concrete types and `Result<T, E>`.
- **Brief comments**: Only for non-obvious logic (why, not what).
- **Split files at ~700 LOC**: When clarity and testability improve.
- **Imports**: Group std, external, crate. No wildcard imports except in tests.
- **No `#[allow(unused)]` in production**: Fix the warning, don't suppress it.

## Tool conventions

- **Capability abstraction**: Think in terms of capabilities (Search, Modify, Execute), not individual tools. The routing layer resolves the best concrete tool.
- **CodeAct**: For multi-step file/code operations. Rhai tried first; falls back to Python on error.
- **Regular tools**: Use for single operations, DB queries, context reads, tool discovery.

## Safety constitution

- Agent cannot modify its own source code (`crates/core/src/`, `crates/common/src/`, `crates/memory/src/`, `crates/tools/src/`)
- Policy engine files are protected from modification
- Destructive commands (`rm -rf /`, `sudo`, `chmod 777`) are blocked
- `git clean` (any argument) is blocked in shell policies
- Sub-agent results must be verified via `SpawnHandle`

## Provider configuration

Fallback models are configured in `~/.pharmakon/config.json`:

```json
{
  "default_agent": {
    "fallback_models": [
      "deepseek/deepseek-v4-flash",
      "gemini/gemini-2.5-flash",
      "groq/llama-3.3-70b-versatile"
    ]
  }
}
```

## Build & test

```bash
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
```

## Verification gates

Before a change is considered complete:
1. `cargo check` passes
2. `cargo test` passes (all tests)
3. `cargo clippy` has no new warnings
4. `cargo fmt` passes

## Event bus

Events are communicated via `broadcast::channel(100)`. Overflow is logged as warnings (not silently dropped after the R2 fix). If the channel is consistently overflowing, increase the capacity or add a receiver.

## Snapshot strategy

- **EventLog**: Append-only JSONL. Max 50,000 lines on disk (~10MB). Older lines auto-truncated.
- **SnapshotStore**: Content-addressed gzip files. 500MB quota. Snapshots evicted oldest-first on quota exceeded.
- **Whole-workspace snapshots**: Only for `shell` and `codeact` tools, with 60-second cooldown.
- **File-level snapshots**: Before `write_file`, `apply_patch`, `mutate_ast` calls.
- **Rollback**: `rollback_to_snapshot()` restores a single file. `rollback_to_event()` restores all files mutated after a given event.

## Background tasks

The following tasks run in the background:
- **Dream Mode**: Decays skill library entries every 5 minutes
- **HeartbeatManager**: Runs maintenance and initiative checks every 30 minutes
- **DetachedTaskRuntime**: Monitors spawned agent tasks with 300s timeout
- **DirectoryIndexingDaemon**: Rebuilds file index in background using `spawn_blocking`

All background tasks check `shutdown_token` for graceful shutdown.

## Skills system

- Skills are stored in-memory in `RhaiSkillLibrary`
- Successful CodeAct scripts are recorded with `SkillGenome` metadata (capabilities, failure modes, token cost, success rate)
- Primitive Darwinism: experimental → stable → core → deprecated → removed (promoted by usage count: 10→stable, 50→core)

## DSGE economics

Token allocation uses a simplified DSGE-inspired model:
- **Production function**: `Q = α(1 - e^{-βT/θ})` (concave, diminishing returns)
- **Shadow price**: Rises as budget depletes (`λ = 1/remaining`)
- **Regime switching**: Normal → Congestion → Crisis → Offline (Markov chain)
- **Model selection**: Economy-aware ROI scoring across available models
