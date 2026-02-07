# String Operations

String builtins provide text manipulation, splitting, joining, and pattern matching. All return new values without modifying the original.

## Case Conversion

### `upper`
Convert a string to uppercase.

```aethershell
upper "hello"          # "HELLO"
"hello" | upper        # "HELLO"
```

### `lower`
Convert a string to lowercase.

```aethershell
lower "HELLO"          # "hello"
"HELLO" | lower        # "hello"
```

## Whitespace

### `trim`
Remove leading and trailing whitespace.

```aethershell
trim "  hello  "       # "hello"
"  spaced  " | trim    # "spaced"
```

## Splitting & Joining

### `split`
Split a string by a delimiter, returning an array.

```aethershell
split "a,b,c" ","      # ["a", "b", "c"]
"one two three" | split " "   # ["one", "two", "three"]

# Split into lines
cat "log.txt" | split "\n"

# Split CSV row
"Alice,25,Engineer" | split "," 
# ["Alice", "25", "Engineer"]
```

### `join`
Join an array of strings with a delimiter.

```aethershell
join ["a", "b", "c"] ","      # "a,b,c"
["hello", "world"] | join " " # "hello world"

# Reassemble after transformation
cat "data.csv"
  | split "\n"
  | where(fn(line) => contains line "ERROR")
  | join "\n"
```

## Search & Match

### `contains`
Check if a string contains a substring.

```aethershell
contains "hello world" "world"    # true
"hello world" | contains "xyz"    # false

# Filter lines
cat "log.txt" | split "\n" | where(fn(l) => contains l "ERROR")
```

### `starts_with`
Check if a string starts with a prefix.

```aethershell
starts_with "hello" "hel"     # true
"README.md" | starts_with "READ"  # true
```

### `ends_with`
Check if a string ends with a suffix.

```aethershell
ends_with "main.rs" ".rs"     # true
"photo.jpg" | ends_with ".png"    # false
```

## Replacement

### `replace`
Replace all occurrences of a substring.

```aethershell
replace "hello world" "world" "Rust"    # "hello Rust"
"foo-bar-baz" | replace "-" "_"          # "foo_bar_baz"

# Multi-step replacement
"Hello, World!"
  | replace "," ""
  | replace "!" ""
  | lower
# "hello world"
```

## String in Pipelines

Strings integrate seamlessly with AetherShell's pipeline model. When a single-parameter lambda receives an array, it auto-maps:

```aethershell
# Uppercase every element
["hello", "world"] | upper
# ["HELLO", "WORLD"]

# Trim all strings
["  a  ", " b ", "c"] | trim
# ["a", "b", "c"]
```

## Practical Examples

### Log Parsing

```aethershell
cat "app.log"
  | split "\n"
  | where(fn(line) => contains line "ERROR")
  | map(fn(line) => {
      let parts = split line " "
      {
        timestamp: parts[0],
        level: parts[1],
        message: join (slice parts 2 (len parts)) " "
      }
  })
  | sort_by "timestamp" "desc"
  | take 10
```

### CSV Processing

```aethershell
let lines = cat "users.csv" | split "\n"
let headers = split (first lines) ","
let rows = lines | slice 1 (len lines) | map(fn(line) => split line ",")

rows | map(fn(row) => {
  name: trim row[0],
  email: lower (trim row[1]),
  dept: upper (trim row[2])
})
```

### Text Transformation

```aethershell
# Snake_case to camelCase
let snake = "my_variable_name"
let parts = split snake "_"
let camel = (first parts) + (parts | slice 1 (len parts) | map(fn(p) => {
  let chars = split p ""
  (upper (first chars)) + (join (slice chars 1 (len chars)) "")
}) | join "")
echo camel   # "myVariableName"
```

### Batch File Renaming

```aethershell
ls "."
  | where(fn(f) => ends_with f.name ".txt")
  | each(fn(f) => {
      let new_name = replace f.name ".txt" ".md"
      mv f.path new_name
  })
```

## Encoding (via `ab_encode` / `ab_decode`)

For base64 and hex encoding, see the crypto builtins (`crypto_base64_encode`, `crypto_hex_encode`), or use the general `ab_encode` / `ab_decode`:

```aethershell
ab_encode "hello" "base64"    # "aGVsbG8="
ab_decode "aGVsbG8=" "base64" # "hello"
```
