# Error Handling

AetherShell provides structured error handling with `try`/`catch` expressions and first-class `Error` values. Errors can be created with `throw`, caught with `try`/`catch`, and inspected like any other value.

## Error Values

`Error` is a first-class value type. It's **falsy** and carries a string message:

```ae
let err = throw "something went wrong"
# err is Value::Error("something went wrong")
```

## Try / Catch

Catch errors and recover gracefully:

```ae
let result = try {
    cat("config.toml")
} catch {
    "fallback value"
}
```

### Binding the Error Message

Use `catch variable` to capture the error message:

```ae
let result = try {
    http_get "https://unreachable.example.com"
} catch e {
    print "Request failed: ${e}"
    null
}
```

The `catch` variable receives the error message as a `String`.

## Throw

Create an error value explicitly:

```ae
throw "file not found"
throw "invalid argument: ${arg}"
```

`throw` evaluates its expression, converts it to a string, and returns a `Value::Error`. If the thrown value is already a String, it's used directly; otherwise it's formatted.

## What Gets Caught

`try`/`catch` handles two categories of errors:

1. **Error values** — produced by `throw`:
   ```ae
   try { throw "oops" } catch e { "caught: ${e}" }
   # => "caught: oops"
   ```

2. **Runtime errors** — produced by invalid operations:
   ```ae
   try { 1 / 0 } catch e { "caught: ${e}" }
   try { null.field } catch e { "caught: ${e}" }
   ```

If the `try` block succeeds (returns a non-error value), the `catch` branch is not executed:

```ae
try { 42 } catch { "never reached" }
# => 42
```

## Common Runtime Errors

| Error | Cause |
|-------|-------|
| `"unknown builtin: name"` | Calling a function that doesn't exist |
| `"cannot call null"` | Trying to call a null value as a function |
| `"field 'x' not found in record"` | Accessing a missing record field |
| `"Cannot reassign immutable variable 'x'"` | Reassigning a `let` binding |
| `"match: no arm matched the value"` | Non-exhaustive match statement |
| `"lambda arity mismatch"` | Wrong number of arguments |
| `"expected Bool"` | Using non-boolean in `if` condition |

## Error Propagation

Without `try`/`catch`, errors propagate up and terminate the current execution:

```ae
let validate = fn(x) =>
    if x < 0 { throw "must be non-negative" }
    else { x }

# This will produce an error since we don't catch it
validate(-5)
```

Wrap the call in `try`/`catch` to handle it:

```ae
let result = try { validate(-5) } catch e {
    print "Validation failed: ${e}"
    0
}
# result is 0
```

## Practical Patterns

### Default on failure

```ae
let config = try { from_json(cat "config.json") } catch {
    {host: "localhost", port: 8080}
}
```

### Retry logic

```ae
let fetch_with_retry = fn(url) => {
    let result = try { http_get url } catch { null }
    if result { result }
    else {
        sleep 1000
        try { http_get url } catch e {
            throw "Failed after retry: ${e}"
        }
    }
}
```

### Validate and collect errors

```ae
let validate_user = fn(user) => {
    if !user.name { throw "name is required" }
    if user.age < 0 { throw "age must be non-negative" }
    if !user.email { throw "email is required" }
    user
}

let result = try { validate_user({name: "", age: -1}) } catch e {
    print "Invalid user: ${e}"
    null
}
```

### Pipeline error handling

```ae
# Errors in pipelines can be caught at any stage
let safe_pipeline = fn(data) =>
    try {
        data
        | from_json
        | where fn(r) => r.value > 0
        | map fn(r) => r.value * 2
    } catch e {
        print "Pipeline failed: ${e}"
        []
    }
```

## Error vs. Null

- `null` represents absence of a value (intentional)
- `Error(msg)` represents a failure with a message (exceptional)
- Both are falsy, but `Error` carries diagnostic information

```ae
let result = find_user("nonexistent")

match result {
    null => "not found",
    Error(msg) => "error: ${msg}",
    user => "found: ${user.name}"
}
```
