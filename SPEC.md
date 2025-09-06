# Æther Shell (æ) Design Spec

*A modern, typed, memory‑safe, AI-first successor to Bash bringing PowerShell‑like structured pipelines, Rust‑like safety, Julia‑like macros, Go‑like clarity, and brain-like automation. Æther is pure command-line gold.*

## TL;DR

Build and run it:

```bash
cargo build
cargo run
```

In the REPL:

```ae
print "hello, ae"
ls "." | print
http_get "https://api.github.com" | print
```

::rocket::

## 0. Elevator Pitch

Aether (`au`) is a next‑gen shell & scripting language:

* **Typed pipelines** (arrays, records, tables, not just text)
* **Memory & type safety** (runtime in Rust, HM type inference)
* **Schema‑aware data tables** with projections, joins, group/agg
* **Macros & metaprogramming** (AST quoting + procedural)
* **Async/await & structured concurrency**
* **Composable with POSIX tools** (lossless text <-> structured)
* **AI & agents as first‑class citizens** (supports open & closed source models)

## 1. Design Goals

1. **Shell ergonomics**: terse, REPL‑friendly, readable.
2. **Type‑safe data flows**: values, not byte streams.
3. **Safety defaults**: immutable vars, sandboxed I/O.
4. **Powerful metaprogramming**.
5. **Static typing**: Hindley–Milner inference + schema validation.
6. **AI integration**: `ai` and `agent` built‑ins.

## 2. Language Features

### 2.1 Pipelines

* `|` pipes typed values between functions.

```ae
ls "." | where fn(r)=>r.type=="file" | take 5
```

### 2.2 Bindings & Types

* Go‑style `:=` with inference, optional annotations:

```ae
files := ls "."          # files: Table
let mut total: Int = 0
```

### 2.3 Lambdas

```ae
[1,2,3] | map fn(x)=>x*2
```

### 2.4 Pattern Matching

```ae
match status {
  200..=299 => print "ok",
  404       => warn  "not found",
  _         => error "status ${status}"
}
```

### 2.5 Collections

* Arrays: `[1,2,3]`
* Records: `{name:"foo",size:42}`
* Tables: results of `ls`, `find`, `http_get`.

### 2.6 Projections & Joins

* **`select`** columns: `ls "." | select "name" "size"`
* **`rename`** cols: `... | rename "name" "file" "size" "bytes"`
* **`join`** tables: `a|join b "id" "id" left`

### 2.7 Grouping & Aggregation

```ae
ls "." | group "type" | agg "count"
ls "." | group "type" | agg "sum" "size"
```

### 2.8 HTTP

* **`http_get URL`** → `{url,status,headers,body}`
* **`http`** → `http METHOD URL [headers] [query] [body]`

```ae
http "GET" "https://api.github.com" {"User-Agent":"au"}
```

### 2.9 AI & Agents

* **`ai(prompt)`** → structured call (providers: `stub|openai|ollama`)
* **`agent(goal, tools, steps, dry_run)`** → LLM plan + tool execution

```ae
read_text "README.md" | ai "summarize"
agent "list big files" ["ls","!"] 3 true
```

## 3. Grammar (Excerpt)

```text
program    := stmt* ;
stmt       := let_stmt | expr ;
let_stmt   := IDENT ":=" expr | "let" ("mut")? IDENT (":" type)? "=" expr ;
expr       := lambda | match_expr | logic_or ;
lambda     := "fn(" params? ")=>" expr ;
match_expr := "match" expr "{" arm+ "}" ;
arm        := pattern ("if" expr)? "=>" expr ;
pipeline   := call ("|" call)* ;
call       := primary ("(" args? ")")? ;
```

## 4. Examples

### Arrays & Reduce

```ae
[1,2,3,4] | map fn(x)=>x*x | reduce fn(a,b)=>a+b 0
```

### Table Filtering

```ae
ls "." | where fn(r)=>r.type=="file" | select "name" "size" | sort "size" true | take 5
```

### Join

```ae
a := ls "src"
b := find "." ".rs"
a | join b "name" "name" left | take 10
```

### Group & Aggregate

```ae
ls "." | group "type" | agg "count"
```

### HTTP

```ae
resp := http_get "https://api.github.com"
resp.status
```

### AI & Agent

```ae
read_text "README.md" | ai "summarize as bullets"
agent "repo tidy" ["ls","!"] 3
```

## 5. Roadmap

* Macro hygiene & AST transforms
* Async pipelines with cancellation
* Module system with typed exports
* Exhaustiveness checks for matches
* Streaming AI tokens

**Aether Shell**: clean Bash/Fish‑like syntax, Go‑like clarity, Rust‑like safety, Julia‑like macros, and AI‑augmented workflows.
