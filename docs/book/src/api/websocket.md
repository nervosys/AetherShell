# WebSocket & SSE

AetherShell provides real-time communication through WebSocket connections and Server-Sent Events (SSE) for streaming results.

## WebSocket

### Connecting

Connect to the WebSocket endpoint:

```
ws://localhost:3000/api/v1/ws
```

```javascript
const ws = new WebSocket("ws://localhost:3000/api/v1/ws");

ws.onopen = () => console.log("Connected");
ws.onmessage = (event) => {
  const msg = JSON.parse(event.data);
  console.log(msg.type, msg);
};
```

### Client Messages

Messages from client to server are JSON objects with a `type` field.

#### `execute`
Execute an AetherShell command.

```json
{
  "type": "execute",
  "id": "req-1",
  "request": {
    "command": "ls \"src\" | len"
  }
}
```

#### `register`
Register as an agent on the network.

```json
{
  "type": "register",
  "agent_id": "my-agent",
  "capabilities": ["code-review", "testing"]
}
```

#### `agent_message`
Send a message to another registered agent.

```json
{
  "type": "agent_message",
  "to": "target-agent-id",
  "payload": { "task": "review", "file": "main.rs" }
}
```

#### `broadcast`
Broadcast a message to all subscribers of a channel.

```json
{
  "type": "broadcast",
  "channel": "status-updates",
  "payload": { "status": "analysis complete" }
}
```

#### `subscribe` / `unsubscribe`
Subscribe to or unsubscribe from a broadcast channel.

```json
{ "type": "subscribe", "channel": "status-updates" }
{ "type": "unsubscribe", "channel": "status-updates" }
```

#### `ping`
Keep-alive ping.

```json
{ "type": "ping", "id": "ping-1" }
```

### Server Messages

Messages from server to client.

#### `response`
Result of an `execute` request.

```json
{
  "type": "response",
  "id": "req-1",
  "response": {
    "success": true,
    "result": "15",
    "value_type": "Int"
  }
}
```

#### `stream`
Streaming event from an execute/eval/pipeline operation.

```json
{
  "type": "stream",
  "id": "req-1",
  "event": {
    "kind": "data",
    "data": "partial result"
  }
}
```

#### `channel`
Broadcast message received on a subscribed channel.

```json
{
  "type": "channel",
  "channel": "status-updates",
  "payload": { "status": "analysis complete" }
}
```

#### `pong`
Response to a ping.

```json
{
  "type": "pong",
  "id": "ping-1",
  "timestamp": 1705300000000
}
```

#### `error`
Error notification.

```json
{
  "type": "error",
  "id": "req-1",
  "message": "Unknown command: invalid_builtin"
}
```

#### `agent_message`
Message from another agent.

```json
{
  "type": "agent_message",
  "from": "analyzer-agent",
  "payload": { "result": "3 issues found" }
}
```

#### `registered`
Confirmation of agent registration.

```json
{
  "type": "registered",
  "agent_id": "my-agent"
}
```

#### `agents`
List of currently registered agents (sent on request or when agent list changes).

```json
{
  "type": "agents",
  "agents": [
    { "id": "agent-1", "capabilities": ["code-review"] },
    { "id": "agent-2", "capabilities": ["testing"] }
  ]
}
```

## Server-Sent Events (SSE)

SSE endpoints provide one-way streaming from server to client. They're used for long-running operations where you want incremental results.

### Endpoints

| Method | Path                      | Description                |
| ------ | ------------------------- | -------------------------- |
| POST   | `/api/v1/stream/execute`  | Stream command execution   |
| POST   | `/api/v1/stream/pipeline` | Stream pipeline processing |
| POST   | `/api/v1/stream/eval`     | Stream code evaluation     |

### Event Format

SSE uses the standard `text/event-stream` format:

```
event: start
data: {"id": "exec-123", "timestamp": 1705300000}

event: progress
data: {"step": 1, "total": 5, "message": "Loading files..."}

event: data
data: {"result": "[{\"name\": \"main.rs\", \"size\": 2048}]"}

event: complete
data: {"result": "final output", "elapsed_ms": 250}
```

### Event Types

| Event      | Description                                  |
| ---------- | -------------------------------------------- |
| `start`    | Operation has begun                          |
| `progress` | Progress update with step number and message |
| `data`     | Intermediate data result                     |
| `complete` | Operation finished successfully              |
| `error`    | Operation failed                             |

### Client Usage

```javascript
// SSE with fetch
const response = await fetch("/api/v1/stream/execute", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ command: "ls src | map(fn(f) => f.name)" })
});

const reader = response.body.getReader();
const decoder = new TextDecoder();

while (true) {
  const { done, value } = await reader.read();
  if (done) break;
  const text = decoder.decode(value);
  // Parse SSE events from text
  for (const line of text.split("\n")) {
    if (line.startsWith("data: ")) {
      const data = JSON.parse(line.slice(6));
      console.log(data);
    }
  }
}
```

### AI Chat Streaming

The AI Model API at `/v1/chat/completions` also supports SSE streaming when `"stream": true`:

```javascript
const response = await fetch("http://localhost:8080/v1/chat/completions", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({
    model: "llama3",
    messages: [{ role: "user", content: "Hello!" }],
    stream: true
  })
});

// Process SSE chunks
// Each chunk: data: {"choices":[{"delta":{"content":"token"}}]}
// Final: data: [DONE]
```

## Dashboard WebSocket

The web dashboard connects to WebSocket at `/api/v1/ws` for real-time updates. The dashboard automatically:

- Reconnects on disconnection (3-second retry)
- Receives agent status updates
- Gets workflow progress notifications
- Monitors marketplace changes

```typescript
// Dashboard auto-connect pattern
const ws = new WebSocket(`ws://${location.host}/api/v1/ws`);
ws.onclose = () => setTimeout(connect, 3000); // Auto-reconnect
```
