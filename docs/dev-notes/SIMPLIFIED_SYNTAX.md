# Simplified Variable Syntax

**Date:** October 18, 2025  
**Change:** Added simple `=` syntax for variable declarations with type inference

## Summary

AetherShell now supports a simplified variable declaration syntax using just `=` without requiring `let` or `:=`. This makes the code cleaner and more concise while maintaining full type inference.

## Syntax Options

### ✅ Recommended: Simple = (New!)

```ae
name = "world"
count = 42
items = [1, 2, 3]
user = {name: "Alice", age: 30}
```

**Benefits:**
- Shortest syntax
- Clean and readable  
- Still has full type inference
- Similar to Python/JavaScript

### Also Supported: let keyword

```ae
let name = "world"
let count = 42
let mut counter = 0  // Mutable
```

**Use when:**
- You prefer explicit declarations
- Need mutable variables (`let mut`)
- Want to match Rust style

### Also Supported: := shorthand

```ae
name := "world"
count := 42
```

**Use when:**
- Maintaining backwards compatibility
- Prefer compact syntax with visual distinction

## Examples

### Before (using :=)
```ae
name := "AetherShell"
version := "0.1.0"
greeting := "Welcome to ${name} v${version}!"
```

### After (using =)
```ae
name = "AetherShell"
version = "0.1.0"
greeting = "Welcome to ${name} v${version}!"
```

Both work identically, but the `=` syntax is shorter and more familiar.

## Type Inference

All three syntaxes have full Hindley-Milner type inference:

```ae
// All these infer types correctly
x = 42                  // Int
y = 3.14                // Float
s = "hello"             // String
arr = [1, 2, 3]         // Array<Int>
rec = {a: 1, b: "x"}    // Record<a: Int, b: String>
func = fn(x) => x * 2   // Lambda: Int -> Int
```

## Implementation

The parser checks for three patterns in `parse_stmt()`:

1. `identifier = expression` → Simple assignment (new!)
2. `identifier := expression` → Shorthand (backwards compat)
3. `let [mut] identifier = expression` → Explicit declaration

All three desugar to the same `Stmt::Let` AST node.

## Migration

No migration needed! All three syntaxes work simultaneously:

```ae
// Mix and match as needed
name = "test"           // Simple =
let count = 42          // Let keyword
items := [1, 2, 3]      // := shorthand

// All work in the same file
print("${name}: ${count} items")
```

## Files Changed

- **src/parser.rs**: Added `identifier = expression` pattern recognition
- **README.md**: Updated examples to use simple `=` syntax  
- **docs/specs/SPEC.md**: Updated variable binding section
- **examples/*.ae**: Updated all 17 examples to use `=` syntax

## Testing

- ✅ All 21 library tests pass
- ✅ Examples 00-06 work correctly
- ✅ All three syntaxes tested together
- ✅ String interpolation works with all syntaxes
- ✅ Type inference unchanged

## Recommendation

**Use `=` for new code.** It's the cleanest, shortest, and most familiar syntax. The other syntaxes remain supported for backwards compatibility and special cases (like `let mut`).
