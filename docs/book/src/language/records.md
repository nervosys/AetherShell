# Records and Tables

Records and tables are AetherShell's structured data types. Records are key-value maps; tables are arrays of records with a defined schema. Together they enable typed data processing pipelines.

## Records

### Creating Records

```ae
let person = {name: "Ada", age: 36, active: true}
let config = {host: "localhost", port: 8080, debug: false}
```

Record keys are always strings. Values can be any type, including nested records and arrays:

```ae
let project = {
    name: "AetherShell",
    version: {major: 0, minor: 3, patch: 0},
    tags: ["shell", "rust", "ai"]
}
```

### Field Access

Use dot notation to access fields:

```ae
person.name      # => "Ada"
person.age       # => 36
project.version.major    # => 0
```

Accessing a non-existent field produces an error:

```ae
person.email     # Error: field 'email' not found in record
```

### Record Operations

```ae
# Get all keys
{name: "Ada", age: 36} | keys
# => ["age", "name"]   (sorted alphabetically)

# Merge records (later values win)
let defaults = {color: "blue", size: 10}
let custom = {size: 20, bold: true}
merge defaults custom
# => {bold: true, color: "blue", size: 20}
```

### Records in Pipelines

Records flow through pipelines as structured data:

```ae
let users = [
    {name: "Ada", score: 95},
    {name: "Bob", score: 82},
    {name: "Eve", score: 91}
]

users
| where fn(u) => u.score > 85
| map fn(u) => {name: u.name, grade: "A"}
# => [{name: "Ada", grade: "A"}, {name: "Eve", grade: "A"}]
```

## Tables

Tables are structured data with named columns, returned by many builtins.

### Table Structure

A table has:
- **rows**: Array of records (each row is a `{key: value}` map)
- **schema**: List of column names defining the structure

### Built-in Table Sources

```ae
# List files — returns a table
ls "."
# Columns: name, path, ext, is_dir, size, modified

# Process listings
ps
# Columns: pid, name, cpu, memory
```

### Pretty Printing

Tables get special column-aligned display in the terminal:

```
┌──────────────┬──────┬─────┬────────┐
│ name         │ ext  │ dir │ size   │
├──────────────┼──────┼─────┼────────┤
│ main.rs      │ rs   │ no  │ 2,451  │
│ lib.rs       │ rs   │ no  │ 1,089  │
│ Cargo.toml   │ toml │ no  │ 456    │
└──────────────┴──────┴─────┴────────┘
```

## Data Pipeline Operations

### `select` — Project Fields

Keep only specific columns:

```ae
ls "." | select "name" "size"
# Records with only name and size fields
```

### `where` — Filter Rows

Keep rows matching a predicate:

```ae
ls "." | where fn(f) => f.size > 1000
ls "." | where fn(f) => f.ext == "rs"
```

### `map` — Transform Rows

Create new values from each row:

```ae
ls "." | map fn(f) => {
    file: f.name,
    kb: f.size / 1024
}
```

### `sort` — Order Rows

```ae
[3, 1, 4, 1, 5] | sort
# => [1, 1, 3, 4, 5]
```

### `group` / `group_by` — Group Rows

Group records by a field value:

```ae
ls "." | group "ext"
# Records grouped by file extension
```

### `reduce` — Aggregate

Collapse an array into a single value:

```ae
ls "." | map fn(f) => f.size | reduce fn(a, b) => a + b, 0
# Total size of all files
```

### `first` / `last` — Take Elements

```ae
[1, 2, 3, 4, 5] | first 3    # => [1, 2, 3]
[1, 2, 3, 4, 5] | last 2     # => [4, 5]
```

### `reverse` — Reverse Order

```ae
[1, 2, 3] | reverse    # => [3, 2, 1]
```

### `unique` — Remove Duplicates

```ae
[1, 2, 2, 3, 3, 3] | unique    # => [1, 2, 3]
```

### `columns` — Get Column Names

```ae
ls "." | columns
# => ["ext", "is_dir", "modified", "name", "path", "size"]
```

## Format Conversion

Convert between structured data and serialization formats:

```ae
# JSON
let data = from_json '{"name": "test"}'
data | to_json

# CSV
let csv_data = from_csv "name,age\nAda,36\nBob,30"
csv_data | to_csv

# YAML
let yaml_data = from_yaml "name: test\ncount: 42"
yaml_data | to_yaml
```

## Practical Examples

### Analyze project files

```ae
ls "src"
| where fn(f) => f.ext == "rs"
| map fn(f) => {name: f.name, kb: f.size / 1024}
| sort
| reverse
# Rust files sorted by size, largest first
```

### Process API response

```ae
http_get "https://api.github.com/repos/user/repo/issues"
| from_json
| where fn(i) => i.state == "open"
| map fn(i) => {title: i.title, labels: i.labels | map fn(l) => l.name}
| first 10
```

### Build a report

```ae
let files = ls "src" | where fn(f) => f.ext == "rs"
let total_size = files | map fn(f) => f.size | reduce fn(a, b) => a + b, 0
let count = files | length

{
    total_files: count,
    total_bytes: total_size,
    avg_size: total_size / count,
    largest: files | sort | reverse | first 1
}
```
