# Channel setup guide

Pharmakon gateway can connect to external messaging platforms.

## Usage

```bash
# Start the gateway with all configured channels
pharmakon gateway --port 19999
```

## Environment variables

| Channel | Variable | Obtained from |
|---|---|---|
| Telegram | `TELOXIDE_TOKEN` | BotFather |
| Discord | `DISCORD_BOT_TOKEN` | Discord Developer Portal (enable MESSAGE_CONTENT intent) |
| Slack | `SLACK_BOT_TOKEN` + `SLACK_SIGNING_SECRET` | Slack API dashboard |

All tokens can also be set via `pharmakon secrets set <KEY> <VALUE>`.

## How it works

The gateway reads environment variables or secrets at startup. When a token for a channel is present, the corresponding bot is started in a background task. Incoming messages are routed to the agent for processing, and responses are sent back through the channel.

## Custom channels

Implement the `Channel` trait:

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, agent: Arc<Agent>) -> Result<()>;
}
```

Add it to the gateway:

```rust
gateway.add_channel(Arc::new(MyCustomChannel));
```
