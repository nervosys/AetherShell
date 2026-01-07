# Quick Start: AI Backend Setup

## 1. Ollama (Easiest - Local)

```bash
# Install Ollama
curl -fsSL https://ollama.com/install.sh | sh

# Start server
ollama serve

# Pull a model
ollama pull llama3

# Use in AetherShell
export AETHER_AI=ollama
ae 'ai("What is the meaning of life?")'
```

## 2. vLLM (High Performance - Local)

```bash
# Install vLLM
pip install vllm

# Start server
python -m vllm.entrypoints.openai.api_server \
    --model meta-llama/Llama-3-8B-Instruct \
    --port 8000

# Use in AetherShell
export AETHER_AI=vllm
ae 'ai("vllm:meta-llama/Llama-3-8B-Instruct", "Hello!")'
```

## 3. llama.cpp (CPU Optimized - Local)

```bash
# Clone and build
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp
make

# Download GGUF model
wget https://huggingface.co/TheBloke/Llama-2-7B-Chat-GGUF/resolve/main/llama-2-7b-chat.Q4_K_M.gguf

# Start server
./server -m llama-2-7b-chat.Q4_K_M.gguf --port 8080

# Use in AetherShell
export AETHER_AI=llamacpp
ae 'ai("llamacpp:model", "Hello!")'
```

## 4. OpenAI (Cloud)

```bash
# Get API key from https://platform.openai.com/api-keys

# Store securely
ae ai keys store openai --key sk-your-key-here

# Use in AetherShell
export AETHER_AI=openai
ae 'ai("What are the latest AI trends?")'
```

## Environment Variables Quick Reference

```bash
# Default backend
export AETHER_AI=ollama        # or: openai, vllm, llamacpp, tgi, compat

# Model URI (overrides AETHER_AI)
export AETHER_MODEL_URI=vllm:meta-llama/Llama-3-8B

# Backend-specific URLs
export OLLAMA_HOST=http://localhost:11434
export VLLM_URL=http://localhost:8000/v1
export LLAMACPP_URL=http://localhost:8080/v1
export TGI_URL=http://localhost:8080
export AETHER_COMPAT_BASE=http://localhost:8000/v1

# Backend-specific models
export OLLAMA_MODEL=llama3
export VLLM_MODEL=meta-llama/Llama-3-8B
export LLAMACPP_MODEL=model
export OPENAI_MODEL=gpt-4o-mini
export AETHER_COMPAT_MODEL=mixtral
```

## Usage Patterns

```bash
# Use default backend from environment
ae 'ai("What is 2+2?")'

# Override with specific backend
ae 'ai("vllm:model", "Complex task")'

# In scripts
ae script.ae  # Uses AETHER_AI environment variable

# Multi-agent with different backends
ae 'agent_swarm([
    {name: "planner", model: "openai:gpt-4o"},
    {name: "coder", model: "vllm:codellama"},
    {name: "reviewer", model: "ollama:llama3"}
], "Build a tool")'
```

## Testing Connectivity

```bash
# Test Ollama
curl http://localhost:11434/api/tags

# Test vLLM
curl http://localhost:8000/v1/models

# Test llama.cpp
curl http://localhost:8080/v1/models

# Test in AetherShell
ae 'ai("test connection")'
```

## Recommended Setup for Different Use Cases

### Development & Testing
```bash
export AETHER_AI=ollama
ollama serve
ollama pull llama3
```

### High-Performance Production
```bash
export AETHER_AI=vllm
python -m vllm.entrypoints.openai.api_server \
    --model meta-llama/Llama-3-70B-Instruct \
    --tensor-parallel-size 4
```

### Low-Resource / CPU Only
```bash
export AETHER_AI=llamacpp
./server -m model.gguf --n-gpu-layers 0
```

### Cloud / Complex Tasks
```bash
ae ai keys store openai --key sk-your-key
export AETHER_AI=openai
```

## Troubleshooting

**Problem**: Connection refused  
**Solution**: Make sure the backend server is running on the correct port

**Problem**: Model not found  
**Solution**: Check that the model is downloaded/pulled for local backends

**Problem**: Out of memory  
**Solution**: Use smaller models or quantized versions (Q4, Q5)

**Problem**: Slow inference  
**Solution**: Use vLLM for GPU or llama.cpp with GPU layers
