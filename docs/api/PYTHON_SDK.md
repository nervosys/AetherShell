# Python SDK Reference

The AetherShell Python SDK (`aethershell`) provides a Pythonic interface to the AetherShell runtime, agent orchestration, and workflow patterns.

**Package:** `integrations/python/python/aethershell`  
**Version:** 0.3.0

---

## Installation

```bash
pip install aethershell
```

Or from a checkout:

```bash
pip install ./integrations/python
```

Requires the `ae` binary on `PATH` or passed explicitly.

The SDK versions independently of the shell: SDK 1.5.0 is current against
shell 7.2.0.

---

## Core Runtime

### `AetherRuntime`

Main entry point for evaluating AetherShell code from Python.

```python
from aethershell import AetherRuntime

rt = AetherRuntime(ae_path="/path/to/ae")  # or auto-discovers on PATH
```

#### `rt.eval(code: str) -> Any`

Evaluate AetherShell code and return a parsed Python object.

```python
result = rt.eval('[1, 2, 3] | map(fn(x) => x * 2)')
# [2, 4, 6]
```

Runs `ae -c <code> --deterministic` with a 60-second timeout. Raises
`RuntimeError` on failure.

> Before 7.2.0 this documented — and the SDK passed — `ae -e <code> --json`.
> Neither flag exists: the binary takes `-c/--command`, and canonical JSON comes
> from `--deterministic`. `clap` rejected the call, so every `eval()` raised
> `RuntimeError`. `tests/sdk_contract.rs` now runs the binary with the flags read
> out of the SDK source, so the two cannot drift apart again.

#### `rt.eval_file(path: str) -> Any`

Evaluate a `.ae` file. 300-second timeout.

```python
result = rt.eval_file("scripts/analysis.ae")
```

#### `rt.create_agent(name, model, tools, max_steps) -> Agent`

Create an AI agent instance.

```python
agent = rt.create_agent(
    name="reviewer",
    model="openai:gpt-4o",
    tools=["cat", "grep", "git"],
    max_steps=15,
)
```

#### `rt.create_swarm(agents, policy, max_iterations) -> Swarm`

Create a multi-agent swarm.

```python
swarm = rt.create_swarm(
    agents=[agent1, agent2],
    policy="round_robin",
    max_iterations=10,
)
```

#### `rt.run_swarm(agents, goal, policy, max_iterations) -> SwarmResult`

Convenience async method to create and run a swarm in one call.

#### `rt.subscribe_a2ui(callback) -> Callable[[], None]`

Subscribe to Agent-to-UI (A2UI) events. Returns an unsubscribe function.

```python
def on_event(event: A2UIEvent):
    print(f"[{event.priority}] {event.event_type}: {event.data}")

unsubscribe = rt.subscribe_a2ui(on_event)
# ... later ...
unsubscribe()
```

---

## Agent

### `Agent`

AI agent with tool execution capabilities.

```python
from aethershell import Agent

agent = Agent(
    name="coder",
    model="openai:gpt-4o-mini",  # Model URI
    tools=["ls", "cat", "git"],
    max_steps=10,
)
```

#### `await agent.run(goal: str) -> AgentResult`

Execute the agent with a goal string.

```python
result = await agent.run("Find all Python files with TODO comments")
```

Returns:
```python
@dataclass
class AgentResult:
    success: bool
    result: Any
    trace: List[Dict[str, Any]]  # Step-by-step execution trace
    steps_taken: int
```

Model URI format: `provider:model_name`
- `openai:gpt-4o`, `openai:gpt-4o-mini`
- `ollama:llama3`, `ollama:codellama`
- `anthropic:claude-3-sonnet`
- `compat:mixtral`

---

## Swarm

### `Swarm`

Multi-agent swarm with coordination policies.

```python
from aethershell import Swarm

swarm = Swarm(
    agents=[planner, coder, tester],
    policy="round_robin",  # or "router"
    max_iterations=10,
)
```

#### `await swarm.run(goal: str) -> SwarmResult`

```python
result = await swarm.run("Build and test a REST API")
```

Returns:
```python
@dataclass
class SwarmResult:
    success: bool
    result: Any
    blackboard: Dict[str, Any]  # Shared state between agents
    iterations: int
```

---

## Pipeline Builder

Fluent API for building typed data pipelines.

```python
from aethershell import pipeline

result = (
    pipeline([1, 2, 3, 4, 5])
    .map("fn(x) => x * 2")
    .filter("fn(x) => x > 4")
    .sort()
    .run()
)
# [6, 8, 10]
```

### Methods

| Method                      | Description                       |
| --------------------------- | --------------------------------- |
| `.map(fn: str)`             | Transform each element            |
| `.filter(fn: str)`          | Keep elements matching predicate  |
| `.reduce(fn: str, initial)` | Fold elements into accumulator    |
| `.sort(fn: str = None)`     | Sort (optionally with comparator) |
| `.reverse()`                | Reverse order                     |
| `.flatten()`                | Flatten nested arrays             |
| `.unique()`                 | Remove duplicates                 |
| `.take(n: int)`             | Take first N elements             |
| `.skip(n: int)`             | Skip first N elements             |
| `.to_code() -> str`         | Get the AetherShell pipeline code |
| `.run(runtime=None) -> Any` | Execute the pipeline              |

The lambda strings use AetherShell syntax: `fn(x) => expr`.

---

## Convenience Function

```python
from aethershell import evaluate

result = evaluate('2 + 2')  # 4
```

Creates a temporary `AetherRuntime` and evaluates the expression.

---

## Data Types

```python
@dataclass
class AgentConfig:
    name: str
    model: str = "openai:gpt-4o-mini"
    tools: List[str] = []
    max_steps: int = 10
    dry_run: bool = False

@dataclass
class A2UIEvent:
    id: str
    timestamp: str
    priority: str
    event_type: str
    data: Dict[str, Any]

class NotificationLevel(Enum):
    INFO = "info"
    SUCCESS = "success"
    WARNING = "warning"
    ERROR = "error"
```

---

## Submodules

### `aethershell.workflows`

Workflow orchestration patterns.

| Class               | Description                                          |
| ------------------- | ---------------------------------------------------- |
| `MapReduceWorkflow` | Split work, process in parallel, combine results     |
| `SagaWorkflow`      | Multi-step with compensating transactions on failure |
| `FanOutWorkflow`    | Fan-out to multiple workers, fan-in results          |
| `PipelineWorkflow`  | Sequential step-by-step execution                    |
| `CircuitBreaker`    | Fault tolerance — stops calls to failing services    |

```python
from aethershell.workflows import CircuitBreaker, CircuitState

breaker = CircuitBreaker(
    failure_threshold=5,
    recovery_timeout_ms=30000,
)
```

`WorkflowResult`:
```python
@dataclass
class WorkflowResult:
    success: bool
    result: Any
    steps_completed: int
    total_steps: int
    duration_ms: float
    errors: List[str]
```

### `aethershell.metrics`

Observability: Prometheus metrics, tracing, health checks.

### `aethershell.distributed`

Distributed agent infrastructure: service registry, leader election, routing.

### `aethershell.langchain`

LangChain integration tools for using AetherShell as a LangChain toolkit.

---

## Examples

### Simple Evaluation

```python
from aethershell import evaluate

files = evaluate('ls(".") | where(fn(f) => f.size > 1000) | select("name")')
print(files)
```

### Agent with Tools

```python
import asyncio
from aethershell import AetherRuntime

async def main():
    rt = AetherRuntime()
    agent = rt.create_agent(
        name="analyst",
        model="openai:gpt-4o",
        tools=["ls", "cat", "grep", "wc"],
    )
    result = await agent.run("Count lines of code in src/")
    print(f"Steps: {result.steps_taken}, Result: {result.result}")

asyncio.run(main())
```

### Multi-Agent Swarm

```python
import asyncio
from aethershell import AetherRuntime

async def main():
    rt = AetherRuntime()
    planner = rt.create_agent("planner", "openai:gpt-4o", ["ls", "cat"])
    coder = rt.create_agent("coder", "openai:gpt-4o", ["cat", "write"])
    reviewer = rt.create_agent("reviewer", "openai:gpt-4o", ["cat", "grep"])

    result = await rt.run_swarm(
        agents=[planner, coder, reviewer],
        goal="Refactor the utils module for better error handling",
        policy="round_robin",
    )
    print(f"Iterations: {result.iterations}, Success: {result.success}")

asyncio.run(main())
```
