# Pharmakon 4.0: Coding & AST Specification

## Overview
Moving from text-based search to structural code understanding using Tree-Sitter and Knowledge Nexus.

## 1. Structural Analysis
### 1.1 AST Ingestion (`tree-sitter`)
- **Parser**: Rust-specific grammar integration.
- **Symbol Extraction**:
  - `Structs`: Includes field names and documentation.
  - `Functions`: Includes signature and full body.
  - `Traits`: Maps implementation relationships.

### 1.2 Relationship Mapping
- **Contains**: Maps file paths to their internal symbols.
- **Calls**: Maps function call sites to definitions (1st-order approximation).
- **Implements**: Links structs to the traits they implement.

## 3. Impact on Engineering
- **Context Preservation**: Agents don't lose track of structural dependencies.
- **Precision**: Edits are applied to specific AST nodes, reducing "hallucinated" lines.

## 4. Token Optimization & Tiered Context
### 4.1 AST Skeletonization
- **Skeleton Mode**: When browsing large files, the system generates "Skeletons" (signatures only, bodies replaced with `{ ... }`).
- **Lazy Loading**: Full function bodies are only loaded into context when explicitly needed or when the agent "pins" a node.

### 4.2 Information Density (Working Memory)
- **Density-Based Packing**: Context is packed by `Importance / Token Count`.
- **Micro-summaries**: Long documentation or research results are automatically compressed into 1-2 line summaries when the token budget is tight.
- **Redundancy Filter**: Duplicate or highly similar (95%+ overlap) context units are automatically dropped.

### 4.3 Graph Expansion
- **Delayed Expansion**: Related nodes in the graph are initially presented as summaries. Full expansion only occurs for high-weight (0.9+) relationships.
- **Safe Refactoring**: The agent "sees" affected traits before changing a struct.
- **Zero-Shot Understanding**: New repositories are indexed structurally, providing immediate high-level overview.
- **Accurate Code Generation**: Context includes related trait definitions, ensuring generated code adheres to existing interfaces.
