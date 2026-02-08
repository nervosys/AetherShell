# Basic Syntax

This guide covers the fundamental syntax of AetherShell.

## Comments

```aethershell
# This is a single-line comment

// This also works for single-line comments

# Multi-line comments use consecutive single-line comments
# like this
# and this
```

## Expressions

Everything in AetherShell is an expression that returns a value:

```aethershell
# Arithmetic
1 + 2 * 3        # 7
10 / 3           # 3 (integer division)
10.0 / 3.0       # 3.333...
10 % 3           # 1 (modulo)
2 ** 10          # 1024 (power)

# Comparison
1 < 2            # true
3 >= 3           # true
"a" == "a"       # true
"a" != "b"       # true

# Logical
true && false    # false
true || false    # true
!true            # false

# String concatenation
"Hello, " + "World!"  # "Hello, World!"

# Ternary-like with match
match x > 0 {
    true => "positive",
    false => "non-positive"
}
```

## Statements

### Variable Declaration

```aethershell
# Immutable (default)
let x = 42
let name = "Alice"
let numbers = [1, 2, 3]

# Mutable
let mut counter = 0
counter = counter + 1

# Multiple assignments
let (a, b, c) = (1, 2, 3)
```

### Blocks

Blocks are sequences of expressions. The last expression is the return value:

```aethershell
let result = {
    let x = 10
    let y = 20
    x + y  # Block returns 30
}

print(result)  # 30
```

## Control Flow

### If Expressions

```aethershell
let max = if a > b { a } else { b }

# Multi-branch
let grade = if score >= 90 {
    "A"
} else if score >= 80 {
    "B"
} else if score >= 70 {
    "C"
} else {
    "F"
}
```

### Match Expressions

```aethershell
let result = match value {
    0 => "zero",
    1 => "one",
    n if n < 0 => "negative",
    n if n > 100 => "large",
    _ => "other"
}

# Destructuring
let point = { x: 10, y: 20 }
match point {
    { x: 0, y: 0 } => "origin",
    { x: 0, y: _ } => "on y-axis",
    { x: _, y: 0 } => "on x-axis",
    _ => "somewhere else"
}
```

### Loops

```aethershell
# For-each loop
for item in [1, 2, 3, 4, 5] {
    print(item)
}

# With index
for (i, item) in enumerate([1, 2, 3]) {
    print(string(i) + ": " + string(item))
}

# While loop
let mut n = 0
while n < 5 {
    print(n)
    n = n + 1
}

# Loop (infinite, use break)
let mut count = 0
loop {
    count = count + 1
    if count > 10 {
        break
    }
}
```

## Functions

### Lambda Syntax

```aethershell
# Single parameter
let double = fn(x) => x * 2
double(21)  # 42

# Multiple parameters
let add = fn(a, b) => a + b
add(2, 3)  # 5

# With block body
let greet = fn(name) => {
    let greeting = "Hello, " + name + "!"
    print(greeting)
    greeting
}

# No parameters
let now = fn() => timestamp()
```

### Named Functions

```aethershell
# Define a named function (syntax sugar)
let factorial = fn(n) => match {
    0 => 1,
    1 => 1,
    n => n * factorial(n - 1)
}

factorial(5)  # 120
```

### Closures

Functions capture their environment:

```aethershell
let make_counter = fn() => {
    let mut count = 0
    fn() => {
        count = count + 1
        count
    }
}

let counter = make_counter()
counter()  # 1
counter()  # 2
counter()  # 3
```

## Operators

### Arithmetic

| Operator | Description    | Example          |
| -------- | -------------- | ---------------- |
| `+`      | Addition       | `1 + 2` → `3`    |
| `-`      | Subtraction    | `5 - 3` → `2`    |
| `*`      | Multiplication | `4 * 3` → `12`   |
| `/`      | Division       | `10 / 3` → `3`   |
| `%`      | Modulo         | `10 % 3` → `1`   |
| `**`     | Power          | `2 ** 8` → `256` |

### Comparison

| Operator | Description           |
| -------- | --------------------- |
| `==`     | Equal                 |
| `!=`     | Not equal             |
| `<`      | Less than             |
| `<=`     | Less than or equal    |
| `>`      | Greater than          |
| `>=`     | Greater than or equal |

### Logical

| Operator | Description |
| -------- | ----------- |
| `&&`     | Logical AND |
| `\|\|`   | Logical OR  |
| `!`      | Logical NOT |

### Pipeline

| Operator | Description                  |
| -------- | ---------------------------- |
| `\|`     | Pipe (pass as argument)      |
| `\|>`    | Pipe (method-style)          |
| `?>`     | Optional pipe (skip on null) |

## String Interpolation

```aethershell
let name = "Alice"
let age = 30

# Concatenation
print("Name: " + name + ", Age: " + string(age))

# Using format
print(format("Name: {}, Age: {}", name, age))
```

## Array Spread

```aethershell
let a = [1, 2, 3]
let b = [4, 5, 6]
let combined = [...a, ...b]  # [1, 2, 3, 4, 5, 6]
```

## Record Spread

```aethershell
let base = { x: 1, y: 2 }
let extended = { ...base, z: 3 }  # { x: 1, y: 2, z: 3 }

# Override fields
let updated = { ...base, x: 10 }  # { x: 10, y: 2 }
```
