# Math Operations

Math builtins operate on numeric values (Int and Float). They handle type promotion automatically — operations between Int and Float produce Float results.

## Rounding

### `floor`
Round down to the nearest integer.

```aethershell
floor 3.7     # 3
floor -2.3    # -3
floor 5       # 5 (no-op for Int)
```

### `ceil`
Round up to the nearest integer.

```aethershell
ceil 3.2      # 4
ceil -2.7     # -2
ceil 5        # 5 (no-op for Int)
```

### `round`
Round to the nearest integer (half rounds up).

```aethershell
round 3.5     # 4
round 3.4     # 3
round -2.5    # -2
```

## Absolute Value

### `abs`
Return the absolute (non-negative) value.

```aethershell
abs -42       # 42
abs 3.14      # 3.14
abs -99.9     # 99.9
```

## Powers & Roots

### `sqrt`
Return the square root as a Float.

```aethershell
sqrt 16       # 4.0
sqrt 2        # 1.4142135623730951
```

### `pow`
Raise a number to a power.

```aethershell
pow 2 10      # 1024
pow 3.0 0.5   # 1.7320508075688772 (same as sqrt 3)
```

## Min / Max

### `min`
Return the smaller of two values, or the minimum of an array.

```aethershell
min 3 7       # 3
[5, 2, 8, 1] | min   # 1
```

### `max`
Return the larger of two values, or the maximum of an array.

```aethershell
max 3 7       # 7
[5, 2, 8, 1] | max   # 8
```

## Aggregation

These also appear in [Collections](collections.md) since they operate on arrays:

### `sum`
Sum all numeric elements of an array.

```aethershell
[1, 2, 3, 4, 5] | sum     # 15
range 1 101 | sum           # 5050
```

### `avg` / `mean`
Compute the arithmetic mean.

```aethershell
[10, 20, 30] | avg          # 20.0
[1.5, 2.5, 3.0] | mean      # 2.3333333333333335
```

### `product`
Multiply all elements together.

```aethershell
[1, 2, 3, 4, 5] | product   # 120 (5!)
range 1 11 | product         # 3628800 (10!)
```

## Arithmetic Operators

Standard arithmetic operators work on both Int and Float:

| Operator | Description    | Example  | Result |
| -------- | -------------- | -------- | ------ |
| `+`      | Addition       | `3 + 4`  | `7`    |
| `-`      | Subtraction    | `10 - 3` | `7`    |
| `*`      | Multiplication | `6 * 7`  | `42`   |
| `/`      | Division       | `10 / 3` | `3`    |
| `%`      | Modulo         | `10 % 3` | `1`    |

**Type promotion**: When mixing Int and Float, the result is Float:

```aethershell
3 + 2.0       # 5.0 (Float)
10 / 3        # 3   (Int division)
10 / 3.0      # 3.3333... (Float division)
```

## Comparison Operators

| Operator | Description      | Example  |
| -------- | ---------------- | -------- |
| `==`     | Equal            | `3 == 3` |
| `!=`     | Not equal        | `3 != 4` |
| `<`      | Less than        | `3 < 5`  |
| `>`      | Greater than     | `5 > 3`  |
| `<=`     | Less or equal    | `3 <= 3` |
| `>=`     | Greater or equal | `5 >= 5` |

## Practical Examples

### Statistical Summary

```aethershell
let data = [23, 45, 12, 67, 34, 89, 11, 56]

let stats = {
  count: len data,
  sum: data | sum,
  min: data | min,
  max: data | max,
  avg: data | avg,
  range: (data | max) - (data | min)
}
echo stats
# { count: 8, sum: 337, min: 11, max: 89, avg: 42.125, range: 78 }
```

### File Size Analysis

```aethershell
let sizes = ls "src" | map(fn(f) => f.size)

echo "Total: ${sum sizes} bytes"
echo "Average: ${round(avg sizes)} bytes"
echo "Largest: ${max sizes} bytes"
echo "Smallest: ${min sizes} bytes"
```

### Fibonacci Sequence

```aethershell
range 0 10 | reduce(fn(acc, _) => {
  let n = len acc
  if n < 2 { push acc (n) }
  else { push acc (acc[n-1] + acc[n-2]) }
}, [])
# [0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
```

### Distance Calculation

```aethershell
let p1 = { x: 3.0, y: 4.0 }
let p2 = { x: 7.0, y: 1.0 }
let dist = sqrt(pow(p2.x - p1.x, 2) + pow(p2.y - p1.y, 2))
echo dist    # 5.0
```

### Percentage Breakdown

```aethershell
ls "src"
  | where(fn(f) => !f.is_dir)
  | map(fn(f) => { name: f.name, size: f.size })
  | map(fn(f) => {
      let total = ls "src" | map(fn(g) => g.size) | sum
      { ...f, pct: round(f.size * 100.0 / total) }
  })
  | sort_by "pct" "desc"
```
