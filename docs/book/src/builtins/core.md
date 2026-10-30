# Core Operations

Core builtins provide essential shell operations: output, environment management, JSON handling, option types, diagnostics, and shell interop.

## Output

### `print`
Print a value to stdout without a trailing newline.

```ironshell
print "hello"       # hello
print 42            # 42
print [1, 2, 3]     # [1, 2, 3]
```

### `echo`
Print a value followed by a newline. Equivalent to `println` in many languages.

```ironshell
echo "Hello, world!"
echo { name: "Ada", age: 36 }
```

### `debug` / `dbg`
Print a value with type and structure information, useful for development.

```ironshell
debug [1, "two", 3.0]
# Array(3): [Int(1), String("two"), Float(3.0)]

let rec = { x: 1, y: [2, 3] }
dbg rec
```

## Help & Inspection

### `help`
Display available commands and usage information.

```ironshell
help              # List all builtins
help "map"        # Help for a specific builtin
```

### `type_of` / `typeof`
Return the type name of a value as a string.

```ironshell
type_of 42           # "Int"
type_of "hello"      # "String"
type_of [1, 2, 3]    # "Array"
type_of { x: 1 }     # "Record"
typeof fn(x) => x    # "Lambda"
```

### `inspect`
Return a detailed string representation of a value including internal structure.

```ironshell
inspect [1, "two", true]
# "[Int(1), String(\"two\"), Bool(true)]"
```

## Option Types

IronShell has first-class `Some`/`None` for representing optional values.

### `Some`
Wrap a value in an option.

```ironshell
let result = Some(42)
echo result          # Some(42)
```

### `None`
The empty option value.

```ironshell
let missing = None
echo missing         # None
```

Options are useful in pipelines where operations may not find a result:

```ironshell
let found = [1, 2, 3] | first
# found is Some(1) or None if array is empty
```

## Environment Variables

### `env`
Return all environment variables as a Record.

```ironshell
let vars = env
echo vars.PATH
echo vars.HOME
```

### `set_env`
Set an environment variable for the current session.

```ironshell
set_env "MY_VAR" "hello"
echo (env).MY_VAR    # hello
```

## JSON

### `json_parse`
Parse a JSON string into a structured Value (Record, Array, etc.).

```ironshell
let data = json_parse '{"name": "Ada", "langs": ["Rust", "Python"]}'
echo data.name       # Ada
echo data.langs[0]   # Rust
```

### `json_stringify`
Serialize any value to a JSON string.

```ironshell
let rec = { x: 1, y: [2, 3] }
let s = json_stringify rec
echo s               # {"x":1,"y":[2,3]}
```

### `save_json` / `write_json`
Write a value as formatted JSON to a file.

```ironshell
let config = { debug: true, port: 8080 }
save_json "config.json" config
```

## Timing & Sleep

### `time`
Measure execution time of an expression. Returns the elapsed time.

```ironshell
time (ls "." | where(fn(f) => f.size > 1000))
# Elapsed: 12ms
```

### `now` / `timestamp`
Return the current Unix timestamp in milliseconds.

```ironshell
let start = now
# ... do work ...
let elapsed = now - start
echo "Took ${elapsed}ms"
```

### `sleep`
Pause execution for a given number of milliseconds.

```ironshell
sleep 1000           # Sleep for 1 second
```

## Shell Interop

### `sh` / `shell`
Execute a raw shell command and return its output as a string.

```ironshell
let result = sh "git status --short"
echo result

# Capture structured output by parsing
sh "git branch" | split "\n" | map(fn(b) => trim b)
```

### `call`
Call a function or builtin by name (as a string).

```ironshell
call "echo" "hello"
let op = "upper"
call op "hello"      # "HELLO"
```

### `exit`
Exit the shell with an optional exit code.

```ironshell
exit           # Exit with code 0
exit 1         # Exit with code 1
```

## Diagnostics

### `assert`
Assert that a condition is true. Throws an error if false.

```ironshell
assert (2 + 2 == 4)            # passes
assert (len [1,2,3] == 3)      # passes
assert false                    # ERROR: assertion failed
```

### `type_assert` / `assert_type`
Assert that a value has a specific type.

```ironshell
type_assert 42 "Int"            # passes
type_assert "hi" "String"       # passes
type_assert 42 "String"         # ERROR: expected String, got Int
```

### `is_error`
Check whether a value is an Error.

```ironshell
let result = try { json_parse "invalid" } catch(e) { e }
echo (is_error result)          # true

echo (is_error 42)              # false
```

### `trace`
Print a trace message with context, useful for debugging pipelines.

```ironshell
[1, 2, 3]
  | map(fn(x) => { trace "processing" x; x * 2 })
  | reduce(fn(a, b) => a + b, 0)
```

## Membership

### `in`
Test whether a value exists in an array or a key exists in a record.

```ironshell
echo (3 in [1, 2, 3])           # true
echo ("x" in { x: 1, y: 2 })   # true
echo (5 in [1, 2, 3])           # false
```

## Configuration

### `config`
Display the current shell configuration.

```ironshell
config                   # Show all config
```

### `config_get` / `config_set`
Read or write individual configuration values.

```ironshell
config_get "theme"
config_set "theme" "dark"
config_set "editor" "vim"
```

### `config_path`
Return the path to the configuration file.

```ironshell
echo (config_path)       # ~/.config/ironshell/config.toml
```

### `config_init`
Create a default configuration file.

### `config_reload`
Reload configuration from disk.

### `themes`
List available shell themes.

```ironshell
themes
# ["dark", "light", "monokai", "solarized", ...]
```
