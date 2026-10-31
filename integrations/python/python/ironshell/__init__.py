"""
IronShell Python SDK

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
from importlib.metadata import PackageNotFoundError
from importlib.metadata import version as _distribution_version
from typing import Any, Callable, Dict, List, Optional, Union
import json
import re
import subprocess
import sys
import os


def _read_version() -> str:
    """The SDK version, from the installed distribution or the source tree.

    pyproject.toml is the one source. An installed distribution reports it
    through importlib.metadata; a bare source checkout reads it from the
    pyproject.toml two levels above this package.
    """
    try:
        return _distribution_version("ironshell")
    except PackageNotFoundError:
        pass
    pyproject = os.path.join(os.path.dirname(__file__), os.pardir, os.pardir, "pyproject.toml")
    try:
        with open(pyproject, encoding="utf-8") as f:
            match = re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.MULTILINE)
    except OSError:
        match = None
    return match.group(1) if match else "0+unknown"


__version__ = _read_version()
__all__ = [
    "IronShellRuntime",
    "Agent",
    "AgentConfig",
    "AgentResult",
    "Swarm",
    "SwarmConfig",
    "SwarmResult",
    "A2UIEvent",
    "NotificationLevel",
    "PipelineBuilder",
    "evaluate",
    "pipeline",
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
        runtime: Optional[IronShellRuntime] = None,
        system_prompt: Optional[str] = None,
    ):
        if system_prompt is not None:
            # The agent builtin's config record has no system prompt key (its
            # keys are goal, tools, max_steps, dry_run), and nothing else in the
            # shell takes one, so the prompt could never reach the model.
            raise ValueError(
                "Agent system_prompt is not supported: the agent builtin has no "
                "system prompt option, so the prompt cannot reach the model; "
                "remove the argument and put the instructions in the goal"
            )
        self.name = name
        self.model = model
        self.tools = tools or []
        self.max_steps = max_steps
        self._runtime = runtime
        self.trace: List[Dict[str, Any]] = []
    
    async def run(self, goal: str) -> AgentResult:
        """Execute the agent with a goal.

        Raises ValueError, before anything runs, for a model other than the
        default or a max_steps outside 1..=1000 (see `_build_agent_code`).
        """
        # Build the call first, so an unsupported option is refused without
        # locating or starting the ish binary.
        code = _build_agent_code(goal, self.model, self.tools, self.max_steps)

        if self._runtime is None:
            self._runtime = IronShellRuntime()

        result = self._runtime.eval(code)

        if isinstance(result, dict):
            return AgentResult(
                success=result.get("success", False),
                result=result.get("result"),
                trace=result.get("trace", []),
                steps_taken=result.get("steps", 0),
            )
        # The builtin returns its final answer as a String and reports no
        # trace or step count. A returned value means the run did not fail.
        return AgentResult(success=True, result=result, trace=[], steps_taken=0)
    
    def __repr__(self) -> str:
        return f"Agent(name={self.name!r}, model={self.model!r}, tools={self.tools})"


class Swarm:
    """Multi-agent swarm with coordination"""
    
    def __init__(
        self,
        agents: List[Agent],
        policy: str = "round_robin",
        max_iterations: int = 10,
        runtime: Optional[IronShellRuntime] = None,
    ):
        self.agents = agents
        self.policy = policy
        self.max_iterations = max_iterations
        self._runtime = runtime
    
    async def run(self, goal: str) -> SwarmResult:
        """Execute the swarm with a goal.

        Raises ValueError, before anything runs, for any option the `swarm`
        builtin cannot honour (see `_build_swarm_code`).
        """
        # Build the call first, so an unsupported option is refused without
        # locating or starting the ish binary.
        code = _build_swarm_code(self.agents, goal, self.policy, self.max_iterations)

        if self._runtime is None:
            self._runtime = IronShellRuntime()

        result = self._runtime.eval(code)

        if isinstance(result, dict):
            return SwarmResult(
                success=result.get("success", False),
                result=result.get("result"),
                blackboard=result.get("blackboard", {}),
                iterations=result.get("iterations", 0),
            )
        # The builtin returns the combined answer as a String and reports no
        # blackboard or iteration count. A returned value means the run did
        # not fail, so success is True and the other fields are empty.
        return SwarmResult(success=True, result=result, blackboard={}, iterations=0)


# Defaults that the SDK's Agent and Swarm classes start from. The `swarm`
# builtin has no model, policy or per-agent step option, so a non-default value
# for any of them is refused rather than silently dropped.
_DEFAULT_AGENT_MODEL = "openai:gpt-4o-mini"
_DEFAULT_AGENT_MAX_STEPS = 10
_DEFAULT_SWARM_POLICY = "round_robin"
# The builtin's upper bound on max_steps (MAX_AGENT_STEPS in src/builtins.rs).
_SWARM_MAX_STEPS = 1000


def _ironshell_string(value: str) -> str:
    """Render a Python str as an IronShell double-quoted string literal.

    The lexer unescapes backslash, double quote, n, r and t, so those are
    escaped here. Any other backslash sequence is not decoded (a backslash
    followed by u is read as a plain u), so no unicode escape is ever emitted:
    non-ASCII text is written as itself, which the lexer reads as characters.

    Three inputs are refused rather than emitted wrong: "${" (the lexer has no
    escape for it, and the evaluator interpolates ${...} in any string literal),
    a control character other than tab, newline or carriage return (the lexer
    has no escape for it, and NUL cannot be passed in a command line), and a
    lone surrogate (it has no UTF-8 encoding).
    """
    if not isinstance(value, str):
        raise TypeError(f"expected a str for an IronShell string, got {type(value).__name__}")
    if "${" in value:
        raise ValueError(
            "text containing '${' cannot be passed to IronShell: string literals "
            "interpolate ${...}, and the language has no escape for it"
        )
    for ch in value:
        code = ord(ch)
        if code < 0x20 and ch not in "\t\n\r":
            raise ValueError(
                f"control character U+{code:04X} cannot be passed to IronShell: "
                "the lexer has no escape for it"
            )
        if 0xD800 <= code <= 0xDFFF:
            raise ValueError(
                f"lone surrogate U+{code:04X} cannot be passed to IronShell: "
                "it has no UTF-8 encoding"
            )
    escaped = (
        value.replace("\\", "\\\\")
        .replace('"', '\\"')
        .replace("\n", "\\n")
        .replace("\r", "\\r")
        .replace("\t", "\\t")
    )
    return f'"{escaped}"'


_IDENTIFIER = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")


def _ironshell_value(value: Any) -> str:
    """Render a Python value as an IronShell literal.

    Strings go through _ironshell_string. Lists become arrays, and dicts become
    records whose keys must be identifiers, since the parser reads record keys
    as bare names. None, True and False become null, true and false.
    """
    if value is None:
        return "null"
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, str):
        return _ironshell_string(value)
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if value != value or value in (float("inf"), float("-inf")):
            raise ValueError(f"{value!r} has no IronShell literal")
        return repr(value)
    if isinstance(value, (list, tuple)):
        return "[" + ", ".join(_ironshell_value(v) for v in value) + "]"
    if isinstance(value, dict):
        fields = []
        for key, item in value.items():
            if not isinstance(key, str) or not _IDENTIFIER.match(key):
                raise ValueError(
                    f"record key {key!r} is not an identifier; IronShell record keys are bare names"
                )
            fields.append(f"{key}: {_ironshell_value(item)}")
        return "{" + ", ".join(fields) + "}"
    raise TypeError(f"{type(value).__name__} has no IronShell literal")


def _check_steps(value: int, owner: str) -> int:
    """Return value if it is a legal step count for the builtin, else raise ValueError."""
    if not isinstance(value, int) or isinstance(value, bool) or not 1 <= value <= _SWARM_MAX_STEPS:
        raise ValueError(
            f"{owner} must be an int in 1..={_SWARM_MAX_STEPS} "
            f"(the builtin's max_steps), got {value!r}"
        )
    return value


def _build_agent_code(goal: str, model: str, tools: List[str], max_steps: int) -> str:
    """Build the IronShell call that runs one agent.

    Produces the record form of the builtin, `agent({goal, tools, max_steps})`.
    Raises ValueError for a model other than the default, since the builtin has
    no model option, and for a max_steps outside 1..=1000.
    """
    if model != _DEFAULT_AGENT_MODEL:
        raise ValueError(
            f"Agent model {model!r} is not supported: the agent builtin has no model "
            f"option; leave the model at {_DEFAULT_AGENT_MODEL!r}"
        )
    steps = _check_steps(max_steps, "max_steps")
    tools_str = ", ".join(_ironshell_string(t) for t in tools)
    return (
        f"agent({{goal: {_ironshell_string(goal)}, tools: [{tools_str}], "
        f"max_steps: {steps}}})"
    )


def _build_swarm_code(
    agents: List[Agent],
    goal: str,
    policy: str,
    max_iterations: int,
) -> str:
    """Build the IronShell call that runs a swarm.

    Produces the record form of the builtin, `swarm({goal, tools, max_steps})`.
    `tools` is the union of the agents' tools, in first-seen order. Raises
    ValueError for an option the builtin cannot honour: a policy other than
    round_robin, a per-agent model or max_steps other than the SDK defaults,
    or a max_iterations outside 1..=1000.
    """
    if policy != _DEFAULT_SWARM_POLICY:
        raise ValueError(
            f"Swarm policy {policy!r} is not supported: the swarm builtin has no "
            f"policy option and always runs {_DEFAULT_SWARM_POLICY!r}"
        )
    if (
        not isinstance(max_iterations, int)
        or isinstance(max_iterations, bool)
        or not 1 <= max_iterations <= _SWARM_MAX_STEPS
    ):
        raise ValueError(
            f"max_iterations must be an int in 1..={_SWARM_MAX_STEPS} "
            f"(the swarm builtin's max_steps), got {max_iterations!r}"
        )

    tools: List[str] = []
    for agent in agents:
        if agent.model != _DEFAULT_AGENT_MODEL:
            raise ValueError(
                f"Agent {agent.name!r} sets model {agent.model!r}, but the swarm builtin "
                "has no per-agent model option; leave the model at its default"
            )
        if agent.max_steps != _DEFAULT_AGENT_MAX_STEPS:
            raise ValueError(
                f"Agent {agent.name!r} sets max_steps={agent.max_steps!r}, but the swarm "
                "builtin has no per-agent step limit; use max_iterations on the swarm"
            )
        for tool in agent.tools:
            if tool not in tools:
                tools.append(tool)

    tools_str = ", ".join(_ironshell_string(t) for t in tools)
    return (
        f"swarm({{goal: {_ironshell_string(goal)}, tools: [{tools_str}], "
        f"max_steps: {max_iterations}}})"
    )


class IronShellRuntime:
    """Main runtime for IronShell execution"""
    
    def __init__(self, ish_path: Optional[str] = None):
        """
        Initialize the IronShell runtime.
        
        Args:
            ish_path: Path to the ish binary. If None, searches PATH.
        """
        self._ish_path = ish_path or self._find_ish()
        self._event_listeners: List[Callable[[A2UIEvent], None]] = []
    
    def _find_ish(self) -> str:
        """Find the ish binary in PATH or common locations"""
        # Check PATH
        for path in os.environ.get("PATH", "").split(os.pathsep):
            ae = os.path.join(path, "ish.exe" if sys.platform == "win32" else "ish")
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
                r"C:\Program Files\IronShell\ish.exe",
                os.path.expanduser(r"~\.cargo\bin\ish.exe"),
            ])
        
        for path in common_paths:
            if os.path.isfile(path):
                return path
        
        raise RuntimeError(
            "Could not find 'ish' binary. Please install IronShell or "
            "provide the path explicitly via ish_path parameter."
        )
    
    def eval(self, code: str) -> Any:
        """
        Evaluate IronShell code and return the result.
        
        Args:
            code: IronShell code to evaluate
            
        Returns:
            Parsed result (Python object)
        """
        # `-c/--command`, not `-e`, and `--deterministic` for canonical JSON.
        # The previous flags (`-e ... --json`) are not accepted by the binary:
        # clap rejected `-e` outright, so every call raised RuntimeError. The
        # SDK's core entry point could never have worked against a released
        # build.
        result = subprocess.run(
            [self._ish_path, "-c", code, "--deterministic"],
            capture_output=True,
            text=True,
            timeout=60,
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"IronShell error: {result.stderr}")
        
        try:
            return json.loads(result.stdout)
        except json.JSONDecodeError:
            return result.stdout.strip()
    
    def eval_file(self, path: str) -> Any:
        """
        Evaluate an IronShell file.
        
        Args:
            path: Path to .ish file
            
        Returns:
            Result of evaluation
        """
        result = subprocess.run(
            [self._ish_path, path, "--deterministic"],
            capture_output=True,
            text=True,
            timeout=300,
        )
        
        if result.returncode != 0:
            raise RuntimeError(f"IronShell error: {result.stderr}")
        
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
            model: Must be the default "openai:gpt-4o-mini". The agent builtin
                has no model option, so any other value raises ValueError when
                the agent runs.
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
            policy: Must be "round_robin". The swarm builtin has no policy
                option, so any other value raises ValueError when the swarm runs.
            max_iterations: Maximum coordination iterations, 1 to 1000

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
            policy: Must be "round_robin" (see create_swarm)
            max_iterations: Maximum iterations, 1 to 1000
            
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
    Convenience function to evaluate IronShell code.
    
    Args:
        code: IronShell code to evaluate
        
    Returns:
        Result
    """
    runtime = IronShellRuntime()
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
        # The shell calls this `where`; there is no `filter` builtin.
        self._ops.append(f"where({fn})")
        return self
    
    def reduce(self, fn: str, initial: Any) -> PipelineBuilder:
        self._ops.append(f"reduce({fn}, {_ironshell_value(initial)})")
        return self
    
    def sort(self, fn: Optional[str] = None) -> PipelineBuilder:
        # `fn` is a key function, `fn(x) => key`: `sort` takes no argument, and
        # `sort_by` orders by the key it returns.
        self._ops.append(f"sort_by({fn})" if fn else "sort()")
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
        """Get the pipeline as IronShell code"""
        return f"{_ironshell_value(self._data)} | {' | '.join(self._ops)}"
    
    def run(self, runtime: Optional[IronShellRuntime] = None) -> Any:
        """Execute the pipeline"""
        rt = runtime or IronShellRuntime()
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
