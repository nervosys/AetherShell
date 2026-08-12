"""
AetherShell Python SDK

AI-powered typed shell with agent orchestration capabilities.

Modules:
    - workflows: Workflow patterns (MapReduce, Saga, Pipeline, Fan-Out)
    - metrics: Observability (Prometheus metrics, tracing, health checks)
    - distributed: Distributed agents (service registry, leader election, routing)
    - langchain: LangChain integration tools
"""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, Union
import json
import subprocess
import sys
import os

__version__ = "1.5.1"
__all__ = [
    # Core
    "AetherRuntime",
    "Agent",
    "AgentConfig",
    "AgentResult",
    "Swarm",
    "SwarmConfig",
    "SwarmResult",
    "A2UIEvent",
    "NotificationLevel",
    "evaluate",
    "pipeline",
    # Submodules
    "workflows",
    "metrics",
    "distributed",
    "langchain",
]


class NotificationLevel(Enum):
    """A2UI notification levels"""
    INFO = "info"
    SUCCESS = "success"
    WARNING = "warning"
    ERROR = "error"


@dataclass
class A2UIEvent:
    """Event from A2UI protocol"""
    id: str
    timestamp: str
    priority: str
    event_type: str
    data: Dict[str, Any]


@dataclass
class AgentConfig:
    """Configuration for an agent"""
    name: str
    model: str = "openai:gpt-4o-mini"
    tools: List[str] = field(default_factory=list)
    max_steps: int = 10
    dry_run: bool = False


@dataclass
class AgentResult:
    """Result from agent execution"""
    success: bool
    result: Any
    trace: List[Dict[str, Any]]
    steps_taken: int


@dataclass
class SwarmConfig:
    """Configuration for a swarm"""
    agents: List[Agent]
    policy: str = "round_robin"
    max_iterations: int = 10


@dataclass
class SwarmResult:
    """Result from swarm execution"""
    success: bool
    result: Any
    blackboard: Dict[str, Any]
    iterations: int


class Agent:
    """AI Agent with tool execution capabilities"""
    
    def __init__(
        self,
        name: str,
        model: str = "openai:gpt-4o-mini",
        tools: Optional[List[str]] = None,
        max_steps: int = 10,
        runtime: Optional[AetherRuntime] = None,
        system_prompt: Optional[str] = None,
    ):
        self.name = name
        self.model = model
        self.tools = tools or []
        self.max_steps = max_steps
        self._runtime = runtime
        self.trace: List[Dict[str, Any]] = []
    
    async def run(self, goal: str) -> AgentResult:
        """Execute the agent with a goal"""
        if self._runtime is None:
            self._runtime = AetherRuntime()
        
        # Build AetherShell agent command
        tools_str = ", ".join(f'"{t}"' for t in self.tools)
        code = f'''agent("{self.model}", "{goal}", [{tools_str}], {self.max_steps})'''
        
        result = self._runtime.eval(code)
        
        return AgentResult(
            success=result.get("success", False),
            result=result.get("result"),
            trace=result.get("trace", []),
            steps_taken=result.get("steps", 0),
        )
    
    def __repr__(self) -> str:
        return f"Agent(name={self.name!r}, model={self.model!r}, tools={self.tools})"


class Swarm:
    """Multi-agent swarm with coordination"""
    
    def __init__(
        self,
        agents: List[Agent],
        policy: str = "round_robin",
        max_iterations: int = 10,
        runtime: Optional[AetherRuntime] = None,
    ):
        self.agents = agents
        self.policy = policy
        self.max_iterations = max_iterations
        self._runtime = runtime
    
    async def run(self, goal: str) -> SwarmResult:
        """Execute the swarm with a goal"""
        if self._runtime is None:
            self._runtime = AetherRuntime()
        
        # Build swarm configuration
        agents_config = []
        for agent in self.agents:
            tools_str = ", ".join(f'"{t}"' for t in agent.tools)
            agents_config.append(
                f'{{name: "{agent.name}", model: "{agent.model}", tools: [{tools_str}]}}'
            )
        
        agents_str = ", ".join(agents_config)
        code = f'''swarm([{agents_str}], "{goal}", {{policy: "{self.policy}", max_iters: {self.max_iterations}}})'''
        
        result = self._runtime.eval(code)
        
        return SwarmResult(
            success=result.get("success", False),
            result=result.get("result"),
            blackboard=result.get("blackboard", {}),
            iterations=result.get("iterations", 0),
        )


class AetherRuntime:
    """Main runtime for AetherShell execution"""
    
    def __init__(self, ae_path: Optional[str] = None):
        """
        Initialize the AetherShell runtime.
        
        Args:
            ae_path: Path to the ae binary. If None, searches PATH.
        """
        self._ae_path = ae_path or self._find_ae()
        self._event_listeners: List[Callable[[A2UIEvent], None]] = []
    
    def _find_ae(self) -> str:
        """Find the ae binary in PATH or common locations"""
        # Check PATH
        for path in os.environ.get("PATH", "").split(os.pathsep):
            ae = os.path.join(path, "ae.exe" if sys.platform == "win32" else "ae")
            if os.path.isfile(ae):
                return ae
        
        # Check common locations
        common_paths = [
            "/usr/local/bin/ae",
            "/usr/bin/ae",
            os.path.expanduser("~/.cargo/bin/ae"),
            os.path.expanduser("~/bin/ae"),
        ]
        if sys.platform == "win32":
            common_paths.extend([
                r"C:\Program Files\AetherShell\ae.exe",
                os.path.expanduser(r"~\.cargo\bin\ae.exe"),
            ])
        
        for path in common_paths:
            if os.path.isfile(path):
                return path
        
        raise RuntimeError(
            "Could not find 'ae' binary. Please install AetherShell or "
            "provide the path explicitly via ae_path parameter."
        )
    
    def eval(self, code: str) -> Any:
        """
        Evaluate AetherShell code and return the result.
        
        Args:
            code: AetherShell code to evaluate
            
        Returns:
            Parsed result (Python object)
        """
        # `-c/--command`, not `-e`, and `--deterministic` for canonical JSON.
        # The previous flags (`-e ... --json`) are not accepted by the binary:
        # clap rejected `-e` outright, so every call raised RuntimeError. The
        # SDK's core entry point could never have worked against a released
        # build.
        result = subprocess.run(
            [self._ae_path, "-c", code, "--deterministic"],
            capture_output=True,
            text=True,
            timeout=60,
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"AetherShell error: {result.stderr}")
        
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return result.stdout.strip()
    
    def eval_file(self, path: str) -> Any:
        """
        Evaluate an AetherShell file.
        
        Args:
            path: Path to .ae file
            
        Returns:
            Result of evaluation
        """
        result = subprocess.run(
            [self._ae_path, path, "--deterministic"],
            capture_output=True,
            text=True,
            timeout=300,
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"AetherShell error: {result.stderr}")
        
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return result.stdout.strip()
    
    def create_agent(
        self,
        name: str,
        model: str = "openai:gpt-4o-mini",
        tools: Optional[List[str]] = None,
        max_steps: int = 10,
    ) -> Agent:
        """
        Create an AI agent.
        
        Args:
            name: Agent identifier
            model: Model URI (e.g., "openai:gpt-4o", "ollama:llama3")
            tools: List of tool names the agent can use
            max_steps: Maximum reasoning steps
            
        Returns:
            Agent instance
        """
        return Agent(
            name=name,
            model=model,
            tools=tools or [],
            max_steps=max_steps,
            runtime=self,
        )
    
    def create_swarm(
        self,
        agents: List[Agent],
        policy: str = "round_robin",
        max_iterations: int = 10,
    ) -> Swarm:
        """
        Create a multi-agent swarm.
        
        Args:
            agents: List of agents in the swarm
            policy: Coordination policy ("round_robin", "router")
            max_iterations: Maximum coordination iterations
            
        Returns:
            Swarm instance
        """
        return Swarm(
            agents=agents,
            policy=policy,
            max_iterations=max_iterations,
            runtime=self,
        )
    
    async def run_swarm(
        self,
        agents: List[Agent],
        goal: str,
        policy: str = "round_robin",
        max_iterations: int = 10,
    ) -> SwarmResult:
        """
        Run a multi-agent swarm with a goal.
        
        Args:
            agents: List of agents
            goal: Goal for the swarm
            policy: Coordination policy
            max_iterations: Maximum iterations
            
        Returns:
            SwarmResult
        """
        swarm = self.create_swarm(agents, policy, max_iterations)
        return await swarm.run(goal)
    
    def subscribe_a2ui(
        self,
        callback: Callable[[A2UIEvent], None],
    ) -> Callable[[], None]:
        """
        Subscribe to A2UI events.
        
        Args:
            callback: Function called for each event
            
        Returns:
            Unsubscribe function
        """
        self._event_listeners.append(callback)
        
        def unsubscribe():
            self._event_listeners.remove(callback)
        
        return unsubscribe
    
    def _emit_event(self, event: A2UIEvent) -> None:
        """Emit an event to all listeners"""
        for listener in self._event_listeners:
            try:
                listener(event)
            except Exception as e:
                print(f"A2UI event listener error: {e}", file=sys.stderr)


def evaluate(code: str) -> Any:
    """
    Convenience function to evaluate AetherShell code.
    
    Args:
        code: AetherShell code to evaluate
        
    Returns:
        Result
    """
    runtime = AetherRuntime()
    return runtime.eval(code)


class PipelineBuilder:
    """Fluent API for building pipelines"""
    
    def __init__(self, data: Any):
        self._data = data
        self._ops: List[str] = []
    
    def map(self, fn: str) -> PipelineBuilder:
        self._ops.append(f"map({fn})")
        return self
    
    def filter(self, fn: str) -> PipelineBuilder:
        self._ops.append(f"filter({fn})")
        return self
    
    def reduce(self, fn: str, initial: Any) -> PipelineBuilder:
        self._ops.append(f"reduce({fn}, {json.dumps(initial)})")
        return self
    
    def sort(self, fn: Optional[str] = None) -> PipelineBuilder:
        self._ops.append(f"sort({fn})" if fn else "sort()")
        return self
    
    def reverse(self) -> PipelineBuilder:
        self._ops.append("reverse()")
        return self
    
    def flatten(self) -> PipelineBuilder:
        self._ops.append("flatten()")
        return self
    
    def unique(self) -> PipelineBuilder:
        self._ops.append("unique()")
        return self
    
    def take(self, n: int) -> PipelineBuilder:
        self._ops.append(f"slice(0, {n})")
        return self
    
    def skip(self, n: int) -> PipelineBuilder:
        self._ops.append(f"slice({n})")
        return self
    
    def to_code(self) -> str:
        """Get the pipeline as AetherShell code"""
        return f"{json.dumps(self._data)} | {' | '.join(self._ops)}"
    
    def run(self, runtime: Optional[AetherRuntime] = None) -> Any:
        """Execute the pipeline"""
        rt = runtime or AetherRuntime()
        return rt.eval(self.to_code())


def pipeline(data: Any) -> PipelineBuilder:
    """
    Create a pipeline builder for fluent data processing.
    
    Args:
        data: Initial data
        
    Returns:
        PipelineBuilder for chaining operations
        
    Example:
        result = (
            pipeline([1, 2, 3, 4, 5])
            .map("fn(x) => x * 2")
            .filter("fn(x) => x > 4")
            .run()
        )
        # [6, 8, 10]
    """
    return PipelineBuilder(data)
