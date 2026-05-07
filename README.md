# 🦞 Pharmakon — Personal AI Assistant (Rust Port)

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Pharmakon** is a high-performance, local-first Rust port of [OpenClaw](https://github.com/openclaw/openclaw). It serves as a unified control plane for autonomous AI engineering, tool orchestration, and multi-channel communication.

## 🚀 Key Features

- **Autonomous Engineering Loop**: Self-reflecting agent that learns from task trajectories, manages its own constraints, and maintains a validated knowledge base.
- **Codex-Style Observability**: Structured execution traces, deterministic replay, and tool reliability scoring for deep transparency into agent behavior.
- **Precision Code Manipulation**: Structured Rust function edits via `mutate_ast` and unified diffs, integrated with LSP (Language Server Protocol) for type-aware refactoring.
- **Epistemic Memory (Knowledge Nexus)**: A sophisticated memory system using LanceDB for embeddings, SQLite for graph relations, and access-aware ranking with bounded decay.
- **Safety First (Dry-Run & Auditing)**: Built-in dry-run simulation for shell/API calls, security auditing for diffs, and progressively sandboxed execution environments.
- **High-Performance Gateway**: Multi-channel connectivity (Telegram, Discord, Slack) and a real-time web dashboard powered by Axum, Tokio, and Xilem.
- **Integrated Tooling Surface**: Built-in browser, shell, git, python interpreter, and MCP (Model Context Protocol) client/server support.
- **WASM Skill System**: Extensible tool system running sandboxed logic via `wasmtime`.

## 📦 Project Structure

Pharmakon is organized as a clean, modular Cargo workspace designed for scalability and reliability:

- **`pharmakon-core`**: The agent's brain. Handles the decision loop, LLM provider abstraction, integrated MCP communication, and multi-agent orchestration.
- **`pharmakon-memory`**: The agent's long-term memory. Implements the Knowledge Nexus (LanceDB + SQLite Graph), semantic search, and context optimization.
- **`pharmakon-tools`**: The agent's hands. A unified library of all built-in tools (Codex OS tools, Browser, Shell, AST manipulation, RepoMap, etc.).
- **`pharmakon-gateway`**: The agent's senses and voice. Orchestrates messaging channels (Telegram, Discord, etc.), serves the real-time Dashboard, and manages Audio I/O.
- **`pharmakon-cli`**: The primary user interface. A unified command-line entry point for onboarding, agent interaction, and service management.
- **`pharmakon-common`**: The shared foundation. Contains core data structures, event definitions, and traits used across all crates.
- **`pharmakon-plugin-sdk`**: The extensibility kit. Provides the necessary types and macros for building external WASM-based tools and skills.

## 📚 Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): Deep dive into the Pharmakon system design and data flow.
- [User Guide](docs/user_guide.md): Installation, setup, and CLI usage.
- [Plugin Development Guide](docs/plugin_development.md): How to create and integrate WASM plugins.
- [Channels Setup Guide](docs/channels.md): Connecting Pharmakon to Telegram, Discord, and Slack.

## 🛠️ Getting Started

### Prerequisites

- Rust 1.75+
- Docker (for sandboxed execution)
- SQLite

### Installation

```bash
git clone https://github.com/Yasuno-5555/Pharmakon.git
cd Pharmakon
cargo build --release
```

### Setup

Run the interactive onboarding flow to configure your API keys and default soul:

```bash
cargo run -- onboard
```

### Running the Gateway

```bash
cargo run -- gateway --port 19999
```

## 🛡️ Security

Pharmakon implements a multi-layered security model:
- **Explicit Approval**: High-risk operations (shell, file write) require real-time user approval via the Dashboard or messaging channel.
- **Dry-Run Simulation**: The agent can simulate destructive actions before commitment to verify intent and safety.
- **Audited Diffs**: All code changes are scanned for security risks (API keys, patterns) before application.

## 📄 License

This project is licensed under the MIT License.
