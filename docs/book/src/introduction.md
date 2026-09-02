# AetherShell

<div class="warning">
Welcome to the official AetherShell documentation!
</div>

**AetherShell** is a next-generation shell that combines the power of typed functional programming with multimodal AI capabilities. It's designed for developers who want a shell that understands structure, not just text.

## Why AetherShell?

Traditional shells treat everything as text, leading to fragile scripts and endless string parsing. AetherShell takes a different approach:

- **🔷 Typed Pipelines**: Data flows as structured values (arrays, records, tables), not raw text
- **🧠 AI-Native**: Built-in AI agents, multi-provider support, tool calling, and reasoning
- **⚡ Functional-First**: Lambdas, pattern matching, and immutable-by-default semantics  
- **🎨 Modern TUI**: Rich terminal interface with multimodal content (images, charts)
- **🔌 Extensible**: Plugin system, MCP protocol support, and Python/Node.js SDKs

## Quick Example

```aethershell
# Type-safe pipelines
let files = ls "." 
  | where(fn(f) => f.size > 1000)
  | sort_by("modified")
  | take(5)

# AI-powered analysis  
let review = ai("Review this code for security issues:
" + cat("app.rs"), {
  model: "gpt-4o"
})

# An agent, with the builtins it may call
agent("Find all TODO comments in src/", ["ls", "cat", "grep"])
```

## Features at a Glance

| Feature                | Description                                                  |
| ---------------------- | ------------------------------------------------------------ |
| **Typed Values**       | Int, Float, String, Bool, Array, Record, Table, Lambda       |
| **Pipeline Operators** | `\|`, `\|>`, `?>` with full type inference                   |
| **Pattern Matching**   | `match` expressions with guards and destructuring            |
| **AI Providers**       | 20 provider types; OpenAI, Anthropic, Google and Ollama have dedicated clients, the rest go over OpenAI-compatible endpoints |
| **Agent Framework**    | Single agents with builtins as tools, over MCP or in-process  |
| **MCP**                | `ae mcp stdio` makes every builtin callable through a three-tool facade |
| **Interactive TUI**    | Real-time chat and multimodal file references                 |

## Getting Started

Ready to dive in? Start with [Installation](./getting-started/installation.md) to set up AetherShell on your system.

If you're coming from Bash or PowerShell, check out our [Quick Start](./getting-started/quick-start.md) guide that translates common patterns to AetherShell.

## Community

- **GitHub**: [nervosys/AetherShell](https://github.com/nervosys/AetherShell)
- **Issues**: [Report a bug or ask a question](https://github.com/nervosys/AetherShell/issues)

## License

AetherShell is open source under the [Apache 2.0 License](https://github.com/nervosys/AetherShell/blob/master/LICENSE).
