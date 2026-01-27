"""
AetherShell LangChain Integration

Provides LangChain tools for executing AetherShell code and pipelines.
"""

from typing import Any, Dict, List, Optional, Type
from pydantic import BaseModel, Field

try:
    from langchain.tools import BaseTool
    from langchain.callbacks.manager import CallbackManagerForToolRun
except ImportError:
    raise ImportError(
        "langchain is required for this integration. "
        "Install with: pip install aethershell[langchain]"
    )

from . import AetherRuntime, Agent as AetherAgent


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
    
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        if self.runtime is None:
            self.runtime = AetherRuntime()
    
    def _run(
        self,
        code: str,
        run_manager: Optional[CallbackManagerForToolRun] = None,
    ) -> str:
        """Execute AetherShell code"""
        try:
            result = self.runtime.eval(code)
            if isinstance(result, (dict, list)):
                import json
                return json.dumps(result, indent=2)
            return str(result)
        except Exception as e:
            return f"Error: {e}"


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
    
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        if self.runtime is None:
            self.runtime = AetherRuntime()
    
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
                import json
                return json.dumps(result, indent=2)
            return str(result)
        except Exception as e:
            return f"Error: {e}"


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
    
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        if self.runtime is None:
            self.runtime = AetherRuntime()
    
    def _run(
        self,
        goal: str,
        tools: List[str] = None,
        run_manager: Optional[CallbackManagerForToolRun] = None,
    ) -> str:
        """Run agent with goal"""
        import asyncio
        
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


def get_aethershell_tools(
    runtime: Optional[AetherRuntime] = None,
) -> List[BaseTool]:
    """
    Get all AetherShell LangChain tools.
    
    Args:
        runtime: Optional AetherRuntime to share across tools
        
    Returns:
        List of LangChain tools
    """
    rt = runtime or AetherRuntime()
    return [
        AetherShellTool(runtime=rt),
        AetherPipelineTool(runtime=rt),
        AetherAgentTool(runtime=rt),
    ]
