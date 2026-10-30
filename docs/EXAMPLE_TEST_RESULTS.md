# Example Test Results

**Test Date**: October 20, 2025  
**Test Command**: `.\target\release\ish.exe <example>`

## Summary Statistics

- **Total Examples**: 24
- **Passing**: 4 (17%)
- **Failing**: 12 (50%)
- **Skipped** (TUI/Interactive): 8 (33%)

---

## ✅ Passing Examples (4)

### `00_hello.ish` ✅
**Output**:
```
"Hello, Æther!"
"Hi, world!"
Bool(false)
```
**Status**: Working perfectly

---

### `06_agent.ish` ✅
**Output**:
```
"(incomplete) max steps reached; trace: [AgentStep { thought: "[ai:stu…
```
**Status**: Working as expected (agent reaches max steps without external tools)

---

### `17_syntax_showcase.ish` ✅
**Output**: Shows core language features (numbers, arrays, records, functions, pipelines, etc.)  
**Status**: Working perfectly, demonstrates most implemented features

---

### `18_mutable_variables.ish` ✅
**Output**: Demonstrates mutable variable patterns (counter, accumulator, state machine, etc.)  
**Status**: Working perfectly

---

## ❌ Failing Examples (12)

### `01_pipelines.ish` ❌
**Error**: `where requires array input, got Str("20")`  
**Root Cause**: Syntax error - missing comma in reduce call  
**Line**: `reduce fn(a,b) => a + b 0` should be `reduce(fn(a,b) => a + b, 0)`  
**Fix Priority**: P0 (documentation/syntax fix)

---

### `02_tables.ish` ❌
**Error**: `unknown character: .`  
**Root Cause**: Dot notation for field access not implemented  
**Missing Feature**: Method chaining (e.g., `r.is_dir`, `r.size`)  
**Fix Priority**: P2 (medium effort - requires parser/eval changes)

---

### `03_http.ish` ❌
**Error**: `unknown character: .`  
**Root Cause**: Dot notation not implemented  
**Missing Features**:
- Dot notation (e.g., `resp.status`, `resp.headers."content-type"`)
- `keys()` and `len()` builtins
**Fix Priority**: P1-P2 (builtins are P1, dot notation is P2)

---

### `04_match.ish` ❌
**Error**: `unexpected token Match`  
**Root Cause**: Match expression not wired at statement level  
**Issue**: Parser has `parse_match()` but doesn't call it from `parse_stmt()`  
**Fix Priority**: P1 (5-line fix, high impact)

---

### `05_ai.ish` ❌
**Error**: `unknown builtin: read_text`  
**Root Cause**: Missing builtin function  
**Missing Feature**: `read_text()` to read file to string  
**Fix Priority**: P1 (10-line implementation)

---

### `07_uri_types.ish` ❌
**Error**: `unknown builtin: type_of`  
**Root Cause**: Missing builtin function  
**Missing Feature**: `type_of()` to get type name of value  
**Fix Priority**: P1 (15-line implementation)

---

### `12_multi_agent_orchestration.ish` ❌
**Error**: `unknown character: #`  
**Root Cause**: Using `#` for comments instead of `//`  
**Fix**: Replace all `#` with `//` in file  
**Fix Priority**: P0 (simple find/replace)

---

### `13_multimodal_ai.ish` ❌
**Error**: `unknown character: #`  
**Root Cause**: Using `#` for comments instead of `//`  
**Fix**: Replace all `#` with `//` in file  
**Fix Priority**: P0 (simple find/replace)

---

### `14_typed_pipelines.ish` ❌
**Error**: `unknown character: #`  
**Root Cause**: Using `#` for comments instead of `//`  
**Fix**: Replace all `#` with `//` in file  
**Fix Priority**: P0 (simple find/replace)

---

### `15_ai_protocols.ish` ❌
**Error**: `unknown character: #`  
**Root Cause**: Using `#` for comments instead of `//`  
**Fix**: Replace all `#` with `//` in file  
**Fix Priority**: P0 (simple find/replace)

---

### `16_mcp_servers.ish` ❌
**Error**: `unknown character: #`  
**Root Cause**: Using `#` for comments instead of `//`  
**Fix**: Replace all `#` with `//` in file  
**Fix Priority**: P0 (simple find/replace)

---

### `19_showcase.ish` ❌
**Error**: `where requires array input, got Str("[2, 4, 6, 8, ")`  
**Root Cause**: Likely syntax error in pipeline or print statement  
**Fix Priority**: P0 (review and fix syntax)

---

## ⏭️ Skipped Examples (8)

These examples require TUI mode (`--tui` flag) or are interactive:

- `08_transpiler.bash` - Bash script for transpiler demo
- `09_tui_multimodal.ish` - TUI multimodal interface
- `10_tui_agent_swarm.ish` - TUI agent swarm visualization
- `11_tui_showcase.ish` - TUI feature showcase
- `20_tui_a2a.ish` - TUI A2A protocol demo
- `21_tui_chat.ish` - TUI chat interface
- `22_tui_mcp.ish` - TUI MCP integration
- `23_tui_nanda.ish` - TUI NANDA protocol

**Note**: These can be tested manually with `.\target\release\ish.exe --tui <example>`

---

## Error Categories

### Syntax/Documentation Issues (7 examples)
- 5 examples using `#` comments instead of `//`
- 2 examples with pipeline syntax errors (missing commas)
- **Fix Time**: 30 minutes
- **Fix Type**: Documentation/example updates

### Missing Builtins (4 functions needed)
- `read_text()` - 1 example affected
- `type_of()` - 1 example affected
- `keys()` - 1 example affected (partial)
- `len()` - 1 example affected (partial)
- **Fix Time**: 1-2 hours total
- **Fix Type**: Code implementation in `src/builtins.rs`

### Parser Issues (2 examples)
- `match` not wired at statement level - 1 example directly affected
- Dot notation not implemented - 2 examples affected
- **Fix Time**: 15 minutes (match) + 8 hours (dot notation)
- **Fix Type**: Code changes across parser/evaluator

---

## Recommended Fix Order

### Phase 1: Quick Wins (30 minutes)
1. Fix `#` comments in 5 files (bulk replace)
2. Fix pipeline syntax errors (2 files)
3. Re-run tests → Expect 10 passing

### Phase 2: Essential Builtins (2 hours)
1. Implement `read_text()`, `type_of()`, `keys()`, `len()`
2. Fix `match` statement parsing (5 lines)
3. Re-run tests → Expect 14 passing

### Phase 3: Advanced Features (1-2 days)
1. Implement dot notation (full postfix parsing)
2. Implement table aggregation functions
3. Re-run tests → Expect 16+ passing

---

## Test Reproduction

```powershell
# Test all non-TUI examples
foreach ($file in Get-ChildItem examples\*.ish | Where-Object { 
    $_.Name -notmatch '^(08|09|10|11|20|21|22|23)_' 
}) {
    Write-Host "`n=== $($file.Name) ===" -ForegroundColor Cyan
    .\target\release\ish.exe $file.FullName
    if ($LASTEXITCODE -eq 0) {
        Write-Host "✅ PASSED" -ForegroundColor Green
    } else {
        Write-Host "❌ FAILED (exit code $LASTEXITCODE)" -ForegroundColor Red
    }
}
```

---

## Related Documentation

- Full feature analysis: `docs/MISSING_FEATURES.md`
- Fix checklist: `docs/EXAMPLE_FIXES_CHECKLIST.md`
- Example smoke tests: `tests/examples_smoke.rs`

---

**Generated**: October 20, 2025  
**Compiler**: `rustc 1.XX` (or current version)  
**Build**: Release mode
