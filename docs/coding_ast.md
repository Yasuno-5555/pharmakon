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

## 2. Tools
### 2.1 `ingest_ast_knowledge`
- Parses entire directories.
- Breaks files into logical blocks.
- Stores blocks in LanceDB with structural metadata.

### 2.2 Structural RAG
- Instead of finding "lines 100-150", the agent finds "the implementation block of the `Agent` struct".
- Guarantees that code snippets are syntactically complete.

## 3. Impact on Engineering
- **Safe Refactoring**: The agent "sees" affected traits before changing a struct.
- **Zero-Shot Understanding**: New repositories are indexed structurally, providing immediate high-level overview.
- **Accurate Code Generation**: Context includes related trait definitions, ensuring generated code adheres to existing interfaces.
