# Channels Setup Guide

Pharmakon allows agents to seamlessly connect to external messaging platforms via the `pharmakon-channels` crate. This guide explains how to configure and enable connections to Telegram, Discord, and Slack.

## General Architecture

Channels in Pharmakon operate asynchronously. They listen for incoming messages from the platform, route them to an `Agent` instance for processing, and then dispatch the agent's response back to the platform.
The channels are typically managed by the Pharmakon Gateway.

## Configuring Channels

To enable a channel, you must provide the respective Bot Tokens via environment variables or securely via the `pharmakon secrets` CLI command.

### 1. Telegram
To connect to Telegram, you need a Bot Token from BotFather.

**Setup:**
```bash
pharmakon secrets set TELEGRAM_BOT_TOKEN <your_telegram_token>
```
When the Gateway starts, it will automatically detect the token and begin polling for updates.

### 2. Discord
To connect to Discord, create an application in the Discord Developer Portal, add a Bot, and copy its Token.

**Setup:**
```bash
pharmakon secrets set DISCORD_BOT_TOKEN <your_discord_token>
```
The Discord channel uses WebSockets (via Serenity) to listen to events in real time. Ensure your bot has the `MESSAGE_CONTENT` intent enabled in the Developer Portal.

### 3. Slack
To connect to Slack, you need an App-Level Token (starting with `xapp-`) for Socket Mode, and a Bot User OAuth Token (starting with `xoxb-`).

**Setup:**
```bash
pharmakon secrets set SLACK_BOT_TOKEN <your_slack_token>
```
*(Note: If Socket Mode requires an App token as well, ensure both are configured in your environment or secrets according to your specific setup.)*

## Gateway Integration

When you run the `pharmakon gateway` command, it will automatically read your secrets and spawn background tasks for each configured channel.

```bash
pharmakon gateway --port 18789
```

Output Example:
```text
[INFO] Registering Telegram channel...
[INFO] Registering Discord channel...
[INFO] Starting Pharmakon Gateway on port 18789
```

## Building Custom Channels

To build a custom channel integration, implement the `Channel` trait defined in `pharmakon_common`:

```rust
#[async_trait]
pub trait Channel: Send + Sync {
    fn name(&self) -> &str;
    async fn run(&self, agent: Arc<Mutex<Agent>>) -> Result<()>;
}
```

Add your custom implementation to the `Gateway` using `gateway.add_channel(Arc::new(MyCustomChannel))`.
