# IronShell v0.1.0 - Release Notes

Historical notes for v0.1.0. See [`13.0.0.md`](13.0.0.md) for the next release's
draft notes and readiness checklist.

**Release Date**: October 23, 2025  
**License**: MIT  
**Status**: ✅ Ready for Public Release

---

## 🎉 What's New

IronShell v0.1.0 is the **first multi-agent shell with typed functional pipelines and multi-modal AI**. This initial release brings together cutting-edge AI capabilities with a modern, type-safe shell language.

### 🌟 Unique Features

1. **Multi-Agent Orchestration** - Coordinate multiple AI agents working together
2. **Typed Functional Pipelines** - Hindley-Milner type inference for shell commands
3. **Multi-Modal AI** - Process images, audio, and video directly in your shell
4. **Protocol Support** - MCP, A2A, and NANDA protocols for agent communication
5. **Rich TUI** - Beautiful terminal interface for AI interactions
6. **AI Model Management** - Built-in CLI tool for model operations

---

## 📦 Installation

### From Source (Current)
```bash
git clone https://github.com/nervosys/AetherShell.git
cd AetherShell
cargo build --release
```

The binaries will be in `target/release/`:
- `ish` - Main IronShell executable
- `aimodel` - AI model management CLI

### Coming Soon
- Pre-built binaries for Windows, macOS, Linux
- Cargo install: `cargo install iron-shell`
- Package managers (Homebrew, Scoop, apt)

---

## 🚀 Quick Start

```bash
# Interactive REPL
ish

# Terminal UI (requires Windows Terminal or modern terminal)
ish tui

# Run example scripts
ish examples/00_hello.ish
ish examples/06_agent.ish

# AI Model Management
aimodel list
aimodel server start
```

---

## 📚 Documentation

- **README.md** - Comprehensive guide with examples
- **docs/TUI_GUIDE.md** - Terminal UI usage and requirements
- **examples/README.md** - All 18 examples explained
- **CONTRIBUTING.md** - How to contribute
- **CHANGELOG.md** - Detailed feature list

---

## ✨ Highlights

### Language Features
- First-class functions with lambda syntax: `fn(x) => x * 2`
- Pattern matching with `match` expressions
- Type-safe pipelines: `[1,2,3] | map(fn(x) => x * 2) | sum`
- Structured data types: Records, Arrays, Lambdas
- String interpolation: `"Hi, ${name}!"`

### AI Capabilities
- Multiple AI providers: OpenAI, Anthropic, Ollama, etc.
- Multi-modal inputs: text, images, audio, video
- Agent swarms with coordinator patterns
- Protocol support: MCP servers, A2A, NANDA
- LLM backend integration: vLLM, TensorRT-LLM, SGLang, llama.cpp

### Developer Experience
- 18 comprehensive examples (100% passing)
- Type inference with helpful error messages
- Bash compatibility layer for migration
- Cross-platform support (Windows, macOS, Linux)
- XDG Base Directory compliance

---

## 🎯 Getting Started Paths

### For Shell Users
Start with basic examples:
1. `00_hello.ish` - Hello world and string interpolation
2. `01_pipelines.ish` - Data pipelines
3. `02_tables.ish` - Working with structured data

### For AI Enthusiasts
Jump into AI features:
1. `05_ai.ish` - Basic AI interactions
2. `06_agent.ish` - Single agent workflows
3. `12_multi_agent_orchestration.ish` - Multi-agent coordination

### For Type System Fans
Explore the type system:
1. `14_typed_pipelines.ish` - Type-safe pipelines
2. `07_uri_types.ish` - Type-directed model selection
3. `99_comprehensive_test.ish` - Complex type interactions

---

## ⚠️ Known Limitations

1. **String multiplication** not yet supported (use `repeat()` function)
2. **If statements** only work in expression context (use `match` instead)
3. **Pipeline operators** may need parentheses in some contexts
4. **Array indexing** via `[n]` not yet implemented (use `first()`, `take()`, etc.)
5. **TUI mode** requires proper terminal support (Windows Terminal, not VS Code terminal)

See [CHANGELOG.md](CHANGELOG.md) for complete details.

---

## 🤝 Contributing

We welcome contributions! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Ways to Contribute
- 🐛 Report bugs
- 💡 Suggest features
- 📝 Improve documentation
- 🧪 Add tests and examples
- 💻 Submit code improvements

---

## 📊 Release Statistics

- **Version**: 0.1.0 (Initial Release)
- **Examples**: 18 (100% passing)
- **Tests**: 25+ comprehensive tests
- **Documentation**: 1000+ lines of guides
- **Dependencies**: 150+ well-maintained crates
- **Minimum Rust**: 1.75

---

## 🔮 What's Next?

See our roadmap in [README.md](README.md#roadmap) for upcoming features:
- Enhanced type system features
- More AI protocols and integrations
- Performance optimizations
- Package manager integrations
- Community-driven features

---

## 🙏 Acknowledgments

Built with amazing Rust crates:
- **ratatui** - TUI framework
- **reqwest** - HTTP client
- **tokio** - Async runtime
- **serde** - Serialization
- **anyhow** - Error handling
- And many more!

---

## 📞 Support & Community

- **Issues**: [GitHub Issues](https://github.com/nervosys/AetherShell/issues)
- **Discussions**: [GitHub Discussions](https://github.com/nervosys/AetherShell/discussions)
- **Documentation**: [docs/](https://github.com/nervosys/AetherShell/tree/master/docs)

---

## 📄 License

MIT License - Copyright 2025 Nervosys

See [LICENSE](LICENSE) for full details.

---

**Ready to revolutionize your shell experience?**

```bash
cargo build --release
./target/release/ish tui
```

Welcome to the future of shell computing! 🚀
