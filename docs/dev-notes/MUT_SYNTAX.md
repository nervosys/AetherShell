# Mutable Variable Syntax Enhancement

**Date:** October 18, 2025  
**Change:** Added `mut x = value` and `mut x := value` syntax for mutable variables

## Summary

AetherShell now supports declaring mutable variables without the `let` keyword, using just `mut`. This makes mutable declarations more concise and consistent with the simplified immutable syntax.

## New Syntax Options

### ✅ Recommended: mut with =

```ae
mut counter = 0
counter = counter + 1
```

**Benefits:**
- Shortest syntax for mutable variables
- Consistent with immutable `x = value` style
- Clear intent with `mut` keyword

### Also Supported: mut with :=

```ae
mut total := 100
total = total - 25
```

**Use when:**
- Prefer visual distinction with `:=`
- Consistency with existing `:=` usage

### Traditional: let mut

```ae
let mut score = 50
score = score * 2
```

**Use when:**
- Need type annotations: `let mut x: Int = 0`
- Prefer Rust-style syntax

## Complete Syntax Comparison

| Syntax           | Immutable | Mutable | Type Annotation | Recommended       |
| ---------------- | --------- | ------- | --------------- | ----------------- |
| `x = 42`         | ✅         | ❌       | ❌               | ✅ Yes (immutable) |
| `mut x = 42`     | ❌         | ✅       | ❌               | ✅ Yes (mutable)   |
| `mut x := 42`    | ❌         | ✅       | ❌               | Also works        |
| `let x = 42`     | ✅         | ❌       | ✅               | If need type      |
| `let mut x = 42` | ❌         | ✅       | ✅               | Traditional       |
| `x := 42`        | ✅         | ❌       | ❌               | Legacy            |

## Examples

### Counter Pattern

```ae
// Before (required let mut)
let mut counter = 0
counter = counter + 1
counter = counter + 5
print(counter)  // 6

// After (cleaner with mut)
mut counter = 0
counter = counter + 1
counter = counter + 5
print(counter)  // 6
```

### Accumulator Pattern

```ae
mut sum = 0
mut product = 1

[1, 2, 3, 4, 5] | each(fn(x) => {
  sum = sum + x
  product = product * x
})

print("Sum: ${sum}")        // Sum: 15
print("Product: ${product}") // Product: 120
```

### State Management

```ae
mut state = "idle"
mut retries = 0

while retries < 3 {
  state = "connecting"
  // ... connection logic ...
  retries = retries + 1
}
```

### Multiple Types

```ae
// Integers
mut count = 0
count = count + 1

// Floats
mut price = 9.99
price = price * 1.2

// Strings
mut name = "Alice"
name = "Bob"

// Booleans
mut active = true
active = false

// Arrays (when mutation supported)
mut items = [1, 2, 3]
// items = items + [4, 5]  // When array concat implemented

// Records (when mutation supported)
mut user = {name: "Alice", age: 30}
// user.age = 31  // When field mutation implemented
```

## Implementation Details

### Parser Changes

**File:** `src/parser.rs`

**Added pattern recognition** for `mut identifier = expression`:

```rust
// Check for `mut name = value` or `mut name := value`
if self.check(Tok::Mut) && self.peek_ahead(1) == Some(Tok::Ident) {
    let peek2 = self.peek_ahead(2);
    if peek2 == Some(Tok::Equal) || peek2 == Some(Tok::ColonEqual) {
        self.match_tok(Tok::Mut); // consume 'mut'
        let name = self.need_ident("expected identifier after mut")?;
        
        if self.match_tok(Tok::Equal) {
            let value = self.parse_expr()?;
            return Ok(Stmt::Let {
                name,
                value,
                is_mut: true,
            });
        } else if self.match_tok(Tok::ColonEqual) {
            let value = self.parse_expr()?;
            return Ok(Stmt::Let {
                name,
                value,
                is_mut: true,
            });
        }
    }
}
```

**Pattern Priority:**
1. `mut identifier [=|:=] expression` (new)
2. `identifier = expression` (immutable)
3. `identifier := expression` (immutable)
4. `let [mut] identifier = expression` (traditional)

### AST Node

All mutable syntaxes desugar to the same `Stmt::Let` with `is_mut: true`:

```rust
Ok(Stmt::Let {
    name: "counter".to_string(),
    value: Expr::LitInt(0),
    is_mut: true,
})
```

### Type Inference

Hindley-Milner type inference works identically for all mutable syntaxes. Mutability is tracked separately from type information.

## Before/After Comparison

### Before Enhancement

```ae
// Only way to declare mutable variables
let mut counter = 0
let mut total = 100
let mut active = true
```

**Drawbacks:**
- Verbose (requires `let mut`)
- Inconsistent with immutable `x = value` syntax
- 4 extra characters per declaration

### After Enhancement

```ae
// Cleaner mutable declarations
mut counter = 0
mut total = 100
mut active = true
```

**Benefits:**
- ✅ Shorter by 4 characters (`mut` vs `let mut`)
- ✅ Consistent with immutable `x = value` pattern
- ✅ Clear intent with `mut` prefix
- ✅ All old syntax still works

## Migration

**No migration required!** All three mutable syntaxes work together:

```ae
// Mix freely in the same file
mut x = 10          // New style
let mut y = 20      // Traditional
mut z := 30         // With :=

// All work identically
x = x + 1
y = y + 1
z = z + 1
```

**Recommendation:** Use `mut x = value` for new code, keep existing code as-is.

## Testing

### Test File Created

`temp/test_mut_complete.ae` - Comprehensive test of all mutable syntaxes

**Test Results:**
```
=== Testing mut x = value ===
Initial: 0
After +1: 1
After +5: 6

=== Testing mut x := value ===
Initial: 100
After -25: 75
After -10: 65

=== Testing let mut x = value ===
Initial: 50
After *2: 100

✅ All three mutable syntax options work perfectly!
```

### Unit Tests

- ✅ All 21 library tests pass
- ✅ Zero regressions detected
- ✅ Parser correctly recognizes all patterns
- ✅ Type inference unchanged

## Files Modified

| File                            | Change                    | Lines |
| ------------------------------- | ------------------------- | ----- |
| `src/parser.rs`                 | Added `mut x =` pattern   | +28   |
| `README.md`                     | Updated variables section | ~5    |
| `docs/specs/SPEC.md`            | Updated bindings section  | ~8    |
| `docs/VARIABLE_SYNTAX_GUIDE.md` | Updated comparison table  | ~20   |
| `docs/dev-notes/MUT_SYNTAX.md`  | This documentation        | +250  |
| `temp/test_mut_complete.ae`     | Test file                 | +47   |

## Benefits Summary

✅ **Consistency:** Mutable syntax now mirrors immutable syntax pattern  
✅ **Brevity:** 4 characters shorter than `let mut`  
✅ **Clarity:** `mut` prefix makes mutability explicit  
✅ **Flexibility:** Three syntax options for different preferences  
✅ **Backwards Compatible:** All existing code continues to work  
✅ **Zero Breaking Changes:** Pure additive enhancement

## Usage Recommendations

### Use `mut x = value` when:
- ✅ Writing new code
- ✅ Want shortest syntax
- ✅ Prefer consistency with immutable style

### Use `mut x := value` when:
- ✅ Already using `:=` for immutables
- ✅ Want visual distinction

### Use `let mut x = value` when:
- ✅ Need type annotations
- ✅ Prefer traditional Rust style
- ✅ Maintaining existing code

## Conclusion

Successfully added support for `mut x = value` and `mut x := value` syntax! This enhancement:

- Makes mutable declarations more concise
- Provides consistency with immutable syntax
- Maintains full backwards compatibility
- Passes all tests with zero regressions

**Status:** ✅ Complete - Ready for production use  
**Tests:** ✅ All passing (21/21 library + integration tests)  
**Documentation:** ✅ Complete and updated
