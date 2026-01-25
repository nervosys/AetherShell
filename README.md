<p align="center">
  <img src="assets/logo.svg" alt="Æther Shell" width="180">
</p>

<h1 align="center">Æther Shell (ae)</h1>

<p align="center">
  <a href="https://crates.io/crates/aether_shell"><img src="https://img.shields.io/crates/v/aether_shell.svg?style=flat-square&logo=rust&color=orange" alt="Crates.io"></a>
  <a href="https://github.com/nervosys/AetherShell/actions"><img src="https://img.shields.io/github/actions/workflow/status/nervosys/AetherShell/security-audit.yml?style=flat-square&logo=github" alt="Build Status"></a>
  <a href="https://github.com/nervosys/AetherShell/blob/master/LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg?style=flat-square" alt="License"></a>
  <a href="https://github.com/nervosys/AetherShell/stargazers"><img src="https://img.shields.io/github/stars/nervosys/AetherShell?style=flat-square&color=yellow" alt="Stars"></a>
</p>

<p align="center">
  <strong>The world's first agentic shell with typed functional pipelines and multi-modal AI.</strong><br>
  <em>Built in Rust for safety and performance, featuring revolutionary AI protocols found nowhere else.</em>
</p>

<p align="center">
  <a href="#-quick-start">Quick Start</a> •
  <a href="#-features">Features</a> •
  <a href="#-examples">Examples</a> •
  <a href="docs/TUI_GUIDE.md">TUI Guide</a> •
  <a href="#-documentation">Docs</a> •
  <a href="#-contributing">Contributing</a>
</p>

---

<p align="center">
  <img src="assets/screenshot.svg" alt="AetherShell Terminal Demo" width="800">
</p>

---

## 🚀 Quick Start

### VS Code Extension (Syntax Highlighting + LSP)

For full IDE support including syntax highlighting, IntelliSense, and error diagnostics:

```bash
# Install the extension from marketplace
code --install-extension admercs.aethershell

# Build the Language Server (for IntelliSense)
cd AetherShell
cargo build -p aethershell-lsp --release

# The extension will auto-detect the LSP server
```

**Features:** Syntax highlighting, autocompletion, hover docs, go-to-definition, error diagnostics.

### Installation

```bash
# Install from source
git clone https://github.com/nervosys/AetherShell && cd AetherShell
cargo install --path . --bin ae

# Launch interactive TUI (recommended)
ae tui

# Or classic REPL
ae

# Run a script file
ae script.ae
```

```ae
# Typed pipelines — structured data, not text streams
[1, 2, 3, 4, 5] | map(fn(x) => x * 2) | sum()   # => 30

# Type inference — no 'let' required
name = "AetherShell"                            # String
count = 42                                       # Int
scores = [95, 87, 92, 88]                        # Array<Int>
user = {name: "Alice", age: 30}                 # Record

# Pattern matching with type guards
match type_of(count) {
    "Int" => "It's an integer: ${count}",
    "String" => "It's a string",
    _ => "Unknown type"
}

# Functional string processing
"hello,world" | split(",") | map(fn(s) => upper(s)) | join(" ")
# => "HELLO WORLD"

# AI-powered analysis
ai("Explain this error message", {context: cat("error.log")})

# Vision AI for screenshots and diagrams
ai("What UI issues do you see?", {images: ["screenshot.png"]})

# Autonomous agent with tool access
agent("Refactor all deprecated API calls in src/", ["ls", "cat", "grep", "git"])
```

> **📝 Note:** Set `OPENAI_API_KEY` for AI features: `export OPENAI_API_KEY="sk-..."`

---

## ✨ Features

<table>
<tr>
<td width="50%">

### 🤖 AI-Native Shell
- **Multi-modal AI**: Images, audio, video analysis
- **Autonomous agents** with tool access
- **130+ MCP tools** across 27 categories
- **Multi-provider**: OpenAI, Ollama, local models
- **Fine-tuning API** for custom model training
- **RAG & Knowledge Graphs** built-in

</td>
<td width="50%">

### 💎 Typed Pipelines
- **Hindley-Milner** type inference
- **Structured data**: Records, Arrays, Tables
- **First-class functions** and lambdas
- **Pattern matching** expressions

</td>
</tr>
<tr>
<td width="50%">

### 🧠 ML & Enterprise
- **Neural networks** creation & evolution
- **Reinforcement learning** (Q-Learning, DQN)
- **Enterprise RBAC** with role-based access
- **Audit logging** & compliance reporting
- **SSO integration** (SAML, OAuth, OIDC)
- **Cluster management** for distributed AI

</td>
<td width="50%">

### 🎨 Developer Experience
- **Interactive TUI** with tabs & themes
- **Language Server Protocol** (LSP)
- **VS Code extension** with IntelliSense
- **Plugin system** with TOML manifests
- **WASM support** for browser REPL
- **Package management** & imports

</td>
</tr>
</table>

---

## 🎯 What Makes AetherShell Unique?

AetherShell is the **only shell** combining these capabilities:

| Feature                             | AetherShell | Traditional Shells | Nushell |
| ----------------------------------- | :---------: | :----------------: | :-----: |
| AI Agents with Tools                |      ✅      |         ❌          |    ❌    |
| Multi-modal AI (Vision/Audio/Video) |      ✅      |         ❌          |    ❌    |
| MCP Protocol (130+ tools)           |      ✅      |         ❌          |    ❌    |
| Neural Networks Built-in            |      ✅      |         ❌          |    ❌    |
| Hindley-Milner Types                |      ✅      |         ❌          |    ✅    |
| Typed Pipelines                     |      ✅      |         ❌          |    ✅    |
| Agent-to-Agent Protocol (A2A)       |      ✅      |         ❌          |    ❌    |
| Consensus Protocol (NANDA)          |      ✅      |         ❌          |    ❌    |
| Enterprise (RBAC, Audit, SSO)       |      ✅      |         ❌          |    ❌    |
| Language Server Protocol (LSP)      |      ✅      |         ❌          |    ✅    |

### Bash vs AetherShell: A Quick Comparison

**Find large Rust files and show their sizes:**

```bash
# Bash: Text parsing, fragile, hard to read
find ./src -name "*.rs" -size +1k -exec ls -lh {} \; | awk '{print $9, $5}' | sort -k2 -h | tail -5
```

```ae
# AetherShell: Typed, composable, readable
ls("./src")
  | where(fn(f) => f.ext == ".rs" && f.size > 1024)
  | map(fn(f) => {name: f.name, size: f.size})
  | sort_by(fn(f) => f.size, "desc")
  | take(5)
```

**Analyze JSON API response:**

```bash
# Bash: Requires jq, string manipulation
curl -s https://api.github.com/repos/nervosys/AetherShell | jq '.stargazers_count, .forks_count'
```

```ae
# AetherShell: Native JSON, type-safe field access  
repo = http_get("https://api.github.com/repos/nervosys/AetherShell")
print("Stars: ${repo.stargazers_count}, Forks: ${repo.forks_count}")
```

**Ask AI to explain an error:**

```bash
# Bash: Not possible without external scripts
```

```ae
# AetherShell: Built-in AI with context
error_log = cat("error.log") | where(fn(l) => contains(l, "FATAL")) | first()
ai("Explain this error and suggest a fix:", {context: error_log})
```

---

## 📐 Language Features at a Glance

AetherShell is a **typed functional language** with 215+ built-in functions across these categories:

<table>
<tr>
<td width="33%">

### Types & Literals
- `Int` — `42`, `-7`
- `Float` — `3.14`, `2.0`
- `String` — `"hello"`, `"${var}"`
- `Bool` — `true`, `false`
- `Null` — `null`
- `Array` — `[1, 2, 3]`
- `Record` — `{a: 1, b: 2}`
- `Lambda` — `fn(x) => x * 2`

</td>
<td width="33%">

### Operators
- Arithmetic: `+` `-` `*` `/` `%` `**`
- Comparison: `==` `!=` `<` `<=` `>` `>=`
- Logical: `&&` `||` `!`
- Pipeline: `|`
- Member: `.`

</td>
<td width="33%">

### Control Flow
- `match` expressions
- Pattern guards
- Wildcard `_` patterns
- Lambda functions
- Pipeline chaining

</td>
</tr>
</table>

### Builtin Categories (215+ functions)

| Category         | Examples                                                    | Count |
| ---------------- | ----------------------------------------------------------- | ----- |
| **Core**         | `help`, `print`, `echo`, `type_of`, `len`                   | 15    |
| **Functional**   | `map`, `where`, `reduce`, `take`, `any`, `all`, `first`     | 12    |
| **String**       | `split`, `join`, `trim`, `upper`, `lower`, `replace`        | 10    |
| **Array**        | `flatten`, `reverse`, `slice`, `range`, `zip`, `push`       | 8     |
| **Math**         | `abs`, `min`, `max`, `sqrt`, `pow`, `floor`, `ceil`         | 8     |
| **Aggregate**    | `sum`, `avg`, `product`, `unique`, `values`, `keys`         | 6     |
| **File System**  | `ls`, `cat`, `pwd`, `cd`, `exists`, `mkdir`, `rm`           | 11    |
| **Config**       | `config`, `config_get`, `config_set`, `themes`              | 7     |
| **Debugging**    | `debug`, `dbg`, `trace`, `assert`, `type_assert`, `inspect` | 7     |
| **Async**        | `async`, `await`, futures support                           | 3     |
| **Errors**       | `try`/`catch`, `throw`, `is_error`                          | 4     |
| **AI**           | `ai`, `agent`, `swarm`, `rag_query`, `finetune_start`       | 20+   |
| **Enterprise**   | `role_create`, `audit_log`, `sso_init`, `compliance_check`  | 22    |
| **Distributed**  | `cluster_create`, `job_submit`, `aggregate_results`         | 15    |
| **Platform**     | `platform`, `is_windows`, `is_linux`, `features`            | 12    |
| **MCP Protocol** | `mcp_tools`, `mcp_call`, 130+ tool integrations             | 130+  |

---

## 📖 Examples

### Core Syntax — Type Inference Without `let`

AetherShell uses **Hindley-Milner type inference** — types are inferred automatically, no `let` keyword needed:

```ae
# Type inference — the compiler knows these types
age = 42                        # Int
pi = 3.14159                    # Float  
name = "AetherShell"            # String
active = true                   # Bool
empty = null                    # Null

# String interpolation with inferred types
greeting = "Hello, ${name}! You're ${age} years old."

# Arrays — homogeneous collections are type-safe
nums = [1, 2, 3, 4, 5]          # Array<Int>
names = ["Alice", "Bob"]        # Array<String>
matrix = [[1, 2], [3, 4]]       # Array<Array<Int>>

# Records — structured data with field access
user = {name: "Alice", age: 30, admin: true}
print(user.name)               # => "Alice"
print(type_of(user))           # => "Record"

# Lambdas — first-class functions with type inference
double = fn(x) => x * 2        # fn(Int) -> Int
add = fn(a, b) => a + b        # fn(Int, Int) -> Int
greet = fn(s) => "Hi, ${s}!"   # fn(String) -> String

print(double(21))              # => 42
print(greet("World"))          # => "Hi, World!"
```

### Strong Types — Compile-Time Safety

```ae
# Type assertions for runtime validation
type_assert(42, "Int")          # Passes
type_assert("hello", "String")  # Passes
type_assert([1,2,3], "Array")   # Passes

# Type inspection
type_of(42)                    # => "Int"
type_of(3.14)                  # => "Float"
type_of("hello")               # => "String"
type_of([1, 2, 3])             # => "Array"
type_of({a: 1})                # => "Record"
type_of(fn(x) => x)            # => "Lambda"

# Pattern matching on types
process = fn(val) => match type_of(val) {
    "Int" => val * 2,
    "String" => upper(val),
    "Array" => len(val),
    _ => null
}

process(21)                    # => 42
process("hello")               # => "HELLO"
process([1,2,3,4,5])           # => 5
```

### Functional Pipelines — Structured Data, Not Text

Unlike traditional shells that pipe text, AetherShell pipes **typed values**:

```ae
# Transform: map applies a function to each element
numbers = [1, 2, 3, 4, 5]
squared = numbers | map(fn(x) => x * x)    # => [1, 4, 9, 16, 25]

# Filter: where keeps elements matching a predicate
evens = numbers | where(fn(x) => x % 2 == 0)  # => [2, 4]

# Aggregate: reduce combines elements into one value
total = numbers | reduce(fn(acc, x) => acc + x, 0)  # => 15

# Chain operations — each step preserves types
result = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  | where(fn(x) => x % 2 == 0)     # [2, 4, 6, 8, 10] - evens only
  | map(fn(x) => x ** 2)           # [4, 16, 36, 64, 100] - squared
  | reduce(fn(a, b) => a + b, 0)   # 220 - sum

# Array manipulation with type safety
reversed = [1, 2, 3, 4, 5] | reverse       # => [5, 4, 3, 2, 1]
flat = [[1, 2], [3, 4]] | flatten          # => [1, 2, 3, 4]
sliced = [1, 2, 3, 4, 5] | slice(1, 4)     # => [2, 3, 4]

# Predicate checks return Bool
has_large = [1, 2, 3, 4, 5] | any(fn(x) => x > 4)   # => true
all_even = [2, 4, 6, 8] | all(fn(x) => x % 2 == 0)  # => true
```

### Pattern Matching — Exhaustive Type-Safe Control Flow

```ae
# Match on values with range patterns
grade = fn(score) => match score {
    100 => "Perfect!",
    90..99 => "A",
    80..89 => "B", 
    70..79 => "C",
    _ => "Keep trying"
}
grade(95)                      # => "A"
grade(100)                     # => "Perfect!"

# Match with guards for complex conditions
classify = fn(n) => match n {
    x if x < 0 => "negative",
    0 => "zero",
    x if x > 0 => "positive"
}
classify(-5)                   # => "negative"
classify(0)                    # => "zero"
classify(42)                   # => "positive"

# Type-based dispatch — powerful polymorphism
describe = fn(val) => match type_of(val) {
    "Int" => "Integer: ${val}",
    "Float" => "Decimal: ${val}",
    "String" => "Text (${len(val)} chars): ${val}",
    "Array" => "Collection of ${len(val)} items",
    "Record" => "Object with keys: ${keys(val)}",
    "Lambda" => "Function",
    _ => "Unknown type"
}

describe(42)                   # => "Integer: 42"
describe("hello")              # => "Text (5 chars): hello"
describe([1, 2, 3])            # => "Collection of 3 items"
describe({x: 1, y: 2})         # => "Object with keys: [x, y]"
```

### String Operations — Built-in Text Processing

```ae
# Manipulation
split("a,b,c", ",")            # => ["a", "b", "c"]
join(["a", "b", "c"], "-")     # => "a-b-c"
trim("  hello  ")              # => "hello"
upper("hello")                 # => "HELLO"
lower("WORLD")                 # => "world"
replace("foo bar foo", "foo", "baz")  # => "baz bar baz"

# Queries
contains("hello world", "world")      # => true
starts_with("hello", "hel")           # => true
ends_with("hello", "lo")              # => true
len("hello")                          # => 5
```

### Math Operations — Scientific Computing

```ae
# Basic math
abs(-42)                       # => 42
min(5, 3)                      # => 3
max(5, 3)                      # => 5
pow(2, 10)                     # => 1024
sqrt(16)                       # => 4.0

# Rounding
floor(3.7)                     # => 3
ceil(3.2)                      # => 4
round(3.5)                     # => 4

# Statistical (on arrays)
sum([1, 2, 3, 4, 5])           # => 15
avg([10, 20, 30])              # => 20
product([2, 3, 4])             # => 24
unique([1, 2, 2, 3, 3, 3])     # => [1, 2, 3]
```

### Error Handling — Try/Catch/Throw

```ae
# Safe operations with try/catch
result = try {
    risky_operation()
} catch {
    "default_value"
}

# Catch with error binding
result = try {
    parse_config("invalid.toml")
} catch e {
    print("Error: ${e}")
    default_config()
}

# Throw custom errors
validate = fn(x) => {
    if x < 0 {
        throw "Value must be non-negative"
    }
    x
}

# Check for errors
is_error(try { throw "oops" } catch e { e })  # => true
```

### Async/Await — Concurrent Operations

```ae
# Define async functions
fetch_data = async fn(url) => http_get(url)

# Await results
data = await fetch_data("https://api.example.com/data")

# Parallel operations with futures
urls = ["https://api1.com", "https://api2.com", "https://api3.com"]
futures = urls | map(fn(u) => async fn() => http_get(u))
results = futures | map(fn(f) => await f())
```

### Debugging — Development Tools

```ae
# Debug prints value with type and returns it (for chaining)
[1, 2, 3] | debug() | map(fn(x) => x * 2)
# Prints: [Debug] Array: [1, 2, 3]
# Returns: [2, 4, 6]

# Trace with labels for pipeline debugging
[1, 2, 3, 4, 5]
  | trace("input")
  | where(fn(x) => x > 2) | trace("filtered")
  | map(fn(x) => x * 2) | trace("doubled")
# Prints each stage with labels

# Assertions for testing
assert(1 + 1 == 2)
assert(len("hello") == 5, "Length should be 5")

# Type assertions
type_assert(42, "Int")
type_assert([1, 2, 3], "Array")

# Deep inspection
inspect([1, 2, 3])
# => {type: "Array", len: 3, values: [1, 2, 3]}
```

### File System — Structured Output

```ae
# List files with structured data
ls("./src")
  | where(fn(f) => f.size > 1000)
  | map(fn(f) => {name: f.name, kb: f.size / 1024})
  | take(5)

# Read and process files
cat("config.toml") | split("\n") | len()

# Check existence
exists("./src/main.rs")        # => true

# Get current directory
pwd()                          # => "/home/user/project"
```

### Configuration System — XDG-Compliant

```ae
# Get full configuration as Record
config()

# Get specific values with dot notation
config_get("colors.theme")           # => "tokyo-night"
config_get("history.max_size")       # => 10000

# Set values persistently
config_set("colors.theme", "dracula")
config_set("editor.tab_size", 4)

# Get all paths (XDG Base Directory compliant)
paths = config_path()
print(paths.config_file)       # ~/.config/aether/config.toml
print(paths.data_dir)          # ~/.local/share/aether

# List all 38 built-in themes
themes() | take(8)
# => ["catppuccin", "dracula", "github-dark", "gruvbox",
#     "monokai", "nord", "one-dark", "tokyo-night"]
```

### AI Agents with Tool Access

```ae
# Simple agent with goal and tools
agent("Find all files larger than 1MB in src/", ["ls", "du"])

# Agent with full configuration
agent({
  goal: "Identify and fix code style violations",
  tools: ["ls", "cat", "grep", "git"],
  max_steps: 20,
  dry_run: true,       # Preview actions before executing
  model: "openai:gpt-4o"
})

# Multi-agent swarm for complex tasks
swarm({
  coordinator: "Orchestrate a full security audit",
  agents: [
    {role: "scanner", goal: "Find vulnerable dependencies"},
    {role: "reviewer", goal: "Check for SQL injection"},
    {role: "reporter", goal: "Generate findings report"}
  ],
  tools: ["ls", "cat", "grep", "cargo"]
})
```

### Multi-Modal AI

```ae
# Analyze images
ai("What's in this screenshot?", {images: ["screenshot.png"]})

# Process audio
ai("Transcribe and summarize this meeting", {audio: ["meeting.mp3"]})

# Video analysis
ai("Extract the key steps from this tutorial", {video: ["tutorial.mp4"]})
```

### Typed Functional Pipelines

```ae
# File system operations return typed Records, not text
large_rust_files = ls("./src")
  | where(fn(f) => f.ext == ".rs" && f.size > 1000)
  | map(fn(f) => {name: f.name, kb: f.size / 1024})
  | sort_by(fn(f) => f.kb, "desc")
  | take(5)

# Statistical operations with proper types
scores = [85, 92, 78, 95, 88]
scores | sum()               # => 438 (Int)
scores | avg()               # => 87.6 (Float)
[1, 2, 1, 3, 2] | unique()   # => [1, 2, 3] (Array<Int>)
{a: 1, b: 2} | values()      # => [1, 2]
```

### MCP Tools (Model Context Protocol)

```ae
# 130 tools across 27 categories
all_tools = mcp_tools()
print(len(all_tools))        # => 130

# Filter by category
mcp_tools({category: "development"})     # git, cargo, npm, etc.
mcp_tools({category: "machinelearning"}) # ollama, tensorboard, etc.
mcp_tools({category: "kubernetes"})      # kubectl, helm, k9s, etc.

# Execute tools via MCP protocol
mcp_call("git", {command: "status"})
mcp_call("cargo", {command: "build --release"})
```

### Neural Networks & Evolution

```ae
# Create a neural network with layer sizes
brain = nn_create("agent", [4, 8, 2])  # 4 inputs, 8 hidden, 2 outputs

# Evolutionary optimization
pop = population(100, {genome_size: 10})
evolved = evolve(pop, fitness_fn, {generations: 50})

# Reinforcement learning
learner = rl_agent("learner", 16, 4)
```

---

## 🌍 Real-World Use Cases

### DevOps: Log Analysis Pipeline

```ae
# Parse and analyze application logs
error_logs = cat("/var/log/app.log")
  | split("\n")
  | where(fn(line) => contains(line, "ERROR"))
  | map(fn(line) => {
      timestamp: line | slice(0, 19),
      level: "ERROR",
      message: line | slice(27, len(line))
    })
  | take(10)

# Count errors by hour
error_counts = error_logs
  | map(fn(e) => e.timestamp | slice(0, 13))  # Extract hour
  | unique()
  | map(fn(hour) => {
      hour: hour,
      count: error_logs | where(fn(e) => starts_with(e.timestamp, hour)) | len()
    })
```

### Data Science: CSV Processing

```ae
# Process CSV data with type-safe pipelines
raw_data = cat("sales.csv") | split("\n")
headers = raw_data | first()
rows = raw_data | slice(1, len(raw_data)) | map(fn(row) => split(row, ","))

# Parse into typed Records
sales = rows | map(fn(r) => {
    date: r[0],
    product: r[1],
    quantity: r[2] + 0,    # Convert to Int
    price: r[3] + 0.0      # Convert to Float
})

# Statistical analysis
total_revenue = sales | map(fn(s) => s.quantity * s.price) | sum()
avg_order = sales | map(fn(s) => s.quantity) | avg()
top_products = sales
  | map(fn(s) => s.product)
  | unique()
  | take(5)

print("Total Revenue: $${total_revenue}")
print("Average Order Size: ${avg_order} units")
```

### Security: Automated Code Audit

```ae
# AI-powered security scan
agent({
  goal: "Find potential security vulnerabilities in the codebase",
  tools: ["grep", "cat", "ls"],
  max_steps: 20
})

# Search for hardcoded secrets
ls("./src") 
  | where(fn(f) => ends_with(f.name, ".rs"))
  | map(fn(f) => {file: f.name, content: cat(f.path)})
  | where(fn(f) => contains(f.content, "password") || contains(f.content, "secret"))
```

### System Administration: Disk Usage Report

```ae
# Generate disk usage report
ls("/home")
  | map(fn(d) => {
      name: d.name,
      size_mb: d.size / (1024 * 1024),
      files: len(ls(d.path))
    })
  | where(fn(d) => d.size_mb > 100)
  | map(fn(d) => "${d.name}: ${round(d.size_mb)}MB (${d.files} files)")
```

### AI-Assisted Development

```ae
# Generate documentation from code
code = cat("src/main.rs")
docs = ai("Generate comprehensive API documentation for this Rust code:", {
    context: code,
    model: "openai:gpt-4o"
})

# Intelligent code review
agent({
  goal: "Review the recent git changes and suggest improvements for:
         - Performance optimizations
         - Security issues  
         - Code style consistency",
  tools: ["git", "cat", "grep"],
  max_steps: 15
})

# Generate tests with context awareness
module_code = cat("src/utils.rs")
test_code = ai("Write comprehensive unit tests covering edge cases:", {
  context: module_code,
  model: "openai:gpt-4o"
})

# Explain complex code
complex_fn = cat("src/parser.rs") | slice(100, 200)
ai("Explain what this function does in simple terms:", {context: complex_fn})
```

### Infrastructure: Kubernetes Monitoring

```ae
# List pods with structured output
mcp_call("kubectl", {command: "get pods -o json"})
  | map(fn(pod) => {
      name: pod.metadata.name,
      status: pod.status.phase,
      restarts: pod.status.containerStatuses[0].restartCount
    })
  | where(fn(p) => p.restarts > 0)
```

### Enterprise: RBAC & Compliance

```ae
# Create roles with permissions
role_create("data_analyst", [
    {resource: "reports", actions: ["read", "export"]},
    {resource: "dashboards", actions: ["read", "create"]}
], "Data analytics team role")

# Grant roles to users
role_grant("user_123", "data_analyst")

# Check permissions before operations
if check_permission("user_123", "reports", "export") {
    audit_log("report_export", {user: "user_123", report: "Q4_sales"})
    # ... export the report
}

# Compliance reporting
compliance_check("GDPR")
compliance_report("SOC2", "json")
```

### AI: Fine-tuning & RAG

```ae
# Start model fine-tuning
finetune_start("gpt-4o-mini", "training_data.jsonl", {
    epochs: 3,
    learning_rate: 0.0001
})

# Check fine-tuning status
finetune_status("ft-abc123")

# Build knowledge base with RAG
rag_index("project_docs", ["README.md", "docs/*.md"])
rag_query("project_docs", "How do I configure themes?")

# Knowledge graphs
kg_add("AetherShell", "language", "Rust")
kg_relate("AetherShell", "has_feature", "typed_pipelines")
kg_query({entity: "AetherShell"})
```

### Distributed Computing

```ae
# Create a compute cluster
cluster_create("ml_cluster", {max_nodes: 10})

# Add worker nodes
cluster_add_node("ml_cluster", "worker_1", {capabilities: ["gpu", "ml"]})
cluster_add_node("ml_cluster", "worker_2", {capabilities: ["gpu", "ml"]})

# Submit distributed jobs
job_submit("ml_cluster", "train_model", {
    model: "neural_net",
    data: "training_set.csv"
})

# Monitor cluster status
cluster_status("ml_cluster")
```

### Interactive Data Exploration

```ae
# Explore JSON APIs with type-safe access
response = http_get("https://api.github.com/repos/nervosys/AetherShell")
print("Stars: ${response.stargazers_count}")
print("Forks: ${response.forks_count}")  
print("Language: ${response.language}")

# Transform API data
topics_upper = response.topics | map(fn(t) => upper(t)) | join(", ")

# Build a dashboard from multiple endpoints
repos = http_get("https://api.github.com/users/nervosys/repos")
stats = repos | map(fn(r) => {
    name: r.name,
    stars: r.stargazers_count,
    lang: r.language
}) | where(fn(r) => r.stars > 0) | sort_by(fn(r) => r.stars, "desc")
```

### Git Workflow Automation

```ae
# Get recent commits with structured data
commits = mcp_call("git", {command: "log --oneline -10"})
  | split("\n")
  | map(fn(line) => {
      hash: line | slice(0, 7),
      message: line | slice(8, len(line))
    })

# Find commits by pattern
bug_fixes = commits | where(fn(c) => contains(lower(c.message), "fix"))

# Analyze git blame for a file
blame = mcp_call("git", {command: "blame src/main.rs"})
authors = blame | split("\n") 
  | map(fn(l) => l | split(" ") | first())
  | unique()
```

### Build & Deploy Automation

```ae
# Platform-aware build script
build_cmd = match platform() {
    "windows" => "cargo build --release --target x86_64-pc-windows-msvc",
    "linux" => "cargo build --release --target x86_64-unknown-linux-gnu",
    "macos" => "cargo build --release --target aarch64-apple-darwin",
    _ => "cargo build --release"
}

# Conditional feature flags
features = features()
build_with_ai = if has_feature("ai") { "--features ai" } else { "" }

# Multi-platform detection
if is_windows() {
    print("Building for Windows...")
} else if is_linux() {
    print("Building for Linux...")
} else if is_macos() {
    print("Building for macOS...")
}
```

### Monitoring & Alerting

```ae
# Check system health and alert
health_check = fn() => {
    cpu = mcp_call("system", {metric: "cpu_usage"})
    memory = mcp_call("system", {metric: "memory_usage"})
    disk = mcp_call("system", {metric: "disk_usage"})
    
    {cpu: cpu, memory: memory, disk: disk}
}

status = health_check()

# Alert on high resource usage
if status.cpu > 90 || status.memory > 85 {
    ai("Generate an alert message for high resource usage:", {
        context: "CPU: ${status.cpu}%, Memory: ${status.memory}%"
    })
}
```

---

## 🎮 TUI Interface

Launch the beautiful terminal UI with `ae tui`:

| Tab        | Description                                |
| ---------- | ------------------------------------------ |
| **Chat**   | Conversational AI with multi-modal support |
| **Agents** | Deploy and monitor AI agent swarms         |
| **Media**  | View images, play audio, preview videos    |
| **Help**   | Quick reference and documentation          |

**Keyboard shortcuts:**
- `Tab` — Switch tabs
- `Enter` — Send message / activate
- `Space` — Select media files
- `q` — Quit
- `Ctrl+C` — Force quit

📖 **Full guide:** [docs/TUI_GUIDE.md](docs/TUI_GUIDE.md)

---

## 📦 Installation

### From Source (Recommended)

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo install --path . --bin ae
```

### From Crates.io

```bash
cargo install aether_shell
```

### VS Code Extension

Get syntax highlighting, snippets, and integrated REPL:

```bash
cd editors/vscode
npm install && npm run compile
# Press F5 to test
```

---

## ⚙️ Configuration

### Environment Variables

```bash
# AI Provider (required for AI features)
export OPENAI_API_KEY="sk-..."

# Agent permissions
export AGENT_ALLOW_CMDS="ls,git,curl,python"

# Alternative AI backend
export AETHER_AI="ollama"  # or "openai"
```

### Secure Key Storage

```bash
# Store keys in OS credential manager (recommended)
ae keys store openai sk-your-key-here

# View stored keys (masked)
ae keys list
```

---

## 📚 Documentation

| Document                                             | Description               |
| ---------------------------------------------------- | ------------------------- |
| [Quick Reference](docs/QUICK_REFERENCE.md)           | One-page syntax guide     |
| [TUI Guide](docs/TUI_GUIDE.md)                       | Terminal UI documentation |
| [Type System](docs/TYPE_SYSTEM_GUIDE.md)             | Type inference details    |
| [MCP Servers](docs/MCP_SERVERS_GUIDE.md)             | Tool integration guide    |
| [AI Backends](docs/AI_BACKENDS.md)                   | Provider configuration    |
| [Security](docs/security/SECURITY_AUDIT_RED_TEAM.md) | Security assessment       |

### Example Scripts

| File                                                  | Topic            |
| ----------------------------------------------------- | ---------------- |
| [00_hello.ae](examples/00_hello.ae)                   | Basic syntax     |
| [01_pipelines.ae](examples/01_pipelines.ae)           | Typed pipelines  |
| [02_tables.ae](examples/02_tables.ae)                 | Table operations |
| [04_match.ae](examples/04_match.ae)                   | Pattern matching |
| [05_ai.ae](examples/05_ai.ae)                         | AI integration   |
| [06_agent.ae](examples/06_agent.ae)                   | Agent deployment |
| [09_tui_multimodal.ae](examples/09_tui_multimodal.ae) | Multi-modal TUI  |

### Coverage Test Scripts

| File                                                              | Topic               |
| ----------------------------------------------------------------- | ------------------- |
| [syntax_comprehensive.ae](tests/coverage/syntax_comprehensive.ae) | All AST constructs  |
| [builtins_core.ae](tests/coverage/builtins_core.ae)               | Core functions      |
| [builtins_functional.ae](tests/coverage/builtins_functional.ae)   | Functional ops      |
| [builtins_string.ae](tests/coverage/builtins_string.ae)           | String operations   |
| [builtins_array.ae](tests/coverage/builtins_array.ae)             | Array operations    |
| [builtins_math.ae](tests/coverage/builtins_math.ae)               | Math functions      |
| [builtins_aggregate.ae](tests/coverage/builtins_aggregate.ae)     | Aggregate functions |
| [builtins_config.ae](tests/coverage/builtins_config.ae)           | Config & themes     |

---

## 🧪 Testing

AetherShell has comprehensive test coverage with **100% pass rate**.

```bash
# Run the full test coverage suite
./scripts/test_coverage.ps1     # Windows PowerShell
./scripts/run_tests.sh          # Linux/macOS

# Run specific test categories
cargo test --test builtins_coverage  # 23 builtin tests
cargo test --test theme_coverage     # 6 theme tests
cargo test --test eval               # 6 evaluator tests
cargo test --test typecheck          # 10 type inference tests
cargo test --test pipeline           # Pipeline tests
cargo test --test smoke              # Smoke tests

# Run all library tests
cargo test --lib
```

### Test Coverage Summary

| Category             | Tests   | Status |
| -------------------- | ------- | ------ |
| Builtins Coverage    | 23      | ✅      |
| Theme System         | 6       | ✅      |
| Core Builtins        | 2       | ✅      |
| Evaluator            | 6       | ✅      |
| Pipelines            | 1       | ✅      |
| Type Inference       | 10      | ✅      |
| Smoke Tests          | 4       | ✅      |
| **.ae Syntax Tests** | 8 files | ✅      |

**Test files:** See [TESTING.md](TESTING.md) for the complete testing strategy and [tests/coverage/](tests/coverage/) for syntax coverage tests.

---

## 🛣️ Roadmap

See [ROADMAP.md](ROADMAP.md) for the complete development roadmap with detailed progress tracking.

### ✅ Completed (January 2026)
- 215+ builtins with comprehensive test coverage
- 38 built-in color themes with XDG-compliant config
- Neural network primitives & evolutionary algorithms
- 130+ MCP tools with protocol compliance
- Multi-modal AI (images, audio, video)
- Reinforcement learning (Q-Learning, DQN, Actor-Critic)
- Distributed agent swarms & cluster management
- Language Server Protocol (LSP) for IDE integration
- VS Code extension v0.2.0 with IntelliSense
- Enterprise features (RBAC, Audit, SSO, Compliance)
- Fine-tuning API for custom model training
- RAG & knowledge graphs
- Plugin system with TOML manifests
- WASM support (browser-based shell)
- Package management & module imports
- 100% test pass rate

### 🔜 Coming Soon
- Advanced video streaming
- Mobile platform support

---

## 🤝 Contributing

We welcome contributions! See our development setup:

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo build
cargo test --lib
```

1. Fork the repository
2. Create a feature branch
3. Add tests for new functionality
4. Submit a pull request

---

## 📜 License

Licensed under the [Apache License 2.0](LICENSE).

---

<p align="center">
  <strong>Ready to experience the future of shell interaction?</strong><br><br>
  <code>ae tui</code>
</p>

<p align="center">
  <a href="https://github.com/nervosys/AetherShell">⭐ Star us on GitHub</a> •
  <a href="https://github.com/nervosys/AetherShell/issues">🐛 Report Issues</a> •
  <a href="https://github.com/nervosys/AetherShell/discussions">💬 Discussions</a>
</p>
