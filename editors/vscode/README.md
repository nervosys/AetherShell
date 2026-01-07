# ÆtherShell Extension for VS Code

This extension provides language support for [AetherShell](https://github.com/nervosys/AetherShell), a typed, functional shell with AI capabilities.

## Features

- **Syntax Highlighting**: Full TextMate grammar for `.ae` files
- **Autocompletion**: Built-in functions, keywords, and snippets
- **Hover Documentation**: Detailed docs for builtins and keywords
- **Diagnostics**: Parse error detection and reporting
- **Go to Definition**: Jump to variable declarations
- **Find References**: Find all usages of a symbol
- **Rename Symbol**: Rename variables across the file
- **Document Symbols**: Outline view showing declarations
- **Formatting**: Automatic code formatting
- **Snippets**: Common code patterns

## Requirements

### Language Server

This extension uses the AetherShell Language Server (LSP) for advanced features. Build it from the AetherShell repository:

```bash
cd AetherShell
cargo build -p aethershell-lsp --release
```

The extension will automatically find the language server if built in the standard location. You can also set a custom path in settings.

### Configuration

- `aethershell.lsp.enabled`: Enable/disable the language server (default: `true`)
- `aethershell.lsp.path`: Custom path to the language server binary
- `aethershell.trace.server`: Trace server communication for debugging

## Quick Start

1. Install the extension
2. Build the language server (see above)
3. Open any `.ae` file
4. Start writing AetherShell code!

## Syntax Examples

```aethershell
# Variables
let name = "Alice"
let mut counter = 0

# Functional pipelines
[1, 2, 3, 4, 5]
    | where(fn(x) => x > 2)
    | map(fn(x) => x * 2)
    | reduce(fn(acc, x) => acc + x, 0)

# Pattern matching
match status {
    200 => "OK",
    404 => "Not Found",
    n if n >= 500 => "Server Error",
    _ => "Unknown"
}

# AI integration
ai("Explain this code")
agent("Find and fix bugs in src/")
```

## Snippets

| Prefix   | Description                |
| -------- | -------------------------- |
| `let`    | Declare immutable variable |
| `letmut` | Declare mutable variable   |
| `fn`     | Lambda function            |
| `map`    | Map pipeline               |
| `where`  | Filter pipeline            |
| `reduce` | Reduce pipeline            |
| `match`  | Pattern matching           |
| `ai`     | AI query                   |
| `agent`  | AI agent                   |
| `swarm`  | Multi-agent swarm          |

## Contributing

Contributions are welcome! Please visit the [AetherShell repository](https://github.com/nervosys/AetherShell).

## License

MIT License - see the [LICENSE](https://github.com/nervosys/AetherShell/blob/main/LICENSE) file.
