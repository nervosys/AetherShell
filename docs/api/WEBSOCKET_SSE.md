# WebSocket & SSE Streaming Reference

AetherShell provides real-time communication via WebSocket for bidirectional agent orchestration and Server-Sent Events (SSE) for streaming execution results.

---

## WebSocket

### Endpoint

```
GET /api/v1/ws
```

Upgrades to a WebSocket connection on the Agent API server (default `ws://127.0.0.1:3002/api/v1/ws`).

---

### Client → Server Messages

All messages are JSON with a `type` discriminator.

#### `execute` — Run a Request

```json
{
  "type": "execute",
  "id": "req-001",
  "request": {
    "action": "call",
    "builtin": "ls",
    "args": { "path": "." }
  }
}
```

The `request` field accepts any `AgentRequest` variant (same as `POST /api/v1/execute`).

#### `register` — Register as an Agent

```json
{
  "type": "register",
  "agent_id": "code-reviewer-1",
  "capabilities": ["code_review", "testing", "search"]
}
```

Registers this connection as a named agent that can receive directed messages and appear in `/api/v1/orchestration/agents`. Broadcasts an `agent_connected` event to the `"agents"` channel.

#### `agent_message` — Send to Another Agent

```json
{
  "type": "agent_message",
  "to": "planner-agent",
  "payload": { "task": "review PR #42" }
}
```

Requires the sender to be registered (via `register`). Delivered to the target agent's WebSocket connection.

#### `broadcast` — Broadcast to Channel

```json
{
  "type": "broadcast",
  "channel": "tasks",
  "payload": { "status": "completed", "task_id": "abc" }
}
```

Sends to all subscribers of the named channel.

#### `subscribe` — Subscribe to Channel

```json
{
  "type": "subscribe",
  "channel": "agents"
}
```

After subscribing, you receive `channel` messages whenever data is broadcast to that channel.

Built-in channels:
- `"agents"` — agent connect/disconnect/publish events
- `"workflows"` — workflow created/updated/cancelled events

#### `unsubscribe` — Unsubscribe from Channel

```json
{
  "type": "unsubscribe",
  "channel": "agents"
}
```

#### `ping` — Keepalive

```json
{
  "type": "ping",
  "id": "optional-correlation-id"
}
```

---

### Server → Client Messages

#### `response` — Execution Result

```json
{
  "type": "response",
  "id": "req-001",
  "response": {
    "success": true,
    "result": [...],
    "result_type": "Array",
    "metadata": { "code_executed": "ls(\".\")" }
  }
}
```

Correlates to an `execute` message by `id`.

#### `stream` — Streaming Event

```json
{
  "type": "stream",
  "id": "req-002",
  "event": {
    "event": "progress",
    "data": { "current": 2, "total": 5, "percentage": 40.0, "message": "Processing step 2/5: filter" }
  }
}
```

Event types:
| `event.event`   | `event.data`                                          | Description                |
| --------------- | ----------------------------------------------------- | -------------------------- |
| `start`         | `{ "message": "..." }`                                | Execution started          |
| `progress`      | `{ "current": N, "total": M, "percentage": P, "message": "..." }` | Step progress |
| `data`          | `{ "result": ..., "result_type": "..." }`             | Intermediate data          |
| `complete`      | `{ "success": true, "result": ..., "result_type": "..." }` | Final result         |
| `error`         | `{ "success": false, "error": "..." }`                | Execution error            |

#### `channel` — Channel Broadcast

```json
{
  "type": "channel",
  "channel": "agents",
  "payload": {
    "type": "agent_connected",
    "agent": {
      "id": "code-reviewer-1",
      "capabilities": ["code_review"],
      "status": "online",
      "connectedAt": 1707300000000
    }
  }
}
```

Common channel events:

**`agents` channel:**
- `agent_connected` — new agent registered
- `agent_disconnected` — agent connection closed
- `agent_published` — agent published to marketplace
- `agent_installed` — agent installed from marketplace
- `agent_uninstalled` — agent uninstalled

**`workflows` channel:**
- `workflow_created` — new workflow definition
- `workflow_update` — workflow status changed (including cancellation)

#### `agent_message` — Directed Message

```json
{
  "type": "agent_message",
  "from": "planner-agent",
  "payload": { "review_result": "approved" }
}
```

#### `registered` — Registration Confirmation

```json
{
  "type": "registered",
  "agent_id": "code-reviewer-1"
}
```

#### `agents` — Connected Agent List

```json
{
  "type": "agents",
  "agents": [
    { "id": "code-reviewer-1", "capabilities": ["code_review"], "connected_at": 1707300000 }
  ]
}
```

#### `pong` — Keepalive Reply

```json
{
  "type": "pong",
  "id": "optional-correlation-id",
  "timestamp": 1707300000
}
```

#### `error` — Error

```json
{
  "type": "error",
  "id": "req-001",
  "message": "Parse error: unexpected token"
}
```

---

### Connection Lifecycle

1. Client opens `GET /api/v1/ws` → upgrade to WebSocket
2. (Optional) Client sends `register` → receives `registered`
3. Client sends `subscribe` to channels of interest
4. Client sends `execute` requests → receives `response` messages
5. On disconnect, if registered, the server broadcasts `agent_disconnected` to the `"agents"` channel and removes the agent from the orchestrator

---

## Server-Sent Events (SSE)

Three SSE endpoints provide streaming execution with progress updates. All return `Content-Type: text/event-stream`.

### POST `/api/v1/stream/execute`

Stream any `AgentRequest` execution.

**Request body:** same as `POST /api/v1/execute` (any `AgentRequest`)

**SSE output:**
```
event: start
data: {"message":"Processing request..."}

event: complete
data: {"success":true,"result":[...],"result_type":"Array"}

```

### POST `/api/v1/stream/pipeline`

Stream pipeline execution with per-step progress.

**Request body:** same as `POST /api/v1/pipeline`

**SSE output:**
```
event: start
data: {"message":"Starting pipeline with 3 steps"}

event: progress
data: {"current":1,"total":3,"percentage":33.3,"message":"Processing step 1/3: ls"}

event: progress
data: {"current":2,"total":3,"percentage":66.6,"message":"Processing step 2/3: where"}

event: progress
data: {"current":3,"total":3,"percentage":100.0,"message":"Processing step 3/3: select"}

event: complete
data: {"success":true,"result":[...],"result_type":"Array"}

```

### POST `/api/v1/stream/eval`

Stream code evaluation.

**Request body:** `{ "code": "..." }`

**SSE output:**
```
event: start
data: {"message":"Evaluating code..."}

event: complete
data: {"success":true,"result":84,"result_type":"Int"}

```

---

### SSE Response Headers

```
Content-Type: text/event-stream
Cache-Control: no-cache
X-Accel-Buffering: no
```

### AI API Streaming (Chat Completions)

The AI Model API (`POST /v1/chat/completions` with `stream: true`) also uses SSE, returning OpenAI-compatible `ChatCompletionChunk` events:

```
data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc","object":"chat.completion.chunk","model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}

data: [DONE]
```

Keep-alive interval: 15 seconds.
