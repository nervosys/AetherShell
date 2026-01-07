# Missing Features for Examples

This document tracks features that are shown in example files but not yet fully implemented in AetherShell.

## Summary

**Status**: 12 out of 24 examples fail due to missing features  
**Passing**: 4 examples (`00_hello.ae`, `06_agent.ae`, `17_syntax_showcase.ae`, `18_mutable_variables.ae`)  
**TUI/Interactive**: 8 examples require `--tui` mode (not tested in batch mode)

---

## Critical Parser/Syntax Issues

### 1. ⚠️ `match` Expression Not Fully Wired
**Status**: PARTIALLY IMPLEMENTED  
**Affected Examples**: `04_match.ae`, `03_http.ae`

**Problem**: The `match` keyword is defined in the lexer, and there's a complete parser implementation (`parse_match()`) and evaluator code (`Expr::Match`), but the statement-level parser doesn't recognize it as a valid statement.

**Current Behavior**:
```
Error: unexpected token Match
```

**What Exists**:
- ✅ Lexer token: `Tok::Match`
- ✅ AST node: `Expr::Match { scrutinee, arms }`
- ✅ Parser function: `parse_match()` with pattern matching support
- ✅ Evaluator: Full match expression evaluation with guards
- ❌ Statement parser: Doesn't call `parse_match()` at statement level

**Fix Required**:
In `src/parser.rs`, the `parse_stmt()` function needs to check for `Tok::Match` and handle it as an expression statement:

```rust
fn parse_stmt(&mut self) -> Result<Stmt> {
    // ... existing code ...
    
    // Check for match expression at statement level
    if self.check(Tok::Match) {
        let expr = self.parse_expr()?;
        return Ok(Stmt::Expr(expr));
    }
    
    // ... rest of code ...
}
```

**Files to Modify**:
- `src/parser.rs` line ~365-410 (in `parse_stmt()`)

**Test Case** (from `04_match.ae`):
```rust
value = Some(42)

match value {
  None() => print("no value"),
  Some(x) if x > 40 => print("big: ${x}"),
  Some(x) => print("small: ${x}")
}
```

---

### 2. ⚠️ Method Chaining / Dot Notation Not Supported
**Status**: NOT IMPLEMENTED  
**Affected Examples**: `02_tables.ae`, `03_http.ae`

**Problem**: Examples use `.` for method chaining (e.g., `resp.status`, `resp.headers."content-type"`), but the lexer doesn't recognize `.` as a token.

**Current Behavior**:
```
Error: unknown character: .
```

**Example Usage**:
```rust
resp = http_get "https://api.github.com"
print(resp.status)              // Field access
print(resp.headers."content-type")  // Nested field access
```

**What's Missing**:
- Lexer token for `.` (dot)
- Parser support for field access expressions
- AST node like `Expr::FieldAccess { object: Box<Expr>, field: String }`
- Evaluator code to extract fields from `Value::Record`

**Implementation Complexity**: MEDIUM  
This requires:
1. Add `Tok::Dot` to lexer (`src/lexer.rs`)
2. Add `Expr::FieldAccess` to AST (`src/ast.rs`)
3. Parse postfix `.field` syntax in `parse_postfix()` (`src/parser.rs`)
4. Evaluate field access in `eval_expr()` (`src/eval.rs`)

**Files to Create/Modify**:
- `src/lexer.rs` - Add dot token recognition
- `src/ast.rs` - Add FieldAccess variant to Expr
- `src/parser.rs` - Add postfix dot parsing
- `src/eval.rs` - Add field access evaluation

---

### 3. ⚠️ Alternative Lambda Syntax (Missing Parentheses)
**Status**: PARTIALLY SUPPORTED  
**Affected Examples**: `01_pipelines.ae`, `19_showcase.ae`

**Problem**: Examples use `fn(x) => ...` (with parentheses around parameters), but some older examples might use `fn x => ...` (without parentheses). The parser expects parentheses after `fn`.

**Current Behavior**:
```
[5,4,3,2,1] | where fn(x) => x > 2 | take 2 | print
Works: [5,4]

[1,2,3,4] | map fn(x) => x * 2 | reduce fn(a,b) => a + b 0 | print
Error: where requires array input, got Str("20")
```

**Issue**: The `reduce` call is missing comma between the lambda and initial value `0`. Examples have syntax errors.

**What's Needed**: 
- Fix example syntax (not a missing feature)
- Document correct syntax: `reduce(fn(a,b) => a + b, 0)` with comma

**Files to Fix**:
- `examples/01_pipelines.ae` - Fix syntax
- `examples/19_showcase.ae` - Fix syntax

---

### 4. ❌ Comment Syntax Inconsistency
**Status**: DOCUMENTATION ISSUE  
**Affected Examples**: `12-16_*.ae` (multi-agent examples)

**Problem**: Several examples use `#` for comments (Unix shell style), but AetherShell only supports `//` comments (C++ style).

**Current Behavior**:
```
Error: unknown character: #
```

**What's Needed**: 
- Option A: Update examples to use `//` comments
- Option B: Add `#` comment support to lexer (would match Unix shell convention)

**Recommendation**: Update examples to use `//` (consistent with Rust-like syntax philosophy)

**Files to Fix**:
- `examples/12_multi_agent_orchestration.ae`
- `examples/13_multimodal_ai.ae`
- `examples/14_typed_pipelines.ae`
- `examples/15_ai_protocols.ae`
- `examples/16_mcp_servers.ae`

---

## Missing Built-in Functions

### 5. ❌ `read_text()` Builtin
**Status**: NOT IMPLEMENTED  
**Affected Examples**: `05_ai.ae`

**Usage**:
```rust
read_text "README.md" | ai "summarize as 3 bullet points"
```

**What's Needed**:
- Implement `bi_read_text()` in `src/builtins.rs`
- Read file to string, return `Value::String`
- Should handle UTF-8 and provide good error messages

**Implementation**:
```rust
fn bi_read_text(args: &[Value], _input: Value) -> Result<Value> {
    let path = args.get(0)
        .ok_or_else(|| anyhow!("read_text requires a file path"))?
        .as_str()?;
    
    let content = std::fs::read_to_string(path)
        .context(format!("Failed to read file: {}", path))?;
    
    Ok(Value::String(content))
}
```

**Files to Modify**:
- `src/builtins.rs` - Add function and register in dispatcher

---

### 6. ❌ `type_of()` Builtin
**Status**: NOT IMPLEMENTED  
**Affected Examples**: `07_uri_types.ae`

**Usage**:
```rust
let a = "https://example.com"
print(type_of(a))  // => Uri
```

**What's Needed**:
- Implement `bi_type_of()` in `src/builtins.rs`
- Return string representation of value's type
- Use `Value::type_name()` helper if available, or match on variants

**Implementation**:
```rust
fn bi_type_of(args: &[Value], input: Value) -> Result<Value> {
    let val = if args.is_empty() {
        input
    } else {
        args[0].clone()
    };
    
    let type_str = match val {
        Value::Null => "Null",
        Value::Bool(_) => "Bool",
        Value::Int(_) => "Int",
        Value::Float(_) => "Float",
        Value::String(_) => "String",
        Value::Uri(_) => "Uri",
        Value::Array(_) => "Array",
        Value::Record(_) => "Record",
        Value::Lambda(_, _, _) => "Lambda",
    };
    
    Ok(Value::String(type_str.to_string()))
}
```

**Files to Modify**:
- `src/builtins.rs` - Add function and register

---

### 7. ⚠️ `group_by()` and `agg()` Builtins
**Status**: PARTIALLY IMPLEMENTED (as `group`)  
**Affected Examples**: `02_tables.ae`

**Problem**: Example uses `group_by` and `agg`, but codebase has `group` (aliased as `Group-Object`).

**Usage in Example**:
```rust
ls "."
  | group_by "is_dir"
  | agg { count: count(), total_bytes: sum("size") }
```

**What's Needed**:
- Either: Add `group_by` as an alias for existing `group`
- Or: Update examples to use `group` instead of `group_by`
- Implement `agg()` builtin for aggregation operations
- Implement `count()` and `sum()` helper functions

**Files to Modify**:
- `src/builtins.rs` - Add aliases/new functions
- `examples/02_tables.ae` - Update syntax to match implementation

---

### 8. ❌ `keys()` and `len()` Builtins
**Status**: NOT IMPLEMENTED  
**Affected Examples**: `03_http.ae`

**Usage**:
```rust
match resp.body {
  { .. } => print("object keys: ${keys(resp.body)}"),
  [ .. ] => print("array length: ${len(resp.body)}"),
  _      => print("body: ${resp.body}")
}
```

**What's Needed**:
- `keys()`: Extract keys from a Record, return Array of Strings
- `len()`: Get length of Array or Record, return Int

**Implementation**:
```rust
fn bi_keys(args: &[Value], input: Value) -> Result<Value> {
    let val = if args.is_empty() { input } else { args[0].clone() };
    
    match val {
        Value::Record(map) => {
            let keys: Vec<Value> = map.keys()
                .map(|k| Value::String(k.clone()))
                .collect();
            Ok(Value::Array(keys))
        }
        _ => Err(anyhow!("keys() requires a Record"))
    }
}

fn bi_len(args: &[Value], input: Value) -> Result<Value> {
    let val = if args.is_empty() { input } else { args[0].clone() };
    
    let length = match val {
        Value::Array(ref v) => v.len(),
        Value::Record(ref m) => m.len(),
        Value::String(ref s) => s.len(),
        _ => return Err(anyhow!("len() requires Array, Record, or String"))
    };
    
    Ok(Value::Int(length as i64))
}
```

**Files to Modify**:
- `src/builtins.rs` - Add both functions

---

## Implementation Priority

### P0 - Quick Fixes (Documentation/Examples)
1. ✅ Fix `#` comments → use `//` in examples (bulk find/replace)
2. ✅ Fix `reduce` syntax in `01_pipelines.ae` and `19_showcase.ae`

### P1 - High Impact, Low Effort
1. 🔧 Fix `match` statement parsing (add 3-5 lines to `parse_stmt()`)
2. 🔧 Implement `read_text()` builtin (~10 lines)
3. 🔧 Implement `type_of()` builtin (~15 lines)
4. 🔧 Implement `keys()` and `len()` builtins (~20 lines each)

### P2 - Medium Impact, Medium Effort
1. 🏗️ Add `group_by` alias and `agg()` builtin (30-50 lines)
2. 🏗️ Method chaining / dot notation (moderate complexity, multiple files)

### P3 - Future Enhancements
1. 🎯 Alternative lambda syntax (if desired for consistency)
2. 🎯 Enhanced pattern matching features (already 90% complete)

---

## Testing Strategy

Once features are implemented, verify with:

```powershell
# Test specific examples
.\target\release\ae.exe .\examples\04_match.ae
.\target\release\ae.exe .\examples\05_ai.ae

# Test all non-TUI examples
foreach ($file in Get-ChildItem examples\*.ae | Where-Object { $_.Name -notmatch '^(09|10|11|20|21|22|23)_' }) {
    Write-Host "`n=== $($file.Name) ===" -ForegroundColor Cyan
    .\target\release\ae.exe $file.FullName
}
```

---

## Feature Completeness Roadmap

### Phase 1: Statement-Level Match (1 hour)
- [x] Identify issue (match not recognized at statement level)
- [ ] Add match check to `parse_stmt()`
- [ ] Test with `04_match.ae`
- [ ] Verify pattern matching with guards works

### Phase 2: Essential Builtins (2 hours)
- [ ] Implement `read_text()`
- [ ] Implement `type_of()`
- [ ] Implement `keys()`
- [ ] Implement `len()`
- [ ] Test with affected examples

### Phase 3: Syntax Cleanup (1 hour)
- [ ] Update all examples to use `//` comments
- [ ] Fix pipeline syntax errors
- [ ] Verify all syntax is consistent

### Phase 4: Method Chaining (8 hours)
- [ ] Design dot notation syntax
- [ ] Implement lexer changes
- [ ] Implement parser changes
- [ ] Implement evaluator changes
- [ ] Add comprehensive tests
- [ ] Update examples

### Phase 5: Table Operations (4 hours)
- [ ] Review existing `group` implementation
- [ ] Add `group_by` alias
- [ ] Implement `agg()` with aggregation functions
- [ ] Implement `count()`, `sum()`, `avg()`, etc.
- [ ] Test table pipeline examples

---

## Notes

- **Agent Examples**: `06_agent.ae` shows "(incomplete) max steps reached" - this is expected behavior for agent demos without external tool access
- **TUI Examples**: Examples 09-11 and 20-23 require interactive TUI mode (`--tui` flag) and cannot be tested in batch mode
- **Transpiler**: `08_transpiler.bash` is a bash script demonstrating transpilation, not an AetherShell script

---

**Last Updated**: October 20, 2025  
**Contributors**: Analysis based on example run outputs and source code review
