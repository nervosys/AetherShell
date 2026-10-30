# AI Backend Configuration Guide

IronShell supports multiple AI inference backends, allowing you to run AI models locally or connect to cloud services.

## Supported Backends

### 1. **OpenAI** (Cloud Service)

```bash
# Environment variables
export IRONSHELL_AI=openai
export OPENAI_API_KEY=sk-your-key-here
export OPENAI_MODEL=gpt-4o-mini  # Optional, default: gpt-4o-mini

# Or use model URI directly
ish 'ai("openai:gpt-4o-mini", "Hello, world!")'
```

**API Endpoint**: `https://api.openai.com/v1`  
**Models**: gpt-4o, gpt-4o-mini, gpt-4-turbo, gpt-3.5-turbo

### 2. **Ollama** (Local Server)

```bash
# Start Ollama server first
ollama serve

# Pull a model
ollama pull llama3

# Environment variables
export IRONSHELL_AI=ollama
export OLLAMA_HOST=http://localhost:11434  # Default
export OLLAMA_MODEL=llama3  # Optional

# Use in IronShell
ish 'ai("ollama:llama3", "Hello, world!")'
```

**Default Endpoint**: `http://localhost:11434`  
**Models**: llama3, mistral, codellama, phi, gemma, etc.

### 3. **vLLM** (High-Performance Local Inference)

vLLM provides high-throughput inference with PagedAttention for efficient memory management.

```bash
# Start vLLM server
python -m vllm.entrypoints.openai.api_server \
    --model meta-llama/Llama-3-8B-Instruct \
    --host 0.0.0.0 \
    --port 8000

# Environment variables
export IRONSHELL_AI=vllm
export VLLM_URL=http://localhost:8000/v1  # Default
export VLLM_MODEL=meta-llama/Llama-3-8B-Instruct

# Use in IronShell
ish 'ai("vllm:meta-llama/Llama-3-8B-Instruct", "Hello, world!")'
```

**Default Endpoint**: `http://localhost:8000/v1`  
**Features**: 
- OpenAI-compatible API
- PagedAttention for efficient memory
- High throughput inference
- Multi-GPU support

### 4. **llama.cpp** (Efficient CPU/GPU Inference)

llama.cpp provides efficient inference optimized for CPUs and Apple Silicon.

```bash
# Start llama.cpp server
./server -m models/llama-3-8b.gguf \
    --host 0.0.0.0 \
    --port 8080 \
    --ctx-size 4096

# Environment variables
export IRONSHELL_AI=llamacpp
export LLAMACPP_URL=http://localhost:8080/v1  # Default
export LLAMACPP_MODEL=model  # Default

# Use in IronShell
ish 'ai("llamacpp:model", "Hello, world!")'
```

**Default Endpoint**: `http://localhost:8080/v1`  
**Features**:
- OpenAI-compatible API
- Optimized for CPU inference
- Apple Silicon support (Metal)
- GGUF format models
- Low memory footprint

### 5. **Text Generation Inference (TGI)** (HuggingFace)

```bash
# Start TGI server (Docker)
docker run --gpus all -p 8080:80 \
    ghcr.io/huggingface/text-generation-inference:latest \
    --model-id mistralai/Mixtral-8x7B-Instruct-v0.1

# Environment variables
export IRONSHELL_AI=tgi
export TGI_URL=http://localhost:8080  # Default

# Use in IronShell
ish 'ai("tgi:mixtral", "Hello, world!")'
```

**Default Endpoint**: `http://localhost:8080`  
**Models**: Any HuggingFace model

### 6. **OpenAI-Compatible** (Generic)

For any server that implements the OpenAI API format (LocalAI, FastChat, etc.).

```bash
# Environment variables
export IRONSHELL_AI=compat
export IRONSHELL_COMPAT_BASE=http://localhost:8000/v1
export IRONSHELL_COMPAT_MODEL=mixtral

# Use in IronShell
ish 'ai("compat:mixtral", "Hello, world!")'
```

## Usage Examples

### Simple Chat

```bash
# Using environment variable
export IRONSHELL_AI=ollama
ish 'ai("What is the capital of France?")'

# Using model URI (overrides env)
ish 'ai("vllm:meta-llama/Llama-3-8B", "What is 2+2?")'
```

### In Scripts

```ironshell
# examples/ai_backends.ish

# OpenAI
let openai_response = ai("openai:gpt-4o-mini", "Explain quantum computing")

# Local Ollama
let local_response = ai("ollama:llama3", "Write a haiku")

# vLLM high-performance
let vllm_response = ai("vllm:meta-llama/Llama-3-70B", "Complex reasoning task")

# llama.cpp efficient
let llamacpp_response = ai("llamacpp:mistral-7b", "Quick factual answer")

print(openai_response)
```

### Agent with Specific Backend

```ironshell
# Use vLLM for a specific agent
let result = agent("vllm:meta-llama/Llama-3-8B", "Analyze this data", {
    tools: ["search", "calculate"]
})
```

### Multi-Agent Swarm with Different Backends

```ironshell
let agents = [
    {
        name: "researcher",
        model: "openai:gpt-4o",  # Cloud for research
        role: "Research and gather information"
    },
    {
        name: "coder",
        model: "vllm:codellama-34b",  # Local for code generation
        role: "Write and optimize code"
    },
    {
        name: "reviewer",
        model: "ollama:llama3",  # Local for review
        role: "Review and provide feedback"
    }
]

let result = agent_swarm(agents, "Build a web scraper")
```

## Configuration Priority

IronShell resolves AI backends in this order:

1. **Model URI in function call**: `ai("vllm:model", "prompt")`
2. **IRONSHELL_MODEL_URI environment variable**: `export IRONSHELL_MODEL_URI=vllm:model`
3. **IRONSHELL_AI environment variable**: `export IRONSHELL_AI=vllm`
4. **Auto-detection**: `export IRONSHELL_AI=auto` (automatically finds available backends)
5. **Default**: `stub` (for testing)

## Automatic Backend Detection

IronShell can automatically detect and use available AI backends:

### Using Auto-Detection

```bash
# Enable auto-detection via environment variable
export IRONSHELL_AI=auto
ish 'ai("What is 2+2?")'  # Automatically uses best available backend

# Or use detection functions in scripts
ish 'let backend = ai_detect(); ai(backend, "Hello!")'
```

### Detection Functions

**`ai_backends()`** - List all available backends:
```ironshell
let backends = ai_backends()
backends | foreach(fn(b) => {
    print("Backend:", b.name, "Available:", b.available)
    print("Models:", b.models)
})
```

**`ai_detect()`** - Auto-select best available backend:
```ironshell
let backend = ai_detect()
print("Using:", backend)  # e.g., "ollama:llama3" or "vllm:model"
ai(backend, "Hello!")
```

### Detection Priority

Auto-detection checks backends in this order (prefers local over cloud):

1. **Ollama** (`http://localhost:11434`) - Checks `/api/tags` endpoint
2. **vLLM** (`http://localhost:8000/v1`) - Checks `/v1/models` endpoint
3. **llama.cpp** (`http://localhost:8080/v1`) - Checks `/v1/models` endpoint
4. **TGI** (`http://localhost:8080`) - Checks `/health` endpoint
5. **OpenAI** (Cloud) - Checks for API key in environment or credential store

### Example: Conditional Backend Selection

```ironshell
# Get all available backends
let backends = ai_backends()

# Filter for local backends
let local = backends | where(fn(b) => 
    b.provider == "Ollama" || b.provider == "VLlm" || b.provider == "LlamaCpp"
)

# Use local if available, otherwise cloud
let backend = if len(local) > 0 then {
    let first = local | first
    first.provider + ":" + (first.models | first)
} else {
    "openai:gpt-4o-mini"
}

ai(backend, "Hello!")
```

### Backend Information Structure

Each backend object returned by `ai_backends()` contains:

```ironshell
{
    name: "Ollama",                    # Human-readable name
    provider: "Ollama",                # Provider type
    endpoint: "http://localhost:11434", # API endpoint URL
    available: true,                   # Connection status
    models: ["llama3", "mistral"]      # Available models
}
```

## Secure API Key Management

Store API keys securely using the OS credential store:

```bash
# Store OpenAI key
ish ai keys store openai --key sk-your-key-here

# Store Anthropic key
ish ai keys store anthropic --key your-key

# List stored keys
ish ai keys list

# Get a key (shows masked version)
ish ai keys get openai

# Delete a key
ish ai keys delete openai
```

Keys are stored in:
- **Windows**: Windows Credential Manager
- **macOS**: Keychain
- **Linux**: Secret Service (GNOME Keyring, KWallet)

## Performance Comparison

| Backend       | Use Case                    | Throughput | Latency | Cost        |
| ------------- | --------------------------- | ---------- | ------- | ----------- |
| **OpenAI**    | Production, complex tasks   | High       | Medium  | $$$         |
| **Ollama**    | Development, local testing  | Medium     | Low     | Free        |
| **vLLM**      | High-volume production      | Very High  | Low     | $ (hosting) |
| **llama.cpp** | Edge devices, CPU inference | Medium     | Low     | Free        |
| **TGI**       | HuggingFace ecosystem       | High       | Medium  | $ (hosting) |

## Troubleshooting

### Connection Issues

```bash
# Test backend connectivity
curl http://localhost:8000/v1/models  # vLLM
curl http://localhost:11434/api/tags   # Ollama
curl http://localhost:8080/health      # llama.cpp
```

### Environment Variables

```bash
# Show the available backends and which provider is selected.
# (There is no IRONSHELL_DEBUG; the source reads no such variable.)
ish 'ai_backends()'
echo "${IRONSHELL_AI:-unset - the default provider is used}"
```

### Port Conflicts

Default ports:
- Ollama: 11434
- vLLM: 8000
- llama.cpp: 8080
- TGI: 8080
- ish ai serve: 8080

Change ports if conflicts occur:

```bash
# vLLM custom port
python -m vllm.entrypoints.openai.api_server --port 8001
export VLLM_URL=http://localhost:8001/v1

# llama.cpp custom port
./server --port 8081
export LLAMACPP_URL=http://localhost:8081/v1
```

## MCP Server Integration

IronShell can automatically detect and integrate MCP (Model Context Protocol) servers to provide AI agents with tool access.

### Detecting MCP Servers

**`mcp_servers()`** - List all available MCP servers:
```ironshell
let servers = mcp_servers()
print("Found", len(servers), "MCP servers")

servers | foreach(fn(s) => {
    print("Server:", s.name)
    print("  Endpoint:", s.endpoint)
    print("  Tools:", len(s.tools))
})
```

**`mcp_detect(endpoint?)`** - Find specific or any MCP server:
```ironshell
# Find specific server
let fs_server = mcp_detect("http://localhost:3001")

# Or find first available
let any_server = mcp_detect()
```

### Default MCP Server Endpoints

IronShell checks these ports automatically:

- `3001` - Filesystem server
- `3002` - Git server
- `3003` - Docker server
- `3004` - AWS server
- `3005` - Database server
- `8080`, `8081` - Custom servers

### AI Backend + MCP Integration

Combine auto-detected AI backends with MCP servers:

```ironshell
# 1. Detect both AI and MCP
let backend = ai_detect()
let servers = mcp_servers()

# 2. Create agent with AI backend and MCP tools
let agent_config = {
    goal: "Analyze log files and generate report",
    model: backend,
    mcp_servers: servers,
    max_steps: 15
}

# 3. Use the integrated system
# The agent now has AI reasoning + tool access
```

### Example: Complete Integration

```ironshell
# Detect available resources
let ai_backends = ai_backends()
let mcp_servers = mcp_servers()

print("AI Backends:", len(ai_backends))
print("MCP Servers:", len(mcp_servers))

# Auto-select best configuration
let backend = ai_detect()
let server = mcp_detect()

# Create intelligent agent with tools
agent(
    "Analyze codebase and suggest improvements",
    backend,
    server.tools
)
```

## Best Practices

1. **Development**: Use Ollama for fast local iteration
2. **Production**: Use vLLM for high throughput, OpenAI for complex reasoning
3. **Edge/Mobile**: Use llama.cpp for efficient CPU inference
4. **Cost Optimization**: Mix local (vLLM, Ollama) and cloud (OpenAI) based on task complexity
5. **Multi-Agent**: Assign different backends to different agent roles based on requirements
6. **Tool Safety**: Use MCP servers for controlled tool access (safer than raw shell commands)
7. **Auto-Detection**: Combine `ai_detect()` and `mcp_detect()` for zero-configuration agents

## Additional Resources

- [Ollama Documentation](https://ollama.ai/docs)
- [vLLM Documentation](https://docs.vllm.ai/)
- [llama.cpp Repository](https://github.com/ggerganov/llama.cpp)
- [Text Generation Inference](https://huggingface.co/docs/text-generation-inference)
- [OpenAI API Reference](https://platform.openai.com/docs/api-reference)
- [MCP Servers Guide](MCP_SERVERS_GUIDE.md) - Detailed MCP server documentation
- [Example: AI + MCP Integration](../examples/14_ai_mcp_integration.ish)
