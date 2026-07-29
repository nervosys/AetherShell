# AGENTS.md - AetherShell Agent & AI Discovery

> This file helps AI coding assistants (GitHub Copilot, Claude, ChatGPT, Cursor, Windsurf, Cody, local models) understand how to work with AetherShell.

## Identity

- **Project**: AetherShell
- **Tagline**: One language, every platform, deterministic typed output
- **Binary**: `ae`
- **Language**: Rust
- **Version**: 1.6.0
- **Repository**: https://github.com/nervosys/AetherShell
- **License**: AGPL-3.0-or-later
- **File Extension**: `.ae`

## Why AetherShell Exists

AI agents today are forced to interact with shells designed decades ago for humans — Bash on Linux, PowerShell on Windows, Zsh on macOS. Each has its own syntax, conventions, and quirks. Worse, their output is unstructured text that varies across OS versions, locales, and tool installations, creating non-deterministic results that make agent workflows fundamentally brittle. An agent that parses `df -h` output on Ubuntu will break on Alpine. A script that scrapes `Get-Process` output will fail when column widths change. Every shell command becomes a fragile integration point.

## What Is AetherShell?

AetherShell eliminates this entire class of failure. It provides a single cross-platform shell language with deterministic, typed output — every command returns structured data (Int, Float, String, Array, Record, Lambda), not raw text. An ontology built into the shell makes commands, their arguments, and their return types discoverable by AI agents without documentation scraping or prompt engineering. It has 1,100+ builtins in 106 modules, native AI agents with multi-modal support, and implements the MCP, A2A, A2UI, and NANDA agentic protocols.

## How To Use AetherShell (for AI agents)

### Option 1: Generate AetherShell Code

Write `.ae` scripts or REPL expressions:

```ae
# Typed pipeline
ls("./src") | where(fn(f) => f.size > 1024) | map(fn(f) => f.name)

# AI query
ai("Explain this code", {context: file.read("main.rs")})

# File editing (safe, cross-platform)
file.replace("config.rs", "DEBUG = false", "DEBUG = true")
```

### Option 2: Call the Agent API (HTTP)

Start: `ae agent serve` (port 3002)

```bash
# Call a builtin
curl -X POST http://localhost:3002/api/v1/execute \
  -H "Content-Type: application/json" \
  -d '{"action": "call", "builtin": "ls", "args": {"path": "."}}'

# Get schema for your AI provider
curl http://localhost:3002/api/v1/schema/openai
```

### Option 3: Python SDK + LangChain

```python
from aethershell import AetherRuntime
from aethershell.langchain import get_agent_api_tools

runtime = AetherRuntime()
result = runtime.eval('sys.hostname()')

# LangChain tools (connect to Agent API server)
tools = get_agent_api_tools("http://localhost:3002")
```

### Option 4: Migrate Existing Shell Scripts

Run Bash, Zsh, or PowerShell scripts directly — AetherShell transpiles them on the fly:

```bash
# Auto-detected by file extension
ae deploy.sh           # Bash → AetherShell
ae setup.zsh           # Zsh  → AetherShell
ae provision.ps1       # PowerShell → AetherShell

# Explicit mode flags
ae --bash -c 'ls -la | grep .rs | wc -l'
ae --zsh  -c 'setopt extended_glob; ls **/*.txt'
ae --pwsh -c 'Get-ChildItem -Recurse | Select-Object Name, Length'

# Pipe via stdin
echo 'cat /etc/hosts' | ae --bash
```

Transpilers map 100+ commands per shell to native AetherShell builtins, with block accumulation for multi-line constructs (`if`/`for`/`while`/`case`/`function`).

### Option 5: Agentic Syntax (Token-Minimized for AI)

Use the `.aeg` extension or `--agentic`/`-a` flag for a token-minimized syntax that reduces LLM token consumption by ~60-70%:

```bash
ae script.aeg                          # Auto-detected by extension
ae --agentic -c 'l"./src"|w~.size>1k|m~.name'
echo 'e"hello"' | ae --agentic
```

**Ultra-compressed syntax (v2) — maximum density:**

| Ultra (v2)            | v1 Compat      | AetherShell                        | Savings  |
| --------------------- | -------------- | ---------------------------------- | -------- |
| `x=42`                | `x=42`         | `let x = 42`                       | 2 tokens |
| `x:=0`                | `x:=0`         | `let mut x = 0`                    | 3 tokens |
| `~x:x*2`              | `\x:x*2`       | `fn(x) => x * 2`                   | 4 tokens |
| `~.size>1k`           | `\.size>1k`    | `fn(__) => __.size > 1000`         | 6 tokens |
| `a\|b`                | `a > b`        | `a \| b`                           | same     |
| `F.r("p")`            | `@f.r("p")`    | `file.read("p")`                   | 2 tokens |
| `S.h()`               | `@s.h()`       | `sys.hostname()`                   | 2 tokens |
| `H.g(url)`            | `@h.g(url)`    | `http.get(url)`                    | 2 tokens |
| `DK.p()`              | `@dk.ps()`     | `docker.ps()`                      | 2 tokens |
| `e"msg"`              | `#e "msg"`     | `echo("msg")`                      | 2 tokens |
| `l"."`                | `#l "."`       | `ls(".")`                          | 2 tokens |
| `w~.size>1k`          | `#w \.size>1k` | `where(fn(__) => __.size > 1000)`  | 8 tokens |
| `\|w.size>1k`          | —              | `\| where(fn(__) => __.size > 1000)` | +2 tokens |
| `m~x:x*2`             | `#m \x:x*2`    | `map(fn(x) => x * 2)`              | 6 tokens |
| `t5`                  | `#t 5`         | `take(5)`                          | 2 tokens |
| `1k` / `1M` / `1G`    | same           | `1000` / `1000000` / `1000000000`  | 1 token  |
| `?val{A=>"x",_=>"z"}` | same           | `match val { A => "x", _ => "z" }` | 3 tokens |
| `!{expr}{"fb"}`       | same           | `try { expr } catch e { "fb" }`    | 5 tokens |
| `; comment`           | same           | `// comment`                       | same     |

**Bare-dot implicit lambda (v3).** In pipe position, the `~` before a field
access is implied for lambda-taking builtins (`w`, `m`, `r`, `a`, `y`):

```ae
l"./src"|w.size>1k|.name        # 11 cl100k tokens
l"./src"|w~.size>1k|m~.name     # 15 cl100k tokens — same program
```

Measured at **23.7% fewer tokens on predicate pipelines**; reproduce with
`cargo run -p agentic-eval --example sigil_audit --features real-tokens`. The
short form is restricted to pipe position on purpose — at statement start
`m.name` is still a field access on a variable named `m`.

**v5 features — auto-parens and for-each:**

| v5 Syntax             | AetherShell                            | Savings   |
| --------------------- | -------------------------------------- | --------- |
| `F.r"path"`           | `file.read("path")`                    | 2 chars   |
| `H.g"https://api.io"` | `http.get("https://api.io")`           | 2 chars   |
| `*[1,2,3]~x:echo(x)`  | `([1,2,3]) \| each(fn(x) => echo(x))`  | 10 tokens |
| `*items~i:proc(i)`    | `(items) \| each(fn(i) => proc(i))`    | 8 tokens  |
| `*arr.range(5)\n:n*n` | `(arr.range(5)) \| each(fn(n) => n*n)` | 8 tokens  |

**Builtin shorthand map (single-char → builtin, used as `#x` or bare `x` — all 26 a–z assigned):**

`a`=all `b`=flatten `c`=cat `d`=debug `e`=echo `f`=find `g`=grep `h`=head `i`=first `j`=join `k`=keys `l`=ls `m`=map `n`=len `o`=sort `p`=print `q`=reverse `r`=reduce `s`=select `t`=take `u`=uniq `v`=values `w`=where `x`=sh `y`=any `z`=last

**Function abbreviation map (single-char → full name):**

`a2a`:s=send | `a2ui`:n=notify | `ansible`:p=playbook | `arr`:f=flatten,l=len,r=range,s=sort,u=unique | `asdf`:i=install,l=list | `audit`:l=log | `buildah`:b=build,i=images | `bun`:i=install,r=run | `cargo`:b=build,r=run,t=test | `container`:p=ps,r=run | `crypto`:h=hash,u=uuid | `db`:o=sqlite_open,q=sqlite_query | `deno`:c=compile,r=run | `direnv`:a=allow,s=status | `docker`:i=images,l=logs,p=ps,r=run,s=stop | `evo`:p=population | `file`:a=append,c=copy,d=delete,l=lines,m=mkdir,r=read,w=write,x=exists | `firewall`:a=allow,r=rules | `gdb`:b=bt,r=run | `gh`:c=clone,i=issue_create,p=pr_list | `glab`:i=issue_create,m=mr_list | `go`:b=build,r=run,t=test | `helm`:i=install,l=list | `http`:d=delete,g=get,p=post,u=put | `hyperv`:l=list,s=start | `iperf3`:c=client,s=server | `json`:p=parse,s=stringify | `just`:l=list,r=run | `k8s`:a=apply,d=delete,l=logs,p=pods,s=services | `math`:a=abs,p=pow,s=sqrt | `mcp`:c=call,r=resources,t=tools | `mise`:i=install,l=list | `nanda`:p=propose | `nc`:c=connect,l=listen | `net`:d=dns_lookup,p=ping | `nn`:c=create | `node`:r=run,v=version | `npm`:i=install,r=run | `objdump`:d=disasm,h=headers | `pipx`:i=install,l=list | `platform`:a=arch,g=gpus,o=os | `pnpm`:i=install,r=run | `podman`:p=ps,r=run | `poetry`:a=add,i=install | `pre_commit`:i=install,r=run | `proc`:k=kill,l=list | `rbac`:c=create | `readelf`:h=headers,s=symbols | `rl`:a=agent | `ruff`:c=check,f=format | `rustup`:l=list,u=update | `screen`:l=list,n=new | `skopeo`:c=copy,i=inspect | `sso`:i=init | `str`:j=join,l=lower,r=replace,s=split,t=trim,u=upper | `sys`:c=cpu_info,e=env,h=hostname,u=uptime | `terraform`:a=apply,p=plan | `tmux`:l=list,n=new | `trivy`:i=image,s=scan | `uv`:i=install,r=run | `valgrind`:c=callgrind,r=run | `virsh`:l=list,s=start | `vm`:l=list,s=start | `wsl`:e=exec,l=list | `yarn`:a=add,i=install | `zoxide`:a=add,q=query

**Module sigil map (uppercase abbreviation → module, used as `XX.func()` — 21 single-char, 71 two-char):**

`A`=arr `A2`=a2a `AG`=agent `AN`=ansible `AR`=arr `AS`=asdf `AU`=audit `AZ`=archive `B`=bun `BD`=buildah `BN`=bun `C`=crypto `CG`=cargo `CL`=clip `CR`=cron `CT`=container `CX`=cluster `D`=db `DE`=direnv `DK`=docker `DN`=deno `E`=evo `EV`=evo `F`=file `FS`=fs `FW`=firewall `G`=gh `GD`=gdb `GL`=glab `GO`=go `GW`=gui `H`=http `HM`=helm `HV`=hyperv `HW`=hw `I`=ai `IN`=input `IP`=iperf3 `J`=json `JU`=just `K`=k8s `M`=math `MC`=mcp `MI`=mise `N`=net `NA`=nanda `NC`=nc `NN`=nn `NO`=node `NP`=npm `OD`=objdump `P`=platform `PC`=pre_commit `PD`=podman `PK`=pkg `PM`=perm `PN`=pnpm `PO`=poetry `PR`=proc `PX`=pipx `R`=str `RB`=rbac `RE`=readelf `RF`=ruff `RL`=rl `RU`=rustup `S`=sys `SC`=screen `SH`=shell `SK`=skopeo `SS`=sso `ST`=str `SV`=svc `TF`=terraform `TV`=trivy `TX`=tmux `U`=uv `UI`=a2ui `US`=user `UV`=uv `V`=vm `VG`=valgrind `VI`=virsh `VM`=vm `W`=wsl `WB`=web `WS`=wsl `Y`=yarn `YR`=yarn `Z`=zoxide `ZO`=zoxide

**Symbol→value mapping (v3) — maximum density:**

`T`→true `N`→null `'text'`→`"text"` `` `cmd` ``→`sh("cmd")` `l./src`→`ls("./src")` `l/usr/bin`→`ls("/usr/bin")` `g*.rs`→`grep("*.rs")`

**Compactness + expandability (v4) — joint optimization:**

`|.name`→`| map(fn(__) => __.name)` `|.data.items`→field chains `|.trim()`→method calls `$HOME`→`sys.env("HOME")` `^cond{then}`→`match (cond) { true => (then), _ => null }` `^cond{then}{else}`→`match (cond) { true => (then), _ => (else) }` `%def name expansion`→user alias

## AetherShell Syntax Quick Reference

```
# Variables (inferred types)
x = 42                              # Int
s = "hello"                         # String
a = [1, 2, 3]                       # Array<Int>
r = {name: "ae", version: "1.5.0"} # Record

# Lambdas
double = fn(x) => x * 2
add = fn(a, b) => a + b

# Pipelines
[1,2,3] | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)

# Pattern matching
match score { 90..100 => "A", 80..89 => "B", _ => "F" }

# Error handling
result = try { risky() } catch e { "fallback" }

# String interpolation
msg = "Hello ${name}, you have ${count} items"

# Module calls
file.read("path")     sys.hostname()     net.ping("host")
crypto.uuid()         math.sqrt(16)      arr.range(10)
```

## Module Directory (106 modules)

| Module       | Purpose        | Example                                               |
| ------------ | -------------- | ----------------------------------------------------- |
| `file`       | File I/O       | `file.read("f.txt")`, `file.write("f.txt", data)`     |
| `sys`        | System info    | `sys.hostname()`, `sys.uptime()`, `sys.cpu_info()`    |
| `proc`       | Processes      | `proc.list()`, `proc.kill(pid)`                       |
| `net`        | Network        | `net.ping("host")`, `net.dns_lookup("host")`          |
| `http`       | HTTP client    | `http.get(url)`, `http.post(url, body)`               |
| `crypto`     | Cryptography   | `crypto.uuid()`, `crypto.hash("sha256", data)`        |
| `db`         | Database       | `db.sqlite_open("db")`, `db.sqlite_query(c, sql)`     |
| `math`       | Mathematics    | `math.sqrt(x)`, `math.pow(a, b)`                      |
| `str`        | Strings        | `str.upper(s)`, `str.split(s, ",")`                   |
| `arr`        | Arrays         | `arr.range(n)`, `arr.flatten(a)`, `arr.unique(a)`     |
| `json`       | JSON           | `json.parse(s)`, `json.stringify(v)`                  |
| `platform`   | Platform       | `platform.os()`, `platform.arch()`, `platform.gpus()` |
| `ai`         | AI queries     | `ai("prompt")`, `ai("model:name", "prompt")`          |
| `agent`      | Agents         | `agent("goal", tools)`, `swarm({...})`                |
| `mcp`        | MCP protocol   | `mcp.tools()`, `mcp.call("tool", args)`               |
| `a2a`        | Agent-to-Agent | `a2a.send("agent", msg)`                              |
| `a2ui`       | Agent-to-UI    | `a2ui.notify("msg", "type")`                          |
| `nanda`      | Consensus      | `nanda.propose("id", config)`                         |
| `rbac`       | Access control | `rbac.create("role", perms)`                          |
| `audit`      | Audit logging  | `audit.log("action", "target", meta)`                 |
| `sso`        | Single sign-on | `sso.init("provider", config)`                        |
| `nn`         | Neural nets    | `nn.create("name", layers)`                           |
| `evo`        | Evolution      | `evo.population(n, type, config)`                     |
| `rl`         | Reinforcement  | `rl.agent("type", states, actions, config)`           |
| `fs`         | Filesystem     | Filesystem operations                                 |
| `gui`        | GUI            | GUI operations                                        |
| `web`        | Web            | Web operations                                        |
| `svc`        | Services       | Service management                                    |
| `cron`       | Scheduling     | Cron/schedule operations                              |
| `archive`    | Archives       | Compression/extraction                                |
| `user`       | Users          | User management                                       |
| `perm`       | Permissions    | Permission operations                                 |
| `pkg`        | Packages       | Package management                                    |
| `hw`         | Hardware       | Hardware access                                       |
| `clip`       | Clipboard      | Clipboard operations                                  |
| `input`      | Input          | User input                                            |
| `shell`      | Shell          | Shell operations                                      |
| `cluster`    | Cluster        | Distributed computing                                 |
| `docker`     | Docker         | `docker.ps()`, `docker.run(image, args)`              |
| `podman`     | Podman         | `podman.ps()`, `podman.run(image, args)`              |
| `container`  | Containers     | `container.ps()`, `container.run(image)`              |
| `k8s`        | Kubernetes     | `k8s.pods()`, `k8s.apply(manifest)`                   |
| `helm`       | Helm           | `helm.list()`, `helm.install(chart)`                  |
| `vm`         | VMs            | `vm.list()`, `vm.start(name)`                         |
| `hyperv`     | Hyper-V        | `hyperv.list()`, `hyperv.start(name)`                 |
| `virsh`      | libvirt        | `virsh.list()`, `virsh.start(name)`                   |
| `wsl`        | WSL            | `wsl.list()`, `wsl.exec(distro, cmd)`                 |
| `terraform`  | Terraform      | `terraform.plan()`, `terraform.apply()`               |
| `ansible`    | Ansible        | `ansible.playbook(file)`                              |
| `firewall`   | Firewall       | `firewall.rules()`, `firewall.allow(port)`            |
| `tmux`       | Terminal mux   | `tmux.new(name)`, `tmux.list()`                       |
| `screen`     | Screen         | `screen.new(name)`, `screen.list()`                   |
| `valgrind`   | Memory debug   | `valgrind.run(prog)`, `valgrind.callgrind(prog)`      |
| `gdb`        | Debugger       | `gdb.run(prog)`, `gdb.bt(prog)`                       |
| `objdump`    | Binary inspect | `objdump.disasm(file)`, `objdump.headers(file)`       |
| `readelf`    | ELF inspect    | `readelf.headers(file)`, `readelf.symbols(file)`      |
| `zoxide`     | Smart cd       | `zoxide.add(path)`, `zoxide.query(term)`              |
| `just`       | Task runner    | `just.run(recipe)`, `just.list()`                     |
| `direnv`     | Dir envs       | `direnv.allow()`, `direnv.status()`                   |
| `asdf`       | Version mgr    | `asdf.install(plugin, ver)`, `asdf.list(plugin)`      |
| `mise`       | Dev tools      | `mise.install(tool, ver)`, `mise.list()`              |
| `uv`         | Python pkg     | `uv.install(pkg)`, `uv.run(script)`                   |
| `pipx`       | Python apps    | `pipx.install(pkg)`, `pipx.list()`                    |
| `poetry`     | Python deps    | `poetry.install()`, `poetry.add(pkg)`                 |
| `cargo`      | Rust pkg       | `cargo.build()`, `cargo.test()`, `cargo.run()`        |
| `rustup`     | Rust toolchain | `rustup.update()`, `rustup.list()`                    |
| `go`         | Go toolchain   | `go.build()`, `go.test()`, `go.run(file)`             |
| `node`       | Node.js        | `node.run(script)`, `node.version()`                  |
| `npm`        | npm            | `npm.install(pkg)`, `npm.run(script)`                 |
| `pnpm`       | pnpm           | `pnpm.install(pkg)`, `pnpm.run(script)`               |
| `yarn`       | Yarn           | `yarn.install()`, `yarn.add(pkg)`                     |
| `bun`        | Bun            | `bun.run(script)`, `bun.install(pkg)`                 |
| `deno`       | Deno           | `deno.run(script)`, `deno.compile(file)`              |
| `gh`         | GitHub CLI     | `gh.pr_list()`, `gh.issue_create(title)`              |
| `glab`       | GitLab CLI     | `glab.mr_list()`, `glab.issue_create(title)`          |
| `pre_commit` | Git hooks      | `pre_commit.run()`, `pre_commit.install()`            |
| `buildah`    | OCI build      | `buildah.build(file)`, `buildah.images()`             |
| `skopeo`     | Container img  | `skopeo.inspect(img)`, `skopeo.copy(src, dst)`        |
| `trivy`      | Security scan  | `trivy.scan(target)`, `trivy.image(img)`              |
| `ruff`       | Python lint    | `ruff.check(path)`, `ruff.format(path)`               |
| `iperf3`     | Network perf   | `iperf3.client(host)`, `iperf3.server()`              |
| `nc`         | Netcat         | `nc.connect(host, port)`, `nc.listen(port)`           |

## Agent API Server Endpoints

| Method | Path                     | Description               |
| ------ | ------------------------ | ------------------------- |
| `POST` | `/api/v1/execute`        | Execute any AgentRequest  |
| `POST` | `/api/v1/call/:builtin`  | Call a single builtin     |
| `POST` | `/api/v1/pipeline`       | Execute a pipeline        |
| `POST` | `/api/v1/eval`           | Evaluate AetherShell code |
| `POST` | `/api/v1/stream/execute` | Stream execution (SSE)    |
| `GET`  | `/api/v1/ws`             | WebSocket connection      |
| `GET`  | `/api/v1/schema`         | Full JSON schema          |
| `GET`  | `/api/v1/schema/:format` | Provider-specific schema  |
| `GET`  | `/api/v1/builtins`       | List builtins             |
| `GET`  | `/api/v1/builtins/:name` | Describe a builtin        |
| `GET`  | `/api/v1/types`          | Type information          |
| `GET`  | `/health`                | Health check              |

Schema formats: `openai`, `claude`, `gemini`, `llama`, `mistral`, `cohere`, `grok`, `deepseek`, `bedrock`, `azure_openai`, `qwen`, `ollama`, `lmstudio`, `vllm`, `huggingface`, `openrouter`, `together`, `groq`, `fireworks`, `ontology`

## Development

```bash
cargo build --bins          # Build ae + aimodel
cargo test                  # Run 1,237 tests
cargo run -- --tui          # TUI mode
cargo run -- -c 'expr'      # Quick eval
```

### Key Source Files

| File               | Purpose                                            |
| ------------------ | -------------------------------------------------- |
| `src/main.rs`      | CLI entry point                                    |
| `src/ast.rs`       | AST definitions                                    |
| `src/parser.rs`    | Parser                                             |
| `src/eval.rs`      | Evaluator                                          |
| `src/value.rs`     | Value types                                        |
| `src/builtins.rs`  | 1,100+ builtins                                    |
| `src/ai.rs`        | AI provider router                                 |
| `src/agent.rs`     | Agent framework                                    |
| `src/agent_api.rs` | Agent API + HTTP server                            |
| `src/typecheck.rs` | Hindley-Milner inference                           |
| `src/transpile/`   | Shell transpilers (Bash, Zsh, PowerShell, Agentic) |
| `src/tui/`         | Terminal UI                                        |

## Context Files

| File                              | Purpose                              |
| --------------------------------- | ------------------------------------ |
| `llms.txt`                        | Short AI context (llms.txt standard) |
| `llms-full.txt`                   | Complete AI context                  |
| `AGENTS.md`                       | This file - agent discovery          |
| `.github/copilot-instructions.md` | GitHub Copilot instructions          |
| `.well-known/ai-plugin.json`      | OpenAI plugin manifest               |
| `.well-known/openapi.yaml`        | OpenAPI 3.1 spec for Agent API       |
| `docs/specs/SPEC.md`              | Language specification               |
| `ROADMAP.md`                      | Development roadmap                  |