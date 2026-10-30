# ÆtherShell Extension for VS Code

This extension provides language support for [IronShell](https://github.com/nervosys/AetherShell), a typed, functional shell with AI capabilities.

## Features

- **Syntax Highlighting**: Full TextMate grammar for `.ish` files
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

This extension uses the IronShell Language Server (LSP) for advanced features. Build it from the IronShell repository:

```bash
cd AetherShell
cargo build -p iron-shell-lsp --release
```

The extension will automatically find the language server if built in the standard location. You can also set a custom path in settings.

### Configuration

- `ironshell.lsp.enabled`: Enable/disable the language server (default: `true`)
- `ironshell.lsp.path`: Custom path to the language server binary
- `ironshell.trace.server`: Trace server communication for debugging

## Installing

The same package installs in VS Code and in the editors built on it. Cursor,
VSCodium, Gitpod and Theia are VS Code forks and take the same `.vsix`; what
differs is where each one looks for it.

### VS Code

```bash
code --install-extension admercs.ironshell
```

### Cursor, VSCodium, and other forks

These resolve extensions from [Open VSX](https://open-vsx.org), not from the
Visual Studio Marketplace, so an extension published only to the Marketplace is
invisible to them. Install from the registry once it is published there:

```bash
cursor --install-extension admercs.ironshell
```

or install a `.vsix` directly, which always works:

```bash
cursor --install-extension ironshell-1.6.0.vsix
```

In the UI: **Extensions → … → Install from VSIX**.

### From source

```bash
cd editors/vscode
npm install
npm run package                 # produces ironshell-<version>.vsix
code --install-extension ironshell-*.vsix
```

Build with `npm run package`, never `vsce package --no-dependencies`: the latter
omits `vscode-languageclient`, producing a package that installs without
complaint and then fails to activate with `MODULE_NOT_FOUND`. `test/package.test.mjs`
checks for this.

## Quick Start

1. Install the extension (above)
2. Build the language server (see above)
3. Open any `.ish` file
4. Start writing IronShell code!

## Syntax Examples

```ironshell
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

Contributions are welcome! Please visit the [IronShell repository](https://github.com/nervosys/AetherShell).

## License

AGPL-3.0-or-later with commercial dual-license option — see the [LICENSE](https://github.com/nervosys/AetherShell/blob/master/LICENSE) file.
