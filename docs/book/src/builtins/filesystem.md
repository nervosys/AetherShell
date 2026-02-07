# File System

File system builtins return structured data, making file operations composable in pipelines.

## Reading

### `ls` / `list`
List directory contents. Returns `Array[Record]` with file metadata.

```aethershell
ls "."
# [
#   { name: "main.rs", path: "./main.rs", size: 2048, modified: "...", is_dir: false, extension: "rs" },
#   { name: "src", path: "./src", size: 4096, modified: "...", is_dir: true, extension: "" },
#   ...
# ]

# Filter to Rust files over 1KB
ls "src" | where(fn(f) => f.extension == "rs" && f.size > 1024) | sort_by "size" "desc"
```

### `cat`
Read the entire contents of a file as a string.

```aethershell
let content = cat "README.md"
echo content

# Use in pipelines
cat "data.csv" | split "\n" | take 5
```

### `read_text`
Read a file as text (alias-like behavior to `cat`).

```aethershell
let cfg = read_text "config.toml"
```

### `head`
Read the first N lines of a file.

```aethershell
head "log.txt" 10    # First 10 lines
```

### `tail`
Read the last N lines of a file.

```aethershell
tail "log.txt" 20    # Last 20 lines
```

## Searching

### `grep`
Search for a pattern in file(s). Returns `Array[Record]` with match details.

```aethershell
grep "TODO" "src/"
# [
#   { file: "src/main.rs", line: 42, content: "// TODO: refactor this", match: "TODO" },
#   ...
# ]

# Count matches per file
grep "unwrap" "src/" | map(fn(m) => m.file) | unique | len
```

### `find`
Find files matching criteria recursively.

```aethershell
find "." "*.rs"
# ["./src/main.rs", "./src/lib.rs", "./tests/eval.rs", ...]
```

### `wc`
Word/line/character count. Returns a Record.

```aethershell
wc "README.md"
# { lines: 150, words: 892, chars: 5431 }
```

## Text Processing (File-oriented)

### `sort`
Sort lines of input alphabetically.

```aethershell
cat "names.txt" | sort
```

### `uniq`
Remove adjacent duplicate lines (typically used after `sort`).

```aethershell
cat "words.txt" | sort | uniq
```

## Writing

### `file_write` / `write_file`
Write content to a file, creating it if needed, overwriting if it exists.

```aethershell
file_write "output.txt" "Hello, world!\n"

# Write data as JSON
let data = { name: "report", items: [1, 2, 3] }
file_write "data.json" (json_stringify data)
```

### `file_append` / `append_file`
Append content to the end of a file.

```aethershell
file_append "log.txt" "New log entry\n"
```

### `file_insert` / `insert_lines`
Insert content at a specific line number.

```aethershell
file_insert "config.txt" 5 "new_setting = true"
```

## Editing

### `file_replace` / `str_replace_in_file`
Replace text in a file.

```aethershell
file_replace "config.toml" "debug = false" "debug = true"
```

### `file_patch` / `patch_file`
Apply a structured patch to a file.

```aethershell
file_patch "main.rs" [
  { line: 10, old: "let x = 1;", new: "let x = 2;" }
]
```

### `file_edit` / `edit_file`
Perform line-based edits on a file.

```aethershell
file_edit "src/lib.rs" { delete_lines: [5, 6], insert: { 10: "// new comment" } }
```

### `file_delete_lines` / `delete_lines`
Remove specific lines from a file.

```aethershell
file_delete_lines "output.txt" 3 5    # Remove lines 3-5
```

## File Operations

### `file_copy` / `cp`
Copy a file or directory.

```aethershell
file_copy "src/main.rs" "backup/main.rs"
cp "data/" "data_backup/"
```

### `file_move` / `mv`
Move or rename a file or directory.

```aethershell
file_move "old_name.txt" "new_name.txt"
mv "temp/output.csv" "results/final.csv"
```

### `file_mkdir` / `mkdir`
Create a directory (and parents if needed).

```aethershell
mkdir "output/reports/2024"
```

### `file_exists` / `exists`
Check whether a file or directory exists.

```aethershell
if (file_exists "config.toml") {
  echo "Config found"
} else {
  echo "Using defaults"
}
```

### `file_diff`
Compare two files and return their differences.

```aethershell
file_diff "v1.txt" "v2.txt"
```

### `file_backup`
Create a timestamped backup copy of a file.

```aethershell
file_backup "important.db"
# Creates important.db.20240115_143022.bak
```

### `pwd`
Return the current working directory as a string.

```aethershell
echo (pwd)     # /home/user/project
```

## Extended File System

### `fs_stat` / `stat`
Get detailed file metadata.

```aethershell
fs_stat "main.rs"
# { size: 2048, modified: "...", created: "...", permissions: "rw-r--r--", ... }
```

### `fs_glob` / `glob`
Find files matching a glob pattern.

```aethershell
fs_glob "src/**/*.rs"
# ["src/main.rs", "src/lib.rs", "src/eval.rs", ...]
```

### `fs_tree` / `tree`
Display directory structure as a tree.

```aethershell
fs_tree "src" 2    # Depth limit of 2
```

### `fs_du` / `du`
Disk usage for a path.

```aethershell
fs_du "target"
# { total: 524288000, files: 1234 }
```

### `fs_df` / `df`
Disk space information for mounted filesystems.

```aethershell
fs_df
# [{ mount: "/", total: 500000000000, used: 250000000000, available: 250000000000 }, ...]
```

### `fs_walk`
Recursively walk a directory tree, returning all entries.

```aethershell
fs_walk "src" | where(fn(f) => f.extension == "rs") | len
# 15
```

### `fs_symlink`
Create a symbolic link.

```aethershell
fs_symlink "target_path" "link_path"
```

### `fs_readlink`
Read the target of a symbolic link.

```aethershell
fs_readlink "link_path"    # "/actual/target/path"
```

### `fs_realpath`
Resolve a path to its absolute canonical form.

```aethershell
fs_realpath "../src/main.rs"
# "/home/user/project/src/main.rs"
```

### `fs_tempfile` / `fs_tempdir`
Create temporary files or directories.

```aethershell
let tmp = fs_tempfile
file_write tmp "scratch data"

let tmpdir = fs_tempdir
# Use tmpdir for temporary work
```

### `fs_watch` / `fs_unwatch`
Watch a path for filesystem changes.

```aethershell
fs_watch "src/" fn(event) => {
  echo "Changed: ${event.path} (${event.kind})"
}
# Later:
fs_unwatch "src/"
```

## Pipeline Examples

```aethershell
# Find the 10 largest files recursively
fs_walk "."
  | where(fn(f) => !f.is_dir)
  | sort_by "size" "desc"
  | take 10
  | map(fn(f) => { name: f.name, mb: round(f.size / 1048576.0) })

# Batch rename files
ls "photos"
  | where(fn(f) => f.extension == "jpeg")
  | each(fn(f) => {
      let new_name = replace f.name ".jpeg" ".jpg"
      mv f.path "photos/${new_name}"
  })

# Disk usage report
ls "."
  | where(fn(f) => f.is_dir)
  | map(fn(d) => { dir: d.name, size: (fs_du d.path).total })
  | sort_by "size" "desc"
```
