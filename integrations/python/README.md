# AetherShell Python SDK

Python bindings for AetherShell - AI-powered typed shell.

## Installation

```bash
pip install aethershell
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
- **A2UI Events**: Subscribe to agent-to-user events

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

MIT License - see [LICENSE](../../LICENSE) for details.
