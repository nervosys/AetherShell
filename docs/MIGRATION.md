# Migration Guide: Bash / Zsh / PowerShell → AetherShell

> **One language, every platform, deterministic typed output.**

This guide helps you migrate scripts and workflows from legacy shells to AetherShell. Every example shows the old way (brittle text parsing) and the new way (typed structured output).

---

## Why Migrate?

Traditional shells produce **non-deterministic text output** that varies across:
- OS versions (Ubuntu vs Alpine vs macOS)
- Locales (`LC_ALL=C` vs `de_DE.UTF-8`)
- Tool versions (`coreutils` 8.x vs 9.x column widths)
- Platform (Bash on Linux vs PowerShell on Windows vs Zsh on macOS)

AetherShell eliminates this. Every command returns **typed, structured data** — the same `Value` on every platform.

---

## Quick Reference

| Concept | Bash/Zsh | PowerShell | AetherShell |
|---------|----------|------------|-------------|
| Variables | `x=42` | `$x = 42` | `x = 42` |
| Strings | `"hello $name"` | `"hello $name"` | `"hello ${name}"` |
| Arrays | `arr=(1 2 3)` | `$arr = @(1,2,3)` | `a = [1, 2, 3]` |
| Records | N/A | `[PSCustomObject]@{…}` | `r = {name: "ae"}` |
| Lambdas | N/A | `{ param($x) $x * 2 }` | `fn(x) => x * 2` |
| Pipelines | `cmd \| grep \| awk` | `cmd \| Where \| Select` | `cmd() \| where(fn(x) => ...) \| select("field")` |
| Conditionals | `if [ ... ]; then` | `if ($x) { }` | `if x > 0 { ... } else { ... }` |
| Pattern match | `case $x in` | `switch ($x) { }` | `match x { 1 => "one", _ => "other" }` |
| Functions | `fn() { ... }` | `function f { }` | `f = fn(x) => x * 2` |
| Error handling | `cmd \|\| fallback` | `try { } catch { }` | `try { risky() } catch e { fallback }` |

---

## File Operations

### Read a file

```bash
# Bash — returns raw text, encoding issues possible
cat file.txt
```

```ae
# AetherShell — returns Value::Str, UTF-8 guaranteed
file.read("file.txt")
```

### List directory

```bash
# Bash — raw text, column format varies by terminal width
ls -la
```

```ae
# AetherShell — always returns Array<Record> with typed fields
ls(".")
# Pipeline-friendly:
ls(".") | where(fn(f) => f.size > 1024) | select("name", "size")
```

---

## System Information

### Hostname

```bash
# Bash — varies: hostname, /etc/hostname, uname -n
hostname

# PowerShell
$env:COMPUTERNAME
```

```ae
# AetherShell — cross-platform, returns Value::Str
sys.hostname()
```

### CPU info

```bash
# Bash/Linux — parse /proc/cpuinfo (format varies)
grep "model name" /proc/cpuinfo | head -1 | cut -d: -f2

# PowerShell/Windows
(Get-CimInstance Win32_Processor).Name
```

```ae
# AetherShell — returns Record with typed fields on all platforms
sys.cpu_info()
# → {brand, cores, frequency, vendor, ...}
```

### Memory

```bash
# Bash — parse 'free' output (column widths vary!)
free -b | awk '/Mem:/ {print $3}'
```

```ae
# AetherShell — typed Record, same schema everywhere
sys.mem_info()
# → {total, used, free, available, use_percent}
```

---

## Process Management

### List processes

```bash
# Bash — parse ps output (columns shift per platform)
ps aux | awk '{print $2, $11}'
```

```ae
# AetherShell — Array<Record> with consistent schema
proc.list()
# → [{pid, name, cpu_percent, memory_bytes}, ...]
proc.list() | where(fn(p) => p.cpu_percent > 10.0) | select("pid", "name")
```

---

## Network Operations

### HTTP requests

```bash
# Bash — depends on curl/wget being installed
curl -s https://api.example.com/data | jq '.items[]'
```

```ae
# AetherShell — built-in, returns typed Value
http.get("https://api.example.com/data")
```

---

## Monitoring & Alerting

### System health check

```bash
# Bash — multi-command, platform-specific, text parsing
echo "CPU: $(top -bn1 | grep 'Cpu(s)' | awk '{print $2}')"
echo "Mem: $(free -m | awk '/Mem:/ {printf "%d/%d MB", $3, $2}')"
echo "Disk: $(df -h / | awk 'NR==2 {print $5}')"
```

```ae
# AetherShell — single command, typed output, cross-platform
monitor.health()
# → {status: "healthy", cpu_percent: 23.5, memory_percent: 64.2,
#    disk_percent: 45.0, alerts: [], timestamp: "..."}
```

### Watch CPU usage

```bash
# Bash — requires custom loop + parsing
while true; do top -bn1 | grep Cpu | awk '{print $2}'; sleep 1; done
```

```ae
monitor.watch_cpu(80.0, 5)
# → {threshold: 80.0, samples: 5, average_cpu: 34.2,
#    breaches: 0, alert: false, readings: [...]}
```

---

## AI Integration

### Query an AI model

```bash
# Bash — requires curl + API key management + JSON parsing
curl -s https://api.openai.com/v1/chat/completions \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -d '{"model":"gpt-4o-mini","messages":[{"role":"user","content":"Hello"}]}' \
  | jq '.choices[0].message.content'
```

```ae
# AetherShell — built-in, provider-agnostic
ai("Hello")
# Or specify a model:
ai("openai:gpt-4o-mini", "Hello")
```

---

## Script Migration Checklist

1. **Replace text-parsing pipelines** with typed field access
   - `awk '{print $2}'` → `.field_name`
   - `grep "pattern"` → `where(fn(x) => str.contains(x.field, "pattern"))`
   - `sort | uniq -c` → `arr.unique(data)`

2. **Replace platform conditionals** with single commands
   - Just call the builtin — it’s cross-platform

3. **Replace external tool dependencies** with builtins
   - `curl` → `http.get()` / `http.post()`
   - `jq` → `json.parse()` / `json.stringify()`
   - `top` / `htop` → `monitor.htop()` / `monitor.health()`

4. **Replace string variables** with typed values
   - `SIZE="1024"` → `size = 1024` (inferred as Int)

5. **Replace error handling** with try/catch
   - `cmd || echo "failed"` → `try { cmd() } catch e { "failed" }`

6. **Replace function definitions** with lambdas
   - `function greet() { echo "Hi $1"; }` → `greet = fn(name) => "Hi ${name}"`

---

## Getting Help

- **REPL**: Launch `ae` and experiment interactively
- **Examples**: See the `examples/` directory
- **Spec**: Read the language specification in `docs/specs/SPEC.md`

---

*AetherShell — one language, every platform, deterministic typed output.*
