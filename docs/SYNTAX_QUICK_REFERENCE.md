# AetherShell Variable Syntax - Complete Reference Card

## 📋 All Variable Declaration Syntaxes

### Immutable Variables (Cannot Change After Declaration)

```ae
// Recommended: Simple assignment
name = "Alice"
count = 42
items = [1, 2, 3]

// Alternative: Explicit let
let name = "Alice"
let count = 42
```

### Mutable Variables (Can Change After Declaration)

```ae
// Recommended: mut with =
mut counter = 0
mut total = 100
mut active = true

// Traditional: let mut
let mut counter = 0
let mut total = 100
```

## 🎯 Quick Decision Guide

**For immutable variables (most common):**
```ae
x = 42              // ✅ USE THIS - simplest and cleanest
```

**For mutable variables:**
```ae
mut x = 42          // ✅ USE THIS - consistent with immutable style
```

**For type annotations:**
```ae
let x: Int = 42     // Use when you need explicit types
let mut x: Float = 0.0
```

## 📊 Complete Comparison Table

| Syntax           | Mutable | Type Annotation | Length | Recommended |
| ---------------- | ------- | --------------- | ------ | ----------- |
| `x = 42`         | ❌       | ❌               | 6      | ✅ Immutable |
| `mut x = 42`     | ✅       | ❌               | 10     | ✅ Mutable   |
| `let x = 42`     | ❌       | ✅               | 10     | When needed |
| `let mut x = 42` | ✅       | ✅               | 14     | Traditional |

## 💡 Common Patterns

### Counter
```ae
mut count = 0
count = count + 1
```

### Accumulator
```ae
mut sum = 0
sum = sum + 10
sum = sum + 20
```

### State Machine
```ae
mut state = "idle"
state = "loading"
state = "ready"
```

### Flag Toggle
```ae
mut active = false
active = true
```

### Progress Tracker
```ae
mut progress = 0.0
progress = 25.0
progress = 50.0
progress = 100.0
```

## 🔄 Type Examples

```ae
// Integers
count = 42
mut counter = 0

// Floats
price = 9.99
mut total = 0.0

// Strings
name = "Alice"
mut message = "Hello"

// Booleans
ready = true
mut active = false

// Arrays
items = [1, 2, 3]
mut list = []

// Records
user = {name: "Bob", age: 30}
mut config = {debug: false}

// Functions
double = fn(x) => x * 2
mut transform = fn(x) => x
```

## ✅ Best Practices

1. **Prefer immutable by default**
   ```ae
   x = 42              // Default to immutable
   mut y = 0           // Only use mut when needed
   ```

2. **Use shortest syntax**
   ```ae
   x = 42              // ✅ Good
   let x = 42          // ❌ Unnecessarily verbose
   ```

3. **Clear mutability intent**
   ```ae
   mut counter = 0     // ✅ Clear that it will change
   let mut counter = 0 // ❌ Extra verbosity
   ```

4. **Consistent style in file**
   ```ae
   // Pick one style and stick to it
   x = 10
   y = 20
   mut z = 0
   
   // Avoid mixing unnecessarily
   x = 10
   let y = 20          // ❌ Inconsistent
   z := 30             // ❌ Inconsistent
   ```

## 🚀 Quick Examples

### Read-only Configuration
```ae
api_url = "https://api.example.com"
timeout = 30
retries = 3
```

### Mutable State
```ae
mut attempts = 0
mut last_error = ""
mut is_connected = false
```

### Mixed Usage
```ae
// Immutable config
max_retries = 3
base_url = "https://api.example.com"

// Mutable state
mut current_retry = 0
mut status = "idle"

// Logic
while current_retry < max_retries {
  current_retry = current_retry + 1
  status = "connecting"
}
```

## 📝 Summary

**Immutable (default):**
- Use: `x = value`
- When value won't change
- Safest option

**Mutable (when needed):**
- Use: `mut x = value`
- When value will change
- Clear intent with `mut`

**Type annotations (rare):**
- Use: `let x: Type = value`
- When type inference needs help
- When documenting types

**Remember:** Immutable by default, mutable when necessary! 🎯
