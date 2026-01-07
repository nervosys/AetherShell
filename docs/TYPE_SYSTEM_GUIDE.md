# AetherShell Type System and Operators Guide

## Assignment Operators: `:=` vs `=`

AetherShell has two assignment operators with distinct purposes:

### `:=` - Type Inference Assignment

Use `:=` when you want the compiler to **infer the type** automatically:

```ae
# Compiler infers type from the value
x := 42                    # Inferred as Int
name := "AetherShell"      # Inferred as String
items := [1, 2, 3]         # Inferred as Array<Int>
result := ai("prompt")     # Inferred from ai() return type

# Complex type inference
data := http("api.com") | json()  # Inferred as Record
agent := agent("task", ["tools"]) # Inferred as Agent type
```

**When to use `:=`**
- ✅ Most common case - let the compiler figure out the type
- ✅ When the type is obvious from the right-hand side
- ✅ For local variables in functions
- ✅ When working with complex pipeline results
- ✅ For intermediate computations

### `=` - Explicit Type Assignment

Use `=` when you have **explicitly defined the type**:

```ae
# Explicit type annotations (future feature)
let x: Int = 42
let name: String = "AetherShell"
let callback: (Int) -> String = fn(n) => "${n}"

# In function signatures
fn process(data: Array<Int>) -> Int {
  # Type is already defined in signature
  total = sum(data)  # Use = here
  total
}

# Type aliases (future feature)
type UserId = Int
let id: UserId = 123  # Use = with explicit type
```

**When to use `=`**
- ✅ With explicit type annotations (`:` type syntax)
- ✅ In function parameter assignments
- ✅ When you want to enforce a specific type
- ✅ For type-constrained bindings

## Current Implementation Status

### ✅ Currently Supported

**Type Inference (`:=`)**
```ae
# These all work with type inference
x := 42
name := "hello"
items := [1, 2, 3]
result := ai("prompt")
data := http("url") | json()
```

### ⏳ Planned Features

**Explicit Type Annotations**
```ae
# Future syntax (not yet implemented)
let x: Int = 42
let name: String = "AetherShell"
let process: (String) -> Int = fn(s) => len(s)
```

**For now, use `:=` for all assignments.** The `=` operator is reserved for future explicit type annotations.

## Hindley-Milner Type Inference

AetherShell uses **Hindley-Milner type inference**, which means:

### Automatic Type Deduction

```ae
# Compiler knows these types without annotations
x := 42                           # Int
y := 3.14                         # Float
name := "Alice"                   # String
items := [1, 2, 3]                # Array<Int>
mixed := [1, "two", 3.0]         # Array<Any> or error (depends on mode)
person := {name: "Bob", age: 30}  # Record {name: String, age: Int}
```

### Function Type Inference

```ae
# Function type is inferred from usage
double := fn(x) => x * 2          # (Int) -> Int or (Float) -> Float
greet := fn(name) => "Hello ${name}"  # (String) -> String

# Generic functions
identity := fn(x) => x            # <T>(T) -> T
map_add := fn(xs, n) => xs | map(fn(x) => x + n)  # (Array<Int>, Int) -> Array<Int>
```

### Pipeline Type Inference

```ae
# Type flows through the pipeline
result := [1, 2, 3, 4, 5]
  | map(fn(x) => x * 2)           # Array<Int> -> Array<Int>
  | filter(fn(x) => x > 5)        # Array<Int> -> Array<Int>
  | reduce(fn(a, b) => a + b, 0)  # Array<Int> -> Int

# Compiler knows result: Int
```

### Type Unification

```ae
# Types must be compatible
x := 42
y := x + 10        # OK: Int + Int = Int
z := x + "hello"   # ERROR: Int + String (type mismatch)

# Polymorphic resolution
map_fn := fn(f, xs) => xs | map(f)
result1 := map_fn(fn(x) => x * 2, [1, 2, 3])      # Array<Int>
result2 := map_fn(fn(s) => len(s), ["a", "bb"])   # Array<Int>
```

## Type System Features

### Structural Types

AetherShell uses **structural typing** for records:

```ae
# Two records with same structure are compatible
person1 := {name: "Alice", age: 30}
person2 := {name: "Bob", age: 25}

# They have the same type: {name: String, age: Int}
people := [person1, person2]  # Array<{name: String, age: Int}>
```

### Option Types

```ae
# Option<T> represents a value that might not exist
maybe_value := Some(42)     # Option<Int>
no_value := None            # Option<T>

result := match maybe_value {
  Some(x) => x * 2,
  None => 0
}
```

### Result Types

```ae
# Result<T, E> represents success or error
success := Ok(42)           # Result<Int, String>
failure := Err("failed")    # Result<Int, String>

result := match computation {
  Ok(value) => process(value),
  Err(error) => handle_error(error)
}
```

### Lambda Types

```ae
# Functions are first-class values
add := fn(a, b) => a + b              # (Int, Int) -> Int
curry_add := fn(a) => fn(b) => a + b # (Int) -> (Int) -> Int

# Higher-order functions
apply := fn(f, x) => f(x)             # <T, R>((T) -> R, T) -> R
compose := fn(f, g) => fn(x) => f(g(x))  # <A, B, C>((B) -> C, (A) -> B) -> (A) -> C
```

## Best Practices

### 1. Use Type Inference by Default

```ae
# ✅ Good: Let compiler infer
x := 42
name := "Alice"
result := process(data)

# ❌ Don't over-specify (when = with annotations is available)
# let x: Int = 42  # Unnecessary if type is obvious
```

### 2. Let Pipelines Flow Naturally

```ae
# ✅ Good: Type flows through pipeline
result := data
  | filter(fn(x) => x > 0)
  | map(fn(x) => x * 2)
  | sum()

# ❌ Don't break up pipelines unnecessarily
# step1 := filter(data, fn(x) => x > 0)
# step2 := map(step1, fn(x) => x * 2)
# result := sum(step2)
```

### 3. Use Pattern Matching for Type Safety

```ae
# ✅ Good: Handle all cases
result := match value {
  Some(x) => x * 2,
  None => 0
}

# ❌ Don't assume values exist
# result := value * 2  # Error if value is None
```

### 4. Name Types for Clarity

```ae
# ✅ Good: Clear names help inference
user_data := fetch_user(123)
processed_user := transform(user_data)

# ❌ Don't use generic names
# x := fetch_user(123)
# y := transform(x)
```

## Type Errors

### Common Type Errors

```ae
# Type mismatch
x := 42
y := x + "hello"  # ERROR: Cannot add Int and String

# Array type mismatch
nums := [1, 2, 3]
nums_with_string := nums + ["four"]  # ERROR: Array<Int> vs Array<String>

# Function type mismatch
double := fn(x) => x * 2
result := double("not a number")  # ERROR: Expected Int or Float, got String
```

### Type Error Messages

AetherShell provides helpful error messages:

```
ERROR: Type mismatch
  Expected: Int
  Got: String
  Location: line 5, column 12
  
  Hint: You're trying to add an Int and a String
  Consider: Converting the string to a number first
```

## Advanced Type Features (Future)

### Type Aliases

```ae
# Future feature
type UserId = Int
type UserName = String
type Age = Int

type User = {
  id: UserId,
  name: UserName,
  age: Age
}

let user: User = {id: 1, name: "Alice", age: 30}
```

### Generic Type Parameters

```ae
# Future feature
fn first<T>(items: Array<T>) -> Option<T> {
  if len(items) > 0 {
    Some(items[0])
  } else {
    None
  }
}
```

### Trait Constraints

```ae
# Future feature
fn sum_all<T: Numeric>(items: Array<T>) -> T {
  items | reduce(fn(a, b) => a + b, 0)
}
```

## Summary

| Feature            | Operator | Usage                   | Example                            |
| ------------------ | -------- | ----------------------- | ---------------------------------- |
| **Type Inference** | `:=`     | Let compiler infer type | `x := 42`                          |
| **Explicit Type**  | `=`      | With type annotation    | `let x: Int = 42` (future)         |
| **Pipelines**      | `\|`     | Type flows through      | `data \| map(...) \| filter(...)`  |
| **Lambda**         | `=>`     | Function definition     | `fn(x) => x * 2`                   |
| **Pattern Match**  | `match`  | Type-safe destructuring | `match x { Some(v) => v, _ => 0 }` |

**Key Takeaway:** 
- **Use `:=` for type inference (most common)**
- **Reserve `=` for explicit type annotations (future feature)**
- **Let the Hindley-Milner system do the work!**

The type system is designed to be **powerful yet invisible** - it catches errors at compile time but doesn't get in your way during development. 🚀
