# AI Providers

AetherShell has built-in AI capabilities with a provider-agnostic architecture. Connect to cloud APIs, local models, or self-hosted inference servers — all using the same simple syntax.

## Quick Start

```aethershell
# Set your API key
set_env "OPENAI_API_KEY" "sk-..."

# Ask a question
ai "What is Rust's ownership model?"

# Specify a model
ai "Explain monads" { model: "openai:gpt-4o" }
```

## Model URI Scheme

AetherShell uses a URI scheme to reference models across providers:

| URI | Provider | Example |
|-----|----------|---------|
| `openai:model-name` | OpenAI | `openai:gpt-4o-mini` |
| `ollama:model-name` | Ollama (local) | `ollama:llama3` |
| `compat:model-name` | OpenAI-compatible API | `compat:mixtral` |
| `tgi:model-name` | HuggingFace TGI | `tgi:mistral-7b` |
| `vllm:model-name` | vLLM | `vllm:meta-llama/Llama-3-8B` |
| `llamacpp:model-name` | llama.cpp | `llamacpp:mistral-7b` |

```aethershell
# Use different providers
ai "Hello" { model: "openai:gpt-4o" }
ai "Hello" { model: "ollama:llama3" }
ai "Hello" { model: "compat:mixtral" }
```

## Providers

### OpenAI
The default cloud provider. Requires an API key.

```aethershell
set_env "OPENAI_API_KEY" "sk-..."
set_env "AETHER_AI" "openai"

ai "Explain closures in Rust"
```

**Environment variables:**
- `OPENAI_API_KEY` — API authentication key
- `OPENAI_MODEL` — Default model (default: `gpt-4o-mini`)

### Ollama (Local)
Run models locally with [Ollama](https://ollama.ai). No API key needed.

```aethershell
set_env "AETHER_AI" "ollama"

ai "Summarize this code" { model: "ollama:codellama" }
```

**Environment variables:**
- `OLLAMA_URL` — Ollama endpoint (default: `http://localhost:11434`)
- `OLLAMA_MODEL` — Default model (default: `llama3`)

### OpenAI-Compatible
Any server implementing the OpenAI API format (LiteLLM, LocalAI, etc.).

```aethershell
set_env "AETHER_AI" "compat"
set_env "AETHER_COMPAT_BASE" "http://localhost:8000/v1"

ai "Hello" { model: "compat:mixtral" }
```

**Environment variables:**
- `AETHER_COMPAT_BASE` — API base URL (default: `http://localhost:8000/v1`)
- `AETHER_COMPAT_MODEL` — Default model (default: `mixtral`)

### HuggingFace TGI
Connect to a Text Generation Inference server.

```aethershell
set_env "AETHER_AI" "tgi"
set_env "TGI_URL" "http://localhost:8080"
```

### vLLM
Connect to a vLLM inference server.

```aethershell
set_env "VLLM_URL" "http://localhost:8000/v1"
set_env "VLLM_MODEL" "meta-llama/Llama-3-8B"
```

### llama.cpp
Connect to a llama.cpp server.

```aethershell
set_env "LLAMACPP_URL" "http://localhost:8080/v1"
```

## Provider Selection

The `AETHER_AI` environment variable selects the default provider:

```aethershell
set_env "AETHER_AI" "openai"    # Use OpenAI
set_env "AETHER_AI" "ollama"    # Use Ollama
set_env "AETHER_AI" "compat"    # Use OpenAI-compatible server
set_env "AETHER_AI" "tgi"       # Use TGI
```

Override per-call with the `model` option:

```aethershell
# Default is OpenAI, but use Ollama for this one call
ai "Quick question" { model: "ollama:llama3" }
```

## Multimodal AI

AetherShell supports sending images, audio, and video to models that accept them.

### Images
```aethershell
ai "Describe this image" { images: ["photo.jpg"] }
ai "Compare these" { images: ["before.png", "after.png"] }
```

### Audio
```aethershell
ai "Transcribe this recording" { audio: ["meeting.mp3"] }
```

### Video
```aethershell
ai "What happens in this clip?" { video: ["demo.mp4"] }
```

### Combined
```aethershell
ai "Analyze this screenshot and narration" {
  images: ["screen.png"],
  audio: ["narration.mp3"]
}
```

> **Note:** Multimodal support depends on the provider. OpenAI supports images; Ollama supports images with vision models. Audio and video support varies by model.

## Backend Detection

Discover which AI backends are available on your system:

```aethershell
ai_backends
# [
#   { provider: "openai", available: true, model: "gpt-4o-mini" },
#   { provider: "ollama", available: true, url: "http://localhost:11434", models: ["llama3", "codellama"] },
#   { provider: "vllm", available: false },
#   ...
# ]
```

## AI Shell Helpers

Built-in AI-powered shell assistance:

```aethershell
# Get command suggestions
ai-suggest "find all rust files larger than 10KB"
# Suggests: ls "." | where(fn(f) => f.extension == "rs" && f.size > 10240)

# Explain a command
ai-explain 'ls "src" | where(fn(f) => f.size > 1000) | sort_by "size" "desc"'

# Fix a broken command
ai-fix 'ls src | filter(size > 100)'

# AI-powered tab completion
ai-complete "ls src | wh"
```

## Pipeline Integration

AI calls compose naturally with pipelines:

```aethershell
# Summarize a file
cat "README.md" | ai "Summarize this document"

# Classify data
["bug report", "feature request", "question"]
  | map(fn(item) => {
      let category = ai "Classify: ${item}" { model: "openai:gpt-4o-mini" }
      { text: item, category: category }
  })

# Generate documentation
ls "src" 
  | where(fn(f) => f.extension == "rs")
  | map(fn(f) => { file: f.name, doc: ai "Write a one-line description of: ${cat f.path}" })
```

## Global Override

Set a global model URI that overrides all defaults:

```aethershell
set_env "AETHER_MODEL_URI" "ollama:codellama"
# All ai/agent calls now use this model unless explicitly overridden
```
