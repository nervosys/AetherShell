# Quick Reference: Missing Features

**TL;DR**: 12 of 24 examples fail. Most can be fixed in ~3 hours total.

## Failure Breakdown

| Category                      | Count  | Fix Time    | Difficulty |
| ----------------------------- | ------ | ----------- | ---------- |
| Comment syntax (`#` vs `//`)  | 5      | 5 min       | Trivial    |
| Pipeline syntax errors        | 2      | 5 min       | Trivial    |
| Missing `match` at stmt level | 1      | 15 min      | Easy       |
| Missing builtins              | 4      | 2 hrs       | Easy       |
| Dot notation                  | 2      | 8 hrs       | Medium     |
| **TOTAL**                     | **12** | **~11 hrs** | **Mixed**  |

## Files to Fix

### Examples (7 files - 10 minutes total)
```
examples/01_pipelines.ae       → Fix reduce syntax
examples/12_*.ae              → Change # to //
examples/13_*.ae              → Change # to //
examples/14_*.ae              → Change # to //
examples/15_*.ae              → Change # to //
examples/16_*.ae              → Change # to //
examples/19_showcase.ae       → Fix pipeline syntax
```

### Source Code (2 files - 2-3 hours total)
```
src/parser.rs                  → Add match at statement level (5 lines)
src/builtins.rs               → Add 4 new functions (~80 lines)
  - read_text()
  - type_of()
  - keys()
  - len()
```

### Optional (Medium Effort)
```
src/lexer.rs                  → Add Tok::Dot
src/ast.rs                    → Add Expr::FieldAccess
src/parser.rs                 → Parse postfix dot notation
src/eval.rs                   → Evaluate field access
```

## One-Command Fixes

### Fix All Comment Syntax
```powershell
12..16 | ForEach-Object {
    $f = (Get-ChildItem "examples\${_}_*.ae")[0]
    (Get-Content $f.FullName) -replace '^#', '//' | Set-Content $f.FullName
}
```

### Test All Examples
```powershell
Get-ChildItem examples\*.ae | 
    Where-Object { $_.Name -notmatch '^(08|09|10|11|20|21|22|23)_' } |
    ForEach-Object {
        Write-Host "`n=== $($_.Name) ===" -ForegroundColor Cyan
        .\target\release\ae.exe $_.FullName
    }
```

## Impact Analysis

| Fix              | Examples Fixed | Cumulative Total |
| ---------------- | -------------- | ---------------- |
| **Before**       | -              | **4 passing**    |
| Comment syntax   | +5             | 9 passing        |
| Pipeline syntax  | +1             | 10 passing       |
| match + builtins | +4             | 14 passing       |
| Dot notation     | +2             | **16 passing**   |

## Most Critical Fix

**Match Statement Parsing** (15 minutes, fixes 1 example immediately)

Add to `src/parser.rs` in `parse_stmt()` around line 365:
```rust
if self.check(Tok::Match) {
    let expr = self.parse_expr()?;
    return Ok(Stmt::Expr(expr));
}
```

All the infrastructure exists - just needs to be wired up!

---

See full docs:
- `docs/MISSING_FEATURES.md` - Complete analysis
- `docs/EXAMPLE_FIXES_CHECKLIST.md` - Implementation steps
- `docs/EXAMPLE_TEST_RESULTS.md` - Test outputs
