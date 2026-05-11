# Pharmakon Gateway API Reference

Welcome to the **Pharmakon API Reference**. Pharmakon provides a high-performance control plane and real-time workspace gateway for autonomous AI agents. The gateway runs a lightweight, fast, and multi-threaded engine written in Rust using [Axum](https://github.com/tokio-rs/axum) and [Tokio](https://github.com/tokio-rs/tokio).

---

## 🌐 Base URL & Connections

By default, the Pharmakon Gateway binds locally to port `19999`.

```http
# REST Base URL
http://localhost:19999

# WebSocket Base URL
ws://localhost:19999
```

> [!NOTE]
> The gateway runs on plain HTTP/WS by default. To enable secure TLS in production, configure the environment variables `PHARMAKON_GATEWAY_TLS_CERT` and `PHARMAKON_GATEWAY_TLS_KEY`.

---

## 🔑 Authentication

Endpoints nested under `/api/v1` are protected via the `x-api-key` header.

| Header | Type | Description |
|:---|:---|:---|
| `x-api-key` | `string` | Your unique Pharmakon Control Plane API token. |

### Auth Resolution Sequence:
1. **Primary**: Environment variable `PHARMAKON_CONTROL_API_KEY`
2. **Secondary**: SQLite secret store persistent key `PHARMAKON_CONTROL_API_KEY`

> [!IMPORTANT]
> **Developer Mode Bypass:** If `PHARMAKON_CONTROL_API_KEY` is not defined or is empty, the gateway will output a console warning and **bypass authentication** for local debugging ease.

---

## 📡 1. REST Endpoints

### 🩺 System Diagnostics & Health

#### `GET /status`
Fetches high-level metadata of the running Pharmakon daemon. This endpoint is public and does not require authentication.

* **Response (`200 OK`):**
  ```json
  {
    "status": "OK",
    "version": "0.1.0",
    "name": "Pharmakon"
  }
  ```

#### `GET /health`
A simple health probe for load-balancers or container engines.

* **Response (`200 OK`):** Empty body.

---

### 🤖 Agent & Workspace Operations

#### `POST /api/v1/agent/chat`
Dispatches a text command or query directly to the currently loaded Agent.

* **Request Headers:**
  ```http
  x-api-key: your-secure-api-key
  Content-Type: application/json
  ```
* **Request Body:**
  ```json
  {
    "message": "Verify code changes in swarm_economy.rs and run tests"
  }
  ```
* **Response (`200 OK`):**
  ```json
  {
    "response": "Analysis complete. The modifications in `swarm_economy.rs` are structurally sound. Unit tests completed successfully with exit code 0."
  }
  ```
* **Error Responses:**
  * `401 Unauthorized` — Invalid or missing API key.
  * `500 Internal Server Error` — Agent execution failure or model provider error.

---

#### `POST /api/v1/tools/execute`
Bypasses conversational loops and executes any verified tool directly in the Workspace context.

* **Request Body:**
  ```json
  {
    "name": "replace_file_content",
    "args": {
      "TargetFile": "/Users/yasuno/projects/Pharmakon/crates/cli/src/tui.rs",
      "StartLine": 7,
      "EndLine": 10,
      "TargetContent": "use pharmakon_common::Event;\nuse pharmakon_core::agent::Agent;",
      "ReplacementContent": "use pharmakon_common::Event;\nuse pharmakon_core::agent::Agent;\nuse pharmakon_common::Config;",
      "AllowMultiple": false,
      "Instruction": "Import Config for inline saving",
      "Description": "Adding Config struct to scope"
    }
  }
  ```
* **Response (`200 OK`):**
  ```json
  {
    "result": "Successfully updated lines 7-10 in /Users/yasuno/projects/Pharmakon/crates/cli/src/tui.rs."
  }
  ```
* **Error Responses:**
  * `404 Not Found` — Tool name is invalid or not registered in the catalog.
  * `500 Internal Server Error` — The tool execution failed or was rejected due to safety profile restrictions.

---

#### `GET /api/v1/state`
Returns a compact summary of the current session's runtime metrics and tracking indexes.

* **Response (`200 OK`):**
  ```json
  {
    "session_id": "84a3c1e9-72df-4db9-a86b-c72fedcb142e",
    "trajectory_steps": 14,
    "history_messages": 28,
    "model": "gemini-2.5-flash"
  }
  ```

---

## 🔌 2. Real-Time WebSocket API (`/ws`)

The WebSocket endpoint provides real-time, bi-directional, event-driven streaming between the Pharmakon Gateway and browser interfaces.

* **Connection Endpoint:**
  ```http
  ws://localhost:19999/ws
  ```

### 📤 Client-to-Server Requests (`Request`)
Clients send JSON payloads to trigger actions.

```json
{
  "SendMessage": { "message": "List files in workspace" }
}
```

#### Available Payloads:
* **`SendMessage { message: String }`** — Starts asynchronous chat stream.
* **`ProvideApproval { id: String, approved: bool }`** — Submits a human-in-the-loop approval decision.
* **`ResetHistory`** — Cleanses memory and resets conversation.
* **`GetSessions`** — Retrieves all active chat sessions.
* **`SwitchSession { id: String }`** — Switches context, loading database history for session.
* **`GetHistory { session_id: String }`** — Loads full historical trajectory.
* **`GetModels`** — Lists available LLM models on the system.
* **`SwitchModel { model_id: String }`** — Dynamically changes active LLM model.
* **`GetTools`** — Fetches metadata of all active capabilities.
* **`GetUsageHistory`** — Fetches token usage and estimated DSGE cost history.

---

### 📥 Server-to-Client Streaming (`Event`)
The server streams events as they occur in the agent logic.

```json
{
  "AgentResponseChunk": {
    "chunk": "Based on my analysis..."
  }
}
```

#### Key Broadcast Events:
* **`AgentThoughtChunk { chunk: String }`** — Real-time reasoning/thought stream.
* **`AgentResponseChunk { chunk: String }`** — Real-time model output stream.
* **`ToolCall { name: String, args: String }`** — Notification that a tool is being invoked.
* **`ToolResult { result: String }`** — Notification that tool completed successfully.
* **`ApprovalRequest { id: String, tool: String, args: String }`** — Pauses loop, prompting for user approval.
* **`GatewayStatus { uptime: u64, connected_clients: usize, memory_usage: u64 }`** — System telemetry telemetry logs.
* **`HistoryList { messages: Vec<Message> }`** — Database history synchronization package.

---

## 📈 3. Agentic Control Plane (ACP) Socket (`/acp`)

A dedicated low-latency channel reserved for coordination and orchestration of nested parallel agents.

* **Connection Endpoint:**
  ```http
  ws://localhost:19999/acp
  ```
Used by `FractalSwarmTool` and `PharmakonTaskTool` to synchronize hierarchical subtasks without polluting main conversation screens.
