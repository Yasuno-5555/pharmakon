# User guide

## Installation

```bash
git clone https://github.com/Yasuno-5555/Pharmakon.git
cd Pharmakon
cargo build --release
```

## First run

```bash
# Run the onboarding wizard
cargo run --release -- onboard

# Or just start using it (config auto-created)
cargo run --release -- agent --message "Hello"
```

## CLI commands

### Agent interaction

```bash
# Interactive session
pharmakon agent

# One-shot query
pharmakon agent --message "List all files modified today"

# With a named session (for continuity)
pharmakon agent --message "Continue the refactoring" --session my-work

# Different model
pharmakon agent --model gemini/gemini-2.5-flash --message "Hello"

# With a custom soul (personality file)
pharmakon agent --soul ~/.pharmakon/souls/expert.md --message "Review this code"
```

### Model commands (at runtime)

```
/model                 List available models (● = current)
/model gemini/gemini-2.5-flash  Switch model
/model auto            Enable automatic model selection
/plan                  Execute world model planner on current task
```

### Scheduling

For cron scheduling, use the cron tool from within an agent session. The heartbeat manager automatically runs maintenance every 30 minutes.

### Gateway

```bash
# Start REST API + WebSocket server
pharmakon gateway --port 19999

# With a specific soul
pharmakon gateway --port 19999 --soul ~/.pharmakon/souls/bot.md
```

## Configuration

File: `~/.pharmakon/config.json` (auto-created)

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

### Secrets

Sensitive values (API keys) can be stored:

```bash
pharmakon secrets set GEMINI_API_KEY <your-key>
pharmakon secrets list
pharmakon secrets get GEMINI_API_KEY
```

## Soul files

Soul files define the agent's personality and constraints. They live in `~/.pharmakon/souls/` as markdown or YAML files.

Example soul (`~/.pharmakon/souls/expert.md`):
```markdown
You are a senior Rust expert. Focus on safety, performance, and idiomatic code.
Always prefer simple solutions over complex ones.
```

## Onboarding wizard

```bash
pharmakon onboard
```

The wizard will:
1. Prompt for API keys (Gemini, Anthropic, OpenAI, etc.)
2. Create the config file
3. Create a default soul
4. Test the connection with the selected model

## Status

```bash
pharmakon status
```

Shows health probes (disk usage, memory, task queue, LLM success rate), running background tasks, and snapshot store usage.

## Desktop GUI (experimental)

```bash
pharmakon gui
```

Launches a native desktop dashboard with tabs for chat, stats, automation, skills, research, graph, logs, and configuration.
