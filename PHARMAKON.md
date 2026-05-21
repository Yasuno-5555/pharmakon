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

## Multi-tier entropy response (from LKO/objeta)

The agent loop applies a 4-tier escalating response to tool call entropy:

| Tier | Threshold | Response |
|------|-----------|----------|
| Normal | ≤ 0.50 | No action |
| Elevated | > 0.50 | Serendipity injection increased (3→6 non-core tools) |
| High | > 0.70 | Strategy reconsideration prompt injected into history |
| Critical | > 0.85 | Automatic fallback model switch |
| Overflow | > 0.95 | Hard-terminate the agent loop |

Thresholds are configurable via:
- `PHARMAKON_ENTROPY_TIER1` (default 0.50)
- `PHARMAKON_ENTROPY_TIER2` (default 0.70)
- `PHARMAKON_ENTROPY_TIER3` (default 0.85)
- `PHARMAKON_MAX_ENTROPY` (default 0.95)

Hysteresis (0.05 margin) prevents oscillation at tier boundaries.

## Cosine stagnation detection (from LKO Adaptive Runtime)

Each iteration's tool execution pattern is converted to a 4-dimensional feature vector
(success rate, latency, tool count, repetition). Cosine similarity between consecutive
iterations is computed:

- `cos > 0.98` for 2 consecutive iterations → `CosineStagnation` signal (early intervention)
- `cos > 0.95` → micro-stagnation logged, no termination
- `cos < 0.90` → healthy state variation

This catches loops that are not detected by identical-argument comparison (e.g., the agent
calls different tools but with the same ineffective pattern).

## Integrated Governor (from objeta OS Runtime)

Three competing control loops with priority arbitration:

| Loop | Priority | Trigger | Response |
|------|----------|---------|----------|
| SafetyGuard | 1 (highest) | Destructive commands, entropy overflow | Immediate block |
| QualityGuard | 2 | Entropy escalation, stagnation | Tool diversity, model switch |
| ResourceGuard | 3 (lowest) | Budget pressure (>80% tokens) | Scope reduction |

Arbitration: QualityGuard (intelligence protection) > ResourceGuard (cost protection).
SafetyGuard wins unconditionally over both.

DynamicLambda scales exploration budgets: `λ = base × entropy_factor × stall_factor`,
clamped to [0.5, 3.0]. Higher entropy + stalling → more exploration allowed.

## Topic clustering (from objeta L3 cache)

`KnowledgeNexus::build_topic_clusters(k)` runs centroid-based clustering on completed
LanceDB embeddings. `search_with_topic_boost()` gives a +15% score boost to results
in the same topic cluster as the top-ranked result, enabling cross-session knowledge
sharing while maintaining session isolation.

## Mock model architecture

All mock/test models implement `AgentModel::is_mock() → true` (default: `false`).
`ModelRouter` and the agent loop use `model.is_mock()` rather than string comparison
(`model.name() == "mock-model"`) for mock detection.

## DSGE economics

Token allocation uses a simplified DSGE-inspired model:
- **Production function**: `Q = α(1 - e^{-βT/θ})` (concave, diminishing returns)
- **Shadow price**: Rises as budget depletes (`λ = 1/remaining`)
- **Regime switching**: Normal → Congestion → Crisis → Offline (Markov chain)
- **Model selection**: Economy-aware ROI scoring across available models

## Dead-End Catalog

Approaches empirically invalidated — in Pharmakon or sibling research projects
(LKO, objeta). Documented to prevent wasted effort on known-futile paths.

### Hidden state caching

- **Finding**: Hidden state `h` rotates rapidly across iterations (`cos ≈ 0`).
  Reusing cached hidden states is fundamentally unreliable.
- **Source**: LKO v8 (Qwen3.6 full-forward), objeta STATUS v1.0 §13
- **Implication**: `SnapshotStore` works for files, not for in-flight agent states.
  Do not attempt hidden-state-level caching in the agent loop.

### Koopman multi-step prediction

- **Finding**: The Koopman operator `A` does not compose — `A^n` diverges from the
  true trajectory within a few steps. Multi-step latent prediction is invalid.
- **Source**: LKO v8 Findings §1 (Experiment 5)
- **Implication**: Sandwich / skip-connection approaches must limit themselves to
  single-step projection. Multi-step rollout is a dead end.

### FFN low-rank rotation

- **Finding**: 22-layer rollout with low-rank FFN rotation collapses to `cos = 0.17`.
  Low-rank approximations of MLP blocks do not preserve trajectory quality.
- **Source**: objeta STATUS v1.0 §13
- **Implication**: Do not attempt to compress computation via low-rank factorization
  in any orchestration path (e.g., skill compression, plan distillation).
