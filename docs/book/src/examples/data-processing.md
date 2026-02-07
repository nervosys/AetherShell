# Data Processing

Real-world examples of using AetherShell for data processing tasks.

## CSV Analysis

### Load and Analyze CSV

```aethershell
# Parse CSV manually
let lines = cat "sales.csv" | split "\n"
let headers = split (first lines) ","
let rows = lines | slice 1 (len lines) | map(fn(line) => split line ",")

# Find top sellers
rows
  | map(fn(r) => { name: r[0], amount: float(r[2]) })
  | sort_by "amount" "desc"
  | take 10
```

### Aggregate by Category

```aethershell
let data = cat "products.csv" | split "\n" | slice 1 100
  | map(fn(line) => {
      let cols = split line ","
      { category: cols[1], price: float(cols[2]), qty: int(cols[3]) }
  })

# Revenue per category
data
  | map(fn(r) => { category: r.category, revenue: r.price * r.qty })
  | group_by "category"
```

## JSON Processing

### API Data Pipeline

```aethershell
# Fetch and process API data
let users = web_json_get "https://jsonplaceholder.typicode.com/users"

users
  | map(fn(u) => { name: u.name, city: u.address.city, company: u.company.name })
  | sort_by "city" "asc"
```

### Merge Multiple Sources

```aethershell
let users = web_json_get "https://api.example.com/users"
let orders = web_json_get "https://api.example.com/orders"

# Join users with their order counts
users | map(fn(u) => {
  let user_orders = orders | where(fn(o) => o.user_id == u.id)
  { ...u, order_count: len user_orders, total_spent: user_orders | map(fn(o) => o.amount) | sum }
}) | sort_by "total_spent" "desc"
```

## Log Analysis

### Error Frequency

```aethershell
cat "app.log"
  | split "\n"
  | where(fn(line) => contains line "ERROR")
  | map(fn(line) => {
      let parts = split line " "
      { date: parts[0], error: join(slice(parts, 3, len(parts)), " ") }
  })
  | map(fn(e) => e.error)
  | sort
  | uniq
```

### Request Latency Analysis

```aethershell
cat "access.log"
  | split "\n"
  | where(fn(line) => contains line "GET /api")
  | map(fn(line) => {
      let parts = split line " "
      float(last parts)
  })
  | map(fn(latencies) => {
      {
        count: len latencies,
        avg_ms: avg latencies,
        p50: sort latencies | nth(len(latencies) / 2),
        max: max latencies
      }
  })
```

## File System Analysis

### Disk Usage Report

```aethershell
ls "."
  | where(fn(f) => f.is_dir)
  | map(fn(d) => {
      let usage = fs_du d.path
      { dir: d.name, size_mb: round(usage.total / 1048576.0), files: usage.files }
  })
  | sort_by "size_mb" "desc"
```

### Find Duplicate Files

```aethershell
fs_walk "."
  | where(fn(f) => !f.is_dir)
  | map(fn(f) => { path: f.path, hash: crypto_hash_file "md5" f.path, size: f.size })
  | sort_by "hash" "asc"
  | reduce(fn(acc, f) => {
      # Group by hash to find duplicates
      ...acc
  }, {})
```

## Statistical Summary

```aethershell
let data = [23, 45, 12, 67, 34, 89, 11, 56, 78, 42]

let stats = {
  n: len data,
  sum: data | sum,
  mean: data | avg,
  min: data | min,
  max: data | max,
  range: (data | max) - (data | min),
  sorted: data | sort
}

echo stats
```
