# AetherShell Variable Syntax - Quick Reference

## Two Ways to Declare Variables

### 🌟 Recommended: Simple Assignment
```ae
x = 42              # Immutable Int
name = "Alice"      # Immutable String  
items = [1, 2, 3]   # Immutable Array<Int>
```
**Use:** Default for all immutable variables

### 🔧 Mutable Variables
```ae
mut count = 0       # Mutable Int
mut total = 100     # Also mutable
let mut score = 50  # Traditional Rust style
```
**Use:** When you need to modify variables

### 📝 Explicit Declaration
```ae
let x = 42              # Explicit immutable
let mut count = 0       # Explicit mutable
let total: Int = 100    # With type annotation
```
**Use:** When you need type annotations or prefer explicit declarations

## Quick Comparison

| Syntax           | Immutable | Mutable | Type Annotation | Recommended       |
| ---------------- | --------- | ------- | --------------- | ----------------- |
| `x = 42`         | ✅         | ❌       | ❌               | ✅ Yes (immutable) |
| `mut x = 42`     | ❌         | ✅       | ❌               | ✅ Yes (mutable)   |
| `let x = 42`     | ✅         | ❌       | ✅               | When need type    |
| `let mut x = 42` | ❌         | ✅       | ✅               | Traditional       |

## Common Patterns

### Variables with Type Inference
```ae
# Types inferred automatically
count = 42                    # Int
price = 99.99                 # Float
name = "Product"              # String
active = true                 # Bool
tags = ["new", "sale"]        # Array<String>
user = {name: "Bob", age: 30} # Record
```

### Mutable Variables
```ae
# Recommended: mut with =
mut counter = 0
counter = counter + 1

# Also works: mut with :=
mut total := 100
total = total - 25

# Traditional: let mut
let mut score = 0
score = score + 10
```

### Old Style: Mutable with let mut
```ae
let mut counter = 0
counter = counter + 1

let mut items = []
items = items + [1, 2, 3]
```

### String Interpolation
```ae
name = "AetherShell"
version = "0.1.0"
print("Welcome to ${name} v${version}!")
```

### Functions
```ae
double = fn(x) => x * 2
add = fn(a, b) => a + b
greet = fn(name) => "Hello, ${name}!"
```

### Complex Types
```ae
# Records
person = {
  name: "Alice",
  age: 30,
  email: "alice@example.com"
}

# Nested arrays
matrix = [
  [1, 2, 3],
  [4, 5, 6],
  [7, 8, 9]
]

# Higher-order functions
operations = [
  fn(x) => x + 1,
  fn(x) => x * 2,
  fn(x) => x * x
]
```

## When to Use Which Syntax

### Use `=` for:
- ✅ Most immutable variables
- ✅ Clean, readable code
- ✅ New projects
- ✅ Simple declarations

### Use `let` for:
- ✅ Mutable variables (`let mut`)
- ✅ Explicit type annotations
- ✅ When you prefer Rust style
- ✅ Complex initialization

### Use `:=` for:
- ✅ Maintaining old code
- ✅ Personal preference
- ✅ Visual distinction from assignment

## Migration Guide

**Old Code (using :=):**
```ae
name := "test"
count := 42
items := [1, 2, 3]
process := fn(x) => x * 2
```

**New Code (using =):**
```ae
name = "test"
count = 42
items = [1, 2, 3]
process = fn(x) => x * 2
```

**Mixed (all work together):**
```ae
name = "test"         # New style
let count = 42        # Explicit
items := [1, 2, 3]    # Old style
# All three work in same file!
```

## Type Inference Examples

```ae
# Primitives
x = 42              # Int
y = 3.14            # Float
s = "hello"         # String
b = true            # Bool

# Collections
arr = [1, 2, 3]     # Array<Int>
rec = {a: 1}        # Record<a: Int>

# Functions
f = fn(x) => x * 2  # Lambda: Int -> Int
g = fn(x, y) => x + y  # Lambda: Int, Int -> Int

# Complex
users = [
  {name: "Alice", age: 30},
  {name: "Bob", age: 25}
]  # Array<Record<name: String, age: Int>>
```

## Remember

✅ **`=` is the new standard** - shortest and cleanest  
✅ **Type inference works automatically** - no annotations needed  
✅ **All three syntaxes are valid** - use what fits your style  
✅ **Mutability requires `let mut`** - no shorthand for mutable  
✅ **Everything is type-safe** - Hindley-Milner inference protects you
