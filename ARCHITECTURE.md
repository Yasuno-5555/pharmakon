# Pharmakon System Architecture

This document describes the high-level design, component boundaries, and data flow of the Pharmakon Personal AI Engineering OS.

**Last updated:** 2026-05-08 (Phase 0–4 complete)

## Core Philosophy

Pharmakon is designed around four pillars:
1. **Local-First Reliability**: Sensitive data and heavy processing (AST indexing, vector search) remain on the user's machine.
2. **Deterministic Engineering**: Event-sourced execution with snapshot-based rollback ensures agent behavior is observable, reproducible, and reversible.
3. **Epistemic Integrity**: A structured memory system (Knowledge Nexus) with causal edge tracking (`caused_by`, `fixed_by`, `invalidated_by`).
4. **Sandboxed Safety**: Progressive isolation from CodeAct scripting (Rhai → Python fallback) to ephemeral Docker containers, guarded by a Constitutional Policy Engine.

## Component Diagram (C4-Style)

```
┌────────────────────────────────────────────────────────────────────┐
│                       External Channels                            │
│            Telegram  │  Discord  │  Slack  │  Web Browser           │
└────────────────────────────┬───────────────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────────────┐
│                    pharmakon-gateway                                │
│  Orchestrator │ WebSocket Hub │ REST API │ Xilem Dashboard           │
└────────────────────────────┬───────────────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────────────┐
│                      pharmakon-core (The Brain)                     │
│                                                                     │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────────────────┐  │
│  │Agent Loop   │  │Soul Manager  │  │Constitutional PolicyEngine│  │
│  │(Entropy     │  │              │  │(immutable safety rules)   │  │
│  │ Monitor)    │  └──────────────┘  └───────────────────────────┘  │
│  └──────┬──────┘                                                    │
│         │                                                           │
│  ┌──────▼──────────────────────────────────────────────────────┐   │
│  │              Control Plane (Phase 1)                         │   │
│  │  ┌─────────────────┐  ┌──────────────┐  ┌────────────────┐  │   │
│  │  │Entropy Monitor  │  │Atomic Rollback│  │Cognitive       │  │   │
│  │  │(stagnation 0.4, │  │(snapshot-based│  │Scheduler       │  │   │
│  │  │ repetition 0.25)│  │ file restore) │  │(LLM classify)  │  │   │
│  │  └─────────────────┘  └──────────────┘  └────────────────┘  │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │              Intelligence Layer (Phase 2)                     │  │
│  │  ┌──────────────────┐  ┌──────────────┐  ┌────────────────┐  │  │
│  │  │Capability        │  │Causal Memory │  │Swarm Return    │  │  │
│  │  │Abstraction       │  │Edges         │  │Channel         │  │  │
│  │  │(65 tools→10 caps)│  │(caused_by etc)│  │(SpawnHandle)   │  │  │
│  │  └──────────────────┘  └──────────────┘  └────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │              Advanced Features (Phase 3)                      │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────────────┐  │  │
│  │  │CodeAct Hybrid│  │Constitutional│  │Durable Task        │  │  │
│  │  │(Rhai engine) │  │Engine         │  │Runtime             │  │  │
│  │  └──────────────┘  └──────────────┘  └────────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐  │
│  │              Foundation (Phase 0)                             │  │
│  │  ┌──────────────────┐  ┌──────────────┐  ┌────────────────┐  │  │
│  │  │ToolMetaRegistry  │  │EventLog +    │  │ExecutionProfile│  │  │
│  │  │(BM25, defer load)│  │SnapshotStore │  │(risk assessment)│  │  │
│  │  └──────────────────┘  └──────────────┘  └────────────────┘  │  │
│  └──────────────────────────────────────────────────────────────┘  │
└────────────────────────────┬───────────────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────────────┐
│                    pharmakon-tools (The Hands)                      │
│  Codex OS Tools │ AST Mutation │ LSP Bridge │ Browser │ Shell       │
│  ToolMetaRegistry (BM25 search, deferred hydration)                 │
└────────────────────────────┬───────────────────────────────────────┘
                             │
┌────────────────────────────▼───────────────────────────────────────┐
│                   pharmakon-memory (The Memory)                     │
│  KnowledgeNexus │ LanceDB Embeddings │ SQLite Graph │ Causal Edges  │
└────────────────────────────────────────────────────────────────────┘
```

## Crate Responsibilities

### `pharmakon-core` — Skill Library (Phase 4)
- **Skill Genome System**: Quantitative metadata per script (capabilities, failure_modes, token_cost, success_rate, composability_score).
- **Composite Skills**: Merge two verified primitives into higher-order functions via `compose_skills()`.
- **Trajectory Compression**: Extract reusable patterns (e.g. `safe_refactor()`) from raw agent traces.
- **Skill Crystallization**: Auto-suggest Rhai→Rust native compilation candidates with `suggest_crystallizations()`.
- **AntiPattern Extraction**: Cluster script failures, generate positive guidance, inject into system prompt.
- **Primitive Darwinism**: Lifecycle management (experimental → stable → core → deprecated → removed) driven by usage counts.
- **Dream Mode**: Background self-play loop — generate tasks, execute scripts, verify, label, store — fully autonomous skill acquisition.

### `pharmakon-core` (The Brain)
- **Decision Loop**: Async iteration handling LLM completions, tool calls, parallel context gathering, and entropy-based loop detection.
- **Entropy Monitor**: Four-factor entropy scoring (stagnation 0.4, repetition 0.25, failure 0.2, token_drift 0.15) with `EntropyOverflow` hard termination.
- **Atomic Rollback**: `rollback_to_snapshot()` / `rollback_to_event()` — file-level restore via content-addressed SnapshotStore.
- **Cognitive Scheduler**: LLM-based task complexity classification (Simple/Standard/Deep) with heuristic fallback. `ManagedTask` with cognitive economics (`priority_score`, `expected_information_gain`, `retry_cost`).
- **CodeAct Hybrid Mode** (Python + Rhai): Rhai tried first (fast, sandboxed); falls back to Python via `python3` on error for higher LLM fluency. 1 LLM turn = 10+ tool calls via control flow in scripts. Marked as Core tool, always available. System prompt explicitly instructs CodeAct as PRIMARY execution mode for multi-step tasks.
- **Constitutional PolicyEngine**: Immutable safety rules preventing self-modification, critical file deletion, and destructive shell commands.
- **Durable Task Runtime**: `suspend()` / `resume()` with EventLog integration and `TaskSnapshot` persistence.
- **Soul Management**: Markdown-based "Soul" files defining personality, constraints, and instructions.
- **Multi-Provider Fallback**: Automatic model switching on API rate limits (429). Fallback chain configurable via `~/.pharmakon/config.json`. Default: deepseek/deepseek-chat → gemini/gemini-2.5-flash → groq/llama-3.3-70b-versatile. DeepSeek registered as first-class provider (API key: `DEEPSEEK_API_KEY`).
- **Integrated MCP**: Native Model Context Protocol support for external tool servers.

### `pharmakon-memory` (The Memory)
- **Knowledge Nexus**: Vector embeddings (LanceDB) + relational graph (SQLite) for hybrid RAG.
- **Causal Memory Edges**: Tracks `caused_by`, `fixed_by`, `invalidated_by` relationships. Auto-recorded by RLFC on success/failure.
- **Access-Aware Decay**: High-access nodes receive decay suppression to prevent loss of critical architectural knowledge.

### `pharmakon-tools` (The Hands)
- **ToolMetaRegistry**: BM25-powered deferred tool loading. 65+ tools indexed by lightweight metadata (~80 bytes/tool). Full implementations hydrated on-demand.
- **Capability Abstraction**: 65 tools mapped to 10 semantic capabilities (`Search`, `Modify`, `Execute`, `Investigate`, `Orchestrate`, `Reflect`, `Validate`, `Learn`, `Coordinate`, `Simulate`). 90% token reduction in prompt injection.
- **ExecutionProfile**: Risk classification via `SideEffectLevel`, `FilesystemScope`, `Reversibility`.
- **Codex Tools**: Execution Trace, Deterministic Replay, Dry-Run, AST mutation, LSP bridging.
- **Standard Tools**: Browser, Shell, File I/O, Web Search, RepoMap.

### `pharmakon-common` (The Foundation)
- **ToolMetaCatalog**: BM25-indexed tool metadata catalog with `capability_summary()`.
- **EventLog**: Append-only JSONL event log with structured `EventKind` variants (`ToolCalled`, `FileMutated`, `EntropyAlert`, etc.).
- **SpawnHandle**: `oneshot::Receiver`-based sub-agent result handle. Replaces fire-and-forget spawn.
- **Shared types**: `AgentSpawner`, `ExecutionProfile`, `Tool`, `Config`, `Event`.

### `pharmakon-gateway` (The Senses & Voice)
- **Multi-Channel Hub**: Telegram, Discord, Slack bots.
- **Real-time Desktop Dashboard**: Xilem+Vello native GUI with 8 tabs (Chat, Dashboard, Automation, Skills, Research, Database, System, Settings). Vello-powered SwarmVisualizer with animated particle system. Feature parity with React/TypeScript Web frontend.
- **Tool Orchestration**: Initializes tools and registers them with the Agent.

## Interaction Flow: The Decision Loop

1. **Input**: Message arrives via Gateway channel.
2. **Task Classification**: Cognitive Scheduler classifies complexity (LLM primary, heuristic fallback).
3. **Parallel Context Gathering**: Knowledge Nexus + Semantic Search + Working Memory queried concurrently.
4. **Capability-Aware Prompt**: 10 capabilities injected instead of 65 tool schemas.
5. **Decision Turn**:
   - Agent sends context + goal to Model.
   - Tool calls executed (potentially in parallel).
   - File mutations trigger SnapshotStore capture + EventLog `FileMutated` recording.
6. **Entropy Check**: Four-factor entropy computed from EventLog. >0.8 warns, >0.95 hard-terminates.
7. **Progress Tracking**: `ProgressTracker` checks for stalls, loops, and entropy overflow.
8. **Self-Correction**: Errors fed back to Model for autonomous recovery.
9. **Response**: Final answer delivered to user.
10. **Reflection Cycle**: Periodic reflection extracts new facts, updates PHARMAKON.md.

## Security & Reliability

- **Constitutional PolicyEngine**: Immutable rules that cannot be bypassed — no self-modification, no critical file deletion, no destructive commands.
- **Atomic Rollback**: Any file mutation can be reversed to its pre-mutation snapshot via `rollback_to_event()`.
- **Dry-Run Mode**: Destructive tools can run in simulation mode.
- **Entropy Overflow Protection**: Pathological loops detected via stagnation analysis and hard-terminated.
- **SpawnHandle**: Sub-agent results are verified via `oneshot` channels — no more hallucinated success.
- **Strict Layering**: `common` → `memory` → `core` → `tools` → `gateway` → `cli`.
