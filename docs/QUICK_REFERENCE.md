# AetherShell Quick Reference Guide

One-page reference for the most common AetherShell patterns and syntax.

## Type System

### Type Inference vs Explicit Types

```aethershell
# Type Inference (:=) - Let the compiler figure it out
name := "AetherShell"           # String
count := 42                     # Int
price := 19.99                  # Float
items := [1, 2, 3]              # Array<Int>
config := {host: "localhost"}   # Record{host: String}

# Explicit Types (=) - Declare the type
name: String = "AetherShell"
count: Int = 42
price: Float = 19.99
items: Array<Int> = [1, 2, 3]
config: Record = {host: "localhost"}
```

**Rule of Thumb**: Use `:=` by default. Use `=` when type isn't obvious or for documentation.

## Core Syntax

### Variables and Functions

```aethershell
# Simple variable
x := 42

# Lambda function
double := fn(x) => x * 2

# Function with explicit signature
add: fn(Int, Int) -> Int = fn(a, b) => a + b

# Multiple parameters
calculate := fn(x, y, z) => (x + y) * z
```

### Pipelines

```aethershell
# Basic pipeline
[1, 2, 3] | map(fn(x) => x * 2)

# Multi-stage pipeline
data
  | filter(fn(x) => x > 10)
  | map(fn(x) => x * 2)
  | reduce(fn(a, b) => a + b, 0)

# Working with files
ls(".")
  | where(fn(f) => f.size > 1000)
  | select("name", "size")
  | sort_by("size")
```

### Pattern Matching

```aethershell
result := match status {
  200 => "OK",
  404 => "Not Found",
  500 => "Server Error",
  _ => "Unknown"
}

# Match with Option
value := match find("key") {
  Some(v) => v,
  None => "default"
}
```

### Control Flow

```aethershell
# If expression
result := if x > 10 {
  "large"
} else {
  "small"
}

# Where clause (filtering)
items | where(fn(x) => x.price < 100)
```

## AI Features

### Basic AI Call

```aethershell
response := ai("What is Rust?", {
  model: "openai:gpt-4o-mini"
})
```

### AI with Context

```aethershell
context := read_text("report.txt")
summary := ai("Summarize this report", {
  model: "openai:gpt-4o",
  context: context
})
```

### Multi-modal AI

```aethershell
# Analyze image
analysis := ai("Describe this image", {
  images: ["photo.jpg"]
})

# Process audio
transcription := ai("Transcribe this audio", {
  audio: ["meeting.mp3"]
})

# Multi-modal
result := ai("Analyze this content", {
  images: ["chart.png"],
  audio: ["presentation.mp3"],
  video: ["demo.mp4"]
})
```

## Agents

### Simple Agent

```aethershell
researcher := agent("Research Rust programming", [
  "http_get",
  "read_text",
  "write_text"
])

result := researcher.execute({
  task: "Find best Rust web frameworks"
})
```

### Agent Swarm

```aethershell
swarm := swarm([
  {
    id: "researcher",
    model: "openai:gpt-4o",
    role: "Research and gather information"
  },
  {
    id: "writer",
    model: "anthropic:claude-3-opus",
    role: "Write comprehensive documentation"
  },
  {
    id: "reviewer",
    model: "openai:gpt-4o",
    role: "Review and improve quality"
  }
], "router")  # or "round-robin", "blackboard"

result := swarm.execute("Create documentation for REST API")
```

## MCP Servers

### Start MCP Server

```aethershell
# Filesystem server
fs_server := mcp_server_start({
  name: "filesystem",
  type: "builtin",
  config: {
    allowed_paths: ["/home/user/projects"]
  }
})

# AWS server
aws_server := mcp_server_start({
  name: "aws",
  type: "cloud",
  config: {
    provider: "aws",
    services: ["s3", "ec2"],
    readonly: true
  }
})
```

### Agent with MCP Tools

```aethershell
devops := agent_with_mcp(
  "DevOps automation agent",
  ["mcp:list_files", "mcp:read_file", "mcp:git_status"],
  fs_server.endpoint
)

result := devops.execute({
  task: "Check Git status and list modified files"
})
```

## AI Protocols

### A2A (Agent-to-Agent Communication)

```aethershell
# Register agents by name -- there is no bus object
a2a_register("agent1")
a2a_register("agent2")
a2a_agents()                  # => [agent1, agent2]

# Send and receive
a2a_send("agent2", "Process this data")
a2a_broadcast("shutting down")
a2a_receive()

a2a_discover()
a2a_status()                  # => {active: true, agents: 2, pending_messages: 0}
a2a_unregister("agent1")
```

### NANDA (Negotiation and Decision Aggregation)

```aethershell
# Propose a decision -- name and data, no coordinator object
let proposal = nanda_propose("TaskAllocation", {
  task: "Implement feature X",
  assignee: "agent2"
})
# => {id: "prop_…", name: "TaskAllocation", status: "pending", threshold: 0.5}

# Vote: proposal id and a boolean
nanda_vote(proposal.id, true)

# Check the outcome
nanda_quorum(proposal.id)
nanda_consensus(proposal.id)
nanda_status()

# Finish
nanda_commit(proposal.id)
# or nanda_abort(proposal.id)
```

## Built-in Functions

### File Operations

```aethershell
ls("path")                    # List directory
read_text("file.txt")         # Read text file
write_text("file.txt", text)  # Write text file
cat("file.txt")               # Display file contents
grep("pattern", "file.txt")   # Search in file
```

### HTTP Operations

```aethershell
http_get("https://api.example.com/data")
http_post("https://api.example.com", {key: "value"})
json_parse(response)
json_stringify(data)
```

### Data Transformations

```aethershell
map(fn, list)                 # Transform each element
filter(fn, list)              # Keep matching elements
reduce(fn, initial, list)     # Aggregate values
sort_by("field", list)        # Sort by field
group_by("field", list)       # Group by field
select("field1", "field2", list)  # Select columns
where(fn, list)               # Filter with predicate
```

### String Operations

```aethershell
split(",", "a,b,c")          # Split string
join(",", ["a", "b"])        # Join strings
trim(text)                   # Remove whitespace
upper(text)                  # Uppercase
lower(text)                  # Lowercase
replace("old", "new", text)  # Replace substring
```

### Array Operations

```aethershell
len([1, 2, 3])               # Length
head([1, 2, 3])              # First element
tail([1, 2, 3])              # Rest of elements
append(4, [1, 2, 3])         # Add to end
concat([1, 2], [3, 4])       # Combine arrays
```

## Common Patterns

### Read, Transform, Write

```aethershell
read_text("input.txt")
  | split("\n")
  | filter(fn(line) => len(line) > 0)
  | map(fn(line) => upper(line))
  | join("\n")
  | write_text("output.txt")
```

### API Data Processing

```aethershell
http_get("https://api.github.com/repos/rust-lang/rust")
  | json_parse()
  | select("name", "stargazers_count", "language")
  | print()
```

### Multi-Agent Workflow

```aethershell
# Stage 1: Research
research_result := researcher.execute({
  task: "Research topic X"
})

# Stage 2: Analysis
analysis_result := analyst.execute({
  task: "Analyze this research",
  context: research_result
})

# Stage 3: Report
final_report := writer.execute({
  task: "Write comprehensive report",
  context: analysis_result
})
```

### Error Handling

```aethershell
result := match http_get("https://api.example.com") {
  Ok(response) => json_parse(response),
  Err(error) => {
    print("Error: ${error}")
    return default_value
  }
}
```

## Model URIs

```aethershell
# OpenAI
"openai:gpt-4o"
"openai:gpt-4o-mini"
"openai:gpt-4-turbo"

# Anthropic
"anthropic:claude-3-opus"
"anthropic:claude-3-sonnet"
"anthropic:claude-3-haiku"

# Local (Ollama)
"ollama:llama3"
"ollama:codellama"
"ollama:mistral"
"ollama:mixtral"

# Azure
"azure:gpt-4"
"azure:gpt-35-turbo"

# Google
"google:gemini-pro"
"google:gemini-pro-vision"
```

## Environment Variables

```bash
# AI Provider Keys
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."
export GOOGLE_API_KEY="..."

# Default AI Provider
export AETHER_AI="openai"

# Agent Tool Permissions
export AGENT_ALLOW_CMDS="ls,cat,grep,git"

# MCP Server Config
export MCP_ALLOWED_PATHS="/home/user/projects"
export MCP_READONLY="true"
```

## Command Line Options

```bash
# Run script
ae script.ae

# Interactive REPL
ae

# TUI mode (interactive terminal UI)
ae tui

# Check syntax
ae --check script.ae

# Transpile to bash
ae --transpile bash script.ae

# Show version
ae --version

# Help
ae --help
```

## VS Code Snippets

Type these prefixes and press Tab:

- `pipe` → Basic pipeline
- `ai` → AI function call
- `agent` → Create agent
- `swarm` → Agent swarm
- `mcpserver` → Start MCP server
- `agentmcp` → Agent with MCP tools
- `a2abus` → A2A message bus
- `nanda` → NANDA coordinator
- `match` → Pattern matching
- `fn` → Lambda function
- `varinfer` → Variable with type inference
- `varexplicit` → Variable with explicit type

## Debugging Tips

```aethershell
# Print intermediate values
data
  | tap(fn(x) => print("After filter: ${x}"))
  | map(fn(x) => x * 2)
  | tap(fn(x) => print("After map: ${x}"))

# Inspect types
inspect(value)  # Shows type and structure

# Try-catch pattern
result := match operation() {
  Ok(v) => v,
  Err(e) => {
    log("Error occurred: ${e}")
    default_value
  }
}
```

## Performance Tips

1. **Use pipelines** instead of intermediate variables
2. **Prefer filter before map** to reduce data early
3. **Use MCP servers** instead of raw shell commands (safer, structured)
4. **Cache AI responses** for repeated queries
5. **Use local models** (Ollama) for development/testing
6. **Batch operations** when working with large datasets

## Further Reading

- [Type System Guide](TYPE_SYSTEM_GUIDE.md) - Deep dive into `:=` vs `=`
- [MCP Servers Guide](MCP_SERVERS_GUIDE.md) - Complete MCP reference
- [AI Protocols Report](AI_PROTOCOLS_FINAL_REPORT.md) - A2A and NANDA details
- [Examples Directory](../examples/) - 17 complete examples

---

**Pro Tip**: When in doubt, use `:=` for type inference. The compiler is smart enough to figure out the types, and your code will be cleaner!
