# Pharmakon User Guide

Welcome to the Pharmakon User Guide. Pharmakon is a highly optimized, multimodal autonomous agent framework built in Rust. It offers a local-first, privacy-focused experience, while providing seamless integrations with multiple LLM providers, tools, and communication channels.

## Getting Started

### 1. Installation and Setup
Ensure you have Rust and Cargo installed, then build the project:
```bash
cargo build --release
```

### 2. Configuration & API Keys
Pharmakon uses a secure Keyring mechanism to store your API keys. You can configure them via the CLI:

```bash
pharmakon secrets set OPENAI_API_KEY <your-key>
pharmakon secrets set GEMINI_API_KEY <your-key>
```
Alternatively, set them as environment variables (e.g., `export OPENAI_API_KEY="..."`).

### 3. Interactive CLI (Agent Mode)
You can directly interact with your agent from the terminal. 

```bash
# Chat with the default model
pharmakon agent --message "What is the weather today?"

# Specify a provider and model
pharmakon agent --provider gemini --model gemini-1.5-pro-latest --message "Analyze this text."
```

### 4. Running the Gateway
The Gateway acts as a central hub, exposing WebSockets and REST APIs for integrations (e.g., channels, UI clients).

```bash
pharmakon gateway --port 19999
```

### 5. Using the TUI Dashboard
For a rich, real-time terminal interface showing the agent's thought process and conversations:
```bash
pharmakon tui
```

### 6. Background Daemon
You can run the gateway in the background:
```bash
pharmakon daemon start
pharmakon daemon status
pharmakon daemon stop
```

### 8. Multi-Agent Teams (Orchestration)
Pharmakon allows you to define specialized agent teams managed by a `Supervisor`. The supervisor decomposes complex goals and delegates tasks to specific agents.

Agents in a team can communicate using the `send_message` tool.

### 9. Dynamic Model Loading & Fallback
You can request specific models using the `--provider` and `--model` flags (or via the configuration file). 
Pharmakon's `AgentRouter` automatically handles fallback:
- If you request `openai:gpt-4o` but haven't set an `OPENAI_API_KEY`, the agent will automatically fall back to your configured default model (e.g., `gemini-1.5-flash`).
- This ensures that your autonomous workflows and scheduled tasks don't fail due to missing credentials or provider outages.

## System Diagnostics
If you encounter issues, use the built-in diagnostic tool to identify and automatically fix problems:
```bash
pharmakon doctor --repair
```


## Further Reading
- For developers looking to create WASM tools, see the [Plugin Development Guide](./plugin_development.md).
- To connect Pharmakon to Discord, Slack, or Telegram, see the [Channels Setup Guide](./channels.md).
