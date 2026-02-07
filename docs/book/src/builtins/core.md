# Core Operations

Core builtins provide essential shell operations: output, environment management, JSON handling, option types, diagnostics, and shell interop.

## Output

### `print`
Print a value to stdout without a trailing newline.

```aethershell
print "hello"       # hello
print 42            # 42
print [1, 2, 3]     # [1, 2, 3]
```

### `echo`
Print a value followed by a newline. Equivalent to `println` in many languages.

```aethershell
echo "Hello, world!"
echo { name: "Ada", age: 36 }
```

### `debug` / `dbg`
Print a value with type and structure information, useful for development.

```aethershell
debug [1, "two", 3.0]
# Array(3): [Int(1), String("two"), Float(3.0)]

let rec = { x: 1, y: [2, 3] }
dbg rec
```

## Help & Inspection

### `help`
Display available commands and usage information.

```aethershell
help              # List all builtins
help "map"        # Help for a specific builtin
```

### `type_of` / `typeof`
Return the type name of a value as a string.

```aethershell
type_of 42           # "Int"
type_of "hello"      # "String"
type_of [1, 2, 3]    # "Array"
type_of { x: 1 }     # "Record"
typeof fn(x) => x    # "Lambda"
```

### `inspect`
Return a detailed string representation of a value including internal structure.

```aethershell
inspect [1, "two", true]
# "[Int(1), String(\"two\"), Bool(true)]"
```

## Option Types

AetherShell has first-class `Some`/`None` for representing optional values.

### `Some`
Wrap a value in an option.

```aethershell
let result = Some(42)
echo result          # Some(42)
```

### `None`
The empty option value.

```aethershell
let missing = None
echo missing         # None
```

Options are useful in pipelines where operations may not find a result:

```aethershell
let found = [1, 2, 3] | first
# found is Some(1) or None if array is empty
```

## Environment Variables

### `env`
Return all environment variables as a Record.

```aethershell
let vars = env
echo vars.PATH
echo vars.HOME
```

### `set_env`
Set an environment variable for the current session.

```aethershell
set_env "MY_VAR" "hello"
echo (env).MY_VAR    # hello
```

## JSON

### `json_parse`
Parse a JSON string into a structured Value (Record, Array, etc.).

```aethershell
let data = json_parse '{"name": "Ada", "langs": ["Rust", "Python"]}'
echo data.name       # Ada
echo data.langs[0]   # Rust
```

### `json_stringify`
Serialize any value to a JSON string.

```aethershell
let rec = { x: 1, y: [2, 3] }
let s = json_stringify rec
echo s               # {"x":1,"y":[2,3]}
```

### `save_json` / `write_json`
Write a value as formatted JSON to a file.

```aethershell
let config = { debug: true, port: 8080 }
save_json "config.json" config
```

## Timing & Sleep

### `time`
Measure execution time of an expression. Returns the elapsed time.

```aethershell
time (ls "." | where(fn(f) => f.size > 1000))
# Elapsed: 12ms
```

### `now` / `timestamp`
Return the current Unix timestamp in milliseconds.

```aethershell
let start = now
# ... do work ...
let elapsed = now - start
echo "Took ${elapsed}ms"
```

### `sleep`
Pause execution for a given number of milliseconds.

```aethershell
sleep 1000           # Sleep for 1 second
```

## Shell Interop

### `sh` / `shell`
Execute a raw shell command and return its output as a string.

```aethershell
let result = sh "git status --short"
echo result

# Capture structured output by parsing
sh "git branch" | split "\n" | map(fn(b) => trim b)
```

### `call`
Call a function or builtin by name (as a string).

```aethershell
call "echo" "hello"
let op = "upper"
call op "hello"      # "HELLO"
```

### `exit`
Exit the shell with an optional exit code.

```aethershell
exit           # Exit with code 0
exit 1         # Exit with code 1
```

## Diagnostics

### `assert`
Assert that a condition is true. Throws an error if false.

```aethershell
assert (2 + 2 == 4)            # passes
assert (len [1,2,3] == 3)      # passes
assert false                    # ERROR: assertion failed
```

### `type_assert` / `assert_type`
Assert that a value has a specific type.

```aethershell
type_assert 42 "Int"            # passes
type_assert "hi" "String"       # passes
type_assert 42 "String"         # ERROR: expected String, got Int
```

### `is_error`
Check whether a value is an Error.

```aethershell
let result = try { json_parse "invalid" } catch(e) { e }
echo (is_error result)          # true

echo (is_error 42)              # false
```

### `trace`
Print a trace message with context, useful for debugging pipelines.

```aethershell
[1, 2, 3]
  | map(fn(x) => { trace "processing" x; x * 2 })
  | reduce(fn(a, b) => a + b, 0)
```

## Membership

### `in`
Test whether a value exists in an array or a key exists in a record.

```aethershell
echo (3 in [1, 2, 3])           # true
echo ("x" in { x: 1, y: 2 })   # true
echo (5 in [1, 2, 3])           # false
```

## Configuration

### `config`
Display the current shell configuration.

```aethershell
config                   # Show all config
```

### `config_get` / `config_set`
Read or write individual configuration values.

```aethershell
config_get "theme"
config_set "theme" "dark"
config_set "editor" "vim"
```

### `config_path`
Return the path to the configuration file.

```aethershell
echo (config_path)       # ~/.config/aethershell/config.toml
```

### `config_init`
Create a default configuration file.

### `config_reload`
Reload configuration from disk.

### `themes`
List available shell themes.

```aethershell
themes
# ["dark", "light", "monokai", "solarized", ...]
```
