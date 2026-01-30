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
let review = ai("Review this code for security issues", {
  model: "gpt-4o",
  context: read("app.rs")
})

# Agent with tools
let assistant = agent("You help with shell tasks", {
  tools: ["ls", "cat", "grep", "http_get"]
})
assistant("Find all TODO comments in src/")
```

## Features at a Glance

| Feature                | Description                                                  |
| ---------------------- | ------------------------------------------------------------ |
| **Typed Values**       | Int, Float, String, Bool, Array, Record, Table, Lambda       |
| **Pipeline Operators** | `\|`, `\|>`, `?>` with full type inference                   |
| **Pattern Matching**   | `match` expressions with guards and destructuring            |
| **AI Providers**       | OpenAI, Claude, Gemini, Llama, Mistral, Cohere, and 20+ more |
| **Agent Framework**    | Single agents, swarms, coordinators, distributed execution   |
| **Workflow Engine**    | MapReduce, Saga, Pipeline, Fan-Out patterns                  |
| **Interactive TUI**    | Real-time chat, image rendering, workflow visualization      |

## Getting Started

Ready to dive in? Start with [Installation](./getting-started/installation.md) to set up AetherShell on your system.

If you're coming from Bash or PowerShell, check out our [Quick Start](./getting-started/quick-start.md) guide that translates common patterns to AetherShell.

## Community

- **GitHub**: [nervosys/AetherShell](https://github.com/nervosys/AetherShell)
- **Discord**: [Join our community](https://discord.gg/aethershell)
- **Twitter**: [@AetherShell](https://twitter.com/AetherShell)

## License

AetherShell is open source under the [Apache 2.0 License](https://github.com/nervosys/AetherShell/blob/master/LICENSE).
