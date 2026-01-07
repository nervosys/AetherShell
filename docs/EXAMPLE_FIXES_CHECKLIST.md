# Example Fixes Checklist

Quick reference for fixing all 12 failing examples.

## Current Status
- ✅ **4 passing**: `00_hello.ae`, `06_agent.ae`, `17_syntax_showcase.ae`, `18_mutable_variables.ae`
- ❌ **12 failing**: Need fixes documented below
- ⏭️ **8 skipped**: TUI/interactive examples (not testable in batch mode)

---

## P0: Quick Documentation Fixes (30 minutes)

### [ ] Fix Comment Syntax in 5 Examples
Replace `#` with `//` in:
- [ ] `examples/12_multi_agent_orchestration.ae`
- [ ] `examples/13_multimodal_ai.ae`
- [ ] `examples/14_typed_pipelines.ae`
- [ ] `examples/15_ai_protocols.ae`
- [ ] `examples/16_mcp_servers.ae`

**Command**:
```powershell
foreach ($f in 12..16) { 
    $file = "examples/${f}_*.ae"
    (Get-Content $file) -replace '^#', '//' | Set-Content $file
}
```

### [ ] Fix Pipeline Syntax Errors
- [ ] `examples/01_pipelines.ae` - Line 2: Add comma in reduce
  - Change: `reduce fn(a,b) => a + b 0`
  - To: `reduce(fn(a,b) => a + b, 0)`
  
- [ ] `examples/19_showcase.ae` - Check all pipeline syntax

---

## P1: High-Impact Code Fixes (2-3 hours)

### [ ] 1. Fix `match` Statement Parsing (15 minutes)

**File**: `src/parser.rs` (~line 365-410 in `parse_stmt()`)

**Add**:
```rust
fn parse_stmt(&mut self) -> Result<Stmt> {
    // ... existing mut/let checks ...
    
    // Handle match expressions at statement level
    if self.check(Tok::Match) {
        let expr = self.parse_expr()?;
        return Ok(Stmt::Expr(expr));
    }
    
    // ... rest of function ...
}
```

**Test**:
```powershell
cargo build --release
.\target\release\ae.exe .\examples\04_match.ae
```

**Affected Examples**:
- [ ] `04_match.ae` - Should print "big: 42"
- [ ] `03_http.ae` - Uses match in body (needs dot notation too)

---

### [ ] 2. Implement `read_text()` Builtin (20 minutes)

**File**: `src/builtins.rs`

**Add function**:
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

**Register in dispatcher** (around line 30):
```rust
"read_text" => bi_read_text(args, input),
```

**Test**:
```powershell
cargo build --release
.\target\release\ae.exe .\examples\05_ai.ae  # May need AI key
```

**Affected Examples**:
- [ ] `05_ai.ae`

---

### [ ] 3. Implement `type_of()` Builtin (20 minutes)

**File**: `src/builtins.rs`

**Add function**:
```rust
fn bi_type_of(args: &[Value], input: Value) -> Result<Value> {
    let val = if args.is_empty() { input } else { args[0].clone() };
    
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

**Register**:
```rust
"type_of" => bi_type_of(args, input),
```

**Test**:
```powershell
.\target\release\ae.exe .\examples\07_uri_types.ae
```

**Affected Examples**:
- [ ] `07_uri_types.ae`

---

### [ ] 4. Implement `keys()` Builtin (20 minutes)

**File**: `src/builtins.rs`

**Add function**:
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
```

**Register**:
```rust
"keys" => bi_keys(args, input),
```

**Affected Examples**:
- [ ] `03_http.ae` (partial - also needs dot notation)

---

### [ ] 5. Implement `len()` Builtin (20 minutes)

**File**: `src/builtins.rs`

**Add function**:
```rust
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

**Register**:
```rust
"len" => bi_len(args, input),
```

**Affected Examples**:
- [ ] `03_http.ae` (partial)

---

## P2: Medium Effort Features (4-8 hours)

### [ ] 6. Implement Dot Notation for Field Access

**Complexity**: Medium (requires changes across 4 files)

#### Step 1: Add Dot Token (5 minutes)
**File**: `src/lexer.rs` or `src/tokens.rs`
```rust
pub enum Tok {
    // ... existing tokens ...
    Dot,  // Add this
}
```

#### Step 2: Add FieldAccess AST Node (5 minutes)
**File**: `src/ast.rs`
```rust
pub enum Expr {
    // ... existing variants ...
    FieldAccess {
        object: Box<Expr>,
        field: String,
    },
}
```

#### Step 3: Parse Postfix Dot (30 minutes)
**File**: `src/parser.rs`

Add postfix parsing after primary expression:
```rust
fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr> {
    while self.match_tok(Tok::Dot) {
        let field = self.need_ident("expected field name after '.'")?;
        expr = Expr::FieldAccess {
            object: Box::new(expr),
            field,
        };
    }
    Ok(expr)
}
```

#### Step 4: Evaluate Field Access (30 minutes)
**File**: `src/eval.rs`
```rust
Expr::FieldAccess { object, field } => {
    let obj_value = eval_expr(object, env)?;
    match obj_value {
        Value::Record(ref map) => {
            map.get(field)
                .cloned()
                .ok_or_else(|| anyhow!("Field '{}' not found", field))
        }
        _ => Err(anyhow!("Cannot access field on non-record value"))
    }
}
```

#### Step 5: Test
**Affected Examples**:
- [ ] `02_tables.ae`
- [ ] `03_http.ae`

---

### [ ] 7. Implement Table Aggregation Functions

**File**: `src/builtins.rs`

#### [ ] Add `group_by` alias
```rust
"group_by" => bi_group_object(args, input),  // Alias for existing 'group'
```

#### [ ] Implement `agg()` builtin (1-2 hours)
Aggregation with `count()`, `sum()`, `avg()`, `min()`, `max()`

**Affected Examples**:
- [ ] `02_tables.ae`

---

## Testing Commands

### Test Individual Examples
```powershell
# After each fix
cargo build --release
.\target\release\ae.exe .\examples\XX_name.ae
```

### Test All Non-TUI Examples
```powershell
foreach ($file in Get-ChildItem examples\*.ae | Where-Object { $_.Name -notmatch '^(09|10|11|20|21|22|23)_' }) {
    Write-Host "`n=== $($file.Name) ===" -ForegroundColor Cyan
    .\target\release\ae.exe $file.FullName
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ PASSED" -ForegroundColor Green
    } else {
        Write-Host "❌ FAILED" -ForegroundColor Red
    }
}
```

### Run Unit Tests
```powershell
cargo test --workspace
```

---

## Expected Outcomes After Fixes

| Example                   | Current Status | After P0 | After P1  | After P2 |
| ------------------------- | -------------- | -------- | --------- | -------- |
| `00_hello.ae`             | ✅ Pass         | ✅        | ✅         | ✅        |
| `01_pipelines.ae`         | ❌ Syntax       | ✅        | ✅         | ✅        |
| `02_tables.ae`            | ❌ Dot          | ❌        | ❌         | ✅        |
| `03_http.ae`              | ❌ Dot          | ❌        | ⚠️ Partial | ✅        |
| `04_match.ae`             | ❌ Match        | ❌        | ✅         | ✅        |
| `05_ai.ae`                | ❌ read_text    | ❌        | ✅         | ✅        |
| `06_agent.ae`             | ✅ Pass         | ✅        | ✅         | ✅        |
| `07_uri_types.ae`         | ❌ type_of      | ❌        | ✅         | ✅        |
| `12_*.ae`                 | ❌ Comments     | ✅        | ✅         | ✅        |
| `13_*.ae`                 | ❌ Comments     | ✅        | ✅         | ✅        |
| `14_*.ae`                 | ❌ Comments     | ✅        | ✅         | ✅        |
| `15_*.ae`                 | ❌ Comments     | ✅        | ✅         | ✅        |
| `16_*.ae`                 | ❌ Comments     | ✅        | ✅         | ✅        |
| `17_syntax_showcase.ae`   | ✅ Pass         | ✅        | ✅         | ✅        |
| `18_mutable_variables.ae` | ✅ Pass         | ✅        | ✅         | ✅        |
| `19_showcase.ae`          | ❌ Syntax       | ✅        | ✅         | ✅        |

**Summary**:
- **After P0**: 10 passing (↑ 6 from syntax fixes)
- **After P1**: 14 passing (↑ 4 from builtins + match)
- **After P2**: 16 passing (↑ 2 from dot notation)

---

**Last Updated**: October 20, 2025
