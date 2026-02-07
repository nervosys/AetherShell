# AI Model API

The AI Model API provides an OpenAI-compatible HTTP interface for managing local AI models and performing inference. It supports model downloading, format conversion, and serving multiple providers through a unified API.

## Starting the Server

```bash
aimodel serve                   # Start on default port
aimodel serve --port 8080       # Custom port
```

## OpenAI-Compatible Endpoints

### POST `/v1/chat/completions`
Chat completion API, compatible with the OpenAI format.

**Request:**
```json
{
  "model": "llama3",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user", "content": "What is Rust?" }
  ],
  "temperature": 0.7,
  "max_tokens": 1024,
  "stream": false
}
```

**Response:**
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1705300000,
  "model": "llama3",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Rust is a systems programming language..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 25,
    "completion_tokens": 150,
    "total_tokens": 175
  }
}
```

**Streaming:** Set `"stream": true` to receive Server-Sent Events:

```
data: {"id":"chatcmpl-abc123","choices":[{"delta":{"content":"Rust"},"index":0}]}

data: {"id":"chatcmpl-abc123","choices":[{"delta":{"content":" is"},"index":0}]}

data: [DONE]
```

### POST `/v1/embeddings`
Generate text embeddings.

**Request:**
```json
{
  "model": "nomic-embed-text",
  "input": "What is the meaning of life?"
}
```

**Response:**
```json
{
  "object": "list",
  "data": [
    {
      "object": "embedding",
      "index": 0,
      "embedding": [0.023, -0.041, 0.015, ...]
    }
  ],
  "model": "nomic-embed-text",
  "usage": { "prompt_tokens": 8, "total_tokens": 8 }
}
```

## Model Management

### GET `/v1/models`
List all available models.

**Response:**
```json
{
  "object": "list",
  "data": [
    {
      "id": "llama3",
      "object": "model",
      "owned_by": "local",
      "created": 1705300000
    }
  ]
}
```

### GET `/v1/models/:model_id`
Get details about a specific model.

### POST `/v1/models/:model_id/download`
Download a model from a supported source.

**Request:**
```json
{
  "source": "huggingface",
  "revision": "main"
}
```

### POST `/v1/models/:model_id/convert`
Convert a model between formats (e.g., GGUF, ONNX).

**Request:**
```json
{
  "target_format": "gguf",
  "quantization": "q4_0"
}
```

### DELETE `/v1/models/:model_id`
Delete a downloaded model.

## Provider Management

### GET `/v1/providers`
List configured inference providers.

**Response:**
```json
[
  { "id": "ollama", "status": "available", "url": "http://localhost:11434" },
  { "id": "openai", "status": "available" },
  { "id": "vllm", "status": "unavailable" }
]
```

### POST `/v1/providers/:provider_id/validate`
Test connectivity to a provider.

**Response:**
```json
{
  "provider": "ollama",
  "valid": true,
  "latency_ms": 12,
  "models_available": 3
}
```

## Storage Management

### GET `/v1/storage/stats`
Get storage usage statistics for downloaded models.

**Response:**
```json
{
  "total_size_bytes": 15000000000,
  "model_count": 5,
  "cache_size_bytes": 500000000,
  "storage_path": "/home/user/.aethershell/models"
}
```

### POST `/v1/storage/cleanup`
Clean up cached files and temporary data.

## Health & Status

### GET `/v1/health`
Quick health check.

**Response:**
```json
{ "status": "ok" }
```

### GET `/v1/status`
Detailed server status with provider information.

**Response:**
```json
{
  "status": "running",
  "version": "0.3.0",
  "uptime_seconds": 7200,
  "providers": { "ollama": "connected", "openai": "configured" },
  "models_loaded": 2,
  "requests_served": 150
}
```

## Documentation

### GET `/swagger-ui`
Interactive API documentation (when `enable_openapi` is configured).

### GET `/api-docs/openapi.json`
OpenAPI specification in JSON format.

## Client Usage

The AI Model API is compatible with any OpenAI client library:

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://localhost:8080/v1",
    api_key="not-needed"  # Local models don't need keys
)

response = client.chat.completions.create(
    model="llama3",
    messages=[{"role": "user", "content": "Hello!"}]
)
print(response.choices[0].message.content)
```

```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model":"llama3","messages":[{"role":"user","content":"Hello"}]}'
```
