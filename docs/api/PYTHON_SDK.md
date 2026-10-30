# Python SDK Reference

The IronShell Python SDK (`ironshell`) provides a Pythonic interface to the IronShell runtime, agent orchestration, and workflow patterns.

**Package:** `integrations/python/python/ironshell`  
**Version:** 0.3.0

---

## Installation

```bash
pip install ironshell
```

Or from a checkout:

```bash
pip install ./integrations/python
```

Requires the `ish` binary on `PATH` or passed explicitly.

The SDK versions independently of the shell: SDK 1.5.1 is current against
shell 7.4.0.

---

## Core Runtime

### `IronShellRuntime`

Main entry point for evaluating IronShell code from Python.

```python
from ironshell import IronShellRuntime

rt = IronShellRuntime(ish_path="/path/to/ae")  # or auto-discovers on PATH
```

#### `rt.eval(code: str) -> Any`

Evaluate IronShell code and return a parsed Python object.

```python
result = rt.eval('[1, 2, 3] | map(fn(x) => x * 2)')
# [2, 4, 6]
```

Runs `ish -c <code> --deterministic` with a 60-second timeout. Raises
`RuntimeError` on failure.

> Before 7.2.0 this documented — and the SDK passed — `ish -e <code> --json`.
> Neither flag exists: the binary takes `-c/--command`, and canonical JSON comes
> from `--deterministic`. `clap` rejected the call, so every `eval()` raised
> `RuntimeError`. `tests/sdk_contract.rs` now runs the binary with the flags read
> out of the SDK source, so the two cannot drift apart again.

#### `rt.eval_file(path: str) -> Any`

Evaluate a `.ish` file. 300-second timeout.

```python
result = rt.eval_file("scripts/analysis.ish")
```

#### `rt.create_agent(name, model, tools, max_steps) -> Agent`

Create an AI agent instance.

```python
agent = rt.create_agent(
    name="reviewer",
    tools=["cat", "grep", "git"],
    max_steps=15,
)
```

`model` must stay at the default `"openai:gpt-4o-mini"`. The agent builtin has no model option, so `run` raises `ValueError` for any other model. `max_steps` must be 1 to 1000.

#### `rt.create_swarm(agents, policy, max_iterations) -> Swarm`

Create a multi-agent swarm. The options are checked when the swarm runs, not here (see the limits under `Swarm` below).

```python
swarm = rt.create_swarm(
    agents=[agent1, agent2],
    policy="round_robin",
    max_iterations=10,
)
```

#### `rt.run_swarm(agents, goal, policy, max_iterations) -> SwarmResult`

Convenience async method to create and run a swarm in one call.

The SDK sends the `swarm` builtin the record form `swarm({goal, tools, max_steps})`:

- `max_iterations` becomes `max_steps` (1 to 1000).
- `tools` is the union of the agents' `tools`, deduplicated, in first-seen order. The swarm itself has no tool list.
- The goal is escaped as an IronShell string literal. A goal containing `${` raises `ValueError`, because the language interpolates `${...}` in string literals and has no escape for it.

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
from ironshell import Agent

agent = Agent(
    name="coder",
    model="openai:gpt-4o-mini",  # Model URI
    tools=["ls", "cat", "git"],
    max_steps=10,
)
```

#### `await agent.run(goal: str) -> AgentResult`

Execute the agent with a goal string. The SDK sends the `agent` builtin the record form `agent({goal, tools, max_steps})`, with the goal escaped as an IronShell string literal. `run` raises `ValueError`, before anything runs, for a non-default model, a `max_steps` outside 1 to 1000, or a goal containing `${`.

```python
result = await agent.run("Find all Python files with TODO comments")
```

The builtin returns its final answer as a String and reports no trace or step count. When `run` gets only that string, `result` holds it, `success` is `True`, `trace` is empty and `steps_taken` is 0.

Returns:
```python
@dataclass
class AgentResult:
    success: bool
    result: Any
    trace: List[Dict[str, Any]]  # Step-by-step execution trace
    steps_taken: int
```

The model URI is recorded on the agent but the `agent` and `swarm` builtins take no model option, so `run` accepts only the default `openai:gpt-4o-mini` and raises `ValueError` for any other value. Model URI format: `provider:model_name`
- `openai:gpt-4o`, `openai:gpt-4o-mini`
- `ollama:llama3`, `ollama:codellama`
- `anthropic:claude-3-sonnet`
- `compat:mixtral`

---

## Swarm

### `Swarm`

Multi-agent swarm, run by the `swarm` builtin.

```python
from ironshell import Swarm

swarm = Swarm(
    agents=[planner, coder, tester],
    policy="round_robin",
    max_iterations=10,
)
```

The builtin takes no model, policy or per-agent step limit, so `run` raises `ValueError` before anything runs when:

- `policy` is anything other than `"round_robin"` (the builtin has no policy option, so `"router"` is refused).
- An agent sets `model` to anything other than the default `"openai:gpt-4o-mini"`. The builtin has no per-agent model option.
- An agent sets `max_steps` to anything other than 10. Use `max_iterations` on the swarm.
- `max_iterations` is outside 1 to 1000.

Agent `name` is a label only and is not sent to the builtin.

#### `await swarm.run(goal: str) -> SwarmResult`

```python
result = await swarm.run("Build and test a REST API")
```

Returns:
```python
@dataclass
class SwarmResult:
    success: bool
    result: Any                 # The builtin's combined answer (a String)
    blackboard: Dict[str, Any]  # Empty: the builtin does not report one
    iterations: int             # 0: the builtin does not report one
```

`success` is `True` when the call returns. A failed call raises `RuntimeError` from the runtime instead.

---

## Pipeline Builder

Fluent API for building typed data pipelines.

```python
from ironshell import pipeline

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
| `.to_code() -> str`         | Get the IronShell pipeline code |
| `.run(runtime=None) -> Any` | Execute the pipeline              |

The lambda strings use IronShell syntax: `fn(x) => expr`.

---

## Convenience Function

```python
from ironshell import evaluate

result = evaluate('2 + 2')  # 4
```

Creates a temporary `IronShellRuntime` and evaluates the expression.

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

### `ironshell.workflows`

Workflow orchestration patterns.

| Class               | Description                                          |
| ------------------- | ---------------------------------------------------- |
| `MapReduceWorkflow` | Split work, process in parallel, combine results     |
| `SagaWorkflow`      | Multi-step with compensating transactions on failure |
| `FanOutWorkflow`    | Fan-out to multiple workers, fan-in results          |
| `PipelineWorkflow`  | Sequential step-by-step execution                    |
| `CircuitBreaker`    | Fault tolerance — stops calls to failing services    |

```python
from ironshell.workflows import CircuitBreaker, CircuitState

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

### `ironshell.metrics`

Observability: Prometheus metrics, tracing, health checks.

### `ironshell.distributed`

Distributed agent infrastructure: service registry, leader election, routing.

### `ironshell.langchain`

LangChain integration tools for using IronShell as a LangChain toolkit.

---

## Examples

### Simple Evaluation

```python
from ironshell import evaluate

files = evaluate('ls(".") | where(fn(f) => f.size > 1000) | select("name")')
print(files)
```

### Agent with Tools

```python
import asyncio
from ironshell import IronShellRuntime

async def main():
    rt = IronShellRuntime()
    agent = rt.create_agent(
        name="analyst",
        tools=["ls", "cat", "grep", "wc"],
    )
    result = await agent.run("Count lines of code in src/")
    print(f"Steps: {result.steps_taken}, Result: {result.result}")

asyncio.run(main())
```

### Multi-Agent Swarm

```python
import asyncio
from ironshell import IronShellRuntime

async def main():
    rt = IronShellRuntime()
    # Leave each agent on the default model: the swarm builtin has no per-agent model option.
    planner = rt.create_agent("planner", tools=["ls", "cat"])
    coder = rt.create_agent("coder", tools=["cat", "write"])
    reviewer = rt.create_agent("reviewer", tools=["cat", "grep"])

    result = await rt.run_swarm(
        agents=[planner, coder, reviewer],
        goal="Refactor the utils module for better error handling",
        policy="round_robin",
        max_iterations=10,
    )
    print(f"Success: {result.success}, Answer: {result.result}")

asyncio.run(main())
```
