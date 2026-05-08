# Pharmakon Architecture: The Cortex-Nervous System Boundary

**Status:** Canonical. All architectural decisions MUST respect this boundary.
**Last updated:** 2026-05-08

## Core Principle

> LLM is the cortex. Runtime is the nervous system. Tools are muscles. Memory is the hippocampus. Scheduler is the prefrontal cortex. Policy engine is the immune system.

The single most important architectural decision in Pharmakon is **what NOT to use the LLM for**. This document defines the boundary and maps every component to its correct side.

## The Boundary

```
                         ┌─────────────────────────────────┐
                         │           LLM (CORTEX)           │
                         │                                 │
                         │  ✓ Planning                     │
                         │  ✓ Retrieval guidance            │
                         │  ✓ Semantic diff interpretation  │
                         │  ✓ Hypothesis generation         │
                         │  ✓ Reflection                    │
                         │  ✓ Prioritization                │
                         │  ✓ Interface (natural language)  │
                         │                                 │
                         │  ✗ Scheduling (deterministic)    │
                         │  ✗ Rollback (deterministic)      │
                         │  ✗ Retry policy (deterministic)  │
                         │  ✗ Filesystem state (determ.)    │
                         │  ✗ Caching (deterministic)       │
                         │  ✗ Sandboxing (deterministic)    │
                         │  ✗ Capability routing (determ.)  │
                         │  ✗ Budget accounting (determ.)   │
                         └──────────────┬──────────────────┘
                                        │
          ┌─────────────────────────────┼─────────────────────────────┐
          │                             │                             │
┌─────────▼─────────┐    ┌──────────────▼──────────┐    ┌───────────▼───────────┐
│  RUNTIME (NERVOUS) │    │     TOOLS (MUSCLES)      │    │  MEMORY (HIPPOCAMPUS) │
│                   │    │                          │    │                       │
│  Scheduling       │    │  Shell execution         │    │  KnowledgeNexus       │
│  Rollback         │    │  File I/O                │    │  Semantic Search      │
│  Retry policy     │    │  CodeAct (Rhai)          │    │  Causal Edges         │
│  Budget accounting│    │  Network requests        │    │  Embeddings           │
│  Caching          │    │  Media processing        │    │  Belief System        │
│  Sandboxing       │    │  AST manipulation        │    │                       │
│  Capability routing│   │  LSP queries             │    │                       │
│  Policy evaluation│    │                          │    │                       │
│  Entropy monitoring│   │                          │    │                       │
└───────────────────┘    └──────────────────────────┘    └───────────────────────┘
```

## Why This Boundary Matters

### LLM Strengths (Cortex)
| Capability | Why LLM | Pharmakon Implementation |
|-----------|---------|------------------------|
| **Planning** | Ambiguity handling — can navigate uncertain problem spaces | System prompt §1-2 (Decomposition + Composition Pattern) |
| **Retrieval guidance** | Relevance estimation — "which of these 50 results matters?" | `semantic_search()` in KnowledgeNexus |
| **Semantic diff interpretation** | Understanding intent behind code changes | RLFC: LLM interprets clippy errors → generates fix |
| **Hypothesis generation** | Creative exploration of solution space | `reflect()` tool for insight extraction |
| **Reflection** | Meta-cognition — "what did I learn?" | `reflect()` extracts facts into PHARMAKON.md |
| **Prioritization** | Value judgment — "which of these is most important?" | `classify_task_complexity()` — LLM primary |
| **Interface** | Natural language is the universal API | All user interaction via chat |

### Runtime Strengths (Nervous System)
| Capability | Why NOT LLM | Pharmakon Implementation |
|-----------|------------|------------------------|
| **Scheduling** | Deterministic cost calculation. LLMs hallucinate estimates. | `ExecutionBudget`, `ProgressTracker` |
| **Rollback** | Must be exact — one wrong byte is catastrophic. | `SnapshotStore::restore()`, `rollback_to_event()` |
| **Retry policy** | Needs consistent classification. LLMs are inconsistent. | `classify_failure()` — pure keyword matching |
| **Filesystem state** | Ground truth. LLMs hallucinate file contents. | `SnapshotStore`, `read_file`, `grep_files` |
| **Caching** | Mechanical optimization. LLMs add latency. | `PromptLayers` (prefix caching), `ToolMetaRegistry` (deferred load) |
| **Sandboxing** | Security boundary. Never trust LLM-generated constraints. | `CodeActEngine` (Rhai sandbox), `ConstitutionalPolicy` |
| **Capability routing** | 65 tools → 10 capabilities is a mechanical mapping. | `Capability::from_tool_name()`, `by_capability()` |
| **Budget accounting** | Token counting is arithmetic. LLMs can't count. | `ExecutionBudget`, `TotalTokens` atomic counter |
| **Policy evaluation** | Security rules must be non-bypassable. LLMs can be jailbroken. | `ConstitutionalPolicy`, `DefaultSecurityPolicy` |
| **Entropy monitoring** | Statistical computation. LLMs can't compute entropy. | `EventLog::recent_tool_entropy()` — bigram analysis |
| **Failure classification** | Pattern matching on error strings. LLMs overthink it. | `classify_failure()` — deterministic keyword matching |

## The Gray Zone: When LLM + Runtime Collaborate

Some capabilities require both. The rule: **LLM proposes, Runtime disposes.**

| Operation | LLM Role | Runtime Role |
|-----------|---------|-------------|
| Task complexity classification | Semantic analysis (primary) | Keyword heuristic (fallback) |
| Code generation | Write the code | Execute in sandbox, verify output |
| Search | Formulate the query | Execute BM25/vector search, return results |
| Tool selection | "I need to investigate this" | `by_capability()` → BM25 → best tool |
| Error recovery | "Try a different approach" | `RetryState::evaluate()` → classify → backoff/switch/abort |
| File modification | Generate patch content | `apply_patch` → verify → SnapshotStore capture |
| Sub-agent spawning | Decide what to delegate | `SpawnDecision` cost-benefit → `SpawnHandle` lifecycle |

## Anti-Patterns (What We Explicitly Avoid)

| Anti-Pattern | Why Bad | Pharmakon's Prevention |
|-------------|---------|----------------------|
| LLM does token counting | LLMs hallucinate numbers | `estimated_tokens()` — char/4 approximation |
| LLM decides retry strategy | Inconsistent; tends to infinite retry | `classify_failure()` → `RetryAction` |
| LLM manages filesystem state | Hallucinates file contents | Always `read_file` before `apply_patch` |
| LLM evaluates security policies | Jailbreakable | `ConstitutionalPolicy` — Rust code, not prompts |
| LLM controls budget | Can't count; will overspend | `ExecutionBudget`, wall-time hard limit |
| LLM routes tools directly | Cognitive branching explosion | `Capability` abstraction → runtime dispatch |
| LLM verifies its own output | Circular trust | Verification Principle in system prompt §3 |

## Validation: Audit Trail

Every architectural decision in this document is backed by concrete code:

```
classify_task_complexity()  → scheduler.rs:169  (LLM primary + heuristic fallback)
classify_failure()          → retry.rs:41       (pure deterministic)
evaluate_tool_call()        → policy.rs:119     (pure deterministic)
recent_tool_entropy()       → event_log.rs:165  (bigram analysis)
rollback_to_event()         → agent.rs:1504     (SnapshotStore restore)
SpawnDecision::analyze()    → swarm.rs:47       (cost-benefit arithmetic)
Capability::from_tool_name()→ capability.rs:109 (static match)
PromptLayers::assemble()    → topology.rs:55    (cache-optimized ordering)
```
