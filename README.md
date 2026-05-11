# Pharmakon

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![CI](https://img.shields.io/badge/CI-passing-brightgreen)]()
[![Tests](https://img.shields.io/badge/tests-64%20passed-brightgreen)]()

Pharmakon is a **local-first autonomous AI agent framework** built in Rust. It provides tools, memory, multi-model routing, and automation capabilities for AI-assisted software engineering.

> This is not an operating system. It is a modular agent runtime that coordinates LLMs, tools, and memory to assist with development tasks.

---

## Features

- **Multi-model agent loop**: Chat with an AI agent that can execute tools, search memory, and maintain conversation state across sessions
- **7+ LLM providers**: Gemini, Anthropic, OpenAI, DeepSeek, Groq, Ollama, OpenRouter — with automatic fallback on rate limits
- **65+ built-in tools**: File I/O, code search, shell execution, web search, browser automation, git operations, AST manipulation
- **Memory system**: Semantic search (LanceDB + embeddings) + relational graph (SQLite) + causal edge tracking
- **CodeAct scripting**: Run multi-step operations in a single LLM turn via Rhai or Python scripts
- **Multi-channel gateway**: REST API, WebSocket, Telegram, Discord, Slack bots
- **Autonomous background tasks**: Cron jobs, periodic maintenance, initiative engine for proactive behavior
- **Entropy-based loop detection**: Monitors tool call patterns to detect and terminate pathological loops
- **Snapshot-based rollback**: Content-addressed file snapshots enable reverting any file mutation

---

## Architecture overview

```
CLI / TUI         Gateway (REST/WS/Telegram/Discord/Slack)
     \                  |
  pharmakon-core ───────┤
     |                  |
  ┌──┴──┐         ┌────┴────┐
  │Agent│◄────────│ModelRouter│
  │Loop │         │(fallback │
  └──┬──┘         │ routing) │
     │            └─────────┘
  ┌──┴───────────────────────────┐
  │ pharmakon-tools (65+ tools)  │
  │ CodeAct / Shell / Git / LSP / Browser / Web search / ... │
  └──────────────────────────────┘
     │
  ┌──┴───────────────────────────┐
  │ pharmakon-memory             │
  │ LanceDB (vectors) + SQLite (graph) │
  └──────────────────────────────┘
```

### Crates

| Crate | Role | Key modules |
|---|---|---|
| `pharmakon-core` | Agent loop, decision logic | `agent.rs`, `model_router.rs`, `orchestration/` |
| `pharmakon-tools` | Tool implementations | `code/`, `search/`, `codex/`, `shell`, `files` |
| `pharmakon-memory` | Long-term memory | `KnowledgeNexus` (LanceDB), `GraphStore` (SQLite), `CausalGraph` |
| `pharmakon-gateway` | External access | REST API, WebSocket, Telegram/Discord/Slack bots |
| `pharmakon-cli` | Terminal interface | REPL, TUI dashboard, onboarding wizard |
| `pharmakon-common` | Shared types | `Tool`, `Event`, `AgentModel` trait, `Config` |
| `pharmakon-plugin-sdk` | Plugin development kit | Tool + event plugin traits |

---

## Quick start

### Prerequisites

- Rust 1.75+
- SQLite (for persistent session storage)

### Install & run

```bash
git clone https://github.com/Yasuno-5555/Pharmakon.git
cd Pharmakon
cargo build --release

# Interactive CLI session
cargo run --release -- agent

# One-shot query
cargo run --release -- agent --message "What files are in the current directory?"

# Start the gateway (REST API + WebSocket)
cargo run --release -- gateway --port 19999

# Run the onboarding wizard
cargo run --release -- onboard
```

### Environment variables

| Variable | Required | Purpose |
|---|---|---|
| `ANTHROPIC_API_KEY` | For Claude models | Anthropic provider |
| `GEMINI_API_KEY` | For Gemini models | Google Gemini provider |
| `OPENAI_API_KEY` | For OpenAI models | OpenAI provider |
| `DEEPSEEK_API_KEY` | For DeepSeek models | DeepSeek provider |
| `GROQ_API_KEY` | For Groq models | Groq provider |
| `PHARMAKON_CONTROL_API_KEY` | For gateway auth | Gateway API key (optional, bypassed when unset) |
| `BRAVE_SEARCH_API_KEY` | Optional | Enables Brave search tool |

### Configuration

Configuration lives at `~/.pharmakon/config.json` (auto-created on first run):

```json
{
  "default_agent": {
    "provider": "gemini",
    "model": "gemini-2.5-flash",
    "fallback_models": [
      "deepseek/deepseek-v4-flash",
      "gemini/gemini-2.5-flash",
      "groq/llama-3.3-70b-versatile"
    ]
  },
  "gateway": {
    "port": 19999
  }
}
```

---

## Usage

### CLI agent

```bash
# Interactive REPL
pharmakon agent

# One-shot command
pharmakon agent --message "Explain the architecture of this project"

# Specify a session for continuity
pharmakon agent --message "Continue the refactoring" --session my-session

# Switch model at runtime
/model gemini/gemini-2.5-flash
/model auto   # Enable automatic model selection
```

### REST API (gateway)

```bash
# Start gateway
pharmakon gateway --port 19999

# Send a message
curl -X POST http://localhost:19999/api/v1/agent/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Hello"}'

# WebSocket connection
ws://localhost:19999/ws
```

### Gateway channels

Enable bots by setting the appropriate environment variables:

| Channel | Environment variables |
|---|---|
| Telegram | `TELOXIDE_TOKEN` |
| Discord | `DISCORD_BOT_TOKEN` |
| Slack | `SLACK_BOT_TOKEN`, `SLACK_SIGNING_SECRET` |

---

## Documentation

- [ARCHITECTURE.md](ARCHITECTURE.md): System architecture, crate layout, data flow
- [PHARMAKON.md](PHARMAKON.md): Engineering patterns, conventions, safety rules
- [docs/user_guide.md](docs/user_guide.md): Setup, configuration, CLI usage
- [docs/channels.md](docs/channels.md): Telegram, Discord, Slack bot setup
- [docs/plugin_development.md](docs/plugin_development.md): Creating custom tools

## Project structure

```
crates/
├── cli/          # CLI binary, TUI, onboarding wizard
├── common/       # Shared types, config, error types
├── core/         # Agent loop, model routing, orchestration
│   ├── src/
│   │   ├── agent.rs              # Main agent loop
│   │   ├── model_router.rs       # Model selection & fallback
│   │   ├── orchestration/        # 30+ submodules
│   │   ├── providers/            # 7+ LLM providers
│   │   ├── security/             # Constitutional policy
│   │   └── automation/           # Cron, heartbeat
│   └── tests/
├── gateway/      # REST API, WebSocket, bots
├── memory/       # Vector store, graph store
├── tools/        # 65+ tools (code, search, codex/)
└── plugin-sdk/   # Plugin traits
```

---

## Security

- **Constitutional policy engine**: Immutable rules blocking self-modification, destructive shell commands, and critical file deletion
- **Execution profiles**: Tools classified by side-effect level, filesystem scope, and reversibility
- **Entropy overflow protection**: Automatic termination of pathological loops
- **Snapshot rollback**: All file mutations can be reverted via `rollback_to_event()`

---

## License

MIT License. See [LICENSE](LICENSE).
