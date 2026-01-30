"""
AetherShell LangChain Integration

Enhanced LangChain tools for AetherShell:
- Core execution tools (code, pipelines, agents)
- Workflow orchestration tools (MapReduce, Saga, Fan-Out)
- Metrics and observability tools
- Distributed agent tools
- Service registry tools
"""

from __future__ import annotations

import asyncio
import json
from typing import Any, Dict, List, Optional, Type, Union

try:
    from langchain.tools import BaseTool
    from langchain.callbacks.manager import CallbackManagerForToolRun
    from pydantic import BaseModel, Field
    LANGCHAIN_AVAILABLE = True
except ImportError:
    LANGCHAIN_AVAILABLE = False
    BaseTool = object
    BaseModel = object
    Field = lambda **kwargs: None
    CallbackManagerForToolRun = None

from . import AetherRuntime, Agent as AetherAgent
from .workflows import (
    MapReduceWorkflow,
    SagaWorkflow,
    SagaStep,
    FanOutWorkflow,
    PipelineWorkflow,
    CircuitBreaker,
    WorkflowResult,
)
from .metrics import (
    MetricsCollector,
    get_metrics_collector,
    Tracer,
    Timer,
)
from .distributed import (
    ServiceRegistry,
    LeaderElection,
    AgentRouter,
    LoadBalancer,
    DistributedSwarm,
)

__all__ = [
    # Original tools
    "AetherShellTool",
    "AetherPipelineTool", 
    "AetherAgentTool",
    # New enhanced tools
    "AetherWorkflowTool",
    "AetherMapReduceTool",
    "AetherSagaTool",
    "AetherMetricsTool",
    "AetherTracingTool",
    "AetherDistributedAgentTool",
    "AetherServiceRegistryTool",
    # Factory functions
    "get_aethershell_tools",
    "get_all_aethershell_tools",
    "create_workflow_agent",
]


def _check_langchain():
    if not LANGCHAIN_AVAILABLE:
        raise ImportError(
            "langchain is required for LangChain integration. "
            "Install with: pip install aethershell[langchain]"
        )


# ============================================================================
# Core Tools (Enhanced)
# ============================================================================

if LANGCHAIN_AVAILABLE:
    
    class AetherShellInput(BaseModel):
        """Input schema for AetherShell tool"""
        code: str = Field(description="AetherShell code to evaluate")


    class AetherShellTool(BaseTool):
        """
        LangChain tool for executing AetherShell code.
        
        AetherShell is a typed shell with AI capabilities. It supports:
        - Typed data pipelines: `[1,2,3] | map(fn(x) => x * 2)`
        - Records and tables: `{name: "John", age: 30}`
        - File operations: `ls "." | where(fn(r) => r.size > 1000)`
        - HTTP requests: `http_get("https://api.example.com")`
        - JSON processing: `parse_json(data) | get("items")`
        """
        
        name: str = "aethershell"
        description: str = (
            "Execute AetherShell code for data processing, file operations, "
            "and system tasks. AetherShell uses typed pipelines for data "
            "transformation. Example: `[1,2,3] | map(fn(x) => x * 2)` returns [2,4,6]"
        )
        args_schema: Type[BaseModel] = AetherShellInput
        
        runtime: AetherRuntime = None
        
        def __init__(self, runtime: Optional[AetherRuntime] = None, **kwargs):
            super().__init__(**kwargs)
            self.runtime = runtime or AetherRuntime()
        
        def _run(
            self,
            code: str,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute AetherShell code"""
            try:
                result = self.runtime.eval(code)
                if isinstance(result, (dict, list)):
                    return json.dumps(result, indent=2)
                return str(result)
            except Exception as e:
                return f"Error: {e}"
        
        async def _arun(
            self,
            code: str,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute AetherShell code asynchronously"""
            result = await asyncio.to_thread(self.runtime.eval, code)
            return json.dumps(result, default=str) if isinstance(result, (dict, list)) else str(result)


    class AetherPipelineInput(BaseModel):
        """Input schema for pipeline tool"""
        data: str = Field(description="JSON data to process")
        operations: str = Field(
            description="Pipeline operations (e.g., 'map(fn(x) => x * 2) | filter(fn(x) => x > 0)')"
        )


    class AetherPipelineTool(BaseTool):
        """
        LangChain tool for executing AetherShell pipelines on data.
        
        Pipelines transform data through a series of operations:
        - map: Transform each element
        - filter: Keep elements matching condition
        - reduce: Aggregate elements
        - sort, reverse, unique, flatten
        - select, where: Table/record operations
        """
        
        name: str = "aether_pipeline"
        description: str = (
            "Process data through AetherShell pipelines. "
            "Provide JSON data and pipeline operations. "
            "Example: data='[1,2,3]', operations='map(fn(x) => x * 2)' returns [2,4,6]"
        )
        args_schema: Type[BaseModel] = AetherPipelineInput
        
        runtime: AetherRuntime = None
        
        def __init__(self, runtime: Optional[AetherRuntime] = None, **kwargs):
            super().__init__(**kwargs)
            self.runtime = runtime or AetherRuntime()
        
        def _run(
            self,
            data: str,
            operations: str,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute pipeline on data"""
            try:
                code = f"{data} | {operations}"
                result = self.runtime.eval(code)
                if isinstance(result, (dict, list)):
                    return json.dumps(result, indent=2)
                return str(result)
            except Exception as e:
                return f"Error: {e}"
        
        async def _arun(
            self,
            data: str,
            operations: str,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute pipeline asynchronously"""
            code = f"{data} | {operations}"
            result = await asyncio.to_thread(self.runtime.eval, code)
            return json.dumps(result, default=str) if isinstance(result, (dict, list)) else str(result)


    class AetherAgentInput(BaseModel):
        """Input schema for agent tool"""
        goal: str = Field(description="Goal for the agent to accomplish")
        tools: List[str] = Field(
            default=[],
            description="Tools the agent can use (e.g., ['http_get', 'read_file'])"
        )


    class AetherAgentTool(BaseTool):
        """
        LangChain tool for running AetherShell AI agents.
        
        Creates an AI agent that can use tools to accomplish goals.
        The agent reasons about the goal, plans steps, and executes
        tools to achieve the objective.
        """
        
        name: str = "aether_agent"
        description: str = (
            "Run an AI agent with tools to accomplish a goal. "
            "The agent will reason and use tools like http_get, read_file, write_file. "
            "Example: goal='Find Python docs homepage', tools=['http_get']"
        )
        args_schema: Type[BaseModel] = AetherAgentInput
        
        runtime: AetherRuntime = None
        model: str = "openai:gpt-4o-mini"
        max_steps: int = 10
        
        def __init__(self, runtime: Optional[AetherRuntime] = None, **kwargs):
            super().__init__(**kwargs)
            self.runtime = runtime or AetherRuntime()
        
        def _run(
            self,
            goal: str,
            tools: List[str] = None,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Run agent with goal"""
            async def run_agent():
                agent = self.runtime.create_agent(
                    name="langchain_agent",
                    model=self.model,
                    tools=tools or [],
                    max_steps=self.max_steps,
                )
                result = await agent.run(goal)
                return result
            
            try:
                result = asyncio.run(run_agent())
                if result.success:
                    return str(result.result)
                else:
                    return f"Agent failed: {result.result}"
            except Exception as e:
                return f"Error: {e}"
        
        async def _arun(
            self,
            goal: str,
            tools: List[str] = None,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Run agent asynchronously"""
            agent = self.runtime.create_agent(
                name="langchain_agent",
                model=self.model,
                tools=tools or [],
                max_steps=self.max_steps,
            )
            result = await agent.run(goal)
            return str(result.result) if result.success else f"Agent failed: {result.result}"


    # ============================================================================
    # Workflow Tools
    # ============================================================================

    class WorkflowInput(BaseModel):
        """Input for workflow execution"""
        input_data: str = Field(description="JSON input data for the workflow")
        workflow_type: str = Field(
            default="pipeline",
            description="Workflow type: 'pipeline', 'map_reduce', 'saga', 'fan_out'"
        )
        steps: str = Field(
            default="[]",
            description="JSON array of step configurations"
        )


    class AetherWorkflowTool(BaseTool):
        """
        Execute AetherShell workflows.
        
        Supports multiple workflow patterns for distributed processing:
        - pipeline: Sequential processing stages
        - map_reduce: Parallel map, then reduce
        - saga: Steps with compensation on failure
        - fan_out: Parallel workers with result gathering
        """
        name: str = "aether_workflow"
        description: str = """Execute an AetherShell workflow.
        Workflow types:
        - 'pipeline': Sequential processing stages
        - 'map_reduce': Parallel map, then reduce
        - 'saga': Steps with compensation on failure
        - 'fan_out': Parallel workers with result gathering
        
        Example: input_data='[1,2,3]' workflow_type='pipeline' steps='[{"code": "x * 2"}]'
        """
        args_schema: Type[BaseModel] = WorkflowInput
        
        runtime: AetherRuntime = None
        
        def __init__(self, runtime: Optional[AetherRuntime] = None, **kwargs):
            super().__init__(**kwargs)
            self.runtime = runtime or AetherRuntime()
        
        def _run(
            self,
            input_data: str,
            workflow_type: str = "pipeline",
            steps: str = "[]",
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute workflow synchronously"""
            data = json.loads(input_data)
            step_configs = json.loads(steps)
            
            result = asyncio.run(self._execute_workflow(data, workflow_type, step_configs))
            return json.dumps({
                "success": result.success,
                "result": result.result,
                "steps_completed": result.steps_completed,
                "total_steps": result.total_steps,
                "duration_ms": result.duration_ms,
                "errors": result.errors,
            }, default=str)
        
        async def _arun(
            self,
            input_data: str,
            workflow_type: str = "pipeline",
            steps: str = "[]",
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute workflow asynchronously"""
            data = json.loads(input_data)
            step_configs = json.loads(steps)
            
            result = await self._execute_workflow(data, workflow_type, step_configs)
            return json.dumps({
                "success": result.success,
                "result": result.result,
                "steps_completed": result.steps_completed,
                "total_steps": result.total_steps,
                "duration_ms": result.duration_ms,
                "errors": result.errors,
            }, default=str)
        
        async def _execute_workflow(
            self,
            data: Any,
            workflow_type: str,
            step_configs: List[Dict],
        ) -> WorkflowResult:
            """Execute the specified workflow type"""
            if workflow_type == "pipeline":
                stages = [
                    lambda d, code=s.get("code", "x"): self.runtime.eval(
                        f'let x = {json.dumps(d)}; {code}'
                    )
                    for s in step_configs
                ]
                workflow = PipelineWorkflow("langchain_pipeline", stages=stages)
                return await workflow.run(data)
            
            elif workflow_type == "map_reduce":
                map_code = step_configs[0].get("map", "x") if step_configs else "x"
                reduce_code = step_configs[0].get("reduce", "a + b") if step_configs else "a + b"
                
                def map_fn(item):
                    return self.runtime.eval(f'let x = {json.dumps(item)}; {map_code}')
                
                def reduce_fn(a, b):
                    return self.runtime.eval(
                        f'let a = {json.dumps(a)}; let b = {json.dumps(b)}; {reduce_code}'
                    )
                
                workflow = MapReduceWorkflow("langchain_mapreduce", map_fn=map_fn, reduce_fn=reduce_fn)
                return await workflow.run(data)
            
            else:
                workflow = PipelineWorkflow("langchain_default")
                return await workflow.run(data)


    class MapReduceInput(BaseModel):
        """Input for MapReduce operations"""
        data: str = Field(description="JSON array of data to process")
        map_expr: str = Field(description="AetherShell expression for map (use 'x' for item)")
        reduce_expr: str = Field(description="AetherShell expression for reduce (use 'a', 'b')")
        chunk_size: int = Field(default=100, description="Size of chunks for parallel processing")


    class AetherMapReduceTool(BaseTool):
        """
        Execute MapReduce operations on data.
        
        Splits data into chunks, processes in parallel, then combines.
        """
        name: str = "aether_mapreduce"
        description: str = """Execute a MapReduce operation.
        Example: data='[1,2,3,4,5]' map_expr='x * 2' reduce_expr='a + b'
        Result: Sums doubled values = 30
        """
        args_schema: Type[BaseModel] = MapReduceInput
        runtime: AetherRuntime = None
        
        def __init__(self, runtime: Optional[AetherRuntime] = None, **kwargs):
            super().__init__(**kwargs)
            self.runtime = runtime or AetherRuntime()
        
        def _run(
            self,
            data: str,
            map_expr: str,
            reduce_expr: str,
            chunk_size: int = 100,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute MapReduce synchronously"""
            result = asyncio.run(self._mapreduce(data, map_expr, reduce_expr, chunk_size))
            return json.dumps(result, default=str)
        
        async def _arun(
            self,
            data: str,
            map_expr: str,
            reduce_expr: str,
            chunk_size: int = 100,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute MapReduce asynchronously"""
            result = await self._mapreduce(data, map_expr, reduce_expr, chunk_size)
            return json.dumps(result, default=str)
        
        async def _mapreduce(self, data: str, map_expr: str, reduce_expr: str, chunk_size: int) -> Dict[str, Any]:
            """Perform MapReduce"""
            items = json.loads(data)
            
            def map_fn(item):
                return self.runtime.eval(f'let x = {json.dumps(item)}; {map_expr}')
            
            def reduce_fn(a, b):
                return self.runtime.eval(f'let a = {json.dumps(a)}; let b = {json.dumps(b)}; {reduce_expr}')
            
            workflow = MapReduceWorkflow("langchain_mapreduce", map_fn=map_fn, reduce_fn=reduce_fn, chunk_size=chunk_size)
            result = await workflow.run(items)
            return {"success": result.success, "result": result.result, "duration_ms": result.duration_ms}


    class SagaInput(BaseModel):
        """Input for Saga workflow"""
        steps: str = Field(description="JSON array of saga steps with 'action' and 'compensate' code")
        input_data: str = Field(description="JSON input data")


    class AetherSagaTool(BaseTool):
        """
        Execute Saga workflow pattern.
        
        Runs steps in sequence; on failure, executes compensation in reverse.
        """
        name: str = "aether_saga"
        description: str = """Execute a Saga workflow with compensation.
        Steps run sequentially. On failure, compensations run in reverse.
        Example: steps='[{"action": "x + 1", "compensate": "x - 1"}]'
        """
        args_schema: Type[BaseModel] = SagaInput
        runtime: AetherRuntime = None
        
        def __init__(self, runtime: Optional[AetherRuntime] = None, **kwargs):
            super().__init__(**kwargs)
            self.runtime = runtime or AetherRuntime()
        
        def _run(
            self,
            steps: str,
            input_data: str,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute Saga synchronously"""
            result = asyncio.run(self._saga(steps, input_data))
            return json.dumps(result, default=str)
        
        async def _saga(self, steps: str, input_data: str) -> Dict[str, Any]:
            """Perform Saga"""
            step_configs = json.loads(steps)
            data = json.loads(input_data)
            
            saga = SagaWorkflow("langchain_saga")
            for i, cfg in enumerate(step_configs):
                action_code = cfg.get("action", "x")
                compensate_code = cfg.get("compensate", "x")
                
                def make_action(code):
                    return lambda d: self.runtime.eval(f'let x = {json.dumps(d)}; {code}')
                
                saga.add_saga_step(f"step_{i}", make_action(action_code), make_action(compensate_code))
            
            result = await saga.run(data)
            return {"success": result.success, "result": result.result, "steps_completed": result.steps_completed}


    # ============================================================================
    # Metrics Tools  
    # ============================================================================

    class MetricsInput(BaseModel):
        """Input for metrics operations"""
        operation: str = Field(description="Operation: 'get', 'increment', 'set', 'record', 'export'")
        metric_name: str = Field(description="Name of the metric")
        value: Optional[float] = Field(default=None, description="Value for set/record operations")
        metric_type: str = Field(default="counter", description="Type: 'counter', 'gauge', 'histogram'")


    class AetherMetricsTool(BaseTool):
        """
        Access AetherShell metrics for monitoring.
        
        Monitor and observe agent/workflow performance.
        """
        name: str = "aether_metrics"
        description: str = """Access metrics for monitoring.
        Operations:
        - 'get': Get current metric value
        - 'increment': Increment a counter
        - 'set': Set a gauge value
        - 'record': Record histogram observation
        - 'export': Export all metrics (Prometheus format)
        """
        args_schema: Type[BaseModel] = MetricsInput
        collector: MetricsCollector = None
        
        def __init__(self, collector: Optional[MetricsCollector] = None, **kwargs):
            super().__init__(**kwargs)
            self.collector = collector or get_metrics_collector()
        
        def _run(
            self,
            operation: str,
            metric_name: str,
            value: Optional[float] = None,
            metric_type: str = "counter",
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute metrics operation"""
            if operation == "get":
                if metric_type == "counter":
                    counter = self.collector.counter(metric_name)
                    return json.dumps({"name": metric_name, "value": counter.value})
                elif metric_type == "gauge":
                    gauge = self.collector.gauge(metric_name)
                    return json.dumps({"name": metric_name, "value": gauge.value})
                elif metric_type == "histogram":
                    hist = self.collector.histogram(metric_name)
                    return json.dumps({"name": metric_name, "count": hist.count, "sum": hist.sum, "mean": hist.mean})
            
            elif operation == "increment":
                counter = self.collector.counter(metric_name)
                counter.inc(value or 1.0)
                return json.dumps({"name": metric_name, "value": counter.value})
            
            elif operation == "set":
                gauge = self.collector.gauge(metric_name)
                gauge.set(value or 0.0)
                return json.dumps({"name": metric_name, "value": gauge.value})
            
            elif operation == "record":
                hist = self.collector.histogram(metric_name)
                hist.observe(value or 0.0)
                return json.dumps({"name": metric_name, "recorded": value})
            
            elif operation == "export":
                return self.collector.to_prometheus()
            
            return json.dumps({"error": f"Unknown operation: {operation}"})


    class TracingInput(BaseModel):
        """Input for tracing operations"""
        operation: str = Field(description="Operation: 'get_trace', 'clear'")
        trace_id: Optional[str] = Field(default=None, description="Trace ID to query")


    class AetherTracingTool(BaseTool):
        """
        Distributed tracing for debugging.
        """
        name: str = "aether_tracing"
        description: str = """Manage distributed traces.
        Operations:
        - 'get_trace': Get all spans (JSON format)
        - 'clear': Clear recorded spans
        """
        args_schema: Type[BaseModel] = TracingInput
        tracer: Tracer = None
        
        def __init__(self, tracer: Optional[Tracer] = None, **kwargs):
            super().__init__(**kwargs)
            self.tracer = tracer or get_metrics_collector().tracer()
        
        def _run(
            self,
            operation: str,
            trace_id: Optional[str] = None,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute tracing operation"""
            if operation == "get_trace":
                spans = self.tracer.get_spans()
                if trace_id:
                    spans = [s for s in spans if s.trace_id == trace_id]
                return json.dumps([s.to_dict() for s in spans], default=str)
            elif operation == "clear":
                self.tracer.clear_spans()
                return json.dumps({"status": "cleared"})
            return json.dumps({"error": f"Unknown operation: {operation}"})


    # ============================================================================
    # Distributed Agent Tools
    # ============================================================================

    class DistributedAgentInput(BaseModel):
        """Input for distributed agent operations"""
        goal: str = Field(description="Goal for the agent(s)")
        agent_name: Optional[str] = Field(default=None, description="Specific agent to use")
        capability: Optional[str] = Field(default=None, description="Required capability")
        broadcast: bool = Field(default=False, description="Broadcast to all agents")


    class AetherDistributedAgentTool(BaseTool):
        """
        Run distributed AI agents.
        
        Routes tasks to agents across a cluster.
        """
        name: str = "aether_distributed_agent"
        description: str = """Run a task on distributed agents.
        Can route by:
        - agent_name: Specific agent
        - capability: Any agent with that capability
        - broadcast=True: All agents
        """
        args_schema: Type[BaseModel] = DistributedAgentInput
        swarm: DistributedSwarm = None
        
        def __init__(self, swarm: Optional[DistributedSwarm] = None, registry: Optional[ServiceRegistry] = None, **kwargs):
            super().__init__(**kwargs)
            if swarm:
                self.swarm = swarm
            else:
                registry = registry or ServiceRegistry()
                self.swarm = DistributedSwarm("langchain_swarm", registry)
        
        def _run(
            self,
            goal: str,
            agent_name: Optional[str] = None,
            capability: Optional[str] = None,
            broadcast: bool = False,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute distributed agent task"""
            result = asyncio.run(self._dispatch(goal, agent_name, capability, broadcast))
            return json.dumps(result, default=str)
        
        async def _arun(
            self,
            goal: str,
            agent_name: Optional[str] = None,
            capability: Optional[str] = None,
            broadcast: bool = False,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute distributed agent task asynchronously"""
            result = await self._dispatch(goal, agent_name, capability, broadcast)
            return json.dumps(result, default=str)
        
        async def _dispatch(self, goal: str, agent_name: Optional[str], capability: Optional[str], broadcast: bool) -> Any:
            """Dispatch task to agents"""
            if broadcast:
                return await self.swarm.broadcast(goal)
            else:
                return await self.swarm.dispatch(goal, agent_name=agent_name, capability=capability)


    class ServiceRegistryInput(BaseModel):
        """Input for service registry operations"""
        operation: str = Field(description="Operation: 'list', 'register', 'deregister', 'health'")
        service_name: Optional[str] = Field(default=None, description="Service name")
        host: Optional[str] = Field(default=None, description="Service host")
        port: Optional[int] = Field(default=None, description="Service port")


    class AetherServiceRegistryTool(BaseTool):
        """
        Manage the service registry.
        """
        name: str = "aether_service_registry"
        description: str = """Manage agent service registry.
        Operations:
        - 'list': List all registered services
        - 'register': Register a new service
        - 'deregister': Remove a service
        - 'health': Get service health status
        """
        args_schema: Type[BaseModel] = ServiceRegistryInput
        registry: ServiceRegistry = None
        
        def __init__(self, registry: Optional[ServiceRegistry] = None, **kwargs):
            super().__init__(**kwargs)
            self.registry = registry or ServiceRegistry()
        
        def _run(
            self,
            operation: str,
            service_name: Optional[str] = None,
            host: Optional[str] = None,
            port: Optional[int] = None,
            run_manager: Optional[CallbackManagerForToolRun] = None,
        ) -> str:
            """Execute registry operation"""
            if operation == "list":
                services = self.registry.get_all_services(healthy_only=False)
                return json.dumps([s.to_dict() for s in services], default=str)
            
            elif operation == "register":
                if not all([service_name, host, port]):
                    return json.dumps({"error": "name, host, and port required"})
                service = self.registry.register(service_name, host, port)
                return json.dumps(service.to_dict(), default=str)
            
            elif operation == "deregister":
                if not service_name:
                    return json.dumps({"error": "service_name required"})
                services = self.registry.get_services_by_name(service_name, healthy_only=False)
                removed = sum(1 for s in services if self.registry.deregister(s.service_id))
                return json.dumps({"removed": removed})
            
            elif operation == "health":
                services = self.registry.get_all_services(healthy_only=False)
                return json.dumps({
                    "total": len(services),
                    "healthy": sum(1 for s in services if s.is_healthy),
                    "unhealthy": sum(1 for s in services if not s.is_healthy),
                })
            
            return json.dumps({"error": f"Unknown operation: {operation}"})


# ============================================================================
# Factory Functions
# ============================================================================

def get_aethershell_tools(runtime: Optional[AetherRuntime] = None) -> List:
    """
    Get the basic set of AetherShell LangChain tools.
    
    Args:
        runtime: Optional AetherRuntime instance
        
    Returns:
        List of LangChain tools
    """
    _check_langchain()
    runtime = runtime or AetherRuntime()
    return [
        AetherShellTool(runtime=runtime),
        AetherPipelineTool(runtime=runtime),
        AetherAgentTool(runtime=runtime),
    ]


def get_all_aethershell_tools(
    runtime: Optional[AetherRuntime] = None,
    metrics_collector: Optional[MetricsCollector] = None,
    service_registry: Optional[ServiceRegistry] = None,
) -> List:
    """
    Get all AetherShell LangChain tools including advanced features.
    
    Args:
        runtime: Optional AetherRuntime instance
        metrics_collector: Optional MetricsCollector
        service_registry: Optional ServiceRegistry
        
    Returns:
        List of all available LangChain tools
    """
    _check_langchain()
    runtime = runtime or AetherRuntime()
    metrics = metrics_collector or get_metrics_collector()
    registry = service_registry or ServiceRegistry()
    
    return [
        # Basic tools
        AetherShellTool(runtime=runtime),
        AetherPipelineTool(runtime=runtime),
        AetherAgentTool(runtime=runtime),
        # Workflow tools
        AetherWorkflowTool(runtime=runtime),
        AetherMapReduceTool(runtime=runtime),
        AetherSagaTool(runtime=runtime),
        # Metrics tools
        AetherMetricsTool(collector=metrics),
        AetherTracingTool(tracer=metrics.tracer()),
        # Distributed tools
        AetherDistributedAgentTool(registry=registry),
        AetherServiceRegistryTool(registry=registry),
    ]


def create_workflow_agent(model: str = "openai:gpt-4o-mini", runtime: Optional[AetherRuntime] = None) -> AetherAgent:
    """
    Create an agent configured for workflow orchestration.
    
    Args:
        model: AI model URI
        runtime: Optional AetherRuntime
        
    Returns:
        Configured Agent instance
    """
    from . import Agent
    runtime = runtime or AetherRuntime()
    
    return Agent(
        name="workflow_orchestrator",
        model=model,
        system_prompt="""You are a workflow orchestration agent for AetherShell.
        You can create and execute data pipelines, MapReduce operations, and distributed agent tasks.
        Prefer pipelines for sequential transformations, MapReduce for large parallel processing.""",
        runtime=runtime,
    )
