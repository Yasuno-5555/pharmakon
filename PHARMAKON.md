# Pharmakon Engineering Mandates

This document serves as the single source of truth for architectural constraints and engineering patterns learned during autonomous operation.

## 1. Code Modification Protocol
- **Precision First**: Never use `write_file` for modifying existing source code. Always use `apply_patch`.
- **Unified Diff Standard**: Patches must be valid unified diffs with proper hunk headers (`@@ -L,C +L,C @@`).
- **Idempotency**: Ensure that repeated tool calls do not create redundant code blocks or invalid states.

## 2. Epistemic Hygiene
- **Reflection Cycle**: Every major task completion MUST trigger a `reflect` call to update this document or internal memory.
- **Fact Verification**: Trust `grep` and `ls` over model internal hallucinations. If a file is not found, verify the path using `ls -R`.

## 3. Task Management
- **Dependency Tracking**: Use the `dependencies` field in `task.md` to manage complex multi-agent workflows.
- **Checkpointing**: For tasks exceeding 10 turns, save state using `checkpoint` to prevent context drift and loss of intent.

## 4. Modern UI & Rich Aesthetics
- **No Placeholders**: Never use generic placeholders for UI components.
- **Visual Excellence**: Prioritize gradients, micro-animations, and sleek dark modes for all frontend work.
