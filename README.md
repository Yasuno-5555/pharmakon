# 🦞 Pharmakon — Personal AI Assistant (Rust Port)

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

**Pharmakon** is a high-performance, local-first Rust port of [OpenClaw](https://github.com/openclaw/openclaw). It serves as a single control plane for your personal AI sessions, tools, and messaging channels.

## 🚀 Key Features

- **Blazing Fast Gateway**: Built with Axum and Tokio for high-concurrency event handling.
- **Sandboxed Execution**: Shell tools run in isolated Docker containers to protect your host.
- **WASM Skill System**: Extensible tool system using `wasmtime` to run skills in a safe sandbox.
- **Multi-Channel Integration**: Discord, Slack, and Telegram support out of the box.
- **Voice & Multimodal**: Native integration with Whisper (STT), OpenAI TTS, ElevenLabs, and Deepgram.
- **Local Persistence**: SQLx-based SQLite storage for conversation history and usage analytics.
- **Secure by Default**: Keyring-based secret management and strict tool approval flows.

## 📦 Project Structure

Pharmakon is organized as a Cargo workspace with the following crates:

- `pharmakon-core`: The heart of the agent, handling LLM providers, persistence, and soul.
- `pharmakon-gateway`: An Axum-based web server and WebSocket RPC hub.
- `pharmakon-common`: Shared types, event definitions, and configuration logic.
- `pharmakon-tools`: Tool definitions (Browser, Shell, Search, WASM).
- `pharmakon-channels`: Adapter implementations for various messaging platforms.
- `pharmakon-cli`: The unified command-line entry point.

## 📚 Documentation

Detailed documentation and guides can be found in the `docs/` directory:
- [User Guide](docs/user_guide.md): Installation, setup, and CLI usage.
- [Plugin Development Guide](docs/plugin_development.md): How to create and integrate WASM plugins.
- [Channels Setup Guide](docs/channels.md): Connecting Pharmakon to Telegram, Discord, and Slack.

API reference can be generated locally using `cargo doc --no-deps --workspace`.

## 🛠️ Getting Started

### Prerequisites

- Rust 1.75+
- Docker (for sandboxed tool execution)
- SQLite

### Installation

```bash
git clone https://github.com/yasuno/Pharmakon.git
cd Pharmakon/pharmakon
cargo build --release
```

### Setup

Run the interactive onboarding flow to configure your API keys and default soul:

```bash
cargo run -- onboard
```

### Running the Gateway

```bash
cargo run -- gateway --port 18789
```

## 🛡️ Security

Pharmakon follows a strict security model:
- **Approval Flow**: Risky tools (like shell access) require manual approval via the Gateway's RPC.
- **Sandboxing**: Every shell command executes in a fresh, temporary Docker container.
- **Secrets**: API keys and tokens are stored in the system keyring, never in plain text.

## 📄 License

This project is licensed under the MIT License.
