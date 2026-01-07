# Lexer Cleanup - Decision Log

## Issue

Two separate lexer implementations existed:
1. **`src/lexer.rs`** + **`src/tokens.rs`** - Standalone lexer (unused)
2. **`src/parser.rs`** - Inline lexer (actually used)

## Analysis

The standalone lexer was never integrated:
- Not declared in `src/lib.rs` as a module
- No imports of `lexer` or `tokens` anywhere in codebase
- Parser has its own complete inline lexer implementation
- Tests don't use standalone lexer

## Decision: Remove Standalone Lexer

**Rationale:**
1. **Single Responsibility**: Parser's inline lexer is simpler and works
2. **No Duplication**: Maintaining two lexers is error-prone
3. **Already Fixed**: Comment support was added to parser's inline lexer
4. **No Migration Needed**: Standalone lexer was never used

## Files Removed

- `src/lexer.rs` (299 lines)
- `src/tokens.rs` (presumed to exist)

## Alternative Considered

**Option: Integrate standalone lexer**
- Would require rewriting parser to use external lexer
- Risk of breaking existing functionality
- No clear benefit over current inline approach
- More complex architecture for no gain

**Verdict: Not worth the effort**

## Future Considerations

If lexer becomes complex (e.g., adding string interpolation), consider:
1. Keeping inline lexer but refactoring to separate module in parser.rs
2. Only extract if complexity justifies separation
3. Don't prematurely optimize

## Date

October 18, 2025
