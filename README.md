# Aurora Shell (ae)

*A modern, typed, memory-safe successor to Bash — blending the ergonomics of Bash/Fish, the safety of Rust, and the metaprogramming of Julia — plus built-in AI & multi-agent automation.*

---

## ✨ Features

* **Typed pipelines**: Pass structured records/tables, not just raw text.
* **Rust-grade safety**: Runtime implemented in Rust, safe I/O by default.
* **Strong types**: Hindley–Milner inference, algebraic data types (`Option`, `Result`).
* **Macros & metaprogramming**: Hygienic macros, AST quoting/splicing.
* **Async/await**: Structured concurrency and cancellation.
* **POSIX interop**: Run existing tools; auto-wraps unknown commands in `sh([...])`.
* **AI integration**: Call LLMs (`ai("summarize this")`), spawn **agents**, or run **swarms** of cooperating agents.
* **Bash compatibility mode**: Transpile `.sh` scripts → Aurora and run them directly.

---

## 🚀 Getting Started

### Install (from source)

```bash
git clone https://github.com/nervosys/AuroraShell
cd AuroraShell
cargo install --path . --bin ae
```

Now run:

```bash
ae
```

and you’re in the Aurora REPL:

```text
Aurora REPL — type Ctrl-D to exit
ae>
```

---

### Hello World

```ae
print("Hello, Aurora!")
```

---

### Pipelines

Structured values flow through `|`:

```ae
[1,2,3,4] | map fn(x) => x*x | reduce fn(a,b) => a+b 0
# → 30
```

List files and filter by type:

```ae
ls "." | where fn(r) => r.type == "file" | take 3
```

---

### Pattern Matching

```ae
let msg = Some(42)
match msg {
  None => print("no value"),
  Some(x) if x > 40 => print("big: ${x}"),
  Some(x) => print("small: ${x}")
}
```

---

### Typed HTTP

```ae
resp := http_get "https://api.github.com"
print(resp.status)
print(resp.headers."content-type")
```

---

### AI Integration

Call an LLM:

```ae
read_text "README.md" | ai "summarize in 3 bullet points"
```

Spawn an agent with tools:

```ae
export AGENT_ALLOW_CMDS=ls,git
agent "tidy repo" ["ls","!git"] 5
```

Run a swarm (multi-agent collaboration):

```ae
swarm "design architecture" ["planner","critic","writer"]
```

---

### Bash Compatibility Mode

Run old scripts seamlessly:

```bash
ae --bash script.sh
```

Or pipe Bash from stdin:

```bash
echo 'echo hello | wc -l' | ae -b
```

Transpiler turns this:

```bash
echo hello | wc -l
```

into Aurora:

```ae
echo("hello") | sh(["wc","-l"])
```

---

## 📚 Learn More

* See [`examples/`](examples/) for more scripts.
* Check `tests/` for detailed typechecker and transpiler coverage.
* Roadmap: async pipelines, module system, AI swarm strategies (Nanda-like).

---
