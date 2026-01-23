# AetherShell Roadmap

> **Last Updated:** January 23, 2026

This document tracks the development progress of AetherShell, the world's first agentic shell with typed functional pipelines and multi-modal AI.

---

## 📊 Progress Overview

| Category         | Status     | Completion             |
| ---------------- | ---------- | ---------------------- |
| Core Language    | ✅ Complete | 100%                   |
| Type System      | ✅ Complete | 100%                   |
| Builtins Library | ✅ Complete | 157+ functions         |
| AI Integration   | ✅ Complete | Multi-provider         |
| TUI Interface    | ✅ Complete | Full featured          |
| Theme System     | ✅ Complete | 38 themes              |
| Config System    | ✅ Complete | XDG-compliant          |
| Plugin System    | ✅ Complete | 7 builtins, 3 handlers |
| Performance      | ✅ Complete | 5 benchmark suites     |
| Test Coverage    | ✅ Complete | 100% pass rate         |
| Documentation    | ✅ Complete | Comprehensive          |
| Publishing       | ✅ Complete | crates.io v0.1.2       |
| WASM Support     | ✅ Complete | Browser REPL ready     |

---

## ✅ Completed Features

### January 2026

#### Testing Infrastructure
- [x] Comprehensive testing strategy (TESTING.md)
- [x] 52 Rust tests across 7 test suites
- [x] 8 AetherShell coverage test files
- [x] 100% test pass rate
- [x] Test coverage runner script (scripts/test_coverage.ps1)
- [x] Known parser quirks documented

#### Documentation & Polish
- [x] README rewrite with comprehensive examples
- [x] Real-world use cases (7 scenarios)
- [x] Language features reference
- [x] 143+ builtins documented by category
- [x] Security audit documentation
- [x] Domain migration (nervosys.com → nervosys.ai)

#### Theme & Configuration System
- [x] 38 built-in color themes
- [x] XDG Base Directory compliance
- [x] Platform-aware config paths (Windows/macOS/Linux)
- [x] `config()`, `config_get()`, `config_set()` builtins
- [x] `config_path()` returns all XDG paths
- [x] `config_init()`, `config_reload()` for management
- [x] `themes()` builtin listing all available themes

#### Syntax Colorization
- [x] Full terminal colorization with theme support
- [x] Colored REPL output
- [x] Error message colorization
- [x] Prompt customization

### December 2025

#### Core Language
- [x] AST-based evaluation engine
- [x] Hindley-Milner type inference
- [x] Typed pipelines (not text streams)
- [x] First-class functions and lambdas
- [x] Pattern matching with guards
- [x] String interpolation
- [x] Record and array literals

#### Builtins Library (143+ functions)
- [x] Core: help, call, clear, echo, print, Some, None
- [x] Functional: map, where, reduce, take, any, all, first, last
- [x] String: split, join, trim, upper, lower, replace, contains
- [x] Array: flatten, reverse, slice, range, zip, push, concat
- [x] Math: abs, min, max, sqrt, pow, floor, ceil, round
- [x] Aggregate: sum, avg, product, unique, values
- [x] File System: ls, cat, pwd, cd, exists, mkdir, rm
- [x] OS Tools: env, which, os, arch, hostname

#### AI Integration
- [x] Multi-provider support (OpenAI, Ollama, local models)
- [x] `ai()` builtin for queries
- [x] Multi-modal support (images, audio, video)
- [x] `agent()` builtin for autonomous agents
- [x] Tool access for agents
- [x] Dry-run mode for previewing actions
- [x] Agent swarms for distributed tasks

#### TUI Interface
- [x] Interactive terminal UI with tabs
- [x] Chat interface with context
- [x] Agent dashboard for swarm control
- [x] Media viewer for images/audio/video
- [x] Keyboard shortcuts
- [x] Help system

#### MCP Protocol
- [x] 130+ MCP tools across 27 categories
- [x] `mcp_tools()` for tool discovery
- [x] `mcp_call()` for tool execution
- [x] Category-based filtering

#### Neural Networks & ML
- [x] `nn_create()` for neural network creation
- [x] Evolutionary algorithms
- [x] Reinforcement learning (Q-Learning, DQN, Actor-Critic)
- [x] NEAT implementation
- [x] Consensus networks for distributed AI

#### Publishing
- [x] Published to crates.io as `aether_shell`
- [x] Version 0.1.2
- [x] Proper metadata and documentation
- [x] GitHub Actions CI/CD

#### Plugin System (January 2026)
- [x] Plugin architecture design (PluginRegistry, traits)
- [x] Dynamic plugin loading from TOML manifests
- [x] Plugin builtins: `plugins()`, `plugin_info()`, `plugin_enable()`, `plugin_disable()`, `plugin_load()`, `plugin_unload()`, `plugin_categories()`
- [x] Plugin API documentation (docs/PLUGINS.md)
- [x] Example plugins (hello-plugin, math-utils, string-utils)
- [x] Built-in file handlers (JSON, CSV, TOML)
- [x] 19 plugin tests (100% pass rate)

#### Performance Optimization (January 2026)
- [x] Benchmark suite (5 benchmark files: parser, eval, pipeline, builtin, MCP)
- [x] Performance documentation (docs/PERFORMANCE.md)
- [x] Baseline measurements established
- [x] Cold start verified (~15ms)
- [x] Hot paths verified efficient (HashMap dispatch, direct pattern matching)

---

## 🔄 In Progress

### Q1 2026

*No items currently in progress*

---

## ✅ Recently Completed

### January 2026

#### Error Recovery & Diagnostics (Completed)
- [x] Line and column tracking in lexer
- [x] All parser errors now include line/column information
- [x] Lexer errors include location information
- [x] Suggestion system for common errors:
  - Unclosed delimiter detection (suggests matching bracket/paren/brace)
  - Keyword typo suggestions (lte→let, fun→fn, ture→true, etc.)
  - Two-identifier-in-a-row detection (missing operator)
- [x] Parser error recovery with `synchronize()` method
- [x] Multiple errors reported in single parse (continues after errors)
- [x] `parse_program_strict()` for cases where single-error-stop is needed
- [x] Safe bounds checking in `peek()` and `prev()`
- [x] 12 error diagnostic tests

#### Async/Await Syntax (Completed)
- [x] `async fn(params) => expr` - Async lambda definition
- [x] `await expr` - Await expression for futures
- [x] `Value::AsyncLambda` and `Value::Future` runtime types
- [x] Calling async lambda returns Future (lazy evaluation)
- [x] Await executes Future and returns result
- [x] type_of() support for async types
- [x] Pipeline integration for async lambdas
- [x] 13 comprehensive tests

#### Error Handling (Completed)
- [x] `try { expr } catch { handler }` - Try/catch expression
- [x] `try { expr } catch e { handler }` - Catch with error binding
- [x] `throw expr` - Throw expression for raising errors
- [x] `Value::Error` type for error values
- [x] `is_error(value)` builtin to check for errors
- [x] Nested try/catch support
- [x] Error propagation through catch blocks
- [x] 16 comprehensive tests

#### Debugging Tools (Completed)
- [x] `debug(value)` - Print value with type and return it for chaining
- [x] `dbg(value)` - Alias for debug
- [x] `trace(label, value)` - Labeled debugging for pipeline traces
- [x] `assert(condition, msg?)` - Runtime assertions with optional message
- [x] `type_assert(value, type)` - Assert value has expected type
- [x] `assert_type(value, type)` - Alias for type_assert
- [x] `inspect(value)` - Detailed value inspection returning Record
- [x] 41 comprehensive tests

#### Conditional Compilation (Completed)
- [x] `#[cfg(platform)]` - Platform checks (windows, linux, macos, unix)
- [x] `#[cfg(feature = "name")]` - Feature flags (via AETHER_FEATURES env var)
- [x] `#[cfg(not(...))]` - Negation combinator
- [x] `#[cfg(all(...))]` - All conditions must match
- [x] `#[cfg(any(...))]` - Any condition can match
- [x] Nested condition support
- [x] 9 comprehensive tests

#### Standard Library (Completed)
- [x] Standard library directory (`lib/`)
- [x] **prelude.ae** - Core utilities (id, not, is_some, is_none, get_or, clamp)
- [x] **math.ae** - Math constants and functions (PI, E, is_even, factorial, gcd, lcm, lerp, square)
- [x] **string.ae** - String utilities (words, unwords, capitalize, title_case, snake_case, kebab_case, repeat, reverse_str)
- [x] **collection.ae** - Set operations (union, intersect, difference, is_subset, partition, sort_desc)
- [x] **functional.ae** - FP utilities (curry, uncurry, partial, complement, find, drop)
- [x] **io.ae** - File utilities (read_json, read_lines, file_ext, file_name, path_join)
- [x] Standard library tests (lib/test_stdlib.ae)
- [x] Comprehensive documentation (lib/README.md)

#### N-ary Lambda Support (Completed)
- [x] Fixed evaluator to support lambdas with 3+ parameters
- [x] Added `call_lambda_n` generic function for arbitrary arity
- [x] Updated `call_value` dispatch for 3, 4, 5+ arg lambdas
- [x] Added tests for n-ary lambdas (lambda_three_args, lambda_four_args, lambda_five_args)

#### Module Visibility System (Completed)
- [x] Module visibility modifiers (pub, private)
  - `pub let x = value` - Public variable/function
  - `pub x = value` - Public shorthand syntax
  - `let x = value` - Private by default
- [x] Module re-exports
  - `export { a, b }` - Export existing items
  - `export { a as x }` - Export with alias
  - `export { a, b } from "path"` - Re-export from module
- [x] Environment visibility tracking (is_exported, exported_vars)
- [x] Import respects visibility (only exported items importable)
- [x] Module visibility tests (10 tests, 100% pass)

#### Package Management (Completed)
- [x] `import` statement with multiple syntax forms:
  - `import "path/to/module.ae"` - Import all exports
  - `import "path" as name` - Import as namespaced record
  - `import { a, b } from "path"` - Selective imports
  - `import { a as x } from "path"` - Aliased imports
  - `import "pkg:name@version"` - Package registry imports
- [x] Package manifest (aether.toml) support
- [x] Module cache with cycle detection
- [x] Package registry client (packages.nervosys.ai)
- [x] Import path resolution (relative, absolute, search paths)
- [x] Semver version management
- [x] Package builtins: pkg_list(), pkg_info(), pkg_cache_dir(), pkg_init()

#### VS Code Extension v0.2.0 (Completed)
- [x] Document Symbol Provider (outline view)
- [x] Folding Range Provider (code folding)
- [x] Hover Provider with 70+ builtin docs
- [x] Markdown preview syntax highlighting
- [x] Published as admercs.aethershell v0.2.0

#### WASM Support (Completed)
- [x] Feature flags for platform-specific code (native/web)
- [x] Core modules (ast, env, parser, value, types) shared across builds
- [x] Native-only modules gated with `#[cfg(feature = "native")]`
- [x] wasm-bindgen bindings with full evaluator
- [x] Browser-based REPL (web/index.html)
- [x] 40+ builtins ported to WASM (map, where, reduce, etc.)
- [x] Pattern matching support in WASM
- [x] Pipeline evaluation in WASM

---

## 📅 Planned Features

### Q2 2026

#### Platform-Specific Features
- [ ] Platform-specific module loading
- [ ] Feature flags for optional functionality

### Q3 2026

#### IDE Integration
- [ ] VS Code extension improvements
- [ ] Language Server Protocol (LSP)
- [ ] IntelliSense support
- [ ] Inline documentation

#### Distributed Computing
- [ ] Remote agent execution
- [ ] Cluster management
- [ ] Job scheduling
- [ ] Result aggregation

### Q4 2026

#### Enterprise Features
- [ ] RBAC (Role-Based Access Control)
- [ ] Audit logging
- [ ] SSO integration
- [ ] Compliance reporting

#### Advanced AI
- [ ] Custom model fine-tuning
- [ ] RAG (Retrieval-Augmented Generation)
- [ ] Knowledge graphs
- [ ] Semantic caching

---

## 🎯 Version Milestones

### v0.1.x (Current)
- Core language features
- Basic AI integration
- TUI interface
- 143+ builtins

### v0.2.0 (Planned)
- Plugin system
- Performance improvements
- Enhanced error messages
- More builtins

### v0.3.0 (Planned)
- WASM support
- Package management
- Module system

### v1.0.0 (Future)
- Production-ready stability
- Full documentation
- Enterprise features
- Long-term support

---

## 🐛 Known Issues

### Parser Quirks
1. Pipeline assignments need parentheses when followed by another assignment:
   ```ae
   x = ([1,2,3] | reverse)  # Needs parentheses
   y = first(x)
   ```

2. Some builtins are pipeline-only: `flatten`, `reverse`, `slice`, `any`, `all`

3. Zero-parameter lambdas not supported (use `fn(_) => expr`)

### To Fix
- [ ] Parser: Allow pipeline assignments without parentheses
- [ ] Parser: Support zero-parameter lambdas
- [ ] Builtins: Consistent call syntax (pipeline vs function)

---

## 📈 Metrics

### Test Coverage
- **Rust Tests:** 52 passing
- **AetherShell Tests:** 8 files, all passing
- **Pass Rate:** 100%

### Codebase
- **Language:** Rust
- **Lines of Code:** ~15,000+
- **Builtins:** 143+
- **Themes:** 38

### Community
- **GitHub Stars:** Growing
- **crates.io Downloads:** Tracking
- **Contributors:** Welcome!

---

## 🤝 Contributing

We welcome contributions! Priority areas:

1. **High Priority**
   - Plugin system implementation
   - WASM compilation support
   - Additional builtins

2. **Medium Priority**
   - Documentation improvements
   - Test coverage expansion
   - Performance optimization

3. **Good First Issues**
   - New theme additions
   - Builtin function enhancements
   - Example scripts

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

---

## 📞 Contact

- **GitHub Issues:** [Report bugs or request features](https://github.com/nervosys/AetherShell/issues)
- **Discussions:** [Join the conversation](https://github.com/nervosys/AetherShell/discussions)
- **Website:** [nervosys.ai](https://nervosys.ai)

---

<p align="center">
  <em>Building the future of shell interaction, one feature at a time.</em>
</p>
