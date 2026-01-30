# Builtins Overview

AetherShell provides a rich set of built-in commands that return structured data for pipeline processing.

## Core Philosophy

Unlike traditional shells where commands return text, AetherShell builtins return typed `Value` objects:

```aethershell
# Traditional shell: ls returns text
# AetherShell: ls returns Array[Record]

ls "."
# Returns: [
#   { name: "file.txt", size: 1234, modified: "2024-01-01T12:00:00Z", is_dir: false },
#   { name: "src", size: 4096, modified: "2024-01-01T10:00:00Z", is_dir: true },
#   ...
# ]
```

This enables powerful pipeline operations:

```aethershell
ls "."
  | where(fn(f) => f.size > 1000)
  | sort_by("modified", "desc")
  | take(5)
  | select("name", "size")
```

## Categories

### File System

| Command              | Description        | Returns         |
| -------------------- | ------------------ | --------------- |
| `ls path`            | List directory     | `Array[Record]` |
| `cat file`           | Read file contents | `String`        |
| `read file`          | Read file (alias)  | `String`        |
| `write file content` | Write to file      | `Bool`          |
| `mkdir path`         | Create directory   | `Bool`          |
| `rm path`            | Remove file/dir    | `Bool`          |
| `mv src dst`         | Move/rename        | `Bool`          |
| `cp src dst`         | Copy               | `Bool`          |
| `pwd`                | Current directory  | `String`        |
| `cd path`            | Change directory   | `()`            |

### Data Processing

| Command               | Description            | Returns         |
| --------------------- | ---------------------- | --------------- |
| `map(fn)`             | Transform each element | `Array`         |
| `filter(fn)`          | Keep matching elements | `Array`         |
| `reduce(fn, init)`    | Fold to single value   | `Any`           |
| `sort_by(field, dir)` | Sort by field          | `Array`         |
| `where(fn)`           | Filter (alias)         | `Array`         |
| `select(fields...)`   | Pick fields            | `Array[Record]` |
| `take(n)`             | First n elements       | `Array`         |
| `skip(n)`             | Skip n elements        | `Array`         |
| `flatten()`           | Flatten nested arrays  | `Array`         |
| `unique()`            | Remove duplicates      | `Array`         |
| `group_by(field)`     | Group by field         | `Record`        |

### Text Processing

| Command               | Description       | Returns         |
| --------------------- | ----------------- | --------------- |
| `grep pattern path`   | Search in files   | `Array[Record]` |
| `split str delim`     | Split string      | `Array[String]` |
| `join arr delim`      | Join to string    | `String`        |
| `trim str`            | Remove whitespace | `String`        |
| `replace old new str` | Replace text      | `String`        |
| `uppercase str`       | To uppercase      | `String`        |
| `lowercase str`       | To lowercase      | `String`        |

### Network

| Command              | Description    | Returns  |
| -------------------- | -------------- | -------- |
| `http_get url`       | GET request    | `Record` |
| `http_post url body` | POST request   | `Record` |
| `http_put url body`  | PUT request    | `Record` |
| `http_delete url`    | DELETE request | `Record` |

### JSON/Data

| Command              | Description       | Returns        |
| -------------------- | ----------------- | -------------- |
| `json_parse str`     | Parse JSON        | `Any`          |
| `json_stringify val` | Serialize to JSON | `String`       |
| `csv_parse str`      | Parse CSV         | `Array[Array]` |
| `csv_stringify data` | Serialize to CSV  | `String`       |

### System

| Command            | Description     | Returns         |
| ------------------ | --------------- | --------------- |
| `env`              | All env vars    | `Record`        |
| `env_get name`     | Get env var     | `String?`       |
| `env_set name val` | Set env var     | `()`            |
| `exec cmd args`    | Run command     | `Record`        |
| `which name`       | Find executable | `String?`       |
| `ps`               | Process list    | `Array[Record]` |

### Math

| Command   | Description    | Returns  |
| --------- | -------------- | -------- |
| `sum arr` | Sum of numbers | `Number` |
| `avg arr` | Average        | `Float`  |
| `min arr` | Minimum        | `Number` |
| `max arr` | Maximum        | `Number` |
| `abs n`   | Absolute value | `Number` |
| `floor n` | Floor          | `Int`    |
| `ceil n`  | Ceiling        | `Int`    |
| `round n` | Round          | `Int`    |
| `sqrt n`  | Square root    | `Float`  |

### Type Conversion

| Command      | Description       | Returns  |
| ------------ | ----------------- | -------- |
| `int val`    | Convert to int    | `Int`    |
| `float val`  | Convert to float  | `Float`  |
| `string val` | Convert to string | `String` |
| `bool val`   | Convert to bool   | `Bool`   |
| `array val`  | Convert to array  | `Array`  |

### Output

| Command              | Description        | Returns  |
| -------------------- | ------------------ | -------- |
| `print val`          | Print value        | `()`     |
| `println val`        | Print with newline | `()`     |
| `debug val`          | Debug output       | `()`     |
| `format str args...` | Format string      | `String` |

### AI

| Command              | Description  | Returns  |
| -------------------- | ------------ | -------- |
| `ai prompt opts?`    | AI query     | `String` |
| `agent prompt opts?` | Create agent | `Agent`  |

## Return Value Structure

### File System Records

```aethershell
# ls returns:
{
    name: String,      # File name
    path: String,      # Full path
    size: Int,         # Size in bytes
    modified: String,  # ISO timestamp
    is_dir: Bool,      # Is directory
    is_file: Bool,     # Is file
    extension: String, # File extension
    permissions: String # Unix permissions
}
```

### HTTP Response

```aethershell
# http_get/post returns:
{
    status: Int,       # HTTP status code
    headers: Record,   # Response headers
    body: String,      # Response body
    ok: Bool          # status >= 200 && status < 300
}
```

### Grep Match

```aethershell
# grep returns:
{
    file: String,      # File path
    line: Int,         # Line number
    content: String,   # Matching line
    match: String      # Matched text
}
```

## Pipeline Examples

```aethershell
# Find large Rust files
ls "src"
  | where(fn(f) => f.extension == "rs" && f.size > 10000)
  | sort_by("size", "desc")
  | select("name", "size")

# API data processing
http_get("https://api.example.com/users")
  | json_parse()
  | where(fn(u) => u.active)
  | map(fn(u) => { name: u.name, email: u.email })
  | take(10)

# Log analysis
cat("app.log")
  | split("\n")
  | where(fn(line) => line.contains("ERROR"))
  | map(fn(line) => {
      let parts = split(line, " ")
      { timestamp: parts[0], message: join(skip(parts, 2), " ") }
  })
```
