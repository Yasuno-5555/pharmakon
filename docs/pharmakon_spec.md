# Pharmakon System Specification

## 1. Concept
Pharmakon is an autonomous, self-evolving AI agent system designed to be a long-term partner for developers and power users. Unlike traditional chatbots, Pharmakon proactively learns from interactions, manages its own memory, and optimizes its internal reasoning processes.

## 2. Core Architecture
Pharmakon is built as a modular workspace of Rust crates, ensuring high performance, memory safety, and concurrency.

### 2.1 Agent Engine (`pharmakon-core`)
- **Reasoning**: Supports tiered reasoning with a "Planner" model for tool selection and a "Generator" model for final response.
- **Hooks**: A parallelized engine that triggers lifecycle events (message, tool, reflection) for all registered plugins.
- **Reflection**: Autonomous background cycle that analyzes conversation trajectories to extract new facts and user preferences.

### 2.2 Memory System (`pharmakon-memory`)
- **MemoryWeaver**: A high-performance vector database (LanceDB) that stores semantic embeddings of past interactions.
- **FactMemory**: A structured SQL database (SQLite with WAL mode) for storing hard facts, configuration, and long-term commitments.
- **Context Anchoring**: A semantic compression mechanism that prunes old messages while preserving core intent as "anchors".

### 2.3 Interface & Connectivity (`pharmakon-gateway`, `pharmakon-gui`)
- **Gateway**: An Axum-based web server providing WebSocket event streaming, HTTP API, and static UI serving with Brotli/Gzip compression.
- **Frontend**: A modern React-based dashboard for real-time visualization of agent thinking, memory state, and orchestration.
- **CLI**: A powerful terminal interface for rapid interaction and system management.

## 3. High-Performance Features
- **Parallel Context Gathering**: Simultaneous retrieval of vector memories and structured facts to minimize TTFT (Time To First Token).
- **Multi-threaded Tooling**: Non-blocking tool execution with I/O multiplexing (stdout/stderr capture).
- **Vision RAG**: Automatic indexing of image descriptions for semantic visual search.

## 4. Technology Stack
- **Language**: Rust (Edition 2024)
- **Async Runtime**: Tokio
- **Vector DB**: LanceDB
- **Embeddings**: FastEmbed (local SIMD-optimized)
- **Database**: SQLite (WAL Mode)
- **Web**: Axum, Tower-HTTP
- **UI**: React, TailwindCSS, TypeScript

## 5. Crate Directory
- `crates/common`: Shared types, traits, and event definitions (The SDK).
- `crates/core`: Main agent logic, hooks, and orchestration.
- `crates/memory`: Vector storage, RAG pipelines, and compaction.
- `crates/gateway`: Web server and WebSocket bridge.
- `crates/tools`: Standard tool library (Shell, Browser, Media, etc.).
- `crates/gui`: System tray and native window management.

## 6. Security & Safety
- **Policy Engine**: Configurable rules for tool execution.
- **Approval Workflow**: Mandatory human-in-the-loop for destructive or high-cost actions.

## 7. Roadmap
- **Swarm Intelligence**: Multi-agent collaboration with specialized roles.
- **Self-Repair**: Autonomous diagnostic and recovery flows.
- **Direct IPC**: Unix Domain Socket support for ultra-low latency local use.
