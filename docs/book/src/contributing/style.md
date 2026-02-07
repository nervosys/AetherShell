# Code Style

Coding conventions and style guidelines for AetherShell contributors.

## Rust Style

### General

- Follow standard Rust conventions (`rustfmt` defaults)
- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Maximum line length: 100 characters (soft limit)

### Naming

```rust
// Types: PascalCase
struct PipelineStage { ... }
enum Value { ... }

// Functions/methods: snake_case
fn evaluate_expr(expr: &Expr) -> Result<Value> { ... }

// Constants: SCREAMING_SNAKE_CASE
const MAX_PIPELINE_DEPTH: usize = 64;

// Module files: snake_case
// src/os_tools.rs, src/shell_features.rs
```

### Error Handling

```rust
// Use anyhow::Result for evaluator functions
fn eval_expr(expr: &Expr, env: &mut Env) -> anyhow::Result<Value> {
    // Add context for debugging
    some_operation().context("failed to evaluate pipeline stage")?;
    
    // Avoid .unwrap() in production code — use .expect() with a message or ?
    let val = map.get("key").context("missing required key")?;
    
    Ok(val)
}
```

### Return Types

Builtins must return structured `Value` types:

```rust
// Good: structured data for pipeline processing
fn builtin_ls(args: &[Value], env: &mut Env) -> Result<Value> {
    Ok(Value::Array(entries.into_iter().map(|e| {
        Value::Record(BTreeMap::from([
            ("name".into(), Value::String(e.name)),
            ("size".into(), Value::Int(e.size as i64)),
            ("is_dir".into(), Value::Bool(e.is_dir)),
        ]))
    }).collect()))
}

// Bad: raw text output
fn builtin_ls_bad() -> Result<Value> {
    Ok(Value::String("file1.txt\nfile2.txt".into()))
}
```

### Imports

```rust
// Group imports: std, external crates, local modules
use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::ast::{Expr, Stmt};
use crate::value::Value;
```

## Commit Messages

Follow [Conventional Commits](https://www.conventionalcommits.org/):

```
type(scope): description

feat(parser): add pattern matching syntax
fix(eval): correct lambda capture semantics
docs(book): add pipeline examples chapter
test(builtins): add filesystem operation tests
refactor(ai): extract provider trait
chore(deps): update tokio to 1.35
```

**Types:** `feat`, `fix`, `docs`, `test`, `refactor`, `chore`, `perf`, `ci`

**Scopes:** `parser`, `eval`, `builtins`, `ai`, `tui`, `agent`, `api`, `transpile`, `docs`, `deps`

## Documentation

### Code Comments

```rust
/// Evaluates an expression in the given environment.
///
/// # Arguments
/// * `expr` - The AST expression to evaluate
/// * `env` - Mutable reference to the variable environment
///
/// # Returns
/// The resulting `Value`, or an error if evaluation fails.
///
/// # Examples
/// ```
/// let result = eval_expr(&Expr::Int(42), &mut env)?;
/// assert_eq!(result, Value::Int(42));
/// ```
fn eval_expr(expr: &Expr, env: &mut Env) -> Result<Value> { ... }
```

### Module-Level Docs

Every `.rs` file should have a module-level doc comment:

```rust
//! Pipeline evaluation and data flow.
//!
//! This module handles the core pipeline operator (`|`), connecting
//! expressions so that the output of one feeds into the next.
```

## Adding New Features

The typical flow for language features:

1. **`ast.rs`** — Add AST node variants
2. **`tokens.rs`** — Add tokens if new syntax is needed
3. **`lexer.rs`** — Tokenize new syntax
4. **`parser.rs`** — Parse tokens into AST
5. **`eval.rs`** — Implement runtime semantics
6. **`typecheck.rs`** — Add type inference rules
7. **`tests/`** — Write comprehensive tests
