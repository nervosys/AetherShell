//! Agentic Syntax → Aether Shell transpiler.
//!
//! A token-minimized syntax mode designed for AI agents. Every construct is
//! compressed to the fewest possible tokens so that LLM context windows and
//! output budgets are used efficiently.
//!
//! # Design Principles
//!
//! 1. **Single-character operators** replace verbose keywords.
//! 2. **Bare builtins** — single lowercase letter immediately before an arg
//!    (e.g., `e"msg"`, `w~.size>1k`). No `#` prefix needed.
//! 3. **Bare modules** — uppercase sigil + dot (e.g., `F.r"path"`, `H.g(url)`).
//!    No `@` prefix needed.
//! 4. **Function abbreviations** — single-char func names auto-expand
//!    (e.g., `file.r(…)` → `file.read(…)`).
//! 5. **`|` pipe** — native AetherShell pipe, no transformation needed.
//! 6. **`~` lambda prefix** — alias for `\`, avoids escape-heavy contexts.
//! 7. **Implicit lambdas** — `~.field` = `fn(__) => __.field`.
//! 8. **Symbol→value mapping** — `T`→true, `N`→null, `'`→`"`, `` ` ``→sh().
//! 9. **Bare path args** — `l./src` auto-quotes to `ls("./src")`.
//! 10. **`|.field` projection** — `|.name` = `| map(fn(__) => __.name)`.
//! 11. **`$VAR` env access** — `$HOME` → `sys.env("HOME")`.
//! 12. **`^cond{then}{else}` conditional** — compact if/else.
//! 13. **`%def` preamble** — user-defined aliases for extensibility.
//! 14. **Backward compatible** — v1 forms (`#x`, `@xx.`, `>` pipe) still work.
//!
//! # Ultra-Compressed Syntax (v2) — Quick Reference
//!
//! ```text
//! ── Pipelines ───────────────────────────────────────────────────────
//! expr|func                     → expr | func                (native pipe)
//! expr > func                   → expr | func                (v1 compat)
//!
//! ── Bare Builtins (no # prefix) ─────────────────────────────────────
//! e"msg"                        → echo("msg")
//! l"."                          → ls(".")
//! w~.size>1k                    → where(fn(__) => __.size > 1000)
//! m~x:x*2                      → map(fn(x) => x * 2)
//! t5                            → take(5)
//! o                             → sort()          (zero-arg after |)
//!
//! ── Bare Modules (no @ prefix) ──────────────────────────────────────
//! F.r("p")                      → file.read("p")
//! F.r"p"                        → file.read("p")  (auto-parens)
//! S.h()                         → sys.hostname()
//! H.g(url)                      → http.get(url)
//! J.p(s)                        → json.parse(s)
//! DK.p()                        → docker.ps()
//! K.p()                         → k8s.pods()
//!
//! ── Function Abbreviations (single-char) ────────────────────────────
//! file.r(…)                     → file.read(…)
//! file.w(…)                     → file.write(…)
//! http.g(…)                     → http.get(…)
//! http.p(…)                     → http.post(…)
//! json.p(…)                     → json.parse(…)
//! json.s(…)                     → json.stringify(…)
//! sys.h()                       → sys.hostname()
//! crypto.u()                    → crypto.uuid()
//!
//! ── Lambdas ─────────────────────────────────────────────────────────
//! ~x:x*2                        → fn(x) => x * 2    (~ prefix)
//! \x:x*2                        → fn(x) => x * 2    (\ prefix, v1)
//! ~x,y:x+y                      → fn(x, y) => x + y
//! ~.size>1k                     → fn(__) => __.size > 1000
//! \.size>1k                     → fn(__) => __.size > 1000
//!
//! ── Assignment ──────────────────────────────────────────────────────
//! x=42                          → let x = 42
//! x:=expr                       → let mut x = expr
//!
//! ── Control Flow ────────────────────────────────────────────────────
//! ?val{A=>"x",B=>"y",_=>"z"}   → match val { A => "x", B => "y", _ => "z" }
//! !{expr}{"fallback"}           → try { expr } catch e { "fallback" }
//!
//! ── v1 Compat (still supported) ─────────────────────────────────────
//! #e "msg"                      → echo("msg")
//! @f.r("p")                     → file.read("p")
//! expr > func                   → expr | func
//!
//! ── Literals ────────────────────────────────────────────────────────
//! 1k  1M  1G                    → 1000, 1000000, 1000000000
//! [1,2,3]                       → [1, 2, 3]               (same)
//! {k:"v"}                       → {k: "v"}                (same)
//!
//! ── Symbol→Value Mapping (v3) ──────────────────────────────────────
//! T                             → true                     (standalone)
//! N                             → null                     (standalone)
//! 'text'                        → "text"                   (single-quote strings)
//! `cmd`                         → sh("cmd")                (backtick exec)
//!
//! ── Bare Path Auto-Quoting (v3) ────────────────────────────────────
//! l./src                        → ls("./src")
//! l../config                    → ls("../config")
//! l/usr/bin                     → ls("/usr/bin")
//! g*.rs                         → grep("*.rs")
//! l.                            → ls(".")                  (lone dot)
//!
//! ── Field Projection (v4) ──────────────────────────────────────────
//! |.name                        → | map(fn(__) => __.name)
//! |.data.items                  → | map(fn(__) => __.data.items)
//! |.trim()                      → | map(fn(__) => __.trim())
//!
//! ── Env Var Access (v4) ────────────────────────────────────────────
//! $HOME                         → sys.env("HOME")
//! $PATH                         → sys.env("PATH")
//! $MY_VAR                       → sys.env("MY_VAR")
//!
//! ── Conditional (v4) ───────────────────────────────────────────────
//! ^x>0{x*2}                     → match (x>0) { true => (x*2), _ => null }
//! ^x>0{x*2}{0}                  → match (x>0) { true => (x*2), _ => (0) }
//!
//! ── For-Each Loop (v5) ─────────────────────────────────────────────
//! *[1,2,3]~x:echo(x)             → ([1, 2, 3]) | each(fn(x) => echo(x))
//! *items~item:proc(item)          → (items) | each(fn(item) => proc(item))
//!
//! ── Preamble Directives (v4) ───────────────────────────────────────
//! %def fetch H.g                → defines alias: fetch → H.g
//! %def parse J.p                → defines alias: parse → J.p
//!
//! ── Comments ────────────────────────────────────────────────────────
//! ; comment                     → // comment
//! ```
//!
//! # Example (v2 ultra-compressed)
//!
//! ```text
//! l"./src"|w~.size>1k|m~.name
//! F.r("README.md")
//! H.g("https://api.com/data")|J.p(resp)|m~.items
//! ```
//!
//! # Example (v3 maximum density)
//!
//! ```text
//! l./src|w~.size>1k|m~.name
//! x=`uname -a`
//! active=T
//! data=N
//! e'hello world'
//! ```
//!
//! # Example (v4 maximum compactness + extensibility)
//!
//! ```text
//! %def api H.g("https://api.example.com")
//! %def parse J.p
//! api|parse(_)|.data.items|w~.active|.name
//! home=$HOME
//! ^n>0{n*2}{0}
//! ```
//!
//! # File Extension
//!
//! `.aeg` (agentic)

use anyhow::Result;
use std::collections::HashMap;

lazy_static::lazy_static! {
    /// Module sigil → full module name mapping
    static ref MODULE_MAP: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // Core modules (single letter where unambiguous)
        m.insert("f",  "file");
        m.insert("s",  "sys");
        m.insert("n",  "net");
        m.insert("h",  "http");
        m.insert("j",  "json");
        m.insert("c",  "crypto");
        m.insert("d",  "db");
        m.insert("m",  "math");
        m.insert("st", "str");
        m.insert("r",  "str");   // R.func() alias — stRing
        m.insert("ar", "arr");
        m.insert("a",  "arr");   // A.func() alias — Array
        m.insert("p",  "platform");
        m.insert("ai", "ai");
        m.insert("i",  "ai");    // I.func() alias — aI/Intelligence
        m.insert("ag", "agent");
        m.insert("mc", "mcp");
        m.insert("pr", "proc");
        m.insert("fs", "fs");
        m.insert("gw", "gui");
        m.insert("wb", "web");
        m.insert("sv", "svc");
        m.insert("cr", "cron");
        m.insert("az", "archive");
        m.insert("us", "user");
        m.insert("pm", "perm");
        m.insert("pk", "pkg");
        m.insert("hw", "hw");
        m.insert("cl", "clip");
        m.insert("in", "input");
        m.insert("sh", "shell");
        m.insert("cx", "cluster");
        m.insert("dk", "docker");
        m.insert("pd", "podman");
        m.insert("ct", "container");
        m.insert("k",  "k8s");
        m.insert("hm", "helm");
        m.insert("vm", "vm");
        m.insert("hv", "hyperv");
        m.insert("vi", "virsh");
        m.insert("v",  "vm");    // V.func() alias — VM
        m.insert("ws", "wsl");
        m.insert("w",  "wsl");   // W.func() alias — WSL
        m.insert("tf", "terraform");
        m.insert("an", "ansible");
        m.insert("fw", "firewall");
        m.insert("tx", "tmux");
        m.insert("sc", "screen");
        m.insert("vg", "valgrind");
        m.insert("gd", "gdb");
        m.insert("od", "objdump");
        m.insert("re", "readelf");
        m.insert("zo", "zoxide");
        m.insert("z",  "zoxide"); // Z.func() alias — Zoxide
        m.insert("ju", "just");
        m.insert("de", "direnv");
        m.insert("as", "asdf");
        m.insert("mi", "mise");
        m.insert("uv", "uv");
        m.insert("u",  "uv");    // U.func() alias — UV
        m.insert("px", "pipx");
        m.insert("po", "poetry");
        m.insert("cg", "cargo");
        m.insert("ru", "rustup");
        m.insert("go", "go");
        m.insert("no", "node");
        m.insert("np", "npm");
        m.insert("pn", "pnpm");
        m.insert("yr", "yarn");
        m.insert("y",  "yarn");  // Y.func() alias — Yarn
        m.insert("bn", "bun");
        m.insert("b",  "bun");   // B.func() alias — Bun
        m.insert("dn", "deno");
        m.insert("g",  "gh");
        m.insert("gl", "glab");
        m.insert("pc", "pre_commit");
        m.insert("bd", "buildah");
        m.insert("sk", "skopeo");
        m.insert("tv", "trivy");
        m.insert("rf", "ruff");
        m.insert("ip", "iperf3");
        m.insert("nc", "nc");
        m.insert("nn", "nn");
        m.insert("ev", "evo");
        m.insert("e",  "evo");   // E.func() alias — Evo
        m.insert("rl", "rl");
        m.insert("rb", "rbac");
        m.insert("au", "audit");
        m.insert("ss", "sso");
        m.insert("a2", "a2a");
        m.insert("ui", "a2ui");
        m.insert("na", "nanda");
        m
    };

    /// Builtin shorthand # codes → (builtin_name, takes_lambda_arg)
    static ref BUILTIN_SHORT: HashMap<char, (&'static str, bool)> = {
        let mut m = HashMap::new();
        m.insert('e', ("echo",   false));  // #e "msg"
        m.insert('l', ("ls",     false));  // #l "."
        m.insert('w', ("where",  true));   // #w \x:x>0
        m.insert('m', ("map",    true));   // #m \x:x*2
        m.insert('r', ("reduce", true));   // #r \a,b:a+b 0
        m.insert('t', ("take",   false));  // #t 5
        m.insert('s', ("select", false));  // #s "name"
        m.insert('g', ("grep",   false));  // #g "pattern"
        m.insert('c', ("cat",    false));  // #c "file"
        m.insert('x', ("sh",     false));  // #x "cmd"
        m.insert('o', ("sort",   false));  // #o
        m.insert('u', ("uniq",   false));  // #u
        m.insert('h', ("head",   false));  // #h 10
        m.insert('k', ("keys",   false));  // #k
        m.insert('v', ("values", false));  // #v
        m.insert('n', ("len",    false));  // #n
        m.insert('f', ("find",   false));  // #f "."
        m.insert('j', ("join",   false));  // #j ","
        m.insert('p', ("print",  false));  // #p "msg"
        m.insert('a', ("all",    true));   // #a \x:x>0
        m.insert('y', ("any",    true));   // #y \x:x>0
        m.insert('d', ("debug",  false));  // #d expr
        m.insert('i', ("first",  false));  // #i
        m.insert('z', ("last",   false));  // #z
        m.insert('b', ("flatten",false));  // #b — flatten nested arrays
        m.insert('q', ("reverse",false));  // #q — reverse array order
        m
    };

    /// Module function abbreviations: "module.X" → "module.full_name"
    /// Only matches single-character function names to avoid clobbering
    /// real function names like `file.read` (the `r` must appear alone: `file.r`).
    static ref FUNC_ABBREV: HashMap<&'static str, &'static str> = {
        let mut m = HashMap::new();
        // file
        m.insert("file.r", "file.read");
        m.insert("file.w", "file.write");
        m.insert("file.a", "file.append");
        m.insert("file.x", "file.exists");
        m.insert("file.d", "file.delete");
        m.insert("file.m", "file.mkdir");
        m.insert("file.l", "file.lines");
        m.insert("file.c", "file.copy");
        // sys
        m.insert("sys.h", "sys.hostname");
        m.insert("sys.u", "sys.uptime");
        m.insert("sys.c", "sys.cpu_info");
        m.insert("sys.e", "sys.env");
        // http
        m.insert("http.g", "http.get");
        m.insert("http.p", "http.post");
        m.insert("http.u", "http.put");
        m.insert("http.d", "http.delete");
        // json
        m.insert("json.p", "json.parse");
        m.insert("json.s", "json.stringify");
        // crypto
        m.insert("crypto.u", "crypto.uuid");
        m.insert("crypto.h", "crypto.hash");
        // db
        m.insert("db.o", "db.sqlite_open");
        m.insert("db.q", "db.sqlite_query");
        // math
        m.insert("math.s", "math.sqrt");
        m.insert("math.p", "math.pow");
        m.insert("math.a", "math.abs");
        // str
        m.insert("str.u", "str.upper");
        m.insert("str.l", "str.lower");
        m.insert("str.s", "str.split");
        m.insert("str.r", "str.replace");
        m.insert("str.j", "str.join");
        m.insert("str.t", "str.trim");
        // arr
        m.insert("arr.r", "arr.range");
        m.insert("arr.f", "arr.flatten");
        m.insert("arr.u", "arr.unique");
        m.insert("arr.s", "arr.sort");
        m.insert("arr.l", "arr.len");
        // proc
        m.insert("proc.l", "proc.list");
        m.insert("proc.k", "proc.kill");
        // net
        m.insert("net.p", "net.ping");
        m.insert("net.d", "net.dns_lookup");
        // docker
        m.insert("docker.p", "docker.ps");
        m.insert("docker.r", "docker.run");
        m.insert("docker.s", "docker.stop");
        m.insert("docker.l", "docker.logs");
        m.insert("docker.i", "docker.images");
        // k8s
        m.insert("k8s.p", "k8s.pods");
        m.insert("k8s.a", "k8s.apply");
        m.insert("k8s.d", "k8s.delete");
        m.insert("k8s.l", "k8s.logs");
        m.insert("k8s.s", "k8s.services");
        // mcp
        m.insert("mcp.t", "mcp.tools");
        m.insert("mcp.c", "mcp.call");
        m.insert("mcp.r", "mcp.resources");
        // platform
        m.insert("platform.o", "platform.os");
        m.insert("platform.a", "platform.arch");
        m.insert("platform.g", "platform.gpus");
        // gh
        m.insert("gh.p", "gh.pr_list");
        m.insert("gh.i", "gh.issue_create");
        m.insert("gh.c", "gh.clone");
        // cargo
        m.insert("cargo.b", "cargo.build");
        m.insert("cargo.t", "cargo.test");
        m.insert("cargo.r", "cargo.run");
        // a2a
        m.insert("a2a.s", "a2a.send");
        // a2ui
        m.insert("a2ui.n", "a2ui.notify");
        // nanda
        m.insert("nanda.p", "nanda.propose");
        // rbac
        m.insert("rbac.c", "rbac.create");
        // audit
        m.insert("audit.l", "audit.log");
        // sso
        m.insert("sso.i", "sso.init");
        // nn
        m.insert("nn.c", "nn.create");
        // evo
        m.insert("evo.p", "evo.population");
        // rl
        m.insert("rl.a", "rl.agent");
        // helm
        m.insert("helm.l", "helm.list");
        m.insert("helm.i", "helm.install");
        // vm
        m.insert("vm.l", "vm.list");
        m.insert("vm.s", "vm.start");
        // hyperv
        m.insert("hyperv.l", "hyperv.list");
        m.insert("hyperv.s", "hyperv.start");
        // virsh
        m.insert("virsh.l", "virsh.list");
        m.insert("virsh.s", "virsh.start");
        // wsl
        m.insert("wsl.l", "wsl.list");
        m.insert("wsl.e", "wsl.exec");
        // terraform
        m.insert("terraform.p", "terraform.plan");
        m.insert("terraform.a", "terraform.apply");
        // ansible
        m.insert("ansible.p", "ansible.playbook");
        // firewall
        m.insert("firewall.r", "firewall.rules");
        m.insert("firewall.a", "firewall.allow");
        // tmux
        m.insert("tmux.n", "tmux.new");
        m.insert("tmux.l", "tmux.list");
        // screen
        m.insert("screen.n", "screen.new");
        m.insert("screen.l", "screen.list");
        // valgrind
        m.insert("valgrind.r", "valgrind.run");
        m.insert("valgrind.c", "valgrind.callgrind");
        // gdb
        m.insert("gdb.r", "gdb.run");
        m.insert("gdb.b", "gdb.bt");
        // objdump
        m.insert("objdump.d", "objdump.disasm");
        m.insert("objdump.h", "objdump.headers");
        // readelf
        m.insert("readelf.h", "readelf.headers");
        m.insert("readelf.s", "readelf.symbols");
        // zoxide
        m.insert("zoxide.a", "zoxide.add");
        m.insert("zoxide.q", "zoxide.query");
        // just
        m.insert("just.r", "just.run");
        m.insert("just.l", "just.list");
        // direnv
        m.insert("direnv.a", "direnv.allow");
        m.insert("direnv.s", "direnv.status");
        // asdf
        m.insert("asdf.i", "asdf.install");
        m.insert("asdf.l", "asdf.list");
        // mise
        m.insert("mise.i", "mise.install");
        m.insert("mise.l", "mise.list");
        // uv
        m.insert("uv.i", "uv.install");
        m.insert("uv.r", "uv.run");
        // pipx
        m.insert("pipx.i", "pipx.install");
        m.insert("pipx.l", "pipx.list");
        // poetry
        m.insert("poetry.i", "poetry.install");
        m.insert("poetry.a", "poetry.add");
        // rustup
        m.insert("rustup.u", "rustup.update");
        m.insert("rustup.l", "rustup.list");
        // go
        m.insert("go.b", "go.build");
        m.insert("go.t", "go.test");
        m.insert("go.r", "go.run");
        // node
        m.insert("node.r", "node.run");
        m.insert("node.v", "node.version");
        // npm
        m.insert("npm.i", "npm.install");
        m.insert("npm.r", "npm.run");
        // pnpm
        m.insert("pnpm.i", "pnpm.install");
        m.insert("pnpm.r", "pnpm.run");
        // yarn
        m.insert("yarn.i", "yarn.install");
        m.insert("yarn.a", "yarn.add");
        // bun
        m.insert("bun.r", "bun.run");
        m.insert("bun.i", "bun.install");
        // deno
        m.insert("deno.r", "deno.run");
        m.insert("deno.c", "deno.compile");
        // glab
        m.insert("glab.m", "glab.mr_list");
        m.insert("glab.i", "glab.issue_create");
        // pre_commit
        m.insert("pre_commit.r", "pre_commit.run");
        m.insert("pre_commit.i", "pre_commit.install");
        // buildah
        m.insert("buildah.b", "buildah.build");
        m.insert("buildah.i", "buildah.images");
        // skopeo
        m.insert("skopeo.i", "skopeo.inspect");
        m.insert("skopeo.c", "skopeo.copy");
        // trivy
        m.insert("trivy.s", "trivy.scan");
        m.insert("trivy.i", "trivy.image");
        // ruff
        m.insert("ruff.c", "ruff.check");
        m.insert("ruff.f", "ruff.format");
        // iperf3
        m.insert("iperf3.c", "iperf3.client");
        m.insert("iperf3.s", "iperf3.server");
        // nc
        m.insert("nc.c", "nc.connect");
        m.insert("nc.l", "nc.listen");
        // container
        m.insert("container.p", "container.ps");
        m.insert("container.r", "container.run");
        // podman
        m.insert("podman.p", "podman.ps");
        m.insert("podman.r", "podman.run");
        m
    };
}

// ═══════════════════════════════════════════════════════════════════════
// ONTOLOGY — Formal, exhaustive specification of every transformation
// ═══════════════════════════════════════════════════════════════════════

/// A single transformation rule in the agentic syntax ontology.
#[derive(Debug, Clone)]
pub struct OntologyRule {
    /// Compressed input pattern (may use `{...}` as placeholder)
    pub pattern: &'static str,
    /// Expanded AetherShell output
    pub expansion: &'static str,
    /// Version when this rule was introduced (1–4)
    pub version: u8,
    /// Constraints under which the rule fires
    pub constraints: &'static str,
    /// Concrete example: `(input, expected_output)`
    pub example: (&'static str, &'static str),
}

/// A category of transformation rules, corresponding to one pipeline stage.
#[derive(Debug, Clone)]
pub struct OntologyCategory {
    /// Category name
    pub name: &'static str,
    /// Pipeline stage order (0 = runs first, 11 = runs last)
    pub stage: u8,
    /// Human-readable description
    pub description: &'static str,
    /// Rules in this category
    pub rules: &'static [OntologyRule],
}

/// Reserved characters and their meaning in agentic syntax.
///
/// Each character has a SINGLE unambiguous interpretation. This table
/// defines the complete set — no character has two meanings at the same
/// syntactic level.
pub const RESERVED_CHARS: &[(char, &str, &str)] = &[
    // (char, meaning, conflict resolution)
    (
        '|',
        "Pipeline operator / field projection prefix",
        "At depth 0 only; |. triggers projection",
    ),
    (
        '>',
        "v1 pipeline (space-delimited) / comparison (bare)",
        "` > ` = pipe; `>` = comparison; `>=` `>>` `=>` preserved",
    ),
    ('^', "Conditional prefix", "Only at line start"),
    ('?', "Match prefix", "Only at line start"),
    (
        '!',
        "Try/catch prefix",
        "Only at line start followed by `{`",
    ),
    (
        '~',
        "Lambda prefix (alias for \\)",
        "Not after alphanumeric (prevents bitwise NOT conflict)",
    ),
    (
        '\\',
        "Lambda prefix (v1)",
        "Followed by params:body or .field",
    ),
    (
        '$',
        "Env var access",
        "Followed by alpha/underscore; not ${...} (string interpolation)",
    ),
    (
        '#',
        "v1 builtin shorthand prefix",
        "Followed by single letter from BUILTIN_SHORT",
    ),
    (
        '@',
        "v1 module sigil prefix",
        "Followed by module abbreviation + `.` or `(`",
    ),
    (
        '%',
        "Preamble directive prefix",
        "Only `%def` at line start",
    ),
    (
        ';',
        "Comment (full-line or inline)",
        "Outside string/backtick literals",
    ),
    (
        '=',
        "Assignment (x=v → let x = v)",
        "Only `=` not `==` `>=` `<=` `!=` `=>`; only for simple identifiers",
    ),
    (
        ':',
        "Mutable assignment (x:=v) / lambda body separator",
        "`:=` at line level; `:` inside lambda params",
    ),
    (
        '\'',
        "Single-quote string (→ double-quote)",
        "Converted in expand_symbols; embedded \" escaped",
    ),
    (
        '`',
        "Backtick exec (→ sh(\"...\"))",
        "Content passed through verbatim including $",
    ),
    (
        'T',
        "true literal (standalone)",
        "Not in identifier, not before `.`",
    ),
    (
        'N',
        "null literal (standalone)",
        "Not in identifier, not before `.`",
    ),
];

/// Conflict resolution rules — the ordering guarantees that determine
/// which transformation wins when syntax could be ambiguous.
///
/// These are invariants the transpiler MUST maintain.
pub const CONFLICT_RULES: &[&str] = &[
    // String safety
    "R01: String literals (\"...\") are NEVER transformed by any pass. Every pass skips them.",
    "R02: Single-quote content is converted to double-quote with embedded \" escaped to \\\".",
    "R03: Backtick content is passed through verbatim — $ inside `` ` `` is NOT expanded.",
    "R04: Inline comments (;) are stripped BEFORE any transformation pass runs.",
    // Identifier safety
    "R05: T/N only expand when standalone: not preceded/followed by alphanumeric/underscore, not before `.`.",
    "R06: Bare builtins (e, l, w, ...) only match when preceded by non-alphanumeric (prevents `let` → `#l et`). All 26 a-z assigned.",
    "R07: Bare modules (F., DK., ...) only match when preceded by non-alphanumeric and followed by `.`. 21 single-char + 71 two-char = 92 entries.",
    "R08: $ only expands when followed by alpha/underscore — ${...} (interpolation) passes through.",
    // Operator disambiguation
    "R09: `>` → pipe ONLY when space-delimited (` > `). Bare `>` is always comparison.",
    "R10: `>=` `>>` `=>` are NEVER converted to pipe, regardless of spacing.",
    "R11: `~` is lambda ONLY when NOT preceded by alphanumeric (prevents bitwise NOT false positive).",
    "R12: `=` is assignment ONLY for simple identifiers at line start, NOT `==` `!=` `<=` `>=` `=>`.",
    // Pipeline stage ordering
    "R13: Preprocessing (bare→sigil) runs BEFORE symbol expansion (T/N/$) BEFORE SI suffixes.",
    "R14: Lambda expansion runs BEFORE builtin expansion (so ~.x inside #w is expanded first).",
    "R15: Pipeline normalization runs BEFORE assignment (so `a > b` is pipe, not assignment RHS).",
    "R16: Conditional/match/try-catch run LAST (they operate on the fully-expanded line).",
    // Preamble
    "R17: %def aliases are textual replacements applied BEFORE any transpilation pass.",
    "R18: %def respects word boundaries and skips string literals.",
    // SI suffixes
    "R19: SI suffixes (k/M/G) only match after digits and NOT followed by alphanumerics (1key stays).",
    // For-each / auto-parens
    "R20: For-each `*` only fires at line start (prevents multiplication false positives).",
    "R21: Auto-parens only fires when a module.func is immediately followed by a string literal without parens.",
];

/// The complete ontology of agentic syntax transformations.
///
/// Each category corresponds to one stage in the transpilation pipeline.
/// The `stage` field indicates execution order (0 runs first).
/// Every rule includes a concrete example that is validated by tests.
pub const ONTOLOGY: &[OntologyCategory] = &[
    // ── Stage 0: Preprocessing (v2 ultra → v1 forms) ─────────────────
    OntologyCategory {
        name: "Preprocessing",
        stage: 0,
        description: "Convert v2 bare builtins/modules into v1 sigil forms (#x, @xx.)",
        rules: &[
            OntologyRule {
                pattern: r#"e"msg""#,
                expansion: r#"#e "msg""#,
                version: 2,
                constraints: "Lowercase letter in BUILTIN_SHORT + arg-start char; not inside identifier",
                example: (r#"e"hello""#, "echo(\"hello\")"),
            },
            OntologyRule {
                pattern: "w~.field",
                expansion: "#w ~.field",
                version: 2,
                constraints: "Builtin letter + tilde lambda",
                example: ("w~.size>1k", "where(fn(__) => __.size>1k)"),
            },
            OntologyRule {
                pattern: "t5",
                expansion: "#t 5",
                version: 2,
                constraints: "Builtin letter + digit",
                example: ("t5", "take(5)"),
            },
            OntologyRule {
                pattern: "l./path",
                expansion: r#"#l "./path""#,
                version: 3,
                constraints: "Builtin letter + dot-slash or slash (auto-quoting)",
                example: ("l./src", "ls(\"./src\")"),
            },
            OntologyRule {
                pattern: "g*.ext",
                expansion: r#"#g "*.ext""#,
                version: 3,
                constraints: "Builtin letter + glob; NOT if */digit (math)",
                example: ("g*.rs", "grep(\"*.rs\")"),
            },
            OntologyRule {
                pattern: "e$VAR",
                expansion: "#e $VAR",
                version: 4,
                constraints: "Builtin letter + $ (env var trigger)",
                example: ("e$USER", "echo(sys.env(\"USER\"))"),
            },
            OntologyRule {
                pattern: "XX.func()",
                expansion: "@xx.func()",
                version: 2,
                constraints: "Uppercase letters + dot → module sigil; must be in MODULE_MAP",
                example: ("F.read(\"p\")", "file.read(\"p\")"),
            },
        ],
    },
    // ── Stage 0.3: For-each loop ─────────────────────────────────────
    OntologyCategory {
        name: "For-Each Loop",
        stage: 0,
        description: "Expand for-each shorthand: *items~x:body → for x in items { body }. Runs before lambda expansion (R20)",
        rules: &[
            OntologyRule {
                pattern: "*items~x:body",
                expansion: "(items) | each(fn(x) => body)",
                version: 5,
                constraints: "* at line start only (R20); supports ~ and \\ lambda syntax",
                example: ("*[1,2,3]~x:echo(x)", "([1,2,3]) | each(fn(x) => echo(x))"),
            },
        ],
    },
    // ── Stage 0.5: Symbol expansion ──────────────────────────────────
    OntologyCategory {
        name: "Symbols",
        stage: 1,
        description: "Expand ASCII shortcuts to full values: T→true, N→null, '→\", `→sh(), $→sys.env()",
        rules: &[
            OntologyRule {
                pattern: "T",
                expansion: "true",
                version: 3,
                constraints: "Standalone only (R05)",
                example: ("x=T", "let x = true"),
            },
            OntologyRule {
                pattern: "N",
                expansion: "null",
                version: 3,
                constraints: "Standalone only (R05)",
                example: ("x=N", "let x = null"),
            },
            OntologyRule {
                pattern: "'text'",
                expansion: r#""text""#,
                version: 3,
                constraints: "Embedded \" escaped to \\\" (R02)",
                example: ("e'hello'", "echo(\"hello\")"),
            },
            OntologyRule {
                pattern: "`cmd`",
                expansion: "sh(\"cmd\")",
                version: 3,
                constraints: "Content verbatim, $ NOT expanded inside (R03)",
                example: ("`uname -a`", "sh(\"uname -a\")"),
            },
            OntologyRule {
                pattern: "$VAR",
                expansion: "sys.env(\"VAR\")",
                version: 4,
                constraints: "$ + alpha/underscore; not ${...} interpolation (R08)",
                example: ("$HOME", "sys.env(\"HOME\")"),
            },
        ],
    },
    // ── Stage 1: SI suffixes ─────────────────────────────────────────
    OntologyCategory {
        name: "SI Suffixes",
        stage: 2,
        description: "SI multiplier suffixes on integer literals (1k/1M/1G). Now \
                      recognized natively by the grammar lexer; the transpiler \
                      passes them through unchanged (the expansion pass is retired).",
        rules: &[
            OntologyRule {
                pattern: "{n}k",
                expansion: "{n}k (grammar lexer scales to {n}*1000)",
                version: 1,
                constraints: "Digit(s) + k/K; not followed by alphanumeric (R19); grammar-native",
                example: ("x=1k", "let x = 1k"),
            },
            OntologyRule {
                pattern: "{n}M",
                expansion: "{n}M (grammar lexer scales to {n}*1000000)",
                version: 1,
                constraints: "Digit(s) + M; not followed by alphanumeric (R19); grammar-native",
                example: ("x=5M", "let x = 5M"),
            },
            OntologyRule {
                pattern: "{n}G",
                expansion: "{n}G (grammar lexer scales to {n}*1000000000)",
                version: 1,
                constraints: "Digit(s) + G; not followed by alphanumeric (R19); grammar-native",
                example: ("x=2G", "let x = 2G"),
            },
        ],
    },
    // ── Stage 2: Lambda expansion ────────────────────────────────────
    OntologyCategory {
        name: "Lambdas",
        stage: 3,
        description: "Expand terse lambda syntax to fn() => form",
        rules: &[
            OntologyRule {
                pattern: r"\x:body",
                expansion: "fn(x) => body",
                version: 1,
                constraints: "Backslash + params + colon + body",
                example: (r"\x:x*2", "fn(x) => x*2"),
            },
            OntologyRule {
                pattern: "~x:body",
                expansion: "fn(x) => body",
                version: 2,
                constraints: "Tilde + params + colon + body; ~ not after alphanumeric (R11)",
                example: ("~x:x*2", "fn(x) => x*2"),
            },
            OntologyRule {
                pattern: r"\x,y:body",
                expansion: "fn(x, y) => body",
                version: 1,
                constraints: "Multi-param with comma separator",
                example: (r"\x,y:x+y", "fn(x, y) => x+y"),
            },
            OntologyRule {
                pattern: r"\.field",
                expansion: "fn(__) => __.field",
                version: 1,
                constraints: "Implicit parameter: dot-prefixed body gets __ prepended",
                example: (r"\.size>100", "fn(__) => __.size>100"),
            },
            OntologyRule {
                pattern: "~.field",
                expansion: "fn(__) => __.field",
                version: 2,
                constraints: "Tilde implicit param; ~ not after alphanumeric (R11)",
                example: ("~.size>100", "fn(__) => __.size>100"),
            },
        ],
    },
    // ── Stage 3: Module sigils ───────────────────────────────────────
    OntologyCategory {
        name: "Module Sigils",
        stage: 4,
        description: "Expand @sigil.func() to module.func() using MODULE_MAP (92 entries, 21 single-char)",
        rules: &[
            OntologyRule {
                pattern: "@xx.func()",
                expansion: "module.func()",
                version: 1,
                constraints: "Sigil must be in MODULE_MAP; followed by `.` or `(`",
                example: ("@f.read(\"p\")", "file.read(\"p\")"),
            },
        ],
    },
    // ── Stage 3.5: Function abbreviations ────────────────────────────
    OntologyCategory {
        name: "Function Abbreviations",
        stage: 5,
        description: "Expand single-char function names in module.X() calls via FUNC_ABBREV (152 entries)",
        rules: &[
            OntologyRule {
                pattern: "module.X()",
                expansion: "module.full_name()",
                version: 2,
                constraints: "X must be exactly 1 char; must be in FUNC_ABBREV",
                example: ("file.r(\"p\")", "file.read(\"p\")"),
            },
        ],
    },
    // ── Stage 3.6: Auto-parens ───────────────────────────────────────
    OntologyCategory {
        name: "Auto Parens",
        stage: 5,
        description: "Wrap bare string args in parens: module.func\"arg\" → module.func(\"arg\"). Runs after func abbreviations (R21)",
        rules: &[
            OntologyRule {
                pattern: "mod.func\"arg\"",
                expansion: "mod.func(\"arg\")",
                version: 5,
                constraints: "Only when func is immediately followed by \" without ( (R21)",
                example: ("file.read\"p\"", "file.read(\"p\")"),
            },
        ],
    },
    // ── Stage 4: Builtin shorthands ──────────────────────────────────
    OntologyCategory {
        name: "Builtin Shorthands",
        stage: 6,
        description: "Expand #X arg to builtin(arg) using BUILTIN_SHORT (26 entries, all a-z assigned)",
        rules: &[
            OntologyRule {
                pattern: "#X arg",
                expansion: "builtin(arg)",
                version: 1,
                constraints: "X must be in BUILTIN_SHORT; args collected until pipe/EOL",
                example: ("#e \"hello\"", "echo(\"hello\")"),
            },
            OntologyRule {
                pattern: "#X",
                expansion: "builtin()",
                version: 1,
                constraints: "Zero-arg form when no argument follows",
                example: ("#k", "keys()"),
            },
        ],
    },
    // ── Stage 5: Pipeline operators ──────────────────────────────────
    OntologyCategory {
        name: "Pipelines",
        stage: 7,
        description: "Normalize pipe operators and expand field projection",
        rules: &[
            OntologyRule {
                pattern: "a > b",
                expansion: "a | b",
                version: 1,
                constraints: "Space-delimited only (R09); not >= >> => (R10)",
                example: ("a > b", "a | b"),
            },
            OntologyRule {
                pattern: "expr|func",
                expansion: "expr | func",
                version: 2,
                constraints: "Normalize spacing on native pipe",
                example: ("foo|bar", "foo | bar"),
            },
            OntologyRule {
                pattern: "|.field",
                expansion: "| map(fn(__) => __.field)",
                version: 4,
                constraints: "Dot after pipe triggers field projection; chains (.a.b) and methods (.f()) supported",
                example: ("foo|.name", "foo | map(fn(__) => __.name)"),
            },
            OntologyRule {
                pattern: ">>",
                expansion: "| each(...)",
                version: 1,
                constraints: "Double-arrow is side-effect pipe",
                example: ("foo >> bar", "foo | each(bar)"),
            },
        ],
    },
    // ── Stage 6: Assignments ─────────────────────────────────────────
    OntologyCategory {
        name: "Assignments",
        stage: 8,
        description: "Expand assignment operators to let bindings",
        rules: &[
            OntologyRule {
                pattern: "x=expr",
                expansion: "let x = expr",
                version: 1,
                constraints: "Simple identifier at line start; not == != <= >= => (R12)",
                example: ("x=42", "let x = 42"),
            },
            OntologyRule {
                pattern: "x:=expr",
                expansion: "let mut x = expr",
                version: 1,
                constraints: "Walrus operator for mutable binding",
                example: ("counter:=0", "let mut counter = 0"),
            },
        ],
    },
    // ── Stage 7: Match ───────────────────────────────────────────────
    OntologyCategory {
        name: "Match",
        stage: 9,
        description: "Expand pattern matching shorthand",
        rules: &[
            OntologyRule {
                pattern: "?val{arms}",
                expansion: "?val{arms} (grammar parses natively as match val { arms })",
                version: 1,
                constraints: "? at line start; scrutinee until {; body passed through",
                example: ("?val{1=>\"a\",_=>\"b\"}", "?val{1=>\"a\",_=>\"b\"}"),
            },
        ],
    },
    // ── Stage 8: Try/Catch ───────────────────────────────────────────
    OntologyCategory {
        name: "Try/Catch",
        stage: 10,
        description: "Expand error handling shorthand",
        rules: &[
            OntologyRule {
                pattern: "!{expr}{fallback}",
                expansion: "try { expr } catch e { fallback }",
                version: 1,
                constraints: "! at line start + {; catch body optional (defaults to null)",
                example: ("!{risky()}{\"safe\"}", "try { risky() } catch e { \"safe\" }"),
            },
        ],
    },
    // ── Stage 9: Conditional ─────────────────────────────────────────
    OntologyCategory {
        name: "Conditional",
        stage: 11,
        description: "Expand compact if/else",
        rules: &[
            OntologyRule {
                pattern: "^cond{then}",
                expansion: "match (cond) { true => (then), _ => null }",
                version: 4,
                constraints: "^ at line start; condition until {; else body optional",
                example: ("^x>0{x*2}", "match (x>0) { true => (x*2), _ => null }"),
            },
            OntologyRule {
                pattern: "^cond{then}{else}",
                expansion: "match (cond) { true => (then), _ => (else) }",
                version: 4,
                constraints: "Second {} block is else branch",
                example: ("^x>0{x*2}{0}", "match (x>0) { true => (x*2), _ => (0) }"),
            },
        ],
    },
    // ── Meta: Comments ───────────────────────────────────────────────
    OntologyCategory {
        name: "Comments",
        stage: 255,
        description: "Comment syntax (handled before pipeline)",
        rules: &[
            OntologyRule {
                pattern: "; text",
                expansion: "// text",
                version: 1,
                constraints: "Full-line or inline; stripped outside string/backtick literals",
                example: ("; hello", "// hello"),
            },
        ],
    },
    // ── Meta: Preamble ───────────────────────────────────────────────
    OntologyCategory {
        name: "Preamble",
        stage: 254,
        description: "User-defined aliases (handled before pipeline)",
        rules: &[
            OntologyRule {
                pattern: "%def name expansion",
                expansion: "(alias registered, not emitted)",
                version: 4,
                constraints: "At line start; textual replacement at word boundaries (R17, R18)",
                example: ("%def fetch H.g", ""),
            },
        ],
    },
];

/// Returns a human-readable description of the complete ontology.
pub fn describe_ontology() -> String {
    let mut out = String::new();
    out.push_str("╔══════════════════════════════════════════════════════════════╗\n");
    out.push_str("║       AGENTIC SYNTAX ONTOLOGY — Complete Mapping           ║\n");
    out.push_str("╚══════════════════════════════════════════════════════════════╝\n\n");

    // Conflict resolution rules
    out.push_str("─── Conflict Resolution Rules ────────────────────────────────\n\n");
    for rule in CONFLICT_RULES {
        out.push_str(rule);
        out.push('\n');
    }
    out.push('\n');

    // Reserved characters
    out.push_str("─── Reserved Characters ──────────────────────────────────────\n\n");
    for (ch, meaning, resolution) in RESERVED_CHARS {
        out.push_str(&format!(
            "  {:>4}  {}  [{}]\n",
            format!("'{}'", ch),
            meaning,
            resolution
        ));
    }
    out.push('\n');

    // Transformation categories (sorted by stage)
    let mut cats: Vec<&OntologyCategory> = ONTOLOGY.iter().collect();
    cats.sort_by_key(|c| c.stage);

    for cat in cats {
        out.push_str(&format!(
            "─── Stage {}: {} ──────────────────────────────────\n",
            cat.stage, cat.name
        ));
        out.push_str(&format!("    {}\n\n", cat.description));
        for rule in cat.rules {
            out.push_str(&format!(
                "    v{}  {:30} → {}\n",
                rule.version, rule.pattern, rule.expansion
            ));
            if !rule.constraints.is_empty() {
                out.push_str(&format!("        Constraints: {}\n", rule.constraints));
            }
        }
        out.push('\n');
    }

    // Dynamic maps
    out.push_str("─── MODULE_MAP (dynamic, 92 entries, 21 single-char) ────────\n\n");
    let mut modules: Vec<(&&str, &&str)> = MODULE_MAP.iter().collect();
    modules.sort_by_key(|(k, _)| *k);
    for (sigil, module) in &modules {
        out.push_str(&format!("    {:>4} → {}\n", sigil.to_uppercase(), module));
    }
    out.push('\n');

    out.push_str("─── BUILTIN_SHORT (26 entries, all a–z assigned) ─────────\n\n");
    let mut builtins: Vec<(&char, &(&str, bool))> = BUILTIN_SHORT.iter().collect();
    builtins.sort_by_key(|(k, _)| *k);
    for (code, (name, takes_lambda)) in &builtins {
        let lambda_note = if *takes_lambda { " (λ)" } else { "" };
        out.push_str(&format!("    {:>4} → {}{}\n", code, name, lambda_note));
    }
    out.push('\n');

    out.push_str("─── FUNC_ABBREV (152 entries) ──────────────────────────────\n\n");
    let mut abbrevs: Vec<(&&str, &&str)> = FUNC_ABBREV.iter().collect();
    abbrevs.sort_by_key(|(k, _)| *k);
    for (short, full) in &abbrevs {
        out.push_str(&format!("    {:>12} → {}\n", short, full));
    }
    out.push('\n');

    out
}

/// Check if a character can appear in a file path or glob pattern.
fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '/' | '\\' | '-' | '_' | '*' | '?' | ':')
}

/// Strip an inline comment (`;` outside string/backtick literals) from a line.
/// Returns `(code_portion, optional_comment_text)`.
fn strip_inline_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        match bytes[i] {
            b'"' => {
                i += 1;
                while i < len && bytes[i] != b'"' {
                    if bytes[i] == b'\\' && i + 1 < len {
                        i += 1;
                    }
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            b'\'' => {
                i += 1;
                while i < len && bytes[i] != b'\'' {
                    if bytes[i] == b'\\' && i + 1 < len {
                        i += 1;
                    }
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            b'`' => {
                i += 1;
                while i < len && bytes[i] != b'`' {
                    if bytes[i] == b'\\' && i + 1 < len {
                        i += 1;
                    }
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            b';' => {
                let code = &line[..i];
                let comment = &line[i + 1..];
                return (code.trim_end(), Some(comment.trim_start()));
            }
            _ => {
                i += 1;
            }
        }
    }

    (line, None)
}

/// Transpile agentic syntax to AetherShell source code.
///
/// The agentic format is a token-minimized syntax designed for AI agent
/// consumption: fewer tokens → cheaper API calls, more room in context
/// windows, faster generation.
pub fn transpile_agentic_to_ae(src: &str) -> Result<String> {
    let mut out = String::new();
    out.push_str("// Transpiled from Agentic \u{2192} Aether\n");

    let mut aliases: Vec<(String, String)> = Vec::new();

    for raw_line in src.lines() {
        let line = raw_line.trim();

        // Empty lines
        if line.is_empty() {
            continue;
        }

        // Preamble directive: %def name expansion
        if line.starts_with("%def ") {
            let rest = &line[5..];
            if let Some(pos) = rest.find(' ') {
                let name = rest[..pos].to_string();
                let expansion = rest[pos + 1..].trim().to_string();
                aliases.push((name, expansion));
            }
            continue;
        }

        // Full-line comments: ; → //
        if line.starts_with(';') {
            out.push_str(&format!("// {}\n", line[1..].trim_start()));
            continue;
        }

        // Split inline comments: code ; comment → transpile code, preserve comment
        let (code_part, comment_part) = strip_inline_comment(line);

        if code_part.is_empty() {
            if let Some(c) = comment_part {
                out.push_str(&format!("// {}\n", c));
            }
            continue;
        }

        // Apply user-defined aliases before transpilation
        let mut expanded_line = code_part.to_string();
        for (name, expansion) in &aliases {
            expanded_line = replace_standalone(&expanded_line, name, expansion);
        }

        // Transpile the line
        let expanded = transpile_line(&expanded_line);
        out.push_str(&expanded);
        if let Some(comment) = comment_part {
            out.push_str(&format!(" // {}", comment));
        }
        out.push('\n');
    }

    Ok(out)
}

/// Transpile a single line of agentic (`.aeg`) syntax to legible `.ae`.
///
/// Phase 5: this is a **single tokenizing pass** (`scan`, plus the two
/// statement-level prefix handlers `try_for_each`/`try_assignment`) that replaced
/// the former 10-stage sequential text-rewrite pipeline. One left-to-right scan,
/// recursing into nested constructs, protects string/backtick literals inline and
/// maps each cipher form directly to its legible form — so a terse token can never
/// be mis-expanded by pass ordering (every transform sees the original token
/// boundaries, never another pass's output).
fn transpile_line(line: &str) -> String {
    let t = line.trim();
    if t.is_empty() {
        return String::new();
    }
    // Statement-level structural prefixes (recurse into `scan` for sub-expressions).
    if let Some(r) = try_for_each(t) {
        return r;
    }
    if let Some(r) = try_assignment(t) {
        return r;
    }
    scan(t)
}

/// For-each prefix: `*iter(~|\)v:body` → `(iter) | each(fn(v) => body)`.
/// Only fires when `*` is the first char (multiplication never starts a line).
fn try_for_each(s: &str) -> Option<String> {
    if !s.starts_with('*') || s.len() < 2 {
        return None;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 1; // skip '*'
    let mut depth = 0i32;
    let iter_start = i;
    while i < chars.len() {
        match chars[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '~' | '\\' if depth == 0 => break,
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    if i >= chars.len() || (chars[i] != '~' && chars[i] != '\\') {
        return None;
    }
    let iterable: String = chars[iter_start..i].iter().collect();
    i += 1; // skip ~ or \
    let params_start = i;
    while i < chars.len() && chars[i] != ':' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let var: String = chars[params_start..i].iter().collect();
    i += 1; // skip ':'
    let body: String = chars[i..].iter().collect();
    Some(format!(
        "({}) | each(fn({}) => {})",
        scan(iterable.trim()),
        var.trim(),
        scan(body.trim())
    ))
}

/// Assignment prefix: `x:=expr` → `let mut x = expr`; `x=expr` → `let x = expr`
/// (when `x` is a simple identifier and not `==`/`=>`). The RHS is scanned.
fn try_assignment(s: &str) -> Option<String> {
    if let Some(pos) = s.find(":=") {
        let lhs = s[..pos].trim();
        if is_simple_identifier(lhs) {
            return Some(format!("let mut {} = {}", lhs, scan(s[pos + 2..].trim())));
        }
    }
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        let id_end = i;
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }
        if i < chars.len()
            && chars[i] == '='
            && (i + 1 >= chars.len() || (chars[i + 1] != '=' && chars[i + 1] != '>'))
        {
            let lhs: String = chars[..id_end].iter().collect();
            let lhs = lhs.trim();
            if is_simple_identifier(lhs) && !s.starts_with("let ") {
                let rhs: String = chars[i + 1..].iter().collect();
                return Some(format!("let {} = {}", lhs, scan(rhs.trim())));
            }
        }
    }
    None
}

/// Copy a double-quoted string literal verbatim (including quotes).
fn copy_dquote(chars: &[char], i: &mut usize, out: &mut String) {
    out.push('"');
    *i += 1;
    while *i < chars.len() && chars[*i] != '"' {
        if chars[*i] == '\\' && *i + 1 < chars.len() {
            out.push(chars[*i]);
            *i += 1;
        }
        out.push(chars[*i]);
        *i += 1;
    }
    if *i < chars.len() {
        out.push('"');
        *i += 1;
    }
}

/// Is the just-emitted context a token boundary (start, or after a non-ident char)?
fn at_boundary(out: &str) -> bool {
    out.is_empty() || {
        let p = out.chars().last().unwrap();
        !p.is_alphanumeric() && p != '_'
    }
}

/// The single left-to-right scan that maps every cipher token to legible `.ae`.
fn scan(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len() * 2);
    let mut i = 0;
    let mut depth = 0i32;
    // True immediately after an explicit `|` pipe — the only place a zero-arg bare
    // builtin (`data|b` → `flatten()`) may fire. A `>`-derived pipe does NOT set
    // this (so `a > b` stays `a | b`, matching the pre-rewrite pass ordering).
    let mut after_bar = false;

    while i < chars.len() {
        let c = chars[i];
        let was_bar = after_bar;
        after_bar = false;

        // ── Literals (protected) ─────────────────────────────────────
        if c == '"' {
            copy_dquote(&chars, &mut i, &mut out);
            continue;
        }
        // Single-quote → double-quote, escaping embedded ".
        if c == '\'' {
            out.push('"');
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i]);
                    i += 1;
                } else if chars[i] == '"' {
                    out.push('\\');
                }
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push('"');
                i += 1;
            }
            continue;
        }
        // Backtick → sh("...") (contents not further expanded).
        if c == '`' {
            i += 1;
            let mut cmd = String::new();
            while i < chars.len() && chars[i] != '`' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    cmd.push(chars[i]);
                    i += 1;
                }
                cmd.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                i += 1;
            }
            out.push_str(&format!("sh(\"{}\")", cmd));
            continue;
        }
        // $VAR → sys.env("VAR")
        if c == '$' && i + 1 < chars.len() && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_') {
            i += 1;
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let var: String = chars[start..i].iter().collect();
            out.push_str(&format!("sys.env(\"{}\")", var));
            continue;
        }

        // ── Inline structural constructs ─────────────────────────────
        if c == '^' {
            if let Some(rep) = consume_conditional(&chars, &mut i) {
                out.push_str(&rep);
                continue;
            }
        }
        if c == '!' && i + 1 < chars.len() && chars[i + 1] == '{' {
            if let Some(rep) = consume_try(&chars, &mut i) {
                out.push_str(&rep);
                continue;
            }
        }

        // ── T / N standalone literals ────────────────────────────────
        if (c == 'T' || c == 'N') && at_boundary(&out) {
            let next_ok = i + 1 >= chars.len() || {
                let n = chars[i + 1];
                !n.is_alphanumeric() && n != '_' && n != '.'
            };
            if next_ok {
                out.push_str(if c == 'T' { "true" } else { "null" });
                i += 1;
                continue;
            }
        }

        // ── @sigil module reference ──────────────────────────────────
        if c == '@' && i + 1 < chars.len() && chars[i + 1].is_alphanumeric() {
            consume_sigil(&chars, &mut i, &mut out);
            continue;
        }

        // ── #X builtin shorthand ─────────────────────────────────────
        if c == '#' && i + 1 < chars.len() && BUILTIN_SHORT.contains_key(&chars[i + 1]) {
            consume_builtin(&chars, &mut i, &mut out, true);
            continue;
        }

        // ── Lambda: \ or ~ ───────────────────────────────────────────
        if (c == '\\' || c == '~') && i + 1 < chars.len() {
            // ~ not inside an identifier (avoid bitwise-not-like usage)
            let tilde_in_ident =
                c == '~' && i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
            if !tilde_in_ident {
                if let Some(rep) = consume_lambda(&chars, &mut i) {
                    out.push_str(&rep);
                    continue;
                }
            }
        }

        // ── Pipeline `|` (and `|.field` projection) ──────────────────
        if c == '|' && depth == 0 {
            consume_pipe(&chars, &mut i, &mut out);
            after_bar = true;
            continue;
        }

        // ── `>` pipeline / comparison ────────────────────────────────
        if c == '>' && depth == 0 {
            consume_gt(&chars, &mut i, &mut out);
            continue;
        }

        // ── Bare UPPERCASE module: XX.func ──────────────────────────
        if c.is_uppercase() && at_boundary(&out) && consume_bare_module(&chars, &mut i, &mut out) {
            continue;
        }

        // ── Identifier: bare lowercase builtin, or module.func ──────
        if c.is_alphabetic() || c == '_' {
            consume_ident(&chars, &mut i, &mut out, was_bar);
            continue;
        }

        // ── Default: copy char, track bracket depth ─────────────────
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        out.push(c);
        i += 1;
    }

    out
}

/// `^cond{then}{else?}` → `match (cond) { true => (then), _ => (else|null) }`.
/// Condition/bodies are scanned recursively. Returns None if it doesn't match.
fn consume_conditional(chars: &[char], i: &mut usize) -> Option<String> {
    let mut j = *i + 1; // skip ^
    let cond_start = j;
    while j < chars.len() && chars[j] != '{' {
        j += 1;
    }
    if j >= chars.len() {
        return None;
    }
    let condition: String = chars[cond_start..j].iter().collect();
    if condition.trim().is_empty() {
        return None;
    }
    let then_body = consume_brace_group(chars, &mut j)?;
    let replacement = if j < chars.len() && chars[j] == '{' {
        let else_body = consume_brace_group(chars, &mut j)?;
        format!(
            "match ({}) {{ true => ({}), _ => ({}) }}",
            scan(condition.trim()),
            scan(then_body.trim()),
            scan(else_body.trim())
        )
    } else {
        format!(
            "match ({}) {{ true => ({}), _ => null }}",
            scan(condition.trim()),
            scan(then_body.trim())
        )
    };
    *i = j;
    Some(replacement)
}

/// `!{try}{catch?}` → `try { try } catch e { catch|null }`. Bodies scanned.
fn consume_try(chars: &[char], i: &mut usize) -> Option<String> {
    let mut j = *i + 1; // skip !
    let try_body = consume_brace_group(chars, &mut j)?;
    let replacement = if j < chars.len() && chars[j] == '{' {
        let catch_body = consume_brace_group(chars, &mut j)?;
        format!(
            "try {{ {} }} catch e {{ {} }}",
            scan(try_body.trim()),
            scan(catch_body.trim())
        )
    } else {
        format!("try {{ {} }} catch e {{ null }}", scan(try_body.trim()))
    };
    *i = j;
    Some(replacement)
}

/// Consume a `{...}` group starting at `*j` (which must point at `{`), returning
/// the inner text and advancing `*j` past the closing `}`. None if unbalanced.
fn consume_brace_group(chars: &[char], j: &mut usize) -> Option<String> {
    if *j >= chars.len() || chars[*j] != '{' {
        return None;
    }
    *j += 1; // skip {
    let start = *j;
    let mut depth = 1i32;
    while *j < chars.len() && depth > 0 {
        match chars[*j] {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        *j += 1;
    }
    if depth != 0 {
        return None;
    }
    Some(chars[start..*j - 1].iter().collect())
}

/// `@sigil` / `@sigil.` / `@sigil(` → resolved module name via MODULE_MAP.
fn consume_sigil(chars: &[char], i: &mut usize, out: &mut String) {
    *i += 1; // skip @
    let start = *i;
    while *i < chars.len() && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
        *i += 1;
    }
    let sigil: String = chars[start..*i].iter().collect();
    let resolved = MODULE_MAP
        .get(sigil.as_str())
        .map(|m| m.to_string())
        .unwrap_or(sigil);
    out.push_str(&resolved);
    if *i < chars.len() && chars[*i] == '.' {
        attach_func_or_autoparens(chars, i, out, &resolved);
    } else {
        maybe_autoparen_string(chars, i, out);
    }
}

/// `#X args…` → `name(args…)`. When `hash` is true the `#` is at `*i`; args run
/// to a pipeline boundary. Reused for the bare-builtin path (hash=false, `*i` at
/// the builtin letter already emitted via caller).
fn consume_builtin(chars: &[char], i: &mut usize, out: &mut String, hash: bool) {
    let code = if hash { chars[*i + 1] } else { chars[*i] };
    let (name, _) = BUILTIN_SHORT.get(&code).copied().unwrap();
    *i += if hash { 2 } else { 1 };
    while *i < chars.len() && chars[*i] == ' ' {
        *i += 1;
    }
    let args_start = *i;
    let mut depth = 0i32;
    while *i < chars.len() {
        match chars[*i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' if depth <= 0 => break,
            ')' | ']' | '}' => depth -= 1,
            '"' => {
                *i += 1;
                while *i < chars.len() && chars[*i] != '"' {
                    if chars[*i] == '\\' {
                        *i += 1;
                    }
                    *i += 1;
                }
            }
            '|' if depth == 0 => break,
            ' ' if depth == 0 => {
                if *i + 2 < chars.len() && chars[*i + 1] == '>' && chars[*i + 2] == ' ' {
                    break;
                }
            }
            _ => {}
        }
        *i += 1;
    }
    let args_raw: String = chars[args_start..*i].iter().collect();
    let args = scan(args_raw.trim());
    if args.is_empty() {
        out.push_str(&format!("{}()", name));
    } else {
        out.push_str(&format!("{}({})", name, args));
    }
}

/// `\x:body`, `~x:body`, `\.f`, `~.f` → `fn(params) => body`. None if not a lambda.
fn consume_lambda(chars: &[char], i: &mut usize) -> Option<String> {
    let mut j = *i + 1; // skip \ or ~
    if j >= chars.len() {
        return None;
    }
    // Implicit-parameter form: \.field / ~.field
    if chars[j] == '.' {
        let body = collect_lambda_body(chars, &mut j);
        *i = j;
        return Some(format!("fn(__) => __{}", scan_lambda_body(&body)));
    }
    let param_start = j;
    while j < chars.len() && chars[j] != ':' && chars[j] != '\n' {
        j += 1;
    }
    if j < chars.len() && chars[j] == ':' {
        let params: String = chars[param_start..j].iter().collect();
        j += 1; // skip ':'
        let body = collect_lambda_body(chars, &mut j);
        let param_list = params
            .split(',')
            .map(|p| p.trim())
            .collect::<Vec<_>>()
            .join(", ");
        *i = j;
        return Some(format!("fn({}) => {}", param_list, scan_lambda_body(&body)));
    }
    None
}

/// Scan a lambda body, but leave a leading `.` (field access) attached as-is.
fn scan_lambda_body(body: &str) -> String {
    scan(body)
}

/// Pipeline `|`: handles `|.field` projection and normalizes `a|b` → `a | b`.
fn consume_pipe(chars: &[char], i: &mut usize, out: &mut String) {
    // Field projection: |.accessor → | map(fn(__) => __.accessor)
    let mut peek = *i + 1;
    while peek < chars.len() && chars[peek] == ' ' {
        peek += 1;
    }
    if peek < chars.len() && chars[peek] == '.' {
        let acc_start = peek;
        let mut acc_depth = 0i32;
        let mut j = peek;
        while j < chars.len() {
            match chars[j] {
                '(' | '[' => {
                    acc_depth += 1;
                    j += 1;
                }
                ')' | ']' => {
                    if acc_depth <= 0 {
                        break;
                    }
                    acc_depth -= 1;
                    j += 1;
                }
                '|' if acc_depth == 0 => break,
                ' ' if acc_depth == 0 => break,
                _ => j += 1,
            }
        }
        let accessor: String = chars[acc_start..j].iter().collect();
        if !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(&format!("| map(fn(__) => __{})", accessor));
        *i = j;
        return;
    }
    if !out.ends_with(' ') {
        out.push(' ');
    }
    out.push_str("| ");
    *i += 1;
    while *i < chars.len() && chars[*i] == ' ' {
        *i += 1;
    }
}

/// `>`: ` > ` (spaced) → pipe; `>=`/`=>`/`>>` preserved; bare `>` is comparison.
fn consume_gt(chars: &[char], i: &mut usize, out: &mut String) {
    // `=>` fat arrow — leave as-is.
    if out.ends_with('=') {
        out.push('>');
        *i += 1;
        return;
    }
    // `>>` → ` | each(body)`
    if *i + 1 < chars.len() && chars[*i + 1] == '>' {
        let trimmed = out.trim_end().len();
        out.truncate(trimmed);
        out.push_str(" | each(");
        *i += 2;
        while *i < chars.len() && chars[*i] == ' ' {
            *i += 1;
        }
        let body_start = *i;
        let mut bd = 0i32;
        while *i < chars.len() {
            match chars[*i] {
                '(' | '[' | '{' => bd += 1,
                ')' | ']' | '}' if bd > 0 => bd -= 1,
                '>' | '|' if bd == 0 => break,
                _ => {}
            }
            *i += 1;
        }
        let body: String = chars[body_start..*i].iter().collect();
        out.push_str(scan(body.trim()).trim());
        out.push(')');
        return;
    }
    // `>=` comparison — leave as-is.
    if *i + 1 < chars.len() && chars[*i + 1] == '=' {
        out.push_str(">=");
        *i += 2;
        return;
    }
    // Spaced ` > ` → pipe; otherwise comparison.
    let space_before = out.ends_with(' ');
    let space_after = *i + 1 < chars.len() && chars[*i + 1] == ' ';
    if space_before && space_after {
        out.push_str("| ");
        *i += 1;
        while *i < chars.len() && chars[*i] == ' ' {
            *i += 1;
        }
    } else {
        out.push('>');
        *i += 1;
    }
}

/// Bare uppercase module: `XX.` where lowercase(XX) is a known module → resolve to
/// the module name, then attach a single-char function abbreviation if present.
/// Returns false (consuming nothing) if not a module reference.
fn consume_bare_module(chars: &[char], i: &mut usize, out: &mut String) -> bool {
    let mut j = *i;
    while j < chars.len() && (chars[j].is_uppercase() || chars[j].is_ascii_digit()) {
        j += 1;
    }
    if j >= chars.len() || chars[j] != '.' {
        return false;
    }
    let sigil: String = chars[*i..j].iter().collect();
    let lower = sigil.to_lowercase();
    let module = match MODULE_MAP.get(lower.as_str()) {
        Some(m) => *m,
        None => return false,
    };
    out.push_str(module);
    *i = j; // position at '.'
    attach_func_or_autoparens(chars, i, out, module);
    true
}

/// After a module name has been emitted and `*i` points at `.`, attach the
/// function: a single-char abbreviation (`.r` → `read`), an auto-parened string
/// (`.read"x"` → `.read("x")`), or a plain `.func`.
fn attach_func_or_autoparens(chars: &[char], i: &mut usize, out: &mut String, module: &str) {
    if *i >= chars.len() || chars[*i] != '.' {
        return;
    }
    let dot = *i;
    let mut j = *i + 1;
    let fstart = j;
    while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
        j += 1;
    }
    let func: String = chars[fstart..j].iter().collect();
    // Single-char abbreviation: module.x where the full name is in FUNC_ABBREV.
    if func.chars().count() == 1 {
        let key = format!("{}.{}", module, func);
        if let Some(full) = FUNC_ABBREV.get(key.as_str()) {
            // FUNC_ABBREV maps to the fully-qualified `module.func`; emit the func
            // part (module already emitted).
            let func_only = full.split('.').last().unwrap_or(full);
            out.push('.');
            out.push_str(func_only);
            *i = j;
            // Auto-parens if a string literal follows.
            maybe_autoparen_string(chars, i, out);
            return;
        }
    }
    if func.is_empty() {
        out.push('.');
        *i = dot + 1;
        return;
    }
    out.push('.');
    out.push_str(&func);
    *i = j;
    maybe_autoparen_string(chars, i, out);
}

/// If a `"string"` directly follows (no `(`), wrap it: `…"x"` → `…("x")`.
fn maybe_autoparen_string(chars: &[char], i: &mut usize, out: &mut String) {
    if *i < chars.len() && chars[*i] == '"' {
        out.push('(');
        copy_dquote(chars, i, out);
        out.push(')');
    }
}

/// Identifier handling: bare lowercase single-char builtins, and `module.func`
/// (with abbreviation/auto-parens). Plain identifiers are copied through.
fn consume_ident(chars: &[char], i: &mut usize, out: &mut String, after_bar: bool) {
    let c = chars[*i];
    // Bare lowercase single-char builtin at a boundary (v2 ultra form).
    if c.is_lowercase() && BUILTIN_SHORT.contains_key(&c) {
        let prev_ok = at_boundary(out) && out.chars().last() != Some('.');
        if prev_ok {
            let next = chars.get(*i + 1).copied();
            match next {
                Some('"') | Some('(') | Some('~') | Some('\\') | Some('[') | Some('{')
                | Some('\'') | Some('$') => {
                    consume_builtin(chars, i, out, false);
                    return;
                }
                Some(d) if d.is_ascii_digit() => {
                    consume_builtin(chars, i, out, false);
                    return;
                }
                Some('/') | Some('*') => {
                    let after = chars.get(*i + 2).copied();
                    if !matches!(after, Some(d) if d.is_ascii_digit()) {
                        emit_bare_path_builtin(chars, i, out, c);
                        return;
                    }
                }
                Some('.') => {
                    let after_dot = chars.get(*i + 2).copied();
                    let looks_like_path = match after_dot {
                        None => true,
                        Some(ch) => !ch.is_alphanumeric() && ch != '_',
                    };
                    if looks_like_path {
                        emit_bare_path_builtin(chars, i, out, c);
                        return;
                    }
                }
                None | Some('|') => {
                    // Zero-arg bare builtin fires ONLY right after an explicit `|`
                    // pipe (`data|b` → `flatten()`). Never at line/sub-expr start
                    // (would misfire on a variable like `x`=the `sh` shorthand in a
                    // conditional body) and never after a `>`-derived pipe (so
                    // `a > b` stays `a | b`).
                    if after_bar {
                        let (name, _) = BUILTIN_SHORT.get(&c).copied().unwrap();
                        out.push_str(&format!("{}()", name));
                        *i += 1;
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    // General identifier; may be `module.func` needing abbrev/auto-parens.
    let start = *i;
    while *i < chars.len() && (chars[*i].is_alphanumeric() || chars[*i] == '_') {
        *i += 1;
    }
    let ident: String = chars[start..*i].iter().collect();
    if *i < chars.len() && chars[*i] == '.' {
        out.push_str(&ident);
        attach_func_or_autoparens(chars, i, out, &ident);
        return;
    }
    out.push_str(&ident);
}

/// Bare builtin with an auto-quoted path/glob argument: `l./src` → `ls("./src")`.
fn emit_bare_path_builtin(chars: &[char], i: &mut usize, out: &mut String, letter: char) {
    let (name, _) = BUILTIN_SHORT.get(&letter).copied().unwrap();
    *i += 1; // skip the letter
    let path_start = *i;
    while *i < chars.len() && is_path_char(chars[*i]) {
        *i += 1;
    }
    let path: String = chars[path_start..*i].iter().collect();
    out.push_str(&format!("{}(\"{}\")", name, path));
}

/// Collect a lambda body up to an unbalanced delimiter.
/// Stops before: `)`, `]`, `}`, ` > ` (space-delimited pipeline), `,` at depth 0, or EOL.
/// Note: bare `>` without surrounding spaces is treated as a comparison operator
/// within the lambda body, not as a pipeline.
fn collect_lambda_body(chars: &[char], i: &mut usize) -> String {
    let mut body = String::new();
    let mut depth = 0i32;

    while *i < chars.len() {
        let ch = chars[*i];

        match ch {
            '(' | '[' | '{' => {
                depth += 1;
                body.push(ch);
            }
            ')' | ']' | '}' => {
                if depth <= 0 {
                    break;
                }
                depth -= 1;
                body.push(ch);
            }
            ' ' if depth == 0 => {
                // Check for ` > ` (pipeline delimiter)
                if *i + 2 < chars.len() && chars[*i + 1] == '>' && chars[*i + 2] == ' ' {
                    break;
                }
                body.push(ch);
            }
            '|' if depth == 0 => break, // v2 pipeline delimiter
            ',' if depth == 0 => break, // argument separator
            '\n' => break,
            _ => body.push(ch),
        }
        *i += 1;
    }

    body.trim().to_string()
}

/// Replace standalone occurrences of `pattern` with `replacement`,
/// respecting word boundaries and string literals.
fn replace_standalone(s: &str, pattern: &str, replacement: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();
    let pat_len = pat_chars.len();
    let mut result = String::with_capacity(s.len());
    let mut i = 0;

    while i < chars.len() {
        // Skip string literals
        if chars[i] == '"' {
            result.push(chars[i]);
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    result.push(chars[i]);
                    i += 1;
                }
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                result.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // Try to match pattern at word boundaries
        if i + pat_len <= chars.len() {
            let matches = chars[i..i + pat_len]
                .iter()
                .zip(pat_chars.iter())
                .all(|(a, b)| a == b);
            if matches {
                let prev_ok = i == 0 || { !chars[i - 1].is_alphanumeric() && chars[i - 1] != '_' };
                let next_ok = i + pat_len >= chars.len() || {
                    let n = chars[i + pat_len];
                    !n.is_alphanumeric() && n != '_'
                };
                if prev_ok && next_ok {
                    result.push_str(replacement);
                    i += pat_len;
                    continue;
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Check if a string is a simple identifier (alphanumeric + underscore, starts with letter/_)
fn is_simple_identifier(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}


#[cfg(test)]
mod tests {
    use super::*;

    // Retained helper unit tests (functions that survive the single-pass rewrite).
    #[test]
    fn test_is_path_char() {
        assert!(is_path_char('.'));
        assert!(is_path_char('/'));
        assert!(is_path_char('a'));
        assert!(is_path_char('_'));
        assert!(is_path_char('-'));
        assert!(is_path_char('*'));
        assert!(!is_path_char('|'));
        assert!(!is_path_char(' '));
        assert!(!is_path_char('('));
    }

    #[test]
    fn test_strip_inline_comment_basic() {
        let (code, comment) = strip_inline_comment("x=42 ; my comment");
        assert_eq!(code, "x=42");
        assert_eq!(comment, Some("my comment"));
    }

    #[test]
    fn test_strip_inline_comment_no_comment() {
        let (code, comment) = strip_inline_comment("x=42");
        assert_eq!(code, "x=42");
        assert_eq!(comment, None);
    }

    #[test]
    fn test_strip_inline_comment_in_string() {
        let (code, comment) = strip_inline_comment(r#"e"a;b""#);
        assert_eq!(code, r#"e"a;b""#);
        assert_eq!(comment, None);
    }

    #[test]
    fn test_strip_inline_comment_in_single_quote() {
        let (code, comment) = strip_inline_comment("e'a;b'");
        assert_eq!(code, "e'a;b'");
        assert_eq!(comment, None);
    }

    #[test]
    fn test_strip_inline_comment_in_backtick() {
        let (code, comment) = strip_inline_comment("`echo;foo`");
        assert_eq!(code, "`echo;foo`");
        assert_eq!(comment, None);
    }

    #[test]
    fn test_strip_inline_comment_after_string() {
        let (code, comment) = strip_inline_comment(r#"e"hello" ; greeting"#);
        assert_eq!(code, r#"e"hello""#);
        assert_eq!(comment, Some("greeting"));
    }

    #[test]
    fn test_replace_standalone_basic() {
        assert_eq!(replace_standalone("fetch(url)", "fetch", "http.get"), "http.get(url)");
    }

    #[test]
    fn test_replace_standalone_not_in_word() {
        assert_eq!(replace_standalone("fetching(url)", "fetch", "http.get"), "fetching(url)");
    }

    #[test]
    fn test_replace_standalone_not_in_string() {
        assert_eq!(replace_standalone("\"fetch\" is good", "fetch", "http.get"), "\"fetch\" is good");
    }

    // End-to-end behaviour through the full single-pass transpiler.
    #[test]
    fn test_inline_comment_integration() {
        let ae = transpile_agentic_to_ae("x=42 ; my var\n").unwrap();
        assert!(ae.contains("let x = 42"), "got:\n{ae}");
        assert!(ae.contains("// my var"), "got:\n{ae}");
    }

    #[test]
    fn test_empty_line_passthrough() {
        let ae = transpile_agentic_to_ae("\n\n\n").unwrap();
        assert_eq!(ae.lines().count(), 1);
    }

    #[test]
    fn test_preamble_def() {
        let ae = transpile_agentic_to_ae("%def fetch H.g\nfetch(\"url\")\n").unwrap();
        assert!(ae.contains("http.get(\"url\")"), "got:\n{ae}");
    }

    #[test]
    fn test_preamble_multiple_defs() {
        let ae = transpile_agentic_to_ae("%def fetch H.g\n%def parse J.p\nfetch(\"url\")|parse(data)\n").unwrap();
        assert!(ae.contains("http.get(\"url\")"), "got:\n{ae}");
        assert!(ae.contains("json.parse(data)"), "got:\n{ae}");
    }

    #[test]
    fn test_auto_parens_full_pipeline() {
        let ae = transpile_agentic_to_ae("F.r\"README.md\"\n").unwrap();
        assert!(ae.contains("file.read(\"README.md\")"), "got:\n{ae}");
    }

    #[test]
    fn test_for_each_full_pipeline() {
        let ae = transpile_agentic_to_ae("*[1,2,3]~x:echo(x)\n").unwrap();
        assert!(ae.contains("([1,2,3]) | each(fn(x) => echo(x))"), "got:\n{ae}");
    }

    #[test]
    fn test_func_abbrev_end_to_end() {
        let ae = transpile_agentic_to_ae("HM.l()\n").unwrap();
        assert!(ae.contains("helm.list()"), "got:\n{ae}");
    }

    // Ontology integrity (examples still transpile correctly through `scan`).
    #[test]
    fn test_ontology_completeness() {
        for cat in ONTOLOGY {
            for rule in cat.rules {
                let (input, expected) = rule.example;
                if expected.is_empty() { continue; }
                let ae = transpile_agentic_to_ae(&format!("{}\n", input)).unwrap();
                assert!(ae.contains(expected),
                    "Ontology validation failed!\n  Category: {}\n  Pattern: {}\n  Input: {}\n  Expected: {}\n  Got: {}",
                    cat.name, rule.pattern, input, expected, ae);
            }
        }
    }

    #[test]
    fn test_ontology_has_all_categories() {
        let names: Vec<&str> = ONTOLOGY.iter().map(|c| c.name).collect();
        for n in ["Preprocessing","Symbols","SI Suffixes","Lambdas","Module Sigils","Function Abbreviations","Builtin Shorthands","Pipelines","Assignments","Match","Try/Catch","Conditional","Comments","Preamble"] {
            assert!(names.contains(&n), "missing {n}");
        }
    }

    #[test]
    fn test_ontology_describe_not_empty() {
        let desc = describe_ontology();
        assert!(desc.len() > 1000, "Ontology description too short");
        assert!(desc.contains("MODULE_MAP"));
        assert!(desc.contains("BUILTIN_SHORT"));
        assert!(desc.contains("FUNC_ABBREV"));
        assert!(desc.contains("Conflict Resolution"));
    }

    #[test]
    fn test_reserved_chars_complete() {
        let chars: Vec<char> = RESERVED_CHARS.iter().map(|(c, _, _)| *c).collect();
        for c in ['|','>','^','?','!','~','$','#','@','%',';','=','T','N'] {
            assert!(chars.contains(&c), "missing reserved char {c}");
        }
    }

    #[test]
    fn test_conflict_rules_numbered() {
        for (i, rule) in CONFLICT_RULES.iter().enumerate() {
            let expected_prefix = format!("R{:02}", i + 1);
            assert!(rule.starts_with(&expected_prefix), "Rule {} should start with {}, got: {}", i, expected_prefix, rule);
        }
    }
}
