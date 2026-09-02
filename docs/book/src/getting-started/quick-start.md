# Quick Start

Get up and running with AetherShell in 5 minutes.

## Launch the Shell

```bash
# Interactive REPL
ae

# TUI mode (rich terminal interface)
ae --tui

# Execute a file
ae script.ae

# Evaluate an expression
ae -e '[1,2,3] | map(fn(x) => x * 2)'
```

## Basic Syntax

### Variables

```aethershell
# Immutable by default
let name = "Alice"
let numbers = [1, 2, 3, 4, 5]
let config = { host: "localhost", port: 8080 }

# Mutable when needed
let mut counter = 0
counter = counter + 1
```

### Types

AetherShell has a rich type system:

```aethershell
# Primitives
let n = 42           # Int
let f = 3.14         # Float
let s = "hello"      # String
let b = true         # Bool

# Collections
let arr = [1, 2, 3]           # Array[Int]
let rec = { x: 1, y: 2 }      # Record
let table = [[1,"a"], [2,"b"]] # Table

# Functions
let double = fn(x) => x * 2   # Lambda
```

### Pipelines

The power of AetherShell is in pipelines:

```aethershell
# Traditional pipeline
[1, 2, 3, 4, 5]
  | filter(fn(x) => x > 2)
  | map(fn(x) => x * 10)
  | sum()
# Result: 120

# File operations
ls "src"
  | where(fn(f) => f.extension == "rs")
  | sort_by("size")
  | take(5)
  | select("name", "size")
```

### Pattern Matching

```aethershell
let describe = fn(x) => match {
    0 => "zero",
    n if n < 0 => "negative",
    n if n > 100 => "large",
    _ => "normal"
}

describe(42)  # "normal"
describe(-5)  # "negative"
```

## AI Features

### Simple Query

```aethershell
# Ask the AI a question
ai("What is the capital of France?")

# With specific model
ai("Explain monads", { model: "claude-3-sonnet" })
```

### Run an Agent

`agent` takes a goal, not a persona, and returns the final answer as a string.
It is not a callable object: there is no `let a = agent(...)` then `a("...")`.

```aethershell
# A goal, with no tools
agent("Explain what this project builds")

# A goal, with the builtins it may call
agent("Find all TODO comments under src/", ["ls", "grep", "cat"])
```

Shell commands are default-deny; export `AGENT_ALLOW_CMDS` before starting `ae`
to permit any. See [Creating Agents](../ai/agents.md).

### Set Up AI Provider

```aethershell
# Set environment variable
env_set("OPENAI_API_KEY", "sk-...")

# Or use the config
# ~/.config/aethershell/config.toml
```

## Common Commands

| Command          | Description                        |
| ---------------- | ---------------------------------- |
| `ls [path]`      | List directory contents as a table |
| `cd path`        | Change directory                   |
| `pwd`            | Print working directory            |
| `cat file`       | Read file contents                 |
| `env`            | List environment variables         |
| `http_get url`   | Make HTTP GET request              |
| `print value`    | Display a value                    |
| `help [builtin]` | Get help                           |

## Example Script

Save as `example.ae`:

```aethershell
# Fetch and process data
let response = http_get("https://api.github.com/repos/nervosys/AetherShell")
let repo = json_parse(response.body)

print("Repository: " + repo.full_name)
print("Stars: " + string(repo.stargazers_count))
print("Language: " + repo.language)

# Find large files in the project
let large_files = ls "."
  | where(fn(f) => f.size > 10000)
  | sort_by("size", "desc")
  | take(10)

print("\nLargest files:")
large_files | each(fn(f) => print("  " + f.name + ": " + string(f.size) + " bytes"))
```

Run it:

```bash
ae example.ae
```

## Next Steps

- [Language Guide](../language/basics.md) - Deep dive into the syntax
- [AI Integration](../ai/introduction.md) - Set up AI providers
- [Builtins Reference](../builtins/overview.md) - All available commands
