# Pharmakon 4.0: Database & Knowledge Nexus Specification

## Overview
The Knowledge Nexus is a hybrid storage system designed to provide the agent with both semantic intuition (Vector) and structural precision (Graph).

## 1. Storage Architecture
The system uses two specialized engines:

### 1.1 Vector Storage (LanceDB)
- **Purpose**: Semantic similarity search and long-term memory.
- **Engine**: LanceDB (Columnar persistent storage).
- **Schema**:
  - `id`: UUID or logical path.
  - `text`: Content snippet.
  - `decay_score`: Importance factor (0.0 - 1.0).
  - `vector`: 384D embedding.

### 1.2 Graph Storage (SQLite)
- **Purpose**: Structural relationship mapping (AST relations, call graphs).
- **Engine**: SQLite via `sqlx`.
- **Tables**:
  - `graph_nodes`: Stores `label`, `summary`, and `embedding_id` links.
  - `graph_edges`: Stores `from_id`, `to_id`, `relation`, and `weight`.

## 2. Retrieval Strategy: Smart Search
1. **Semantic Probe**: Query LanceDB for Top-K results.
2. **Structural Expansion**: For each result, find related items in the Graph.
3. **Context Weighting**: Combine scores using `decay_score` and `graph_weight`.

## 3. Maintenance
- **Decay Logic**: Runtime decay is bounded and should not exceed a 2% reduction per cycle. Retrieval separately boosts frequently accessed and structural nodes, so long-term architecture context remains recoverable.
- **Pruning**: Memories with persistently low `decay_score`, low access count, and no graph importance may be moved to archival storage. Pinned or high-access nodes are not candidates for routine pruning.
