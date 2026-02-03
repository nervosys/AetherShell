---

## External Integrations

Connect AetherShell to external LLM providers and MCP tool servers.

### External LLMs

```ae
# Auto-detect best available backend
model = ai.detect()                      # => "ollama:llama3.2:3b"
ai.backends()                            # List all available providers

# OpenAI (set OPENAI_API_KEY)
ai("openai:gpt-4o", "Explain quantum computing")

# Anthropic Claude (set ANTHROPIC_API_KEY)
ai("anthropic:claude-3-opus", "Write detailed analysis")

# Local Ollama (free, private)
ai("ollama:llama3.2:3b", "Hello!")

# vLLM (high-performance local)
ai("vllm:mistral-7b", "Generate code for...")
```

### External MCP Tools (e.g., SiliconMonitor)

```ae
# Connect to external MCP server
monitor = mcp.connect("http://localhost:3006")
print(monitor.tools)                     # => ["cpu_usage", "memory_info", ...]

# Create agent with external tool access
agent(
    "Monitor system health",
    ai.detect(),
    monitor.tools,
    5
)

# Agent with MCP endpoint
agent.with_mcp("Check health", monitor.tools, "http://localhost:3006")
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| OPENAI_API_KEY | OpenAI API key |
| ANTHROPIC_API_KEY | Anthropic Claude API key |
| AETHER_AI | Default AI provider |
| OLLAMA_HOST | Ollama server URL |
| VLLM_API_BASE | vLLM server endpoint |
| COMPAT_API_BASE | Custom OpenAI-compatible endpoint |
| AGENT_ALLOW_CMDS | Whitelist of allowed shell commands |

