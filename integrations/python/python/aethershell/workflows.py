"""
AetherShell Workflow Integration for Python

Provides workflow templates and orchestration capabilities:
- MapReduce, Saga, Fan-Out/Fan-In, Pipeline patterns
- Circuit breaker for fault tolerance
- Workflow state persistence
"""

from __future__ import annotations

import asyncio
import json
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable, Dict, List, Optional, TypeVar, Generic
from datetime import datetime

from . import AetherRuntime

__all__ = [
    "Workflow",
    "WorkflowStep",
    "WorkflowResult",
    "WorkflowPattern",
    "MapReduceWorkflow",
    "SagaWorkflow",
    "SagaStep",
    "FanOutWorkflow",
    "PipelineWorkflow",
    "CircuitBreaker",
    "CircuitState",
]

T = TypeVar("T")


class WorkflowPattern(Enum):
    """Workflow execution patterns"""
    MAP_REDUCE = "map_reduce"
    SAGA = "saga"
    FAN_OUT_FAN_IN = "fan_out_fan_in"
    PIPELINE = "pipeline"
    SCATTER_GATHER = "scatter_gather"


class CircuitState(Enum):
    """Circuit breaker states"""
    CLOSED = "closed"
    OPEN = "open"
    HALF_OPEN = "half_open"


@dataclass
class WorkflowStep:
    """A step in a workflow"""
    name: str
    action: Callable[..., Any]
    compensate: Optional[Callable[..., Any]] = None
    timeout_ms: int = 30000
    retry_count: int = 0
    

@dataclass
class WorkflowResult:
    """Result of workflow execution"""
    success: bool
    result: Any
    steps_completed: int
    total_steps: int
    duration_ms: float
    errors: List[str] = field(default_factory=list)


@dataclass 
class CircuitBreaker:
    """
    Circuit breaker for fault tolerance.
    
    Prevents cascading failures by stopping calls to a failing service
    after a threshold of failures is reached.
    """
    name: str
    failure_threshold: int = 5
    reset_timeout_ms: int = 30000
    _state: CircuitState = CircuitState.CLOSED
    _failures: int = 0
    _last_failure_time: Optional[datetime] = None
    _runtime: Optional[AetherRuntime] = None
    
    def __post_init__(self):
        if self._runtime is None:
            self._runtime = AetherRuntime()
    
    @property
    def state(self) -> CircuitState:
        """Get current circuit state"""
        if self._state == CircuitState.OPEN:
            # Check if reset timeout has passed
            if self._last_failure_time:
                elapsed = (datetime.now() - self._last_failure_time).total_seconds() * 1000
                if elapsed >= self.reset_timeout_ms:
                    self._state = CircuitState.HALF_OPEN
        return self._state
    
    def call(self, func: Callable[..., T], *args, **kwargs) -> T:
        """
        Execute function through circuit breaker.
        
        Args:
            func: Function to call
            *args, **kwargs: Arguments to pass
            
        Returns:
            Function result
            
        Raises:
            RuntimeError: If circuit is open
        """
        if self.state == CircuitState.OPEN:
            raise RuntimeError(f"Circuit breaker '{self.name}' is open")
        
        try:
            result = func(*args, **kwargs)
            self._on_success()
            return result
        except Exception as e:
            self._on_failure()
            raise
    
    async def call_async(self, func: Callable[..., T], *args, **kwargs) -> T:
        """Async version of call"""
        if self.state == CircuitState.OPEN:
            raise RuntimeError(f"Circuit breaker '{self.name}' is open")
        
        try:
            if asyncio.iscoroutinefunction(func):
                result = await func(*args, **kwargs)
            else:
                result = func(*args, **kwargs)
            self._on_success()
            return result
        except Exception as e:
            self._on_failure()
            raise
    
    def _on_success(self):
        """Handle successful call"""
        self._failures = 0
        self._state = CircuitState.CLOSED
    
    def _on_failure(self):
        """Handle failed call"""
        self._failures += 1
        self._last_failure_time = datetime.now()
        
        if self._failures >= self.failure_threshold:
            self._state = CircuitState.OPEN
    
    def reset(self):
        """Manually reset the circuit breaker"""
        self._failures = 0
        self._state = CircuitState.CLOSED
        self._last_failure_time = None


class Workflow:
    """Base workflow class"""
    
    def __init__(
        self,
        name: str,
        runtime: Optional[AetherRuntime] = None,
    ):
        self.name = name
        self._runtime = runtime or AetherRuntime()
        self._steps: List[WorkflowStep] = []
    
    def add_step(self, step: WorkflowStep) -> Workflow:
        """Add a step to the workflow"""
        self._steps.append(step)
        return self
    
    async def run(self, input_data: Any) -> WorkflowResult:
        """Execute the workflow"""
        raise NotImplementedError("Subclasses must implement run()")


class MapReduceWorkflow(Workflow):
    """
    MapReduce workflow pattern.
    
    Splits data into chunks, processes in parallel, then combines results.
    """
    
    def __init__(
        self,
        name: str,
        map_fn: Callable[[Any], Any],
        reduce_fn: Callable[[Any, Any], Any],
        chunk_size: int = 100,
        runtime: Optional[AetherRuntime] = None,
    ):
        super().__init__(name, runtime)
        self.map_fn = map_fn
        self.reduce_fn = reduce_fn
        self.chunk_size = chunk_size
    
    async def run(self, input_data: List[Any]) -> WorkflowResult:
        """Execute MapReduce on input data"""
        start_time = datetime.now()
        errors: List[str] = []
        
        try:
            # Split into chunks
            chunks = [
                input_data[i:i + self.chunk_size]
                for i in range(0, len(input_data), self.chunk_size)
            ]
            
            # Map phase (parallel)
            map_tasks = [
                asyncio.create_task(self._map_chunk(chunk, idx))
                for idx, chunk in enumerate(chunks)
            ]
            mapped_results = await asyncio.gather(*map_tasks, return_exceptions=True)
            
            # Handle map errors
            valid_results = []
            for i, result in enumerate(mapped_results):
                if isinstance(result, Exception):
                    errors.append(f"Map chunk {i} failed: {result}")
                else:
                    valid_results.extend(result)
            
            # Reduce phase
            if valid_results:
                reduced = valid_results[0]
                for item in valid_results[1:]:
                    reduced = self.reduce_fn(reduced, item)
            else:
                reduced = None
            
            duration = (datetime.now() - start_time).total_seconds() * 1000
            
            return WorkflowResult(
                success=len(errors) == 0,
                result=reduced,
                steps_completed=len(chunks),
                total_steps=len(chunks),
                duration_ms=duration,
                errors=errors,
            )
        except Exception as e:
            duration = (datetime.now() - start_time).total_seconds() * 1000
            return WorkflowResult(
                success=False,
                result=None,
                steps_completed=0,
                total_steps=len(input_data) // self.chunk_size,
                duration_ms=duration,
                errors=[str(e)],
            )
    
    async def _map_chunk(self, chunk: List[Any], idx: int) -> List[Any]:
        """Map a single chunk"""
        return [self.map_fn(item) for item in chunk]


@dataclass
class SagaStep:
    """A step in a saga with compensation"""
    name: str
    action: Callable[..., Any]
    compensate: Callable[..., Any]
    

class SagaWorkflow(Workflow):
    """
    Saga pattern for distributed transactions.
    
    Executes steps in sequence. If any step fails, runs compensation
    actions for all completed steps in reverse order.
    """
    
    def __init__(
        self,
        name: str,
        steps: Optional[List[SagaStep]] = None,
        runtime: Optional[AetherRuntime] = None,
    ):
        super().__init__(name, runtime)
        self.saga_steps = steps or []
    
    def add_saga_step(
        self,
        name: str,
        action: Callable[..., Any],
        compensate: Callable[..., Any],
    ) -> SagaWorkflow:
        """Add a step with compensation"""
        self.saga_steps.append(SagaStep(name, action, compensate))
        return self
    
    async def run(self, input_data: Any) -> WorkflowResult:
        """Execute the saga"""
        start_time = datetime.now()
        completed_steps: List[SagaStep] = []
        results: List[Any] = []
        errors: List[str] = []
        current_data = input_data
        
        try:
            for step in self.saga_steps:
                try:
                    if asyncio.iscoroutinefunction(step.action):
                        result = await step.action(current_data)
                    else:
                        result = step.action(current_data)
                    
                    results.append(result)
                    completed_steps.append(step)
                    current_data = result
                except Exception as e:
                    errors.append(f"Step '{step.name}' failed: {e}")
                    
                    # Run compensations in reverse
                    for comp_step in reversed(completed_steps):
                        try:
                            if asyncio.iscoroutinefunction(comp_step.compensate):
                                await comp_step.compensate(current_data)
                            else:
                                comp_step.compensate(current_data)
                        except Exception as comp_error:
                            errors.append(f"Compensation '{comp_step.name}' failed: {comp_error}")
                    
                    break
            
            duration = (datetime.now() - start_time).total_seconds() * 1000
            
            return WorkflowResult(
                success=len(errors) == 0,
                result=current_data if len(errors) == 0 else None,
                steps_completed=len(completed_steps),
                total_steps=len(self.saga_steps),
                duration_ms=duration,
                errors=errors,
            )
        except Exception as e:
            duration = (datetime.now() - start_time).total_seconds() * 1000
            return WorkflowResult(
                success=False,
                result=None,
                steps_completed=len(completed_steps),
                total_steps=len(self.saga_steps),
                duration_ms=duration,
                errors=[str(e)],
            )


class FanOutWorkflow(Workflow):
    """
    Fan-Out/Fan-In pattern.
    
    Dispatches work to multiple workers in parallel, then gathers results.
    """
    
    def __init__(
        self,
        name: str,
        workers: List[Callable[[Any], Any]],
        gather_fn: Callable[[List[Any]], Any],
        timeout_ms: int = 30000,
        runtime: Optional[AetherRuntime] = None,
    ):
        super().__init__(name, runtime)
        self.workers = workers
        self.gather_fn = gather_fn
        self.timeout_ms = timeout_ms
    
    async def run(self, input_data: Any) -> WorkflowResult:
        """Execute fan-out/fan-in"""
        start_time = datetime.now()
        errors: List[str] = []
        
        try:
            # Fan-out: dispatch to all workers
            tasks = []
            for idx, worker in enumerate(self.workers):
                if asyncio.iscoroutinefunction(worker):
                    tasks.append(asyncio.create_task(worker(input_data)))
                else:
                    tasks.append(asyncio.create_task(asyncio.to_thread(worker, input_data)))
            
            # Wait with timeout
            timeout_secs = self.timeout_ms / 1000
            done, pending = await asyncio.wait(
                tasks,
                timeout=timeout_secs,
                return_when=asyncio.ALL_COMPLETED,
            )
            
            # Cancel pending tasks
            for task in pending:
                task.cancel()
                errors.append(f"Worker timed out after {self.timeout_ms}ms")
            
            # Gather results
            results = []
            for task in done:
                try:
                    results.append(task.result())
                except Exception as e:
                    errors.append(f"Worker failed: {e}")
            
            # Fan-in: combine results
            final_result = self.gather_fn(results) if results else None
            
            duration = (datetime.now() - start_time).total_seconds() * 1000
            
            return WorkflowResult(
                success=len(errors) == 0,
                result=final_result,
                steps_completed=len(done),
                total_steps=len(self.workers),
                duration_ms=duration,
                errors=errors,
            )
        except Exception as e:
            duration = (datetime.now() - start_time).total_seconds() * 1000
            return WorkflowResult(
                success=False,
                result=None,
                steps_completed=0,
                total_steps=len(self.workers),
                duration_ms=duration,
                errors=[str(e)],
            )


class PipelineWorkflow(Workflow):
    """
    Pipeline pattern.
    
    Passes data through a series of processing stages sequentially.
    """
    
    def __init__(
        self,
        name: str,
        stages: Optional[List[Callable[[Any], Any]]] = None,
        runtime: Optional[AetherRuntime] = None,
    ):
        super().__init__(name, runtime)
        self.stages = stages or []
    
    def add_stage(self, stage: Callable[[Any], Any]) -> PipelineWorkflow:
        """Add a processing stage"""
        self.stages.append(stage)
        return self
    
    async def run(self, input_data: Any) -> WorkflowResult:
        """Execute the pipeline"""
        start_time = datetime.now()
        current_data = input_data
        completed = 0
        errors: List[str] = []
        
        try:
            for idx, stage in enumerate(self.stages):
                try:
                    if asyncio.iscoroutinefunction(stage):
                        current_data = await stage(current_data)
                    else:
                        current_data = stage(current_data)
                    completed += 1
                except Exception as e:
                    errors.append(f"Stage {idx} failed: {e}")
                    break
            
            duration = (datetime.now() - start_time).total_seconds() * 1000
            
            return WorkflowResult(
                success=len(errors) == 0,
                result=current_data if len(errors) == 0 else None,
                steps_completed=completed,
                total_steps=len(self.stages),
                duration_ms=duration,
                errors=errors,
            )
        except Exception as e:
            duration = (datetime.now() - start_time).total_seconds() * 1000
            return WorkflowResult(
                success=False,
                result=None,
                steps_completed=completed,
                total_steps=len(self.stages),
                duration_ms=duration,
                errors=[str(e)],
            )


# LangChain integration for workflows
def create_workflow_tool(workflow: Workflow):
    """
    Create a LangChain tool from a workflow.
    
    Args:
        workflow: Workflow instance
        
    Returns:
        LangChain BaseTool
    """
    try:
        from langchain.tools import BaseTool
        from pydantic import BaseModel, Field
        
        class WorkflowInput(BaseModel):
            input_data: str = Field(description="JSON input data for the workflow")
        
        class WorkflowTool(BaseTool):
            name: str = f"workflow_{workflow.name}"
            description: str = f"Execute {workflow.name} workflow"
            args_schema = WorkflowInput
            
            def _run(self, input_data: str) -> str:
                import asyncio
                data = json.loads(input_data)
                result = asyncio.run(workflow.run(data))
                return json.dumps({
                    "success": result.success,
                    "result": result.result,
                    "steps_completed": result.steps_completed,
                    "duration_ms": result.duration_ms,
                })
        
        return WorkflowTool()
    except ImportError:
        raise ImportError("langchain required: pip install aethershell[langchain]")
