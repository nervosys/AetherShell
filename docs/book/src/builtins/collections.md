# Collections

Collection builtins are the backbone of AetherShell's pipeline-oriented design. They operate on Arrays, Records, and Tables, returning structured data for further processing.

## Transforming

### `map`
Apply a function to each element in an array, returning a new array.

```aethershell
[1, 2, 3] | map(fn(x) => x * 2)
# [2, 4, 6]

# With records
ls "." | map(fn(f) => { name: f.name, kb: f.size / 1024 })
```

### `each`
Like `map` but intended for side effects. Returns the original array unchanged.

```aethershell
[1, 2, 3] | each(fn(x) => print "${x} ")
# Prints: 1 2 3
# Returns: [1, 2, 3]
```

## Filtering

### `where`
Keep only elements that satisfy a predicate.

```aethershell
[1, 2, 3, 4, 5] | where(fn(x) => x > 3)
# [4, 5]

ls "." | where(fn(f) => f.extension == "rs")
```

### `any`
Return `true` if at least one element satisfies the predicate.

```aethershell
[1, 2, 3] | any(fn(x) => x > 2)   # true
[1, 2, 3] | any(fn(x) => x > 5)   # false
```

### `all`
Return `true` if every element satisfies the predicate.

```aethershell
[2, 4, 6] | all(fn(x) => x % 2 == 0)   # true
[2, 4, 5] | all(fn(x) => x % 2 == 0)   # false
```

## Reducing

### `reduce`
Fold an array down to a single value with an accumulator.

```aethershell
[1, 2, 3, 4] | reduce(fn(acc, x) => acc + x, 0)
# 10

# Build a record from an array
["a", "b", "c"] | reduce(fn(acc, x) => { ...acc, [x]: true }, {})
# { a: true, b: true, c: true }
```

### `sum`
Sum all numeric elements.

```aethershell
[1, 2, 3, 4] | sum        # 10
ls "." | map(fn(f) => f.size) | sum
```

### `avg` / `mean`
Compute the arithmetic mean.

```aethershell
[10, 20, 30] | avg         # 20.0
```

### `product`
Multiply all elements together.

```aethershell
[2, 3, 4] | product        # 24
```

### `min` / `max`
Return the minimum or maximum value.

```aethershell
[3, 1, 4, 1, 5] | min      # 1
[3, 1, 4, 1, 5] | max      # 5
```

## Selecting

### `first`
Return the first element of an array.

```aethershell
[10, 20, 30] | first        # 10
```

### `last`
Return the last element of an array.

```aethershell
[10, 20, 30] | last         # 30
```

### `take`
Return the first N elements.

```aethershell
[1, 2, 3, 4, 5] | take 3   # [1, 2, 3]
```

### `slice`
Extract a sub-array by start index and length.

```aethershell
[10, 20, 30, 40, 50] | slice 1 3
# [20, 30, 40]
```

## Ordering

### `sort_by`
Sort an array of records by a field, with optional direction.

```aethershell
ls "." | sort_by "size" "desc" | take 5
# Top 5 largest files

let people = [
  { name: "Charlie", age: 30 },
  { name: "Alice", age: 25 },
  { name: "Bob", age: 28 }
]
people | sort_by "name" "asc"
```

### `reverse`
Reverse the order of elements.

```aethershell
[1, 2, 3] | reverse        # [3, 2, 1]
```

## Combining

### `push`
Append an element to an array.

```aethershell
[1, 2, 3] | push 4         # [1, 2, 3, 4]
```

### `concat`
Concatenate two arrays.

```aethershell
concat [1, 2] [3, 4]       # [1, 2, 3, 4]
```

### `zip`
Combine two arrays into an array of pairs.

```aethershell
zip ["a", "b", "c"] [1, 2, 3]
# [["a", 1], ["b", 2], ["c", 3]]
```

### `flatten`
Flatten nested arrays by one level.

```aethershell
[[1, 2], [3, 4], [5]] | flatten
# [1, 2, 3, 4, 5]
```

## Uniqueness

### `unique`
Remove duplicate values from an array.

```aethershell
[1, 2, 2, 3, 3, 3] | unique    # [1, 2, 3]
```

## Generators

### `range`
Generate a sequence of integers.

```aethershell
range 1 5         # [1, 2, 3, 4]
range 0 10 2      # [0, 2, 4, 6, 8]  (with step)
```

## Record Operations

### `keys`
Return the keys of a record as an array.

```aethershell
keys { x: 1, y: 2, z: 3 }     # ["x", "y", "z"]
```

### `values`
Return the values of a record as an array.

```aethershell
values { x: 1, y: 2, z: 3 }   # [1, 2, 3]
```

## Size

### `len` / `length`
Return the number of elements in an array, characters in a string, or keys in a record.

```aethershell
len [1, 2, 3]       # 3
len "hello"          # 5
len { a: 1, b: 2 }  # 2
```

## Membership

### `in`
Test if a value is in an array or a key is in a record.

```aethershell
3 in [1, 2, 3]                # true
"name" in { name: "Ada" }     # true
```

## Pipeline Composition

Collections compose naturally through the pipe operator:

```aethershell
# Data analysis pipeline
ls "src"
  | where(fn(f) => f.extension == "rs")
  | map(fn(f) => { name: f.name, lines: len(split(cat(f.path), "\n")) })
  | sort_by "lines" "desc"
  | take 5

# Aggregation
range 1 100
  | where(fn(x) => x % 3 == 0 || x % 5 == 0)
  | sum
# 2318

# Nested transformation
[
  { dept: "eng", people: ["Alice", "Bob"] },
  { dept: "hr", people: ["Charlie"] }
]
  | map(fn(d) => d.people | map(fn(p) => { name: p, dept: d.dept }))
  | flatten
```
