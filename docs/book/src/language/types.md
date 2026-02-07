# Types and Values

AetherShell is a typed shell where every expression produces a structured `Value`. Unlike traditional shells that pipe raw text, AetherShell pipelines carry rich, typed data.

## Core Types

| Type | Example | Description |
|------|---------|-------------|
| `Null` | `null` | Absence of value |
| `Bool` | `true`, `false` | Boolean |
| `Int` | `42`, `-7` | 64-bit signed integer |
| `Float` | `3.14`, `-0.5` | 64-bit floating point |
| `String` | `"hello"` | Text with interpolation support |
| `Uri` | `openai:gpt-4o-mini` | URI with scheme (RFC 3986) |
| `Array` | `[1, "two", true]` | Heterogeneous ordered list |
| `Record` | `{name: "Ada", age: 36}` | Ordered key-value map |
| `Table` | output of `ls` | Structured table with schema |
| `Lambda` | `fn(x) => x * 2` | First-class function |
| `Error` | `throw "oops"` | Error value |

### Integers and Floats

```ae
let x = 42       # Int
let y = 3.14     # Float
let z = x + y    # Float (auto-promoted)
```

Integer division always produces a Float:

```ae
10 / 3   # => 3.3333...
```

### Strings

Strings support `${expr}` interpolation:

```ae
let name = "world"
let greeting = "Hello, ${name}!"    # => "Hello, world!"
let math = "2 + 2 = ${2 + 2}"      # => "2 + 2 = 4"
```

String concatenation works with `+` and auto-converts the other operand:

```ae
"count: " + 42        # => "count: 42"
100 + " items"         # => "100 items"
```

### URIs

URIs identify resources with a scheme prefix, commonly used for AI model references:

```ae
let model = openai:gpt-4o-mini
let local = ollama:llama3
```

### Arrays

Arrays hold any mix of types:

```ae
let nums = [1, 2, 3]
let mixed = [1, "two", true, [4, 5]]
```

### Records

Records are key-value maps with sorted keys:

```ae
let person = {name: "Ada", age: 36, langs: ["Rust", "Python"]}
person.name    # => "Ada"
person.langs   # => ["Rust", "Python"]
```

### Tables

Tables are structured arrays of records with a defined schema. Many builtins return tables:

```ae
ls "."    # => Table with columns: name, path, ext, is_dir, size, modified
```

Tables get special pretty-printed column-aligned output in the terminal.

## Type Conversion

AetherShell performs automatic numeric promotion in arithmetic:

| Expression | Result Type | Rule |
|-----------|-------------|------|
| `Int + Int` | `Int` | Integer arithmetic |
| `Int + Float` | `Float` | Promote to float |
| `Float + Float` | `Float` | Float arithmetic |
| `Int ^ Int` (positive) | `Int` | Integer power |
| `Int ^ Int` (negative) | `Float` | Float power |
| `Int / Int` | `Float` | Always float division |

Equality comparison between `Int` and `Float` works via casting.

## Truthiness

All values have a boolean interpretation:

| Falsy | Truthy |
|-------|--------|
| `null` | Non-null values |
| `false` | `true` |
| `0`, `0.0` | Non-zero numbers |
| `""` (empty string) | Non-empty strings |
| `[]` (empty array) | Non-empty arrays |
| `{}` (empty record) | Non-empty records |
| `Error(...)` | Lambdas, Builtins |

## JSON Interop

Values convert bidirectionally with JSON:

```ae
let data = from_json '{"name": "test", "count": 42}'
data.name    # => "test"

let json_str = to_json {name: "test", count: 42}
# => '{"count":42,"name":"test"}'
```

## Type Inspection

```ae
describe 42          # => "Int"
describe "hello"     # => "String"
describe [1, 2, 3]   # => "Array(3 elements)"
```
