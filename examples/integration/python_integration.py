#!/usr/bin/env python3
"""
AetherShell Agent API - Python Integration Example

This example demonstrates how to integrate AetherShell's Agent API with
popular Python AI frameworks like OpenAI, Anthropic, and LangChain.

Prerequisites:
    pip install openai anthropic httpx

Start the Agent API server:
    ae --agent-api
"""

import json
import httpx
from typing import Any, Dict, List, Optional

# =============================================================================
# AetherShell Agent API Client
# =============================================================================

class AetherShellClient:
    """Client for interacting with AetherShell's Agent API."""
    
    def __init__(self, base_url: str = "http://localhost:3002"):
        self.base_url = base_url
        self.client = httpx.Client()
    
    def _post(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Send a POST request to the Agent API."""
        response = self.client.post(self.base_url, json=data)
        response.raise_for_status()
        return response.json()
    
    def eval(self, code: str) -> Dict[str, Any]:
        """Evaluate AetherShell code."""
        return self._post({"action": "eval", "code": code})
    
    def call(self, builtin: str, args: Optional[Dict[str, Any]] = None) -> Dict[str, Any]:
        """Call a builtin function."""
        data = {"action": "call", "builtin": builtin}
        if args:
            data["args"] = args
        return self._post(data)
    
    def list_builtins(self, category: Optional[str] = None) -> Dict[str, Any]:
        """List available builtins."""
        data = {"action": "list_builtins"}
        if category:
            data["category"] = category
        return self._post(data)
    
    def describe(self, builtin: str) -> Dict[str, Any]:
        """Get detailed information about a builtin."""
        return self._post({"action": "describe", "builtin": builtin})
    
    def schema(self, format: str) -> Dict[str, Any]:
        """Generate tool schema for a specific AI provider."""
        return self._post({"action": "schema", "format": format})
    
    def pipeline(self, steps: List[Dict[str, Any]]) -> Dict[str, Any]:
        """Execute a multi-step pipeline."""
        return self._post({"action": "pipeline", "steps": steps})


# =============================================================================
# OpenAI Integration
# =============================================================================

def openai_integration_example():
    """Demonstrate integration with OpenAI's API."""
    try:
        from openai import OpenAI
    except ImportError:
        print("Install openai: pip install openai")
        return
    
    # Initialize clients
    aether = AetherShellClient()
    openai_client = OpenAI()  # Uses OPENAI_API_KEY env var
    
    # Get AetherShell tool schema in OpenAI format
    schema = aether.schema("openai")
    if not schema.get("success"):
        print(f"Failed to get schema: {schema.get('error')}")
        return
    
    tools = schema["result"]["tools"]
    
    # Create a chat completion with tools
    messages = [
        {"role": "system", "content": "You are a helpful shell assistant. Use the provided tools to help users with file operations."},
        {"role": "user", "content": "List the files in the current directory"}
    ]
    
    response = openai_client.chat.completions.create(
        model="gpt-4o-mini",  # or "gpt-5", "o3", "o4-mini"
        messages=messages,
        tools=tools,
        tool_choice="auto"
    )
    
    # Handle tool calls
    if response.choices[0].message.tool_calls:
        for tool_call in response.choices[0].message.tool_calls:
            function_name = tool_call.function.name
            function_args = json.loads(tool_call.function.arguments)
            
            print(f"AI requested: {function_name}({function_args})")
            
            # Execute via AetherShell
            result = aether.call(function_name, function_args)
            
            if result.get("success"):
                print(f"Result: {result['result']}")
            else:
                print(f"Error: {result.get('error')}")
    else:
        print(f"AI response: {response.choices[0].message.content}")


# =============================================================================
# Anthropic/Claude Integration
# =============================================================================

def anthropic_integration_example():
    """Demonstrate integration with Anthropic's Claude API."""
    try:
        import anthropic
    except ImportError:
        print("Install anthropic: pip install anthropic")
        return
    
    # Initialize clients
    aether = AetherShellClient()
    claude_client = anthropic.Anthropic()  # Uses ANTHROPIC_API_KEY env var
    
    # Get AetherShell tool schema in Claude format
    schema = aether.schema("claude")
    if not schema.get("success"):
        print(f"Failed to get schema: {schema.get('error')}")
        return
    
    tools = schema["result"]["tools"]
    
    # Create a message with tools
    message = claude_client.messages.create(
        model="claude-3-5-sonnet-20241022",  # or claude-4.5-opus, claude-4.5-sonnet
        max_tokens=1024,
        tools=tools,
        messages=[
            {"role": "user", "content": "What files are in the current directory?"}
        ]
    )
    
    # Handle tool use
    for block in message.content:
        if block.type == "tool_use":
            tool_name = block.name
            tool_input = block.input
            
            print(f"Claude requested: {tool_name}({tool_input})")
            
            # Execute via AetherShell
            result = aether.call(tool_name, tool_input)
            
            if result.get("success"):
                print(f"Result: {result['result']}")
            else:
                print(f"Error: {result.get('error')}")
        elif block.type == "text":
            print(f"Claude: {block.text}")


# =============================================================================
# LangChain Integration
# =============================================================================

def langchain_integration_example():
    """Demonstrate integration with LangChain."""
    try:
        from langchain_core.tools import StructuredTool
        from langchain_openai import ChatOpenAI
        from langchain.agents import AgentExecutor, create_tool_calling_agent
        from langchain_core.prompts import ChatPromptTemplate
    except ImportError:
        print("Install langchain: pip install langchain langchain-openai")
        return
    
    # Initialize AetherShell client
    aether = AetherShellClient()
    
    # Get builtin descriptions
    builtins_response = aether.list_builtins()
    if not builtins_response.get("success"):
        print(f"Failed to list builtins: {builtins_response.get('error')}")
        return
    
    # Create LangChain tools dynamically from AetherShell builtins
    tools = []
    for builtin in builtins_response["result"]["builtins"][:10]:  # Limit for demo
        name = builtin["name"]
        description = builtin["description"]
        
        # Get detailed info for the builtin
        detail = aether.describe(name)
        if not detail.get("success"):
            continue
        
        def make_tool_func(builtin_name):
            def tool_func(**kwargs):
                result = aether.call(builtin_name, kwargs if kwargs else None)
                if result.get("success"):
                    return json.dumps(result["result"])
                return f"Error: {result.get('error')}"
            return tool_func
        
        tool = StructuredTool.from_function(
            func=make_tool_func(name),
            name=name,
            description=description,
        )
        tools.append(tool)
    
    # Create agent
    llm = ChatOpenAI(model="gpt-4o-mini")
    prompt = ChatPromptTemplate.from_messages([
        ("system", "You are a helpful shell assistant. Use the provided tools to help users."),
        ("human", "{input}"),
        ("placeholder", "{agent_scratchpad}"),
    ])
    
    agent = create_tool_calling_agent(llm, tools, prompt)
    agent_executor = AgentExecutor(agent=agent, tools=tools, verbose=True)
    
    # Run agent
    result = agent_executor.invoke({"input": "List files in the current directory"})
    print(f"Final result: {result['output']}")


# =============================================================================
# Direct API Usage Examples
# =============================================================================

def direct_api_examples():
    """Show direct API usage without AI framework."""
    aether = AetherShellClient()
    
    print("=== Direct API Examples ===\n")
    
    # 1. Evaluate code
    print("1. Evaluate expression:")
    result = aether.eval("1 + 2 * 3")
    print(f"   1 + 2 * 3 = {result['result']}")
    
    # 2. List builtins by category
    print("\n2. List filesystem builtins:")
    result = aether.list_builtins("filesystem")
    if result.get("success"):
        for b in result["result"]["builtins"][:5]:
            print(f"   - {b['name']}: {b['description']}")
    
    # 3. Call builtin
    print("\n3. Call 'pwd' builtin:")
    result = aether.call("pwd")
    if result.get("success"):
        print(f"   Current directory: {result['result']}")
    
    # 4. Execute pipeline
    print("\n4. Execute pipeline (ls | filter | select):")
    result = aether.pipeline([
        {"builtin": "ls", "args": {"path": "."}},
        {"eval": "take(3)"},
        {"eval": "select(\"name\", \"size\")"}
    ])
    if result.get("success"):
        print(f"   Result: {json.dumps(result['result'], indent=2)}")
    
    # 5. Get schema for different providers
    print("\n5. Available schema formats:")
    formats = ["openai", "claude", "gemini", "llama", "mistral", "groq", "ollama"]
    for fmt in formats:
        schema = aether.schema(fmt)
        if schema.get("success"):
            model_count = len(schema["result"].get("compatible_models", []))
            print(f"   - {fmt}: {model_count} compatible models")


# =============================================================================
# Main
# =============================================================================

if __name__ == "__main__":
    print("AetherShell Agent API - Python Integration Examples\n")
    print("=" * 60)
    
    # Check if Agent API is running
    try:
        client = httpx.Client()
        response = client.post("http://localhost:3002", json={"action": "list_builtins"})
        if response.status_code != 200:
            raise Exception("API not responding")
    except Exception as e:
        print(f"Error: Agent API not running on localhost:3002")
        print("Start it with: ae --agent-api")
        exit(1)
    
    print("Agent API is running!\n")
    
    # Run direct examples (no external API keys needed)
    direct_api_examples()
    
    print("\n" + "=" * 60)
    print("\nTo run AI integration examples:")
    print("  - OpenAI: Set OPENAI_API_KEY, then call openai_integration_example()")
    print("  - Claude: Set ANTHROPIC_API_KEY, then call anthropic_integration_example()")
    print("  - LangChain: Set OPENAI_API_KEY, then call langchain_integration_example()")
