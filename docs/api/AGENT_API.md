# Agent API Reference

The Agent API provides a structured JSON-based HTTP interface for AI agents to interact with AetherShell. Instead of generating shell code, AI agents use typed JSON requests to execute builtins, pipelines, and evaluations.

**Default server:** `http://127.0.0.1:3002`

---

## Common Response Format

All endpoints return `AgentResponse`:

```json
{
  "success": true,
  "result": <any>,
  "error": "string (on failure)",
  "result_type": "String | Int | Array | Record | ...",
  "metadata": { "code_executed": "..." }
}
```

- **200 OK** — `success: true`
- **400 Bad Request** — `success: false` with `error` field

---

## Execution Endpoints

### POST `/api/v1/execute`

Universal entry point. Accepts any `AgentRequest` variant via the `action` discriminator.

**Request body** — one of the tagged union variants:

| `action`        | Fields                                 | Purpose                       |
| --------------- | -------------------------------------- | ----------------------------- |
| `call`          | `builtin: string`, `args: any`         | Execute a single builtin      |
| `pipeline`      | `steps: PipelineStep[]`, `input?: any` | Execute a pipeline            |
| `eval`          | `code: string`                         | Evaluate raw AetherShell code |
| `describe`      | `builtin: string`                      | Get builtin metadata          |
| `list_builtins` | `category?: string`                    | List available builtins       |
| `schema`        | `format?: SchemaFormat`                | Get language ontology/schema  |
| `type_info`     | `type_name?: string`                   | Get type system information   |

**Example — call:**
```json
POST /api/v1/execute
{
  "action": "call",
  "builtin": "ls",
  "args": { "path": "." }
}
```

**Example — pipeline:**
```json
POST /api/v1/execute
{
  "action": "pipeline",
  "steps": [
    { "builtin": "ls", "args": { "path": "." } },
    { "builtin": "where", "predicate": "size > 1000" },
    { "builtin": "select", "args": ["name"] }
  ]
}
```

---

### POST `/api/v1/call/:builtin`

Shorthand to call a single builtin by name.

| Parameter  | Location | Type   | Description                              |
| ---------- | -------- | ------ | ---------------------------------------- |
| `:builtin` | path     | string | Builtin function name (e.g., `ls`, `cd`) |
| body       | JSON     | any    | Arguments — object, array, or scalar     |

**Example:**
```json
POST /api/v1/call/ls
{ "path": "/tmp" }
```

---

### POST `/api/v1/pipeline`

Execute a multi-step pipeline.

**Request body:**
```json
{
  "steps": [
    {
      "builtin": "string",
      "args": {},
      "select": "field_name",
      "predicate": "size > 100"
    }
  ],
  "input": [1, 2, 3]
}
```

| Field               | Type           | Required | Description                              |
| ------------------- | -------------- | -------- | ---------------------------------------- |
| `steps`             | PipelineStep[] | yes      | Ordered list of pipeline operations      |
| `steps[].builtin`   | string         | yes      | Function name                            |
| `steps[].args`      | any            | no       | Positional array or named object args    |
| `steps[].select`    | string         | no       | Field selector (e.g., `"name"`)          |
| `steps[].predicate` | string         | no       | Filter expression (e.g., `"size > 100"`) |
| `input`             | any            | no       | Initial pipeline input value             |

---

### POST `/api/v1/eval`

Evaluate raw AetherShell code.

**Request body:**
```json
{ "code": "let x = 42; x * 2" }
```

---

## Discovery Endpoints

### GET `/api/v1/schema`

Returns the full language ontology in compact `Ontology` format, including types, builtins, operators, and syntax patterns.

### GET `/api/v1/schema/:format`

Returns the language schema formatted for a specific AI provider's function-calling convention.

| `:format` aliases                 | Provider         |
| --------------------------------- | ---------------- |
| `openai`, `gpt`, `chatgpt`        | OpenAI           |
| `claude`, `anthropic`             | Anthropic Claude |
| `gemini`, `google`                | Google Gemini    |
| `llama`, `meta`, `llama3`         | Meta Llama       |
| `mistral`, `codestral`, `pixtral` | Mistral AI       |
| `cohere`, `command`, `command-r`  | Cohere           |
| `grok`, `xai`                     | xAI Grok         |
| `deepseek`, `deepseek-r1`         | DeepSeek         |
| `bedrock`, `aws`, `amazon`        | AWS Bedrock      |
| `azure`, `azure_openai`           | Azure OpenAI     |
| `qwen`, `alibaba`, `dashscope`    | Alibaba Qwen     |
| `ollama`                          | Ollama           |
| `vllm`                            | vLLM             |
| `huggingface`, `hf`, `tgi`        | HuggingFace TGI  |
| `openrouter`                      | OpenRouter       |
| `kimi`, `moonshot`                | Moonshot Kimi    |
| `yi`, `01ai`                      | 01.AI Yi         |
| `glm`, `chatglm`, `zhipu`         | Zhipu GLM        |
| `reka`                            | Reka AI          |
| `ai21`, `jamba`, `jurassic`       | AI21 Labs        |
| `perplexity`, `sonar`             | Perplexity AI    |
| `together`, `together-ai`         | Together AI      |
| `groq`                            | Groq             |
| `fireworks`, `fireworks-ai`       | Fireworks AI     |
| `json`, `jsonschema`              | Raw JSON Schema  |

### GET `/api/v1/builtins`

List all available builtins.

| Query param | Type   | Description                       |
| ----------- | ------ | --------------------------------- |
| `category`  | string | Filter by category (e.g., `"io"`) |

**Response:**
```json
{
  "success": true,
  "result": {
    "count": 42,
    "builtins": [
      {
        "name": "ls",
        "description": "List directory contents",
        "category": "filesystem",
        "signature": "ls(path?: String) -> Array<Record>"
      }
    ]
  }
}
```

### GET `/api/v1/builtins/:name`

Describe a single builtin with full metadata, parameters, examples, and JSON schema.

**Response (200):**
```json
{
  "success": true,
  "result": {
    "name": "ls",
    "description": "List directory contents",
    "category": "filesystem",
    "signature": "ls(path?: String) -> Array<Record>",
    "parameters": [...],
    "return_type": "Array",
    "examples": [...],
    "json_schema": { ... }
  },
  "result_type": "BuiltinDefinition"
}
```

**Response (404):** `{ "success": false, "error": "Builtin 'xyz' not found" }`

### GET `/api/v1/types`

Returns all AetherShell type definitions (Int, Float, String, Array, Record, Lambda, etc.) with JSON equivalents and field definitions.

---

## Orchestration Endpoints

### GET `/api/v1/orchestration/agents`

List all connected agents (registered via WebSocket).

**Response:**
```json
{
  "success": true,
  "agents": [
    { "id": "agent-1", "capabilities": ["code", "search"], "connected_at": 1707300000 }
  ]
}
```

### GET `/api/v1/orchestration/tasks`

List pending tasks in the orchestration queue.

### POST `/api/v1/orchestration/tasks`

Create a new task.

**Request body:**
```json
{
  "name": "analyze-logs",
  "payload": { "file": "/var/log/app.log" },
  "priority": 5
}
```

**Response (201):**
```json
{ "success": true, "task_id": "uuid" }
```

### POST `/api/v1/orchestration/workflows`

Create a new workflow definition.

**Request body:**
```json
{
  "name": "data-pipeline",
  "steps": [
    {
      "name": "fetch",
      "agent_capability": "http",
      "request": { "action": "call", "builtin": "http_get", "args": { "url": "..." } },
      "on_success": "transform",
      "on_failure": null
    }
  ],
  "context": {}
}
```

**Response (201):** `{ "success": true, "workflow_id": "uuid" }`

### GET `/api/v1/orchestration/workflows`

List all workflows with their status.

### GET `/api/v1/orchestration/workflows/:id`

Get a specific workflow by ID.

**Response (404):** `{ "success": false, "error": "Workflow not found" }`

### POST `/api/v1/orchestration/workflows/:id/cancel`

Cancel a running workflow (sets status to `Failed`). Broadcasts a `workflow_update` event.

### GET `/api/v1/orchestration/metrics`

Get orchestration metrics snapshot.

**Response:**
```json
{
  "success": true,
  "metrics": {
    "agents": { "total": 3, "online": 3 },
    "workflows": { "total": 12, "running": 2, "completed": 8, "failed": 2 },
    "tasks": { "total": 50, "pending": 5 },
    "timestamp": 1707300000
  }
}
```

---

## Marketplace Endpoints

### GET `/api/v1/marketplace/search`

Search the agent marketplace registry.

| Query param | Type   | Description                                  |
| ----------- | ------ | -------------------------------------------- |
| `q`         | string | Search query                                 |
| `category`  | string | Filter by category                           |
| `sort`      | string | Sort: `stars`, `recent`, `name`, `downloads` |
| `page`      | int    | Page number                                  |
| `per_page`  | int    | Results per page                             |

**Response:**
```json
{
  "success": true,
  "query": "code review",
  "agents": [
    {
      "id": "code-reviewer",
      "name": "Code Reviewer",
      "description": "...",
      "author": "acme",
      "version": "1.0.0",
      "downloads": 1500,
      "stars": 42,
      "tags": ["code", "review"],
      "verified": true
    }
  ],
  "total": 15,
  "page": 1,
  "per_page": 20
}
```

### GET `/api/v1/marketplace/agents`

List installed agents.

### POST `/api/v1/marketplace/install`

Install an agent from the marketplace.

**Request body:**
```json
{ "name": "code-reviewer", "version": "1.0.0" }
```

**Response (200):**
```json
{
  "success": true,
  "message": "Installed Code Reviewer v1.0.0",
  "agent": { "name": "code-reviewer", "version": "1.0.0", "path": "..." }
}
```

### POST `/api/v1/marketplace/uninstall`

Uninstall an installed agent.

**Request body:**
```json
{ "name": "code-reviewer" }
```

### POST `/api/v1/marketplace/publish`

Publish an agent definition. Broadcasts an `agent_published` event.

**Request body:**
```json
{
  "name": "my-agent",
  "description": "Does things",
  "systemPrompt": "You are...",
  "tools": ["ls", "cat"],
  "model": "openai:gpt-4o"
}
```

---

## Health

### GET `/health`

```json
{
  "status": "healthy",
  "service": "aethershell-agent-api",
  "version": "0.x.x",
  "features": { "websocket": true, "sse_streaming": true, "orchestration": true },
  "supported_agents": ["openai", "claude", "gemini", ...],
  "schema_formats": { ... }
}
```
