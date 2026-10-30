# Installation

This guide covers installing IronShell on your system.

## Requirements

- **Rust 1.88+** (for building from source)
- **OS**: Windows, macOS, or Linux
- **Optional**: API keys for AI providers (OpenAI, Anthropic, etc.)

## Install with Cargo

The recommended way to install IronShell:

```bash
cargo install iron-shell
```

This installs two binaries:
- `ish` - The main IronShell executable
- `aimodel` - AI model management CLI

## Install from Source

For the latest development version:

```bash
git clone https://github.com/nervosys/AetherShell.git
cd AetherShell
cargo build --release

# Add to PATH
cp target/release/ish ~/.local/bin/
cp target/release/aimodel ~/.local/bin/
```

## Pre-built Binaries

Download pre-built binaries from the [releases page](https://github.com/nervosys/AetherShell/releases):

### macOS

```bash
# Intel Mac
curl -LO https://github.com/nervosys/AetherShell/releases/latest/download/ironshell-x86_64-apple-darwin.tar.gz
tar xzf ironshell-x86_64-apple-darwin.tar.gz

# Apple Silicon
curl -LO https://github.com/nervosys/AetherShell/releases/latest/download/ironshell-aarch64-apple-darwin.tar.gz
tar xzf ironshell-aarch64-apple-darwin.tar.gz
```

### Linux

```bash
curl -LO https://github.com/nervosys/AetherShell/releases/latest/download/ironshell-x86_64-unknown-linux-gnu.tar.gz
tar xzf ironshell-x86_64-unknown-linux-gnu.tar.gz
```

### Windows

Download `ironshell-x86_64-pc-windows-msvc.zip` from releases and extract to a directory in your PATH.

## Verify Installation

```bash
ish --version
# IronShell 0.2.0

ish --help
```

## VS Code Extension

Install the IronShell extension for syntax highlighting and IDE features:

```bash
code --install-extension admercs.ironshell
```

Or search for "IronShell" in the VS Code marketplace.

## Next Steps

- [Quick Start](./quick-start.md) - Your first IronShell commands
- [Configuration](./configuration.md) - Set up AI providers and preferences
