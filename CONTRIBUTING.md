# Contributing to AetherShell

Thank you for your interest in contributing to AetherShell! This document provides guidelines and instructions for contributing.

## 🎯 Ways to Contribute

- **Bug Reports**: Found a bug? Open an issue with reproduction steps
- **Feature Requests**: Have an idea? Discuss it in issues first
- **Code Contributions**: Fix bugs, add features, improve documentation
- **Examples**: Add new example scripts showcasing AetherShell features
- **Testing**: Help improve test coverage and reliability
- **Documentation**: Improve guides, fix typos, add clarifications

## 🚀 Getting Started

### Prerequisites

- Rust 1.75 or later
- Git
- Basic understanding of Rust and shell scripting

### Setting Up Development Environment

```bash
# Clone the repository
git clone https://github.com/nervosys/AetherShell
cd AetherShell

# Build the project
cargo build --release

# Run tests
cargo test --all-features

# Run specific test suite
cargo test --test ai_router
```

### Project Structure

```
AetherShell/
├── src/              # Core Rust source code
│   ├── main.rs       # Main binary (ae)
│   ├── lib.rs        # Library exports
│   ├── ai.rs         # AI integration
│   ├── agent.rs      # Agent system
│   ├── eval.rs       # Expression evaluator
│   ├── parser.rs     # Language parser
│   └── tui/          # Terminal UI modules
├── examples/         # Example .ae scripts
├── tests/            # Integration tests
├── docs/             # Documentation
└── Cargo.toml        # Package manifest
```

## 📝 Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/issue-number-description
```

### 2. Make Changes

- Write clean, idiomatic Rust code
- Follow existing code style and conventions
- Add tests for new functionality
- Update documentation as needed

### 3. Test Your Changes

```bash
# Run all tests
cargo test --all-features

# Run specific tests
cargo test ai_
cargo test tui_

# Check code formatting
cargo fmt --check

# Run clippy lints
cargo clippy --all-features -- -D warnings

# Build release binary
cargo build --release

# Test examples
./target/release/ae examples/00_hello.ae
```

### 4. Commit Guidelines

Write clear, descriptive commit messages:

```
feat: add support for video analysis in AI pipeline
fix: resolve panic in pattern matching with nested records
docs: update MCP servers guide with AWS examples
test: add integration tests for multi-agent negotiation
```

Use conventional commit prefixes:
- `feat:` - New features
- `fix:` - Bug fixes
- `docs:` - Documentation changes
- `test:` - Test additions/changes
- `refactor:` - Code refactoring
- `perf:` - Performance improvements
- `chore:` - Maintenance tasks

### 5. Submit Pull Request

1. Push your branch to GitHub
2. Open a Pull Request with:
   - Clear title describing the change
   - Detailed description of what and why
   - Link to related issues
   - Screenshots/examples if applicable

## 🧪 Testing Guidelines

### Writing Tests

- Place unit tests in the same file as the code being tested
- Place integration tests in `tests/` directory
- Use descriptive test names that explain what is being tested

Example:

```rust
#[test]
fn test_dot_notation_with_nested_records() {
    let code = r#"
        data = {user: {name: "Alice", age: 30}}
        result = data.user.name
        print(result)
    "#;
    let output = run_test(code);
    assert_eq!(output.trim(), "\"Alice\"");
}
```

### Test Coverage

- Aim for comprehensive test coverage
- Test both success and failure cases
- Test edge cases and boundary conditions
- Add regression tests for fixed bugs

## 📚 Documentation Guidelines

### Code Documentation

- Document all public APIs with `///` comments
- Include examples in doc comments
- Explain complex logic with inline comments

Example:

```rust
/// Evaluates a binary operation on two values.
///
/// # Arguments
/// * `op` - The binary operator (Add, Sub, Mul, etc.)
/// * `lhs` - The left-hand side value
/// * `rhs` - The right-hand side value
///
/// # Returns
/// The result of the operation as a `Value`
///
/// # Example
/// ```
/// let result = eval_binop(BinOp::Add, Value::Int(2), Value::Int(3));
/// assert_eq!(result, Value::Int(5));
/// ```
fn eval_binop(op: BinOp, lhs: Value, rhs: Value) -> Result<Value> {
    // Implementation
}
```

### Example Scripts

When adding examples:

- Use clear, descriptive filenames (e.g., `08_multimodal_analysis.ae`)
- Include comments explaining each section
- Show practical, real-world use cases
- Keep examples concise but complete

## 🎨 Code Style

### Rust Code Style

- Follow standard Rust formatting (`cargo fmt`)
- Use `cargo clippy` to catch common issues
- Prefer explicit error handling over `.unwrap()`
- Use meaningful variable and function names
- Keep functions focused and reasonably sized

### AetherShell Script Style

```ae
// Use line comments (// not #)
// Prefer = for variable assignment
name = "Alice"

// Use meaningful variable names
user_data = {id: 1, name: "Alice", age: 30}

// Use pipelines for data transformations
results = data
  | where(fn(x) => x.value > 100)
  | map(fn(x) => {name: x.name, doubled: x.value * 2})
  | sort_by(fn(x) => x.doubled, "desc")
```

## 🐛 Bug Reports

When reporting bugs, please include:

1. **Description**: Clear description of the issue
2. **Reproduction**: Minimal code to reproduce
3. **Expected**: What you expected to happen
4. **Actual**: What actually happened
5. **Environment**:
   - OS (Windows/Linux/macOS)
   - Rust version (`rustc --version`)
   - AetherShell version
6. **Logs**: Relevant error messages or stack traces

Example:

```markdown
## Bug: Panic when using dot notation with null values

**Reproduction:**
```ae
data = {user: null}
result = data.user.name  // Panics here
```

**Expected:** Should return null or error gracefully

**Actual:** Panics with "attempted to access field on null value"

**Environment:**
- OS: Windows 11
- Rust: 1.75.0
- AetherShell: 0.1.0
```

## 💡 Feature Requests

When proposing features:

1. **Problem**: Describe the problem or use case
2. **Proposed Solution**: Your suggested approach
3. **Alternatives**: Other approaches considered
4. **Examples**: Show how it would work

## 🔍 Code Review Process

All submissions require review:

1. Automated checks must pass:
   - Tests (`cargo test`)
   - Formatting (`cargo fmt --check`)
   - Lints (`cargo clippy`)
2. At least one maintainer approval
3. All feedback addressed or discussed

## 🤝 Community Guidelines

- Be respectful and constructive
- Help others learn and grow
- Provide specific, actionable feedback
- Assume good intentions
- Follow the [Rust Code of Conduct](https://www.rust-lang.org/policies/code-of-conduct)

## 📞 Getting Help

- **GitHub Issues**: For bugs and feature requests
- **Discussions**: For questions and general discussion
- **Documentation**: Check `docs/` directory

## 📄 License

By contributing to AetherShell, you agree that your contributions will be licensed under the MIT License.

---

**Thank you for contributing to AetherShell! Together we're building the future of shell interaction.** 🚀
