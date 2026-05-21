# Pharmakon

[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/Yasuno-5555/pharmakon/actions/workflows/ci.yml/badge.svg)](https://github.com/Yasuno-5555/pharmakon/actions)

Pharmakon is a **local-first autonomous AI agent framework** written in Rust. It provides tools, memory, multi-model routing, and automation capabilities for AI-assisted software engineering.

> This is not an operating system. It is a modular agent runtime that coordinates LLMs, tools, and memory to assist with development tasks.

---

## Features

- **Multi-model agent loop**: Chat with an AI agent that can execute tools, search memory, and maintain conversation state across sessions
- **7+ LLM providers**: Gemini, Claude, GPT, DeepSeek, Groq, Ollama, OpenRouter — with automatic fallback on rate limits
- **65+ built-in tools**: File I/O, code search, shell execution, web search, browser automation, git, AST manipulation
- **Memory system**: Semantic search (LanceDB vectors) + relational graph (SQLite) + causal DAG + topic clustering
- **CodeAct scripting**: Multi-step operations in a single LLM turn via Rhai or Python
- **Multi-channel gateway**: REST API, WebSocket, Telegram, Discord, Slack
- **Autonomous background tasks**: Cron, heartbeat with structured health probes
- **DSGE token economics**: Budget allocation with shadow pricing and production functions
- **Snapshot-based rollback**: All file mutations reversible via content-addressed snapshots
- **Multi-tier entropy response**: 4-tier escalating loop protection with hysteresis
- **Integrated Governor**: 3-loop competing control (Safety > Quality > Resource) with DynamicLambda budget scaling
- **Cosine stagnation detection**: Iteration embedding similarity catches subtle agent loops

---

## Quick start

```bash
git clone https://github.com/Yasuno-5555/pharmakon.git
cd pharmakon
cargo build --release

# One-shot query
cargo run --release -- "What files are in this directory?"

# Interactive session
cargo run --release -- agent

# REST API gateway
cargo run --release -- gateway --port 19999
```

### Install via cargo

```bash
cargo install --path crates/cli
pharmakon --help
```

### Environment variables

| Variable | Required | Purpose |
|---|---|---|
| `GEMINI_API_KEY` | For Gemini models | Google Gemini provider |
| `ANTHROPIC_API_KEY` | For Claude models | Anthropic provider |
| `OPENAI_API_KEY` | For GPT models | OpenAI provider |
| `DEEPSEEK_API_KEY` | For DeepSeek models | DeepSeek provider |
| `GROQ_API_KEY` | For Groq models | Groq provider |

---

## Usage

### CLI

```bash
# Interactive REPL
pharmakon agent

# One-shot
pharmakon "Explain the architecture of this project"

# With explicit session
pharmakon --session my-work --message "List all open TODOs"

# Switch model at runtime
/model gemini/gemini-2.5-flash    # Manual switch
/model auto                        # Automatic economy-aware selection
```

### Gateway

```bash
# Start server
pharmakon gateway --port 19999

# Send message via REST API
curl -X POST http://localhost:19999/api/v1/agent/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"Hello"}'

# WebSocket endpoint
ws://localhost:19999/ws
```

### Bots

| Platform | Env variable | Documentation |
|---|---|---|
| Telegram | `TELOXIDE_TOKEN` | [docs/channels.md](docs/channels.md) |
| Discord | `DISCORD_BOT_TOKEN` | [docs/channels.md](docs/channels.md) |
| Slack | `SLACK_BOT_TOKEN` | [docs/channels.md](docs/channels.md) |

---

## Configuration

`~/.pharmakon/config.json` is created automatically on first run:

```json
{
  "default_agent": {
    "provider": "gemini",
    "model": "gemini-2.5-flash",
    "fallback_models": [
      "deepseek/deepseek-v4-flash",
      "groq/llama-3.3-70b-versatile"
    ]
  },
  "gateway": {
    "port": 19999
  }
}
```

Sensitive values can be stored in the OS keyring:

```bash
pharmakon secrets set GEMINI_API_KEY <your-key>
```

---

## Architecture

```
pharmakon-cli          # CLI / TUI
pharmakon-gateway      # REST API, WebSocket, Telegram/Discord/Slack
pharmakon-core         # Agent loop, model routing, orchestration (30+ submodules)
pharmakon-tools        # 65+ tool implementations
pharmakon-memory       # LanceDB vectors + SQLite graph + causal DAG
pharmakon-common       # Shared types (Tool trait, Event, Config)
pharmakon-plugin-sdk   # Traits for custom tool plugins
```

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full component diagram and decision loop flow.

---

## Project status

**Current**: All core functionality implemented and integration-tested. Ready for personal/team use.

```bash
cargo test --workspace   # 81+ passing
cargo check --workspace  # 0 errors
cargo build --release    # single binary, ~20MB
```

See [docs/user_guide.md](docs/user_guide.md) for detailed usage, [docs/channels.md](docs/channels.md) for bot setup, and [docs/plugin_development.md](docs/plugin_development.md) for creating custom tools.

---

## License

MIT
