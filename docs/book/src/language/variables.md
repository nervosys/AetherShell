# Variables and Bindings

IronShell supports several forms for declaring and binding variables, from explicit `let` declarations to concise shorthand syntax.

## Let Bindings

The standard way to declare a variable:

```ish
let name = "IronShell"
let version = 3
let features = ["typed", "pipelines", "AI"]
```

Variables are **immutable by default**. Attempting to reassign an immutable variable produces an error:

```ish
let x = 10
x = 20    # Error: Cannot reassign immutable variable 'x'. Use 'let mut x' to make it mutable.
```

## Mutable Variables

Use `mut` to allow reassignment:

```ish
let mut counter = 0
counter = counter + 1    # OK
counter = 42             # OK
```

Or with shorthand:

```ish
mut counter = 0
counter = counter + 1
```

## Shorthand Syntax

IronShell offers shorter forms for common patterns:

```ish
# These are all equivalent:
let x = 10
x = 10       # Inferred let
x := 10      # Walrus-style binding
```

## Public Variables

Variables can be marked public for export from modules:

```ish
pub let API_URL = "https://api.example.com"
pub let VERSION = "1.0.0"
```

Public variables are accessible when the module is imported.

## Type Annotations

Optional type annotations can be added to bindings:

```ish
let name: String = "hello"
let count: Int = 42
```

Type annotations are parsed but currently used for documentation purposes. The type inference engine (`typecheck.rs`) handles type validation.

## Scoping Rules

IronShell uses a flat environment model:

- All variables share a single scope
- Lambda parameters are temporarily bound during execution and restored after
- There are no block-level scopes or shadowing in the traditional sense
- Variables are visible once declared and remain available for the rest of the session

```ish
let x = 10
let f = fn(x) => x * 2    # Lambda parameter 'x' temporarily shadows outer 'x'
f(5)                        # => 10
x                           # => 10 (outer x unchanged)
```

## Environment Variables

Shell environment variables are accessible and can be set:

```ish
# Access environment variable
let home = $HOME

# Set environment variable
export PATH = "/usr/local/bin:${$PATH}"
```

## Assignment vs. Declaration

| Syntax | Meaning |
|--------|---------|
| `let x = expr` | Immutable declaration |
| `let mut x = expr` | Mutable declaration |
| `x = expr` | Shorthand immutable declaration (or reassign if mutable) |
| `x := expr` | Shorthand immutable declaration |
| `mut x = expr` | Shorthand mutable declaration |
| `pub let x = expr` | Public immutable declaration |
