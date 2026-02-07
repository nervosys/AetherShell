# Agent API

The Agent API provides HTTP endpoints for executing AetherShell code, managing agents, orchestrating workflows, and accessing the marketplace. The server uses [axum](https://github.com/tokio-rs/axum) and runs on port 3000 by default.

## Starting the Server

```bash
ae serve                    # Start on default port 3000
ae serve --port 8080        # Custom port
```

## Execution Endpoints

### POST `/api/v1/execute`
Execute an AetherShell command and return the result.

**Request:**
```json
{
  "command": "ls \"src\" | where(fn(f) => f.extension == \"rs\") | len"
}
```

**Response:**
```json
{
  "success": true,
  "result": "15",
  "type": "Int"
}
```

### POST `/api/v1/call/:builtin`
Call a specific builtin by name with arguments.

**Request:**
```json
{
  "args": ["src"]
}
```

**Example:** `POST /api/v1/call/ls`

### POST `/api/v1/pipeline`
Execute a multi-step pipeline.

**Request:**
```json
{
  "input": [1, 2, 3, 4, 5],
  "steps": ["map(fn(x) => x * 2)", "where(fn(x) => x > 4)"]
}
```

### POST `/api/v1/eval`
Evaluate an arbitrary AetherShell expression.

**Request:**
```json
{
  "code": "let x = 42; x * 2"
}
```

## Streaming Endpoints (SSE)

These endpoints return Server-Sent Events for long-running operations.

### POST `/api/v1/stream/execute`
Stream execution results as they're produced.

### POST `/api/v1/stream/pipeline`
Stream pipeline results step-by-step.

### POST `/api/v1/stream/eval`
Stream evaluation output.

**SSE Event Format:**
```
event: start
data: {"id": "exec-123"}

event: progress
data: {"step": 1, "total": 5, "message": "Processing..."}

event: data
data: {"result": "partial output"}

event: complete
data: {"result": "final result", "elapsed_ms": 150}

event: error
data: {"message": "Syntax error at line 3"}
```

## Discovery Endpoints

### GET `/api/v1/schema`
Return the complete AetherShell language schema (types, builtins, syntax).

### GET `/api/v1/schema/:format`
Return the schema in a specific format (e.g., `json`, `openapi`).

### GET `/api/v1/builtins`
List all available builtins with their descriptions.

**Response:**
```json
[
  { "name": "ls", "description": "List directory contents", "category": "filesystem" },
  { "name": "map", "description": "Transform each element", "category": "collections" },
  ...
]
```

### GET `/api/v1/builtins/:name`
Get detailed information about a specific builtin.

**Response:**
```json
{
  "name": "map",
  "description": "Apply a function to each element in an array",
  "category": "collections",
  "signature": "map(fn) -> Array",
  "examples": ["[1,2,3] | map(fn(x) => x * 2)"]
}
```

### GET `/api/v1/types`
List all AetherShell value types and their properties.

## Orchestration Endpoints

### GET `/api/v1/orchestration/agents`
List all registered agents and their status.

**Response:**
```json
[
  {
    "id": "agent-1",
    "status": "idle",
    "capabilities": ["code-review", "testing"],
    "model": "openai:gpt-4o-mini"
  }
]
```

### GET `/api/v1/orchestration/tasks`
List all tasks.

### POST `/api/v1/orchestration/tasks`
Create a new task.

**Request:**
```json
{
  "goal": "Analyze code quality in src/",
  "tools": ["ls", "cat", "grep"],
  "max_steps": 10
}
```

### POST `/api/v1/orchestration/workflows`
Create and start a new workflow.

### GET `/api/v1/orchestration/workflows`
List all workflows.

### GET `/api/v1/orchestration/workflows/:id`
Get workflow details and status.

### POST `/api/v1/orchestration/workflows/:id/cancel`
Cancel a running workflow.

### GET `/api/v1/orchestration/metrics`
Get orchestration metrics (agent count, task counts, performance).

**Response:**
```json
{
  "total_agents": 3,
  "active_tasks": 2,
  "completed_tasks": 15,
  "avg_task_duration_ms": 2300
}
```

## Marketplace Endpoints

### GET `/api/v1/marketplace/search?q=code-review&category=dev`
Search the agent marketplace.

### GET `/api/v1/marketplace/agents`
List all marketplace agents.

### POST `/api/v1/marketplace/install`
Install an agent from the marketplace.

**Request:**
```json
{
  "name": "code-reviewer",
  "version": "1.0.0"
}
```

### POST `/api/v1/marketplace/uninstall`
Uninstall a marketplace agent.

**Request:**
```json
{
  "name": "code-reviewer"
}
```

### POST `/api/v1/marketplace/publish`
Publish an agent to the marketplace.

**Request:**
```json
{
  "name": "my-agent",
  "description": "A helpful coding agent",
  "system_prompt": "You are a code reviewer...",
  "tools": ["cat", "grep"],
  "model": "openai:gpt-4o-mini"
}
```

## Health

### GET `/health`
Health check endpoint.

**Response:**
```json
{
  "status": "healthy",
  "version": "0.3.0",
  "uptime_seconds": 3600
}
```
