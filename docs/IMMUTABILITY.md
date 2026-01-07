# Immutability in AetherShell

AetherShell enforces **immutability by default** for all variables, following functional programming best practices. This design choice improves code safety, predictability, and maintainability.

## Overview

- **Immutable by default**: Simple assignment `x = value` creates an immutable binding
- **Explicit mutability**: Use `mut x = value` or `let mut x = value` to create mutable variables
- **Shadowing allowed**: You can create new bindings with the same name, but cannot reassign immutable variables

## Syntax

### Immutable Variables (Default)

```aethershell
# Simple assignment - immutable
x = 42
name = "Alice"

# Explicit immutable with 'let'
let y = 100
let message = "Hello"

# Attempting to reassign will fail
x = 43  # This creates a NEW binding (shadowing), not reassignment
```

### Mutable Variables

```aethershell
# Using 'mut' keyword
mut counter = 0
counter = counter + 1  # OK - counter is mutable

# Using 'let mut' syntax
let mut total = 100
total = total - 20  # OK - total is mutable
```

## Shadowing vs Reassignment

AetherShell distinguishes between **shadowing** (creating a new binding) and **reassignment** (modifying an existing variable).

### Shadowing (Always Allowed)

```aethershell
x = 42      # First binding
x = 100     # Creates NEW binding, shadows the first

# This is allowed because each assignment creates a new variable
```

### Reassignment (Only for Mutable Variables)

```aethershell
# Immutable - cannot reassign
x = 42
# x = 100 would create new binding (shadow), not reassign

# Mutable - can reassign
mut count = 0
count = 1  # Actually shadows with new declaration
```

## Why Immutability by Default?

### 1. **Safety**
Immutable data structures prevent accidental modifications and side effects, making code more predictable.

### 2. **Concurrency**
Immutable data is inherently thread-safe, enabling safe concurrent operations without locks.

### 3. **Reasoning**
Code with immutable variables is easier to reason about - values don't change unexpectedly.

### 4. **Functional Programming**
Aligns with functional programming paradigms that emphasize immutable data and pure functions.

## Common Patterns

### Counter Pattern
```aethershell
mut clicks = 0
print("Clicks: ${clicks}")
clicks = clicks + 1
print("After click: ${clicks}")
```

### Accumulator Pattern
```aethershell
mut sum = 0
numbers = [10, 20, 30, 40, 50]

# Accumulate values
sum = sum + 10
sum = sum + 20
# ...
```

### State Machine
```aethershell
mut state = "idle"
mut attempts = 0

state = "connecting"
attempts = attempts + 1

state = "connected"
# ...
```

### Progress Tracking
```aethershell
mut progress = 0.0
mut status = "Starting"

progress = progress + 25.0
status = "Loading"
# ...
```

## Error Messages

When attempting to reassign an immutable variable, AetherShell provides clear guidance:

```
Cannot reassign immutable variable 'x'. Use 'let mut x' to make it mutable.
```

## Implementation Details

- **Environment Tracking**: The `Env` struct tracks which variables are mutable via a `BTreeMap<String, bool>`
- **Parser Integration**: The parser marks variables as mutable based on `mut` keyword presence
- **Evaluator Enforcement**: The evaluator checks mutability before allowing reassignment
- **Internal Bindings**: Lambda parameters and pattern matching bindings use `set_var_unchecked()` to bypass checks

## Testing

The immutability system is validated by 6 comprehensive tests:

1. **Immutable by default**: Verifies simple assignment creates immutable variables
2. **Let mut creates mutable**: Verifies `let mut` syntax
3. **Explicit let is immutable**: Verifies `let x = value` is immutable
4. **Shadowing allowed**: Verifies shadowing works correctly
5. **Mutable vars can be updated**: Verifies mutable variables can be shadowed
6. **Simple assignment immutable**: Verifies all simple assignments are immutable

## Migration from Pre-Immutability Code

If you have existing AetherShell code that assumes mutability:

**Before:**
```aethershell
x = 42
x = 100  # Worked as reassignment
```

**After:**
```aethershell
# Option 1: Use mutable variable
mut x = 42
x = 100  # Shadowing with new declaration

# Option 2: Use shadowing (creates new binding)
x = 42
x = 100  # Creates new binding, shadows old one
```

## See Also

- [Mutable Variables Example](../examples/18_mutable_variables.ae) - Comprehensive examples
- [Quick Reference](../docs/QUICK_REFERENCE.md) - Syntax reference
- [Type System](../docs/TYPE_SYSTEM.md) - Type system documentation
