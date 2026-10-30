# Python SDK

The IronShell Python SDK (`ironshell` package v1.5.0) provides a Pythonic interface for evaluating IronShell code, running agents, building pipelines, and integrating with Python AI ecosystems.

## Installation

```bash
pip install ironshell
```

## Quick Start

```python
from ironshell import evaluate, pipeline

# Evaluate IronShell code
result = evaluate('[1, 2, 3] | map(fn(x) => x * 2)')
print(result)  # [2, 4, 6]

# Build a pipeline
result = pipeline([1, 2, 3, 4, 5]).filter(lambda x: x > 2).map(lambda x: x * 10).run()
print(result)  # [30, 40, 50]
```

## IronShellRuntime

The core runtime class for evaluating IronShell code.

```python
from ironshell import IronShellRuntime

runtime = IronShellRuntime()

# Evaluate a single expression
result = runtime.eval('42 * 2')

# Evaluate a file
result = runtime.eval_file('script.ish')
```

### Creating Agents

```python
agent = runtime.create_agent(
    name="finder",
    tools=["ls", "cat", "grep"],
    max_steps=10,
)
result = await agent.run("Find all files over 10KB")
print(result.result)  # the final answer; trace and steps_taken are empty
```

### Creating Swarms

```python
planner = runtime.create_agent("planner", tools=["ls", "cat"])
reviewer = runtime.create_agent("reviewer", tools=["cat", "grep", "wc"])
swarm = runtime.create_swarm([planner, reviewer], max_iterations=20)
result = await swarm.run("Review src/ for code quality issues")
print(result.result)  # the combined answer
```

The swarm sends the `swarm` builtin `{goal, tools, max_steps}`. `tools` is the union of the agents' tools and `max_iterations` is `max_steps`. The builtin has no model, policy or per-agent step option, so `run` raises `ValueError` for a `policy` other than `"round_robin"`, an agent model other than the default `openai:gpt-4o-mini`, or an agent `max_steps` other than 10. See the [Python SDK reference](../../../api/PYTHON_SDK.md#swarm).

### A2UI Events

Subscribe to agent-to-UI events for real-time feedback:

```python
def on_event(event):
    if event.type == "progress":
        print(f"Progress: {event.data['step']}/{event.data['total']}")
    elif event.type == "notification":
        print(f"[{event.level}] {event.message}")

runtime.subscribe_a2ui(on_event)
```

## PipelineBuilder

A fluent API for building data transformation pipelines:

```python
from ironshell import pipeline

result = (
    pipeline([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
    .filter(lambda x: x % 2 == 0)     # Keep even numbers
    .map(lambda x: x ** 2)             # Square them
    .sort()                             # Sort ascending
    .take(3)                            # First 3
    .run()
)
print(result)  # [4, 16, 36]
```

### Pipeline Methods

| Method                   | Description                      |
| ------------------------ | -------------------------------- |
| `.map(fn)`               | Transform each element           |
| `.filter(fn)`            | Keep elements matching predicate |
| `.reduce(fn, init)`      | Fold to single value             |
| `.sort()` / `.sort(key)` | Sort elements                    |
| `.reverse()`             | Reverse order                    |
| `.flatten()`             | Flatten nested arrays            |
| `.unique()`              | Remove duplicates                |
| `.take(n)`               | First N elements                 |
| `.skip(n)`               | Skip N elements                  |
| `.to_code()`             | Generate IronShell code        |
| `.run()`                 | Execute the pipeline             |

### Generating IronShell Code

```python
code = (
    pipeline([1, 2, 3])
    .map(lambda x: x * 2)
    .filter(lambda x: x > 2)
    .to_code()
)
print(code)  # [1, 2, 3] | map(fn(x) => x * 2) | where(fn(x) => x > 2)
```

## Workflows

Build structured AI workflows with retry and circuit-breaker patterns:

```python
from ironshell.workflows import Workflow, WorkflowStep, WorkflowPattern

# Create a sequential workflow
wf = Workflow(pattern=WorkflowPattern.SEQUENTIAL)
wf.add_step(WorkflowStep(name="gather", code='ls "src"'))
wf.add_step(WorkflowStep(name="analyze", code='grep "TODO" "src/"'))
wf.add_step(WorkflowStep(name="report", code='echo "Analysis complete"'))

result = await wf.run(input_data={})
print(result.outputs)
```

### MapReduce Workflow

```python
from ironshell.workflows import MapReduceWorkflow

wf = MapReduceWorkflow(
    map_code='fn(item) => ai("Summarize: " + item)',
    reduce_code='fn(summaries) => join(summaries, "\n\n")'
)
result = await wf.run(["doc1.md", "doc2.md", "doc3.md"])
```

### Circuit Breaker

Protect against cascading failures:

```python
from ironshell.workflows import CircuitBreaker

breaker = CircuitBreaker(
    failure_threshold=3,
    recovery_timeout=30.0
)

try:
    result = await breaker.call_async(lambda: runtime.eval('http_get "https://api.example.com"'))
except Exception:
    print("Circuit open — using fallback")
```

## Metrics

Production-grade observability with Prometheus-compatible metrics:

```python
from ironshell.metrics import Counter, Histogram, Timer

# Count operations
requests = Counter("requests_total", "Total requests processed")
requests.inc()

# Track latencies
latency = Histogram("request_duration_seconds", "Request latency")

with Timer(latency):
    result = runtime.eval('ai("Summarize this")')

# Export for Prometheus
print(latency.to_prometheus())
```

## Distributed

Service discovery and leader election for multi-node deployments:

```python
from ironshell.distributed import ServiceRegistry, ServiceInfo

registry = ServiceRegistry()

# Register a service
registry.register(ServiceInfo(
    id="worker-1",
    name="ironshell-worker",
    address="192.168.1.10",
    port=3000,
    metadata={"gpu": "true"}
))

# Discover services
workers = registry.get_services_by_name("ironshell-worker")
for w in workers:
    print(f"{w.id} at {w.address}:{w.port}")
```

### Leader Election

```python
from ironshell.distributed import LeaderElection

election = LeaderElection(service_info)

election.on_leadership_change(lambda is_leader:
    print("I am the leader!" if is_leader else "Following leader")
)

if election.is_leader():
    # Coordinate work distribution
    pass
```

## LangChain Integration

Use IronShell tools within LangChain agents:

```python
from ironshell.langchain import IronShellTool, IronShellAgentTool

# Use IronShell as a LangChain tool
shell_tool = IronShellTool()
result = shell_tool.run('ls "src" | where(fn(f) => f.size > 1000)')

# Run an IronShell agent from LangChain
agent_tool = IronShellAgentTool(tools=["ls", "cat"])
result = agent_tool.run("Find TODO comments in the project")
```

## Cloud Deployment

Deploy IronShell as serverless functions:

```python
from ironshell.cloud import LambdaRuntime, FunctionConfig

runtime = LambdaRuntime()
handler = runtime.create_handler(FunctionConfig(
    name="data-processor",
    code='fn(event) => event.body | json_parse | map(fn(x) => x * 2)',
    timeout=30
))

# Generate deployment configuration
deployment = runtime.generate_deployment()
```

Supported platforms:
- **AWS Lambda** via `LambdaRuntime`
- **Azure Functions** via `AzureFunctionsRuntime`
