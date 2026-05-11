# User guide

## Installation

```bash
git clone https://github.com/Yasuno-5555/pharmakon.git
cd pharmakon
cargo build --release
```

## First run

```bash
# Run the onboarding wizard
cargo run --release -- onboard

# Or just start using it (config is auto-created)
cargo run --release -- agent --message "Hello"
```

## CLI commands

### Agent

```bash
# Interactive session
pharmakon agent

# One-shot query
pharmakon agent --message "List all files modified today"

# Named session (for continuity across invocations)
pharmakon agent --message "Continue the refactoring" --session my-work

# Different model
pharmakon agent --model gemini/gemini-2.5-flash --message "Hello"

# Custom soul (personality file)
pharmakon agent --soul ~/.pharmakon/souls/expert.md --message "Review this code"
```

### Model commands at runtime

```
/model                          List available models (● = current)
/model gemini/gemini-2.5-flash  Switch model
/model auto                     Enable economy-aware auto-selection
/plan                           Execute world model planner
```

### Gateway

```bash
# Start REST API + WebSocket
pharmakon gateway --port 19999

# With a custom soul
pharmakon gateway --port 19999 --soul ~/.pharmakon/souls/bot.md
```

### Diagnostics

```bash
pharmakon doctor
```

Shows health probes: disk usage, memory pressure, background task queue, LLM success rate, snapshot store usage.

### Secrets

```bash
pharmakon secrets set GEMINI_API_KEY <your-key>
pharmakon secrets list
pharmakon secrets get GEMINI_API_KEY
```

### Trajectory

```bash
pharmakon trajectory --session <session-id>
```

## Configuration

File: `~/.pharmakon/config.json` (auto-created on first run)

```json
{
  "default_agent": {
    "provider": "gemini",
    "model": "gemini-2.5-flash",
    "fallback_models": ["deepseek/deepseek-v4-flash", "groq/llama-3.3-70b-versatile"]
  },
  "gateway": {
    "port": 19999
  }
}
```

## Soul files

Soul files define the agent's personality and behavior. They live in `~/.pharmakon/souls/` as markdown files. If the file is valid YAML it's parsed as structured soul data; otherwise the entire file is used as the system prompt.

Example:

```markdown
You are a senior Rust expert. Focus on safety, performance, and idiomatic code.
Always prefer simple solutions over complex ones.
```

## Onboarding wizard

```bash
pharmakon onboard
```

Walks through API key setup and creates the default config.

## Desktop GUI (experimental)

```bash
pharmakon gui
```
