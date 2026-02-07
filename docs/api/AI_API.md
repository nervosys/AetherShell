# AI Model API Reference

The AI Model API provides an OpenAI-compatible HTTP server for managing AI models, running inference, and interacting with multiple providers. Built with Axum and documented via OpenAPI/Swagger UI.

**Default server:** configured via `APIConfig` (host/port)  
**Base path:** `/v1`  
**Swagger UI:** `/swagger-ui` (when `enable_openapi` is true)

---

## Authentication

When `security.require_api_key` is enabled, all inference endpoints require:

```
Authorization: Bearer <api_key>
```

Returns `401 Unauthorized` with:
```json
{
  "error": {
    "message": "Missing API key" | "Invalid API key",
    "type": "authentication_error"
  }
}
```

---

## Error Format

All error responses use the standard `APIError` envelope:

```json
{
  "error": {
    "message": "Human-readable error description",
    "type": "internal_error | not_found | invalid_request_error | ...",
    "param": "field_name (optional)",
    "code": "error_code (optional)"
  }
}
```

---

## Model Management

### GET `/v1/models`

List available models from all registered providers.

| Query param  | Type   | Description                                                                                 |
| ------------ | ------ | ------------------------------------------------------------------------------------------- |
| `provider`   | string | Filter by provider name (`openai`, `anthropic`, `local`, etc.)                              |
| `capability` | string | Filter: `chat`, `embeddings`, `image_generation`, `image_understanding`, `function_calling` |
| `local_only` | bool   | Only return locally stored models                                                           |

**Response (200):** `ModelInfo[]`
```json
[
  {
    "id": "gpt-4o",
    "object": "model",
    "created": 1707300000,
    "owned_by": "openai",
    "provider": "openai",
    "context_length": 128000,
    "max_output": 4096,
    "capabilities": {
      "chat": true,
      "completions": true,
      "embeddings": false,
      "image_generation": false,
      "image_understanding": true,
      "audio_generation": false,
      "audio_understanding": false,
      "video_understanding": false,
      "function_calling": true,
      "streaming": true
    },
    "format": "OpenAI",
    "size_bytes": null,
    "local_path": null,
    "pricing": { "prompt": 0.005, "completion": 0.015 },
    "metadata": {}
  }
]
```

### GET `/v1/models/:model_id`

Get details for a specific model.

**Response (200):** `ModelInfo`  
**Response (404):** `APIError` with `type: "not_found"`

---

## Inference

### POST `/v1/chat/completions`

OpenAI-compatible chat completion. Supports both streaming and non-streaming.

**Request body (`ChatCompletionRequest`):**
```json
{
  "model": "gpt-4o",
  "provider": "openai",
  "messages": [
    { "role": "system", "content": "You are a helpful assistant." },
    { "role": "user", "content": "Hello!" }
  ],
  "max_tokens": 1024,
  "temperature": 0.7,
  "top_p": 1.0,
  "n": 1,
  "stream": false,
  "stop": ["\n"],
  "presence_penalty": 0.0,
  "frequency_penalty": 0.0,
  "tools": [
    {
      "type": "function",
      "function": {
        "name": "get_weather",
        "description": "Get weather for a city",
        "parameters": { "type": "object", "properties": { "city": { "type": "string" } } }
      }
    }
  ],
  "tool_choice": "auto",
  "user": "user-123"
}
```

**Non-streaming response (200):** `ChatCompletionResponse`
```json
{
  "id": "chatcmpl-abc123",
  "object": "chat.completion",
  "created": 1707300000,
  "model": "gpt-4o",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Hello! How can I help you?"
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 15,
    "completion_tokens": 8,
    "total_tokens": 23
  }
}
```

**Streaming response (`stream: true`):** SSE stream of `ChatCompletionChunk`

Each chunk is sent as an SSE `data:` event:
```
data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1707300000,"model":"gpt-4o","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1707300000,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-abc123","object":"chat.completion.chunk","created":1707300000,"model":"gpt-4o","choices":[{"index":0,"delta":{"content":"!"},"finish_reason":"stop"}]}

data: [DONE]
```

Keep-alive: every 15 seconds.

### POST `/v1/embeddings`

Generate embeddings for input text.

**Request body (`EmbeddingRequest`):**
```json
{
  "model": "text-embedding-3-small",
  "provider": "openai",
  "input": ["Hello world", "Goodbye world"],
  "encoding_format": "float",
  "dimensions": 1536,
  "user": "user-123"
}
```

**Response (200):** `EmbeddingResponse`
```json
{
  "object": "list",
  "data": [
    { "object": "embedding", "index": 0, "embedding": [0.0023, -0.0091, ...] }
  ],
  "model": "text-embedding-3-small",
  "usage": { "prompt_tokens": 4, "completion_tokens": 0, "total_tokens": 4 }
}
```

---

## Model Lifecycle

### POST `/v1/models/:model_id/download`

Download a model from a remote source.

**Request body (optional):**
```json
{
  "source": "huggingface",
  "format": "gguf",
  "quantization": "Q4_K_M"
}
```

| Field          | Type   | Default       | Description                                                |
| -------------- | ------ | ------------- | ---------------------------------------------------------- |
| `source`       | string | `huggingface` | Source: `huggingface`, `url`                               |
| `format`       | string | auto-detect   | Preferred format: `gguf`, `safetensors`, `pytorch`, `onnx` |
| `quantization` | string | none          | Quantization level (e.g., `Q4_K_M`)                        |

**Response (200):**
```json
{
  "success": true,
  "model_id": "llama-3-8b",
  "size_bytes": 4500000000,
  "format": "GGUF",
  "file_path": "/path/to/model.gguf",
  "sha256": "abc123...",
  "downloaded_at": "2026-02-07T12:00:00Z"
}
```

### POST `/v1/models/:model_id/convert`

Convert a downloaded model to a different format.

**Request body:**
```json
{
  "target_format": "gguf",
  "output_id": "my-model-gguf",
  "quantization": "Q4_K_M"
}
```

**Response (200):**
```json
{
  "status": "queued",
  "source_model": "llama-3-8b",
  "source_format": "SafeTensors",
  "target_format": "GGUF",
  "output_id": "llama-3-8b_gguf",
  "message": "Model conversion has been queued..."
}
```

Supported formats: `gguf`, `safetensors`, `pytorch`/`pt`, `onnx`

### DELETE `/v1/models/:model_id`

Delete a locally stored model.

**Response (200):**
```json
{ "success": true, "model_id": "llama-3-8b", "message": "Model successfully deleted" }
```

---

## Provider Management

### GET `/v1/providers`

List registered providers.

**Response (200):** `ProviderInfo[]`
```json
[
  { "id": "openai", "name": "OpenAI", "description": "OpenAI GPT models", "enabled": true, "status": "active" },
  { "id": "anthropic", "name": "Anthropic", "description": "Anthropic Claude models", "enabled": true, "status": "active" },
  { "id": "local", "name": "Local Models", "description": "Locally hosted models", "enabled": true, "status": "active" }
]
```

Dynamically detected backends: `vllm` (`:8000`), `tensorrt-llm` (`:8001`), `sglang` (`:30000`), `llama.cpp` (`:8080`).

### POST `/v1/providers/:provider_id/validate`

Test provider connectivity and credentials.

**Response (200):**
```json
{
  "valid": true,
  "provider_id": "openai",
  "status": "API key configured",
  "checked_at": "2026-02-07T12:00:00Z"
}
```

---

## Storage

### GET `/v1/storage/stats`

Get storage statistics for downloaded models.

**Response (200):**
```json
{
  "total_models": 5,
  "total_size": "12.34 GB",
  "total_size_bytes": 13249974272,
  "cache_size": "1.5 GB",
  "cache_size_bytes": 1610612736,
  "data_directory": "/home/user/.local/share/aethershell",
  "config_directory": "/home/user/.config/aethershell",
  "format_breakdown": { "GGUF": 3, "SafeTensors": 2 }
}
```

### POST `/v1/storage/cleanup`

Clean up cache files older than 30 days.

**Response (200):**
```json
{
  "success": true,
  "freed_bytes": 524288000,
  "freed_human": "500.00 MB",
  "cleanup_age_days": 30,
  "timestamp": "2026-02-07T12:00:00Z"
}
```

---

## Health & Status

### GET `/v1/health`

```json
{ "status": "healthy", "timestamp": "2026-02-07T12:00:00Z" }
```

### GET `/v1/status`

```json
{
  "status": "running",
  "version": "0.x.x",
  "config": { "cors_enabled": true, "openapi_enabled": true },
  "uptime": "1d 2h 30m 45s",
  "uptime_seconds": 95445,
  "timestamp": "2026-02-07T12:00:00Z"
}
```

---

## Security Headers

All responses include:

| Header                      | Value                                      |
| --------------------------- | ------------------------------------------ |
| `X-Content-Type-Options`    | `nosniff`                                  |
| `X-Frame-Options`           | `DENY`                                     |
| `X-XSS-Protection`          | `1; mode=block`                            |
| `Strict-Transport-Security` | `max-age=31536000; includeSubDomains`      |
| `Referrer-Policy`           | `strict-origin-when-cross-origin`          |
| `Permissions-Policy`        | `geolocation=(), microphone=(), camera=()` |
