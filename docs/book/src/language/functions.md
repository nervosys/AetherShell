# Functions and Lambdas

AetherShell treats functions as first-class values using lambda syntax. Lambdas can be passed to pipelines, stored in variables, and composed freely.

## Lambda Syntax

The basic form is `fn(params) => body`:

```ae
let double = fn(x) => x * 2
let add = fn(a, b) => a + b
let greet = fn() => "hello"
```

Lambdas are expressions — they return the value of their body:

```ae
double(5)      # => 10
add(3, 4)      # => 7
greet()        # => "hello"
```

## Multi-Parameter Lambdas

```ae
let clamp = fn(value, lo, hi) =>
    if value < lo { lo }
    else if value > hi { hi }
    else { value }

clamp(15, 0, 10)    # => 10
clamp(-5, 0, 10)    # => 0
```

## Calling Conventions

AetherShell supports multiple ways to call functions:

### Parenthesized Calls

Standard function calling:

```ae
double(5)
add(3, 4)
```

### Word Calls

At the top level, functions can be called without parentheses (shell-style):

```ae
print "hello"          # equivalent to print("hello")
echo "world"           # equivalent to echo("world")
cd "/home"             # equivalent to cd("/home")
```

> **Note**: Word-call syntax is disabled inside lambda bodies to prevent ambiguous parsing.

### Pipeline Calls

Functions can receive input through the pipe operator:

```ae
5 | double             # => 10
[1, 2, 3] | double     # => [2, 4, 6] (auto-maps over arrays)
```

## Auto-Mapping

When a **1-parameter lambda** receives an **Array** through a pipeline, it automatically maps over each element:

```ae
[1, 2, 3] | fn(x) => x * 2           # => [2, 4, 6]
["a", "b"] | fn(s) => s + "!"        # => ["a!", "b!"]
```

A single (non-array) value is passed directly:

```ae
5 | fn(x) => x * 2    # => 10
```

## Index Parameter

Callback functions used with `map` and `where` can accept a second parameter for the index:

```ae
["a", "b", "c"] | map fn(item, i) => "${i}: ${item}"
# => ["0: a", "1: b", "2: c"]
```

## Closures

Lambdas capture variables from the enclosing environment by reference. Variables are resolved at call time:

```ae
let multiplier = 3
let scale = fn(x) => x * multiplier
scale(10)              # => 30

# If multiplier changes (if mutable), scale reflects the new value
```

## Async Functions

Async lambdas create futures that must be awaited:

```ae
let fetch_data = async fn(url) => http_get(url)
let future = fetch_data("https://api.example.com/data")
let result = await future
```

## Recursion

Lambdas can reference themselves through their binding name:

```ae
let factorial = fn(n) =>
    if n <= 1 { 1 }
    else { n * factorial(n - 1) }

factorial(5)    # => 120
```

This works because variable lookup happens at evaluation time, so `factorial` resolves to the lambda in the environment.

## Higher-Order Functions

Functions that take or return functions:

```ae
let apply_twice = fn(f, x) => f(f(x))
apply_twice(fn(x) => x + 1, 0)    # => 2

let make_adder = fn(n) => fn(x) => x + n
let add5 = make_adder(5)
add5(10)    # => 15
```

## Builtins as Values

Builtin functions can be referenced and passed around:

```ae
let my_sort = sort
[3, 1, 2] | my_sort    # => [1, 2, 3]
```

## Common Patterns

### Pipeline with inline lambda

```ae
ls "." | where fn(f) => f.size > 1000 | map fn(f) => f.name
```

### Compose transformations

```ae
let transform = fn(data) =>
    data
    | where fn(r) => r.active
    | map fn(r) => {name: r.name, score: r.points * 10}
    | sort
```

### Reduce / fold

```ae
[1, 2, 3, 4, 5] | reduce fn(acc, x) => acc + x, 0    # => 15
```
