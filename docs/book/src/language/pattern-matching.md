# Pattern Matching

AetherShell provides `match` expressions for destructuring values and branching on their structure. Pattern matching works with all value types including arrays, records, and constructor-style tagged values.

## Basic Match

```ae
let x = 42

match x {
    0 => "zero",
    1 => "one",
    _ => "something else"
}
# => "something else"
```

The `_` wildcard matches anything without binding a name.

## Binding Patterns

Identifier patterns match any value and bind it to a name:

```ae
let result = match get_status() {
    "ok" => "all good",
    "error" => "something broke",
    other => "unexpected: ${other}"
}
```

## Literal Patterns

Match against exact values:

```ae
match value {
    42 => "the answer",
    "hello" => "greeting",
    true => "affirmative",
    null => "nothing",
    _ => "default"
}
```

Supported literal patterns:

| Pattern          | Matches       |
| ---------------- | ------------- |
| `42`             | Exact integer |
| `"hello"`        | Exact string  |
| `true` / `false` | Exact boolean |
| `null`           | Null value    |

## Array Patterns

Destructure arrays by matching their elements:

```ae
match [1, 2, 3] {
    [] => "empty",
    [x] => "one element: ${x}",
    [x, y] => "two: ${x}, ${y}",
    [x, y, z] => "three: ${x}, ${y}, ${z}",
    _ => "more than three"
}
# => "three: 1, 2, 3"
```

Array patterns require an **exact length match**. `[x, y]` won't match a 3-element array.

Patterns can be nested:

```ae
match [[1, 2], [3, 4]] {
    [[a, b], [c, d]] => a + b + c + d,
    _ => 0
}
# => 10
```

## Record Patterns

Match records by their fields:

```ae
let person = {name: "Ada", age: 36, role: "engineer"}

match person {
    {name: "Ada", role} => "Found Ada, role: ${role}",
    {name, age} => "${name} is ${age} years old",
    _ => "unknown"
}
# => "Found Ada, role: engineer"
```

**Shorthand**: `{name}` is equivalent to `{name: name}` — it matches the field `name` and binds its value to variable `name`.

Record patterns match if **all specified fields exist**. Extra fields are ignored:

```ae
# This matches even though the record has 'role' too
match {name: "Ada", age: 36, role: "engineer"} {
    {name, age} => "${name}, ${age}",
    _ => "no match"
}
# => "Ada, 36"
```

## Constructor Patterns

AetherShell uses tagged records to represent algebraic data types. The `Some` and `None` constructors create tagged records:

```ae
let result = Some(42)    # => {_tag: "Some", _value: 42}
let empty = None         # => {_tag: "None"}
```

Match on constructors:

```ae
match Some(42) {
    Some(x) => "got value: ${x}",
    None => "nothing",
    _ => "unexpected"
}
# => "got value: 42"
```

Zero-argument constructors just check the `_tag`:

```ae
match None {
    Some(x) => "got ${x}",
    None => "nothing here",
}
# => "nothing here"
```

## Guards

Add conditions to match arms with `if`:

```ae
match value {
    x if x > 100 => "large",
    x if x > 10 => "medium",
    x if x > 0 => "small",
    0 => "zero",
    x => "negative: ${x}"
}
```

Guards are evaluated with the pattern's bindings in scope. If the guard is falsy, the next arm is tried:

```ae
match {name: "Ada", age: 36} {
    {name, age} if age >= 21 => "${name} is an adult",
    {name, age} => "${name} is ${age} years old",
    _ => "unknown"
}
# => "Ada is an adult"
```

## Exhaustiveness

If no arm matches the value, a runtime error is produced:

```
Error: match: no arm matched the value
```

Always include a `_` wildcard as the last arm to handle unexpected cases:

```ae
match status {
    "active" => handle_active(),
    "paused" => handle_paused(),
    _ => throw "Unknown status: ${status}"
}
```

## Match as Expression

`match` is an expression — it returns a value:

```ae
let label = match count {
    0 => "none",
    1 => "one",
    _ => "many"
}

print label
```

## Practical Examples

### Implicit scrutinee in lambdas

When `match` is used inside a lambda body, the scrutinee can be omitted — it defaults to the lambda's first parameter:

```ae
# Explicit scrutinee (always works)
let grade = fn(score) => match score {
    90..100 => "A",
    80..89  => "B",
    _       => "C"
}

# Implicit scrutinee (same result, cleaner)
let grade = fn(score) => match {
    90..100 => "A",
    80..89  => "B",
    _       => "C"
}

grade(85)  # "B"
```

This works with any single-parameter lambda, including in pipelines:

```ae
[1, 2, 3, 100] | map fn(x) => match {
    _ if x > 50 => "big",
    _            => "small"
}
# => ["small", "small", "small", "big"]
```

> **Note**: `match expr { ... }` with an explicit scrutinee is always supported. The implicit form only applies inside lambdas and uses the first parameter.

### Processing command output

```ae
let files = ls "."

files | map fn(f) => match {
    {is_dir: true, name} => "📁 ${name}/",
    {ext: "rs", name} => "🦀 ${name}",
    {ext: "md", name} => "📝 ${name}",
    {name} => "   ${name}"
}
```

### Option handling

```ae
let find_user = fn(id) =>
    if id == 1 { Some({name: "Ada", role: "admin"}) }
    else { None }

match find_user(1) {
    Some({name, role}) => "Found ${name} (${role})",
    None => "User not found"
}
# => "Found Ada (admin)"
```

### HTTP response handling

```ae
let response = http_get "https://api.example.com/status"

match response {
    {status: 200, body} => from_json(body),
    {status: 404} => throw "Not found",
    {status} => throw "HTTP error: ${status}"
}
```
