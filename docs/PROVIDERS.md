# AetherShell Universal Provider System

## Overview

AetherShell now includes a comprehensive provider abstraction layer that enables any AI model to interact with operating systems through a standardized interface. This system supports 19 LLM providers and includes a rich OS operation ontology.

## Architecture

### Core Components

```
src/providers/
├── mod.rs          # Core types (ModelUri, ProviderType, ProviderConfig)
├── traits.rs       # LLMProvider trait and request/response types
├── ontology.rs     # OS operations ontology (1900+ lines)
├── schema.rs       # Tool schema generation for different formats
├── registry.rs     # Provider registry with auto-discovery
├── bridge.rs       # Compatibility layer with existing ai.rs
└── impls/
    ├── mod.rs      # Provider factory
    ├── openai.rs   # OpenAI/OpenAI-compatible implementation
    ├── anthropic.rs # Native Anthropic Claude implementation
    ├── google.rs   # Native Google Gemini implementation
    └── ollama.rs   # Local Ollama implementation
```

### Supported Providers (19 total)

| Provider     | URI Scheme    | Example                           | Notes             |
| ------------ | ------------- | --------------------------------- | ----------------- |
| OpenAI       | `openai:`     | `openai:gpt-4o`                   | Full support      |
| Anthropic    | `anthropic:`  | `anthropic:claude-3-5-sonnet`     | Native API        |
| Google       | `google:`     | `google:gemini-1.5-pro`           | Native API        |
| Azure OpenAI | `azure:`      | `azure:gpt-4/deployment`          | OpenAI-compatible |
| AWS Bedrock  | `bedrock:`    | `bedrock:anthropic.claude-v2`     | OpenAI-compatible |
| Ollama       | `ollama:`     | `ollama:llama3`                   | Local, native API |
| Together AI  | `together:`   | `together:meta-llama/Llama-3-70b` | OpenAI-compatible |
| Groq         | `groq:`       | `groq:mixtral-8x7b`               | OpenAI-compatible |
| Mistral      | `mistral:`    | `mistral:mistral-large`           | OpenAI-compatible |
| Cohere       | `cohere:`     | `cohere:command-r-plus`           | OpenAI-compatible |
| Perplexity   | `perplexity:` | `perplexity:llama-3.1-sonar`      | OpenAI-compatible |
| Fireworks    | `fireworks:`  | `fireworks:llama-v3-70b`          | OpenAI-compatible |
| DeepSeek     | `deepseek:`   | `deepseek:deepseek-chat`          | OpenAI-compatible |
| xAI          | `xai:`        | `xai:grok-beta`                   | OpenAI-compatible |
| OpenRouter   | `openrouter:` | `openrouter:anthropic/claude-3`   | OpenAI-compatible |
| vLLM         | `vllm:`       | `vllm:meta-llama/Llama-3`         | OpenAI-compatible |
| TGI          | `tgi:`        | `tgi:mixtral`                     | OpenAI-compatible |
| llama.cpp    | `llamacpp:`   | `llamacpp:model.gguf`             | OpenAI-compatible |
| Local/Custom | `local:`      | `local:http://localhost:8080`     | OpenAI-compatible |

### OS Operation Ontology

The ontology defines 19 capability domains with 80+ operations:

| Domain      | Operations                            | Description                   |
| ----------- | ------------------------------------- | ----------------------------- |
| FileSystem  | read_file, write_file, list_dir, etc. | File and directory operations |
| Process     | run_command, kill_process, etc.       | Process management            |
| Network     | http_request, ping, dns_lookup        | Network operations            |
| Environment | get_env, set_env, list_env            | Environment variables         |
| User        | get_current_user, list_users          | User management               |
| Package     | install_package, list_packages        | Package managers              |
| Service     | start_service, stop_service           | System services               |
| Cron        | schedule_job, list_jobs               | Scheduled tasks               |
| Clipboard   | get_clipboard, set_clipboard          | Clipboard access              |
| System      | get_cpu_info, get_memory_info         | System information            |
| Git         | git_status, git_commit                | Version control               |
| Docker      | docker_run, docker_ps                 | Containerization              |
| Database    | query_db, execute_sql                 | Database access               |
| Web         | fetch_url, parse_html                 | Web scraping                  |
| AI          | complete_prompt, embed_text           | AI operations                 |
| Search      | grep_files, find_files                | File search                   |
| Archive     | compress_files, extract_archive       | Compression                   |
| Security    | encrypt_data, hash_data               | Security operations           |
| Shell       | eval_script, source_file              | Shell evaluation              |

### Tool Schema Generation

The system generates tool schemas in multiple formats:

```rust
// OpenAI format
let schema = ToolSchemaGenerator::new(ToolFormat::OpenAI)
    .generate(&os_operation);

// Anthropic format
let schema = ToolSchemaGenerator::new(ToolFormat::Anthropic)
    .generate(&os_operation);

// Google format
let schema = ToolSchemaGenerator::new(ToolFormat::Google)
    .generate(&os_operation);
```

## Usage

### Basic Chat

```rust
use aethershell::providers::{
    create_provider, ChatRequest, ModelUri, ProviderConfig, ProviderType
};

// Parse model URI
let uri = ModelUri::parse("openai:gpt-4o")?;

// Create provider from environment
let config = ProviderConfig::from_env(uri.provider);
let provider = create_provider(config);

// Make chat request
let request = ChatRequest::simple(uri, "Hello, world!");
let response = provider.chat(request).await?;

println!("{}", response.text());
```

### With Tools

```rust
use aethershell::providers::{ChatRequest, ToolSchema};

let tools = vec![
    ToolSchema::new("read_file", "Read a file")
        .add_param("path", "string", "File path to read", true),
];

let request = ChatRequest::new(uri, messages).with_tools(tools);
let response = provider.chat(request).await?;

if let Some(tool_calls) = response.tool_calls {
    for call in tool_calls {
        println!("Tool: {}, Args: {}", call.name, call.arguments);
    }
}
```

### Embeddings

```rust
use aethershell::providers::EmbeddingRequest;

let request = EmbeddingRequest {
    model: ModelUri::parse("openai:text-embedding-3-small")?,
    input: vec!["Hello".to_string(), "World".to_string()],
    dimensions: None,
};

let response = provider.embed(request).await?;
```

### Provider Registry

```rust
use aethershell::providers::ProviderRegistry;

let registry = ProviderRegistry::new();
registry.add_provider(config, weight)?;

// Auto-select based on request
let provider = registry.select_provider(&request)?;

// Or route with custom conditions
let provider = registry.route_with_condition(RoutingCondition::HasTools)?;
```

## Environment Variables

| Variable               | Provider  | Required |
| ---------------------- | --------- | -------- |
| `OPENAI_API_KEY`       | OpenAI    | Yes      |
| `ANTHROPIC_API_KEY`    | Anthropic | Yes      |
| `GOOGLE_API_KEY`       | Google    | Yes      |
| `AZURE_OPENAI_API_KEY` | Azure     | Yes      |
| `TOGETHER_API_KEY`     | Together  | Yes      |
| `GROQ_API_KEY`         | Groq      | Yes      |
| ...                    | ...       | ...      |

## Testing

```bash
# Run all provider tests
cargo test providers:: --lib

# Run specific implementation tests
cargo test providers::impls:: --lib
```

## Integration with Existing Code

The bridge module provides compatibility with the existing `ai.rs` module:

```rust
use aethershell::providers::bridge::{
    complete_with_provider,
    chat_with_provider,
    UniversalBackend,
};

// Simple completion
let response = complete_with_provider("Hello!", Some("openai:gpt-4o"))?;

// Use as LlmBackend
let backend = UniversalBackend::from_uri("anthropic:claude-3-5-sonnet")?;
let response = backend.chat(&messages)?;
```

## Future Enhancements

- [ ] Streaming support for all providers
- [ ] AWS Bedrock native implementation
- [ ] Cohere native implementation
- [ ] Model cost tracking
- [ ] Request caching
- [ ] Automatic failover
- [ ] Load balancing across providers
