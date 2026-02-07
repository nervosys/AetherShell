# Pipelines

Pipelines are the heart of AetherShell. The pipe operator `|` connects expressions so that the output of one becomes the input of the next — but unlike traditional shells, AetherShell pipelines carry **typed, structured data**, not raw text.

## Basic Syntax

```ae
expression | transform | transform | ...
```

Each `|` takes the value on the left and passes it to the right:

```ae
[3, 1, 4, 1, 5] | sort | reverse | first
# [3,1,4,1,5] → [1,1,3,4,5] → [5,4,3,1,1] → 5
```

## How Values Flow

The pipe operator is **left-associative**: `a | b | c` is parsed as `(a | b) | c`.

When the right side of a pipe is evaluated, the left side's value is available as **pipeline input**. How it's consumed depends on what's on the right:

| Right-hand side | Behavior |
|----------------|----------|
| Lambda literal | Called with left value as argument |
| Named lambda (variable) | Called with left value as explicit argument |
| Builtin function | Receives left value as `input` parameter |
| Other expression | Left value set as implicit input during evaluation |

### Example: builtins in pipelines

```ae
ls "."                                # Array of file records
| where fn(f) => f.size > 1000       # Filter: keep large files
| map fn(f) => f.name                # Transform: extract names
| sort                                # Sort alphabetically
```

### Example: lambda in pipelines

```ae
42 | fn(x) => x * 2       # => 84
"hello" | fn(s) => upper(s) # => "HELLO"
```

## Auto-Mapping

When a **1-parameter lambda** receives an **Array**, it automatically maps over each element:

```ae
[1, 2, 3] | fn(x) => x * 2     # => [2, 4, 6]
```

This applies to both inline lambdas and named lambdas. If you want to operate on the array as a whole, use the `length` or similar builtin directly:

```ae
[1, 2, 3] | length    # => 3  (operates on the whole array)
```

## Data Pipeline Builtins

These builtins are designed for pipeline use:

### Filtering

```ae
[1, 2, 3, 4, 5] | where fn(x) => x > 3
# => [4, 5]

ls "." | where fn(f) => f.ext == "rs"
# Only Rust files
```

### Mapping

```ae
[1, 2, 3] | map fn(x) => x * 10
# => [10, 20, 30]

# With index parameter
["a", "b", "c"] | map fn(item, i) => "${i}: ${item}"
# => ["0: a", "1: b", "2: c"]
```

### Reducing

```ae
[1, 2, 3, 4, 5] | reduce fn(acc, x) => acc + x, 0
# => 15
```

### Selecting fields

```ae
ls "." | select "name" "size"
# => Array of records with only name and size fields
```

### Grouping

```ae
ls "." | group "ext"
# Records grouped by file extension
```

### Sorting

```ae
[3, 1, 4, 1, 5] | sort
# => [1, 1, 3, 4, 5]
```

## Structured Data Pipelines

Since `ls`, `ps`, and other builtins return structured data, you can build powerful queries:

```ae
# Find the 5 largest Rust files
ls "src"
| where fn(f) => f.ext == "rs"
| sort
| reverse
| first 5
| select "name" "size"
```

```ae
# Calculate total size of all .toml files
ls "."
| where fn(f) => f.ext == "toml"
| map fn(f) => f.size
| reduce fn(a, b) => a + b, 0
```

## Format Conversion Pipelines

Convert between data formats inline:

```ae
# JSON to CSV
from_json '[{"name":"Ada","age":36},{"name":"Bob","age":30}]' | to_csv

# Process HTTP response
http_get "https://api.example.com/users" | from_json | where fn(u) => u.active
```

## Pipeline Input in Builtins

Builtins can access pipeline input implicitly. For example, `sort` works both ways:

```ae
sort [3, 1, 2]        # Direct call with argument
[3, 1, 2] | sort      # Pipeline: input received implicitly
```

This dual calling convention makes builtins equally useful in both interactive and pipeline contexts.

## Chaining with AI

Pipelines compose naturally with AI operations:

```ae
# Read a file, ask AI to summarize it
cat "README.md" | ai "Summarize this document in 3 bullet points"
```

```ae
# Generate code, then format it
ai "Write a Python function to sort a list" | save "sort.py"
```
