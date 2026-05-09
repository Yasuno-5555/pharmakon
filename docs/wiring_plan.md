# Wiring Plan — Dead Component Resurrection

**Date**: 2026-05-09
**Target**: Make 66 unused tools discoverable + wire 7 orphaned modules

## Part A: Codex Serendipity Injection (HIGH IMPACT)

**Problem**: BM25 search matches tool descriptions against user query. Codex tools
("Run counterfactual simulations") never match queries like "fix the auth bug".

**Solution**: Add a second injection path that randomly samples 3 non-core tools.
Weight by historical success rate from `ToolReliability` or observed call counts.

**Files**: `agent.rs` (modify tool_definitions block)

## Part B: Retry Strategy Wiring (HIGH IMPACT)

**Problem**: `classify_failure()` exists in `retry.rs` but the agent loop never uses it.
Tool failures are just logged and reported, no strategic retry.

**Solution**: After tool failure, classify the error, and apply retry strategy:
- Transient (429 rate limit) → retry with backoff
- Terminal (file not found) → skip, inform LLM
- Strategic (compile error) → feed to RLFC for auto-fix

**Files**: `agent.rs` (modify tool error handling)

## Part C: Research Notebook Activation (MEDIUM IMPACT)

**Problem**: `Agent.research_notebook` is `"Uninitialized"` and never written to.

**Solution**: Auto-record task outcomes after each session. Use notebook entries
as working memory context for future related tasks.

**Files**: `agent.rs` (add recording at session end)

## Part D: Forensic Background Analysis (LOW IMPACT)

**Problem**: `ForensicLog` events are emitted but never analyzed.

**Solution**: Background task that periodically analyzes event log for patterns
and feeds insights back to Knowledge Nexus.

**Files**: `agent.rs` (spawn background task), `security/forensic.rs` (add analysis)
