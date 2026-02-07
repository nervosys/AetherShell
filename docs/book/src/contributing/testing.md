# Testing

How to write and run tests for AetherShell.

## Running Tests

```bash
# All tests
cargo test

# Library tests only
cargo test --lib

# Specific test file
cargo test --test eval
cargo test --test pipeline

# Tests matching a pattern
cargo test parse_lambda
cargo test "test_builtin_"

# With output (for debugging)
cargo test -- --nocapture

# Single-threaded (for tests that share state)
cargo test -- --test-threads=1
```

## Test Organization

| Location | Purpose |
|----------|---------|
| `tests/eval.rs` | Core evaluator tests |
| `tests/parse.rs` | Parser unit tests |
| `tests/pipeline.rs` | Pipeline operator tests |
| `tests/builtins.rs` | Builtin function tests |
| `tests/typecheck.rs` | Type inference tests |
| `tests/smoke.rs` | Quick validation / smoke tests |
| `tests/transpile_bash.rs` | Bash transpiler tests |
| `tests/ai_*.rs` | AI integration tests |
| `tests/tui_*.rs` | TUI component tests |
| `test-scripts/` | Script-based integration tests |

## Writing Tests

### Evaluator Tests

Test that AetherShell expressions produce expected values:

```rust
use aether_shell::{eval_str, Value};

#[test]
fn test_arithmetic() {
    let result = eval_str("2 + 3").unwrap();
    assert_eq!(result, Value::Int(5));
}

#[test]
fn test_pipeline() {
    let result = eval_str("[1,2,3] | map(fn(x) => x * 2)").unwrap();
    assert_eq!(result, Value::Array(vec![
        Value::Int(2), Value::Int(4), Value::Int(6)
    ]));
}

#[test]
fn test_record_access() {
    let result = eval_str("let r = {name: \"test\", val: 42}; r.val").unwrap();
    assert_eq!(result, Value::Int(42));
}
```

### Parser Tests

Verify that source text parses to the expected AST:

```rust
use aether_shell::parser::parse;
use aether_shell::ast::*;

#[test]
fn test_parse_let() {
    let stmts = parse("let x = 42").unwrap();
    assert!(matches!(&stmts[0], Stmt::Let { name, .. } if name == "x"));
}

#[test]
fn test_parse_lambda() {
    let stmts = parse("fn(x) => x + 1").unwrap();
    // Verify AST structure
    assert!(matches!(&stmts[0], Stmt::Expr(Expr::Lambda { .. })));
}
```

### Builtin Tests

Test that builtins return the correct structured values:

```rust
#[test]
fn test_builtin_len() {
    let result = eval_str("len [1, 2, 3]").unwrap();
    assert_eq!(result, Value::Int(3));
}

#[test]
fn test_builtin_map() {
    let result = eval_str("[1,2,3] | map(fn(x) => x * 10)").unwrap();
    assert_eq!(result, Value::Array(vec![
        Value::Int(10), Value::Int(20), Value::Int(30)
    ]));
}

#[test]
fn test_builtin_where() {
    let result = eval_str("[1,2,3,4,5] | where(fn(x) => x > 3)").unwrap();
    assert_eq!(result, Value::Array(vec![Value::Int(4), Value::Int(5)]));
}
```

### Type Checker Tests

Verify type inference results:

```rust
#[test]
fn test_typecheck_int() {
    let ty = typecheck_str("42").unwrap();
    assert_eq!(ty, Type::Int);
}

#[test]
fn test_typecheck_lambda() {
    let ty = typecheck_str("fn(x) => x + 1").unwrap();
    assert!(matches!(ty, Type::Function(_, _)));
}
```

## Test Patterns

### Error Cases

Always test error conditions:

```rust
#[test]
fn test_divide_by_zero() {
    let result = eval_str("10 / 0");
    assert!(result.is_err());
}

#[test]
fn test_undefined_variable() {
    let result = eval_str("unknown_var");
    assert!(result.is_err());
}
```

### Script-Based Tests

For complex scenarios, use `.ae` test scripts in `test-scripts/`:

```aethershell
# test-scripts/builtins/test_basic.ae
# Each line is a self-contained assertion

let x = 42
assert x == 42

let arr = [1, 2, 3]
assert (len arr) == 3

let s = upper "hello"
assert s == "HELLO"
```

### AI Tests

AI tests that require API keys should check for availability:

```rust
#[test]
fn test_ai_completion() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("Skipping: OPENAI_API_KEY not set");
        return;
    }
    
    let result = eval_str(r#"ai "Say hello" { model: "openai:gpt-4o-mini" }"#).unwrap();
    assert!(matches!(result, Value::String(_)));
}
```

## Coverage

While there's no strict coverage requirement, aim for:

- **Core evaluator**: High coverage — test every expression type
- **Builtins**: At least one test per builtin function
- **Parser**: Test both valid syntax and error cases
- **Type checker**: Test each inference rule
- **AI/TUI**: Test structure and state, mock network calls

## Continuous Integration

Tests run automatically on pull requests. Ensure:

1. `cargo test` passes with no failures
2. `cargo clippy` produces no warnings
3. `cargo fmt --check` shows no formatting issues
