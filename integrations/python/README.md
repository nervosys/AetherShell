# AetherShell Python SDK

Python bindings for AetherShell - AI-powered typed shell with workflow orchestration and cloud deployment.

## Installation

> **⚠ Not published yet — do not run `pip install aethershell`.**
>
> As of 2026-08-06 this package is **not on PyPI** and the name is
> **unregistered**. `pip install aethershell` would not install this SDK; it
> would install whatever a third party has uploaded under that name, and pip
> executes package code at install time. Install from source until this notice
> is removed.

```bash
# Core SDK, from a checkout of this repository
pip install ./integrations/python

# With LangChain integration
pip install "./integrations/python[langchain]"

# With cloud deployment support
pip install "./integrations/python[cloud]"

# Everything
pip install "./integrations/python[all]"
```

## Quick Start

```python
from aethershell import AetherRuntime, Agent

# Create runtime
runtime = AetherRuntime()

# Evaluate AetherShell code
result = runtime.eval('[1, 2, 3] | map(fn(x) => x * 2)')
print(result)  # [2, 4, 6]

# Create and run an agent
agent = runtime.create_agent(
    name="researcher",
    model="openai:gpt-4o",
    tools=["http_get", "search"]
)

result = await agent.run("Find the latest Python release version")
print(result)
```

## Features

- **Evaluation**: Execute AetherShell code from Python
- **Pipelines**: Process data with typed pipelines
- **Agents**: Create and orchestrate AI agents
- **Swarms**: Multi-agent coordination
- **Workflows**: MapReduce, Saga, Fan-Out patterns
- **Metrics**: Prometheus metrics, tracing, health checks
- **Distributed**: Service discovery, leader election, load balancing
- **Cloud**: Deploy as serverless functions (AWS, Azure, GCP, K8s)
- **LangChain**: Full LangChain tool integration

## Workflow Orchestration

```python
from aethershell.workflows import (
    MapReduceWorkflow,
    SagaWorkflow,
    PipelineWorkflow,
    CircuitBreaker,
)

# MapReduce for parallel processing
workflow = MapReduceWorkflow(
    name="word_count",
    map_fn=lambda text: len(text.split()),
    reduce_fn=lambda a, b: a + b,
)
result = await workflow.run(["hello world", "foo bar baz"])
print(result.result)  # 5

# Saga with compensation
saga = SagaWorkflow("order")
saga.add_saga_step("reserve", reserve_inventory, rollback_reservation)
saga.add_saga_step("charge", charge_payment, refund_payment)
saga.add_saga_step("ship", ship_order, cancel_shipment)
result = await saga.run(order_data)

# Circuit breaker for fault tolerance
breaker = CircuitBreaker(name="api", failure_threshold=5)
result = breaker.call(lambda: api_request())
```

## Metrics & Observability

```python
from aethershell.metrics import (
    MetricsCollector,
    Counter,
    Gauge,
    Histogram,
    Tracer,
)

# Create metrics
collector = MetricsCollector(namespace="myapp")
requests = collector.counter("requests_total")
latency = collector.histogram("request_latency_seconds")

# Track metrics
requests.inc()
latency.observe(0.125)

# Export to Prometheus
print(collector.to_prometheus())

# Distributed tracing
tracer = collector.tracer("my-service")
with tracer.start_span("handle_request") as span:
    span.set_attribute("user_id", "123")
    # ... process request
```

## Distributed Agents

```python
from aethershell.distributed import (
    ServiceRegistry,
    LeaderElection,
    LoadBalancer,
    DistributedSwarm,
)

# Service discovery
registry = ServiceRegistry()
registry.register("agent-nlp", "host1", 8080)
registry.register("agent-nlp", "host2", 8080)

# Load balancing
lb = LoadBalancer(registry, strategy="round_robin")
service = lb.select_service("agent-nlp")

# Leader election
election = LeaderElection("node-1", registry, "cluster")
await election.run_election()
if election.is_leader:
    print("I am the leader!")

# Distributed swarm
swarm = DistributedSwarm("my-swarm", registry)
swarm.add_local_agent(my_agent)
result = await swarm.dispatch(goal="analyze data", capability="nlp")
```

## Cloud Deployment

Deploy agents as serverless functions:

```python
from aethershell.cloud import (
    CloudProvider,
    FunctionConfig,
    DeploymentConfig,
    deploy_agent,
)

# Configure function
config = DeploymentConfig(
    provider=CloudProvider.AWS_LAMBDA,
    region="us-east-1",
    function_config=FunctionConfig(
        name="my-agent",
        memory_mb=512,
        timeout_seconds=60,
    ),
)

# Generate deployment files
agent_code = '''
def create_agent(runtime):
    return Agent(name="analyst", model="openai:gpt-4o", runtime=runtime)
'''

files = deploy_agent(config, agent_code, output_dir="./deploy")
# Creates: handler.py, template.yaml, samconfig.toml, requirements.txt
```

Supported platforms:
- **AWS Lambda** (SAM template)
- **Azure Functions** (Bicep)
- **GCP Cloud Functions** (Terraform)
- **Kubernetes/Knative** (manifests + Skaffold)

## LangChain Integration

```python
from aethershell.langchain import (
    get_all_aethershell_tools,
    AetherWorkflowTool,
    AetherMapReduceTool,
    AetherMetricsTool,
)

# Get all tools for LangChain agent
tools = get_all_aethershell_tools()

# Use with LangChain
from langchain.agents import initialize_agent
agent = initialize_agent(tools, llm, agent="zero-shot-react-description")
agent.run("Process this data with MapReduce: [1,2,3,4,5]")
```

## API Reference

### AetherRuntime

```python
runtime = AetherRuntime()

# Evaluate code
result = runtime.eval(code: str) -> Any

# Create agent
agent = runtime.create_agent(
    name: str,
    model: str = "openai:gpt-4o-mini",
    tools: List[str] = [],
    max_steps: int = 10
) -> Agent

# Run swarm
result = await runtime.run_swarm(
    agents: List[Agent],
    goal: str,
    policy: str = "round_robin",
    max_iterations: int = 10
) -> SwarmResult
```

### Agent

```python
agent = Agent(name="agent1", model="openai:gpt-4o")

# Run agent
result = await agent.run(goal: str) -> AgentResult

# Get trace
trace = agent.trace  # List of steps taken
```

### A2UI Events

```python
# Subscribe to events
def on_event(event: A2UIEvent):
    print(f"Event: {event.type}")

runtime.subscribe_a2ui(on_event)

# Event types
# - Notify: Notifications
# - Progress: Progress updates
# - Prompt: User prompts
# - AgentThinking: Agent reasoning
```

## Development

```bash
# Clone repository
git clone https://github.com/nervosys/AetherShell.git
cd AetherShell/integrations/python

# Install development dependencies
pip install -e ".[dev]"

# Run tests
pytest

# Build package
python -m build
```

## License

AGPL-3.0-or-later with commercial dual-license option — see [LICENSE](../../LICENSE) for details.
