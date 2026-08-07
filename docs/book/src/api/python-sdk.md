# Python SDK

The AetherShell Python SDK (`aethershell` package v0.3.0) provides a Pythonic interface for evaluating AetherShell code, running agents, building pipelines, and integrating with Python AI ecosystems.

## Installation

> **⚠ Not published yet — do not run `pip install aethershell`.**
>
> As of 2026-08-06 this package is **not on PyPI** and the name is
> **unregistered**, so installing it would fetch whatever a third party has
> uploaded under that name — and `pip install` runs package code at install
> time. Install from the repository until this notice is removed.

```bash
pip install ./integrations/python
```

## Quick Start

```python
from aethershell import evaluate, pipeline

# Evaluate AetherShell code
result = evaluate('[1, 2, 3] | map(fn(x) => x * 2)')
print(result)  # [2, 4, 6]

# Build a pipeline
result = pipeline([1, 2, 3, 4, 5]).filter(lambda x: x > 2).map(lambda x: x * 10).run()
print(result)  # [30, 40, 50]
```

## AetherRuntime

The core runtime class for evaluating AetherShell code.

```python
from aethershell import AetherRuntime

runtime = AetherRuntime()

# Evaluate a single expression
result = runtime.eval('42 * 2')

# Evaluate a file
result = runtime.eval_file('script.ae')
```

### Creating Agents

```python
agent = runtime.create_agent(
    goal="Find large files in src/",
    tools=["ls", "cat", "grep"],
    max_steps=10,
    model="openai:gpt-4o-mini"
)
result = await agent.run("Find all files over 10KB")
print(result.output)
```

### Creating Swarms

```python
swarm = runtime.create_swarm(
    goal="Analyze project quality",
    tools=["ls", "cat", "grep", "wc"],
    max_steps=20
)
result = await swarm.run("Review src/ for code quality issues")
print(result.output)
```

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
from aethershell import pipeline

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
| `.to_code()`             | Generate AetherShell code        |
| `.run()`                 | Execute the pipeline             |

### Generating AetherShell Code

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
from aethershell.workflows import Workflow, WorkflowStep, WorkflowPattern

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
from aethershell.workflows import MapReduceWorkflow

wf = MapReduceWorkflow(
    map_code='fn(item) => ai("Summarize: " + item)',
    reduce_code='fn(summaries) => join(summaries, "\n\n")'
)
result = await wf.run(["doc1.md", "doc2.md", "doc3.md"])
```

### Circuit Breaker

Protect against cascading failures:

```python
from aethershell.workflows import CircuitBreaker

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
from aethershell.metrics import Counter, Histogram, Timer

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
from aethershell.distributed import ServiceRegistry, ServiceInfo

registry = ServiceRegistry()

# Register a service
registry.register(ServiceInfo(
    id="worker-1",
    name="aethershell-worker",
    address="192.168.1.10",
    port=3000,
    metadata={"gpu": "true"}
))

# Discover services
workers = registry.get_services_by_name("aethershell-worker")
for w in workers:
    print(f"{w.id} at {w.address}:{w.port}")
```

### Leader Election

```python
from aethershell.distributed import LeaderElection

election = LeaderElection(service_info)

election.on_leadership_change(lambda is_leader:
    print("I am the leader!" if is_leader else "Following leader")
)

if election.is_leader():
    # Coordinate work distribution
    pass
```

## LangChain Integration

Use AetherShell tools within LangChain agents:

```python
from aethershell.langchain import AetherShellTool, AetherAgentTool

# Use AetherShell as a LangChain tool
shell_tool = AetherShellTool()
result = shell_tool.run('ls "src" | where(fn(f) => f.size > 1000)')

# Run an AetherShell agent from LangChain
agent_tool = AetherAgentTool(tools=["ls", "cat"])
result = agent_tool.run("Find TODO comments in the project")
```

## Cloud Deployment

Deploy AetherShell as serverless functions:

```python
from aethershell.cloud import LambdaRuntime, FunctionConfig

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
