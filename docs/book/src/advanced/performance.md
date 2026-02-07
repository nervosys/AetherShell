# Performance

Tips and techniques for optimizing AetherShell performance in production workloads.

## Pipeline Optimization

### Lazy Evaluation

AetherShell pipelines process elements lazily where possible. Order your operations to minimize work:

```aethershell
# Good: filter first, then transform
ls "." | where(fn(f) => f.size > 10000) | map(fn(f) => expensive_operation(f))

# Bad: transform everything, then filter
ls "." | map(fn(f) => expensive_operation(f)) | where(fn(f) => f.size > 10000)
```

### Use `take` Early

Limit results early to avoid processing unnecessary elements:

```aethershell
# Good: limit early
ls "." | sort_by "size" "desc" | take 5

# Bad: process all, then take (sort_by processes all anyway, but downstream operations are limited)
```

### Prefer Builtins Over AI

Builtins execute instantly; AI calls have network latency:

```aethershell
# Fast: builtin string operation
"hello world" | upper

# Slow: AI for simple text tasks
ai "Convert to uppercase: hello world"
```

## AI Performance

### Semantic Caching

Avoid redundant API calls by caching responses:

```aethershell
# First call: hits API
let answer = ai "What is Rust?"
semantic_cache "What is Rust?" answer

# Second call: cache hit (< 1ms vs ~500ms API call)
let cached = semantic_cache_get "Tell me about Rust"
```

### Model Selection

Choose the right model for the task:

| Model | Speed | Cost | Best For |
|-------|-------|------|----------|
| `gpt-4o-mini` | Fast | Low | Simple queries, classification |
| `gpt-4o` | Medium | High | Complex reasoning, code generation |
| `ollama:llama3` | Variable | Free | Privacy-sensitive, offline |

### Batch Operations

Process multiple items in a single AI call when possible:

```aethershell
# One call for multiple items (faster)
let items = join(data, "\n")
ai "Classify each line:\n${items}"

# vs N separate calls (slower)
data | map(fn(item) => ai "Classify: ${item}")
```

## Agent Performance

### Limit Max Steps

Set appropriate `max_steps` to prevent runaway agents:

```aethershell
# Quick lookup: 3-5 steps
agent "What's in src/main.rs?" ["cat"] 3

# Deep analysis: 10-15 steps
agent "Full project review" ["ls", "cat", "grep"] 15
```

### Targeted Tool Sets

Give agents only the tools they need—fewer tools means faster decisions:

```aethershell
# Good: specific tools
agent "Find TODOs" ["grep"]

# Bad: everything
agent "Find TODOs" ["ls", "cat", "grep", "find", "wc", "head", "tail", "sort"]
```

## RAG Performance

### Index Size

The built-in RAG uses in-memory indexing with hash-based embeddings. Performance characteristics:

| Documents | Index Time | Search Time |
|-----------|-----------|-------------|
| 100 | < 100ms | < 10ms |
| 1,000 | < 1s | < 50ms |
| 10,000 | < 10s | < 200ms |

### Search Tuning

Adjust `top_k` based on your needs:

```aethershell
rag_search query 3    # Fast, fewer results
rag_search query 20   # Slower, more comprehensive
```

## Memory Management

### Max Messages

The TUI limits chat history to prevent memory growth:

```
config.max_messages = 1000    # Default
```

Older messages are dropped when the limit is reached.

### Cache Limits

Semantic cache has built-in limits:
- **Max entries**: 1,000
- **TTL**: 1 hour
- Oldest entries evicted automatically

## Build Optimization

### Release Builds

Always use release builds for production:

```bash
cargo build --release --bins
```

Release builds are significantly faster than debug builds (10-100x for CPU-bound operations).

### Binary Size

Strip debug symbols for smaller binaries:

```bash
cargo build --release
strip target/release/ae
```
