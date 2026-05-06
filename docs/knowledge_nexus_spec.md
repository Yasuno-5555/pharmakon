# Pharmakon 4.0 Knowledge Nexus: Implementation Specifications

This document outlines the technical specifications and implementation details of the Pharmakon 4.0 "Knowledge Nexus" architecture, focusing on Deep Research, Coding (AST), and Database layers.

---

## 1. Database Layer: Knowledge Nexus Architecture
The Knowledge Nexus is a hybrid memory system that unifies semantic vector search with structural graph relationships.

### 1.1 Semantic Layer (LanceDB)
- **Engine**: LanceDB (Persistent vector store).
- **Embedding Model**: `TextEmbedding` (FastEmbed / BGE-Small-EN-v1.5).
- **Schema**:
    | Field | Type | Description |
    | :--- | :--- | :--- |
    | `id` | UTF-8 | Unique identifier (UUID or File:Symbol). |
    | `text` | UTF-8 | Content snippet or logical block code. |
    | `decay_score` | Float32 | Importance score (1.0 initial, decays over time). |
    | `access_count` | UInt32 | Number of times this entry was retrieved. |
    | `vector` | FixedSizeList | 384-dimensional dense vector. |

### 1.2 Structural Layer (SQLite GraphStore)
- **Engine**: SQLite (via `sqlx`).
- **Node Types**: `file`, `function`, `struct`, `trait`, `enum`, `concept`.
- **Edge Types (Relations)**:
    - `contains`: File -> Item.
    - `calls`: Function -> Function.
    - `implements`: Struct -> Trait.
    - `depends_on`: Module -> Module.
- **Node Metadata**: Includes line ranges, summaries, and embedding IDs for cross-linking with LanceDB.

### 1.3 Retrieval Logic (Smart Search)
1.  **Vector Probe**: Find Top-K snippets in LanceDB.
2.  **Graph Expansion**: For each hit, query the GraphStore for related nodes (1-hop neighbors).
3.  **Context Augmentation**: Inject related nodes into the prompt to provide the agent with "neighborhood awareness" (e.g., seeing a function definition and its associated struct simultaneously).

---

## 2. Deep Research Layer: Multi-Layered Investigation
The Deep Research system optimizes token usage while enabling exhaustive investigations.

### 2.1 Working Memory (Context Packing Buffer)
- **Structure**: A fixed-size priority queue (Top 10 items) based on **Importance**.
- **Context Packing**: Before each LLM iteration, the agent "packs" the most relevant Working Memory units into the system prompt.
- **Eviction**: Low-importance items are automatically evicted or moved back to long-term storage (LanceDB).

### 2.2 Research Notebook
- **State Persistence**: Tracks `verified_facts`, `pending_questions`, `dead_ends`, and the `research_tree`.
- **Research Depth**:
    - `Skim`: Metadata and title only.
    - `Summary`: Key findings and bullet points.
    - `Deep`: Full text analysis with RAG block extraction.

### 2.3 Query Planner (Adaptive Strategy)
- **Hybrid**: Standard RAG for routine queries.
- **DeepResearch**: Activated for complex, multi-step investigations. Uses a beam-search approach to explore the research tree (max_depth: 3, beam_width: 2).

---

## 3. Coding Layer: AST-Aware Autonomous Engineering
Move beyond text-based search to compiler-level understanding.

### 3.1 AST-Aware Ingestion
- **Parser**: `tree-sitter` with `tree-sitter-rust`.
- **Granularity**: Logical blocks (Functions, Impls, Structs) are indexed separately.
- **Tool**: `ingest_ast_knowledge`
    - Parses `.rs` files.
    - Extracts full code blocks for each symbol.
    - Generates embeddings and graph nodes for each block.
    - Automatically builds relationship edges (e.g., which file "contains" which function).

### 3.2 Repo Mapping 2.0
- **Dynamic Mapping**: Unlike static repo maps, the Knowledge Nexus allows the agent to "zoom in" on a specific file's AST structure on-demand.
- **Structural RAG**: When the agent searches for "database connection settings", it finds the exact `struct` or `impl` block rather than just the file name.

### 3.3 Swarm Orchestration
- **Inheritance**: Sub-agents spawned in a swarm inherit the parent's `KnowledgeNexus` handle.
- **Shared Brain**: Multiple agents can read from the same Knowledge Graph, ensuring consistency in complex refactoring tasks.

---

## 4. Maintenance & Optimization
- **Bounded Memory Decay**: A periodic background process may reduce `decay_score`, but the runtime clamps aggressive factors so long-term project knowledge cannot silently vanish after a few idle weeks.
- **Access-Aware Ranking**: Retrieval combines vector similarity, keyword overlap, graph expansion, node type, `access_count`, and last access time. High-traffic and structural code nodes are favored rather than treated like disposable chat snippets.
- **Pinned/Structural Context**: Working memory keeps high-importance entries, while `code_struct` and `code_trait` nodes receive slower freshness decay during smart search.
- **Conflict Handling**: The Knowledge Nexus supports isolated writes and commit-time conflict logging. For semantic conflicts, use `semantic_conflict_resolution`, which prefers source code and compiler/test evidence over stale notes.
