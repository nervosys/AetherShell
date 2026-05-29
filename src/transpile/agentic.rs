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

/// Expand ASCII symbol shortcuts to their full values.
///
/// - `T` → `true` (standalone, not part of identifier or module sigil)
/// - `N` → `null` (standalone)
/// - `'text'` → `"text"` (single-quote strings become double-quote)
/// - `` `cmd` `` → `sh("cmd")` (backtick shell execution)
fn expand_symbols(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Skip double-quoted strings
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

        // Single-quote → double-quote: 'text' → "text"
        // Embedded " are escaped to \" for valid double-quote output.
        if chars[i] == '\'' {
            result.push('"');
            i += 1;
            while i < chars.len() && chars[i] != '\'' {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    result.push(chars[i]);
                    i += 1;
                } else if chars[i] == '"' {
                    result.push('\\');
                }
                result.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                result.push('"');
                i += 1;
            }
            continue;
        }

        // Backtick execution: `cmd` → sh("cmd")
        if chars[i] == '`' {
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
                i += 1; // skip closing backtick
            }
            result.push_str(&format!("sh(\"{}\")", cmd));
            continue;
        }

        // $VAR → sys.env("VAR") (env var expansion)
        if chars[i] == '$'
            && i + 1 < chars.len()
            && (chars[i + 1].is_alphabetic() || chars[i + 1] == '_')
        {
            i += 1; // skip $
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let var_name: String = chars[start..i].iter().collect();
            result.push_str(&format!("sys.env(\"{}\")", var_name));
            continue;
        }

        // T → true, N → null (standalone — not part of identifier or module)
        if chars[i] == 'T' || chars[i] == 'N' {
            let prev_ok = result.is_empty() || {
                let p = result.chars().last().unwrap();
                !p.is_alphanumeric() && p != '_'
            };
            let next_ok = i + 1 >= chars.len() || {
                let n = chars[i + 1];
                !n.is_alphanumeric() && n != '_' && n != '.'
            };
            if prev_ok && next_ok {
                match chars[i] {
                    'T' => result.push_str("true"),
                    'N' => result.push_str("null"),
                    _ => unreachable!(),
                }
                i += 1;
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Pre-process ultra-compressed v2 forms into v1 forms.
///
/// This converts the denser v2 syntax (bare builtins, bare modules) into v1
/// agentic syntax (`#x`, `@xx.`) which the existing transformation pipeline
/// then handles. Processing order: uppercase modules → bare builtins.
fn preprocess_ultra(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut result = String::with_capacity(line.len() + 16);
    let mut i = 0;

    while i < chars.len() {
        // ── Skip string literals ─────────────────────────────────────
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

        // ── Bare uppercase module: XX.func → @xx.func ────────────────
        if chars[i].is_uppercase() {
            let prev_ok = result.is_empty() || {
                let p = result.chars().last().unwrap();
                !p.is_alphanumeric() && p != '_'
            };
            if prev_ok {
                // Peek ahead: collect uppercase letters + digits
                let mut j = i + 1;
                while j < chars.len() && (chars[j].is_uppercase() || chars[j].is_ascii_digit()) {
                    j += 1;
                }
                // Must be followed by '.' to be a module reference
                if j < chars.len() && chars[j] == '.' {
                    let sigil: String = chars[i..j].iter().collect();
                    let lower = sigil.to_lowercase();
                    if MODULE_MAP.contains_key(lower.as_str()) {
                        result.push('@');
                        result.push_str(&lower);
                        i = j; // leave the '.' for normal processing
                        continue;
                    }
                }
            }
            // Not a module — pass through
            result.push(chars[i]);
            i += 1;
            continue;
        }

        // ── Bare lowercase builtin: x"..." / x~ / x\ / x( / x[ / x{digit → #x ...
        if chars[i].is_lowercase() && BUILTIN_SHORT.contains_key(&chars[i]) {
            let prev_ok = result.is_empty() || {
                let p = result.chars().last().unwrap();
                !p.is_alphanumeric() && p != '_' && p != '.'
            };
            if prev_ok {
                let letter = chars[i];
                let next = if i + 1 < chars.len() {
                    Some(chars[i + 1])
                } else {
                    None
                };
                match next {
                    // Arg-taking builtins: letter followed by arg-start char
                    Some('"') | Some('(') | Some('~') | Some('\\') | Some('[') | Some('{')
                    | Some('\'') | Some('$') => {
                        result.push('#');
                        result.push(letter);
                        result.push(' ');
                        i += 1;
                        continue;
                    }
                    Some(c) if c.is_ascii_digit() => {
                        result.push('#');
                        result.push(letter);
                        result.push(' ');
                        i += 1;
                        continue;
                    }
                    // Path/glob arg: auto-quote until delimiter
                    // Skip if followed by digit (likely math: x*2, x/3)
                    Some('/') | Some('*') => {
                        let after = if i + 2 < chars.len() {
                            Some(chars[i + 2])
                        } else {
                            None
                        };
                        if !matches!(after, Some(c) if c.is_ascii_digit()) {
                            result.push('#');
                            result.push(letter);
                            result.push(' ');
                            result.push('"');
                            i += 1;
                            while i < chars.len() && is_path_char(chars[i]) {
                                result.push(chars[i]);
                                i += 1;
                            }
                            result.push('"');
                            continue;
                        }
                    }
                    // Dot-prefixed path: ./ or .. or lone dot
                    Some('.') => {
                        let after_dot = if i + 2 < chars.len() {
                            Some(chars[i + 2])
                        } else {
                            None
                        };
                        let looks_like_path = match after_dot {
                            None => true, // lone dot at end
                            Some(c) => !c.is_alphanumeric() && c != '_',
                        };
                        if looks_like_path {
                            result.push('#');
                            result.push(letter);
                            result.push(' ');
                            result.push('"');
                            i += 1;
                            while i < chars.len() && is_path_char(chars[i]) {
                                result.push(chars[i]);
                                i += 1;
                            }
                            result.push('"');
                            continue;
                        }
                    }
                    // Zero-arg builtin: letter at end or before pipe
                    None | Some('|') => {
                        // Only match zero-arg after pipe or at stage start.
                        // Don't match after '>' — it's ambiguous before pipe
                        // normalization (could be comparison RHS).
                        let after_pipe =
                            result.trim_end().ends_with('|') || result.trim_end().is_empty();
                        if after_pipe {
                            result.push('#');
                            result.push(letter);
                            i += 1;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
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

/// Transpile a single line of agentic syntax.
fn transpile_line(line: &str) -> String {
    let mut result = line.to_string();

    // 0. Pre-process v2 ultra-compressed forms into v1 forms
    //    (bare builtins → #x, bare modules → @xx., ~ handled by lambda step)
    result = preprocess_ultra(&result);

    // 0.3. Expand for-each shorthand: *items~x:body → for x in items { body }
    //       Must run BEFORE lambda expansion (which would consume the ~x:body part)
    result = expand_for_each(&result);

    // 0.5. Expand ASCII symbol shortcuts: T→true, N→null, 'x'→"x", `cmd`→sh("cmd")
    result = expand_symbols(&result);

    // 1. SI suffixes (1k/1M/1G) are now handled natively by the grammar lexer
    //    (`read_number`), so the transpiler passes them through unchanged — the
    //    expansion pass is retired (see `expand_si_suffixes`, kept for reference).

    // 2. Expand terse lambdas: \x:expr → fn(x) => expr, also ~ prefix
    result = expand_lambdas(&result);

    // 3. Expand module sigils: @mod.func(...) → module.func(...)
    result = expand_module_sigils(&result);

    // 3.5. Expand single-char function abbreviations: file.r(...) → file.read(...)
    result = expand_func_abbreviations(&result);

    // 3.6. Auto-parens: module.func"arg" → module.func("arg")
    result = expand_auto_parens(&result);

    // 4. Expand builtin shorthands: #e "msg" → echo("msg")
    result = expand_builtin_shorthands(&result);

    // 5. Expand pipeline operator: > → |  (but not >= or >> or >")
    //    Also normalizes | spacing: a|b → a | b
    result = expand_pipelines(&result);

    // 6. Expand mutable assignment: x:=expr → let mut x = expr
    //    and plain assignment: x=expr → let x = expr
    //    (only at statement level, not inside expressions)
    result = expand_assignments(&result);

    // 7. `?val{...}` match shorthand is now parsed natively by the grammar
    //    (`Tok::Question` → parse_match), so the transpiler passes it through —
    //    the expansion pass is retired (see `expand_match`, kept for reference).

    // 8. Expand try/catch shorthand: !{expr}{"fallback"} → try { expr } catch e { "fallback" }
    result = expand_try_catch(&result);

    // 9. Expand conditional shorthand: ^cond{then}{else} → if cond { then } else { else }
    result = expand_conditional(&result);

    result
}

/// Expand SI suffixes: 1k → 1000, 5M → 5000000, 2G → 2000000000.
/// Retired from the transpile pipeline — SI scaling now lives in the grammar
/// lexer. Kept for reference and the ontology description.
#[allow(dead_code)]
fn expand_si_suffixes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check if we're inside a string literal
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

        // Check for number followed by SI suffix
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            let num_str: String = chars[start..i].iter().collect();

            if i < chars.len() {
                let suffix = chars[i];
                // Make sure the suffix isn't followed by an alphanumeric (e.g., 1key)
                let next_is_alnum =
                    i + 1 < chars.len() && (chars[i + 1].is_alphanumeric() || chars[i + 1] == '_');
                match suffix {
                    'k' | 'K' if !next_is_alnum => {
                        if let Ok(n) = num_str.parse::<u64>() {
                            result.push_str(&(n * 1_000).to_string());
                            i += 1;
                            continue;
                        }
                    }
                    'M' if !next_is_alnum => {
                        if let Ok(n) = num_str.parse::<u64>() {
                            result.push_str(&(n * 1_000_000).to_string());
                            i += 1;
                            continue;
                        }
                    }
                    'G' if !next_is_alnum => {
                        if let Ok(n) = num_str.parse::<u64>() {
                            result.push_str(&(n * 1_000_000_000).to_string());
                            i += 1;
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            result.push_str(&num_str);
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Expand terse lambdas:
///   `\x:x*2`         → `fn(x) => x * 2`
///   `\x,y:x+y`       → `fn(x, y) => x + y`
///   `\.field>val`     → `fn(__) => __.field > val`  (implicit param)
fn expand_lambdas(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    let chars: Vec<char> = s.chars().collect();
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

        // Detect lambda: \ or ~ followed by params:body  or  \./~. (implicit param)
        if (chars[i] == '\\' || chars[i] == '~') && i + 1 < chars.len() {
            let lambda_char = chars[i];
            // Ensure ~ is not inside an identifier (bitwise NOT case)
            if lambda_char == '~'
                && i > 0
                && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')
            {
                result.push(chars[i]);
                i += 1;
                continue;
            }
            i += 1;

            // Implicit parameter form: \.field...
            if chars[i] == '.' {
                // Collect the body until we hit a delimiter
                let body = collect_lambda_body(&chars, &mut i);
                result.push_str(&format!("fn(__) => __{}", body));
                continue;
            }

            // Named params form: \x:body or \x,y:body
            let param_start = i;
            while i < chars.len() && chars[i] != ':' && chars[i] != '\n' {
                i += 1;
            }
            if i < chars.len() && chars[i] == ':' {
                let params: String = chars[param_start..i].iter().collect();
                i += 1; // skip ':'
                let body = collect_lambda_body(&chars, &mut i);
                let param_list = params
                    .split(',')
                    .map(|p| p.trim())
                    .collect::<Vec<_>>()
                    .join(", ");
                result.push_str(&format!("fn({}) => {}", param_list, body));
                continue;
            }

            // Not a lambda, emit the original prefix and what we consumed
            result.push(lambda_char);
            i = param_start;
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
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

/// Expand module sigils: @mod.func(...) → module.func(...)
/// Also handles @ai(...) and @ag(...) as direct calls.
fn expand_module_sigils(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    let chars: Vec<char> = s.chars().collect();
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

        // Detect @sigil
        if chars[i] == '@' && i + 1 < chars.len() && chars[i + 1].is_alphanumeric() {
            i += 1; // skip @

            // Collect the sigil (1-2 alphanumeric chars before '.' or '(')
            let sigil_start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let sigil: String = chars[sigil_start..i].iter().collect();

            if i < chars.len() && chars[i] == '.' {
                // @mod.func style
                i += 1; // skip '.'
                if let Some(full_module) = MODULE_MAP.get(sigil.as_str()) {
                    result.push_str(full_module);
                    result.push('.');
                } else {
                    // Unknown sigil, pass through as-is
                    result.push_str(&sigil);
                    result.push('.');
                }
            } else if i < chars.len() && chars[i] == '(' {
                // @func(...) style — direct call (e.g., @ai("prompt"))
                if let Some(full_name) = MODULE_MAP.get(sigil.as_str()) {
                    result.push_str(full_name);
                } else {
                    result.push_str(&sigil);
                }
            } else {
                // Just an identifier
                if let Some(full_name) = MODULE_MAP.get(sigil.as_str()) {
                    result.push_str(full_name);
                } else {
                    result.push_str(&sigil);
                }
            }
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Expand single-character function abbreviations within module calls.
///
/// After module expansion, patterns like `file.r("path")` have the full module
/// name but an abbreviated function. This replaces `module.X(` with
/// `module.full_name(` when X is exactly one character.
fn expand_func_abbreviations(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
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

        // Look for identifier.X pattern where X is a single char
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();

            // Check for .X pattern (dot + single lowercase letter + non-ident)
            if i < chars.len()
                && chars[i] == '.'
                && i + 1 < chars.len()
                && chars[i + 1].is_lowercase()
            {
                let func_char = chars[i + 1];
                let after_func = if i + 2 < chars.len() {
                    Some(chars[i + 2])
                } else {
                    None
                };

                // Only match if the function name is exactly 1 character
                let is_single = match after_func {
                    None => true,
                    Some(c) => !c.is_alphanumeric() && c != '_',
                };

                if is_single {
                    let key = format!("{}.{}", ident, func_char);
                    if let Some(full) = FUNC_ABBREV.get(key.as_str()) {
                        result.push_str(full);
                        i += 2; // skip '.' and func_char
                        continue;
                    }
                }
            }

            result.push_str(&ident);
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Auto-parens: `module.func"arg"` → `module.func("arg")`
///
/// After module + func abbreviation expansion, detects cases where a function
/// call is missing parentheses and the argument is a string literal.
///   `file.read"path"` → `file.read("path")`
///   `http.get"url"` → `http.get("url")`
fn expand_auto_parens(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 8);
    let chars: Vec<char> = s.chars().collect();
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

        // Look for identifier.identifier pattern (module.func)
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let module: String = chars[start..i].iter().collect();

            if i < chars.len() && chars[i] == '.' {
                i += 1; // skip dot
                let func_start = i;
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    i += 1;
                }
                let func_name: String = chars[func_start..i].iter().collect();

                if !func_name.is_empty() && i < chars.len() && chars[i] == '"' {
                    // module.func"string" → module.func("string")
                    result.push_str(&module);
                    result.push('.');
                    result.push_str(&func_name);
                    result.push('(');
                    // Copy the string literal
                    result.push(chars[i]); // opening "
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
                        result.push(chars[i]); // closing "
                        i += 1;
                    }
                    result.push(')');
                    continue;
                } else {
                    // Not auto-parens — push module.func as-is
                    result.push_str(&module);
                    result.push('.');
                    result.push_str(&func_name);
                    continue;
                }
            }

            result.push_str(&module);
            continue;
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Expand for-each loop shorthand: `*items~x:body` → `for x in items { body }`
///
/// The `*` prefix at line start triggers a for-each loop. The iterator
/// expression runs until `~` or `\` (lambda), which names the loop variable.
///   `*[1,2,3]~x:echo(x)` → `for x in [1, 2, 3] { echo(x) }`
///   `*items~item:proc(item)` → `for item in items { proc(item) }`
fn expand_for_each(s: &str) -> String {
    let trimmed = s.trim_start();
    if !trimmed.starts_with('*') {
        return s.to_string();
    }

    // Guard: don't fire if * is inside an expression (e.g., multiplication)
    // Only fire when * is at the absolute start of the (trimmed) line
    if trimmed.len() < 2 {
        return s.to_string();
    }

    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 1; // skip '*'
    let mut depth = 0i32;

    // Collect the iterable expression until we hit ~ or \ at depth 0
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
        return s.to_string();
    }

    let iterable: String = chars[iter_start..i].iter().collect();
    i += 1; // skip ~ or \

    // Collect params until ':'
    let params_start = i;
    while i < chars.len() && chars[i] != ':' {
        i += 1;
    }
    if i >= chars.len() {
        return s.to_string();
    }
    let var_name: String = chars[params_start..i].iter().collect();
    i += 1; // skip ':'

    let body: String = chars[i..].iter().collect();

    format!(
        "({}) | each(fn({}) => {})",
        iterable.trim(),
        var_name.trim(),
        body.trim()
    )
}

/// Expand builtin shorthands: #e "msg" → echo("msg")
fn expand_builtin_shorthands(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    let chars: Vec<char> = s.chars().collect();
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

        // Detect #X where X is a known shorthand letter
        if chars[i] == '#' && i + 1 < chars.len() {
            let code = chars[i + 1];
            if let Some((builtin_name, _takes_lambda)) = BUILTIN_SHORT.get(&code) {
                i += 2; // skip #X

                // Skip whitespace after shorthand
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }

                // Collect the remaining args up to a pipeline operator or EOL
                let args_start = i;
                let mut depth = 0i32;
                while i < chars.len() {
                    match chars[i] {
                        '(' | '[' | '{' => depth += 1,
                        ')' | ']' | '}' if depth <= 0 => break,
                        ')' | ']' | '}' => depth -= 1,
                        '"' => {
                            i += 1;
                            while i < chars.len() && chars[i] != '"' {
                                if chars[i] == '\\' {
                                    i += 1;
                                }
                                i += 1;
                            }
                        }
                        '|' if depth == 0 => break, // v2 pipeline delimiter
                        ' ' if depth == 0 => {
                            // Check for ` > ` (pipeline delimiter)
                            if i + 2 < chars.len() && chars[i + 1] == '>' && chars[i + 2] == ' ' {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }

                let args_str: String = chars[args_start..i].iter().collect();
                let args_trimmed = args_str.trim();

                if args_trimmed.is_empty() {
                    result.push_str(&format!("{}()", builtin_name));
                } else {
                    result.push_str(&format!("{}({})", builtin_name, args_trimmed));
                }
                continue;
            }
        }

        result.push(chars[i]);
        i += 1;
    }

    result
}

/// Expand pipeline operators:
///   `>` → `|`  (but not `>=`, `>>`, or inside strings/parens)
///   `>>` → `| each`
fn expand_pipelines(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut depth = 0i32;

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

        match chars[i] {
            '(' | '[' | '{' => {
                depth += 1;
                result.push(chars[i]);
            }
            ')' | ']' | '}' => {
                depth -= 1;
                result.push(chars[i]);
            }
            '>' if depth == 0 => {
                // Check if preceded by '=' — this is '=>' (fat arrow), leave as-is
                if !result.is_empty() && result.ends_with('=') {
                    result.push('>');
                    i += 1;
                    continue;
                }
                // Check for >> (each/side-effect pipe)
                if i + 1 < chars.len() && chars[i + 1] == '>' {
                    // >> → | each — trim trailing whitespace to avoid double space
                    let trimmed = result.trim_end().len();
                    result.truncate(trimmed);
                    result.push_str(" | each(");
                    i += 2;
                    // The next expression becomes the each argument
                    // We need to collect it and close the paren
                    while i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                    let body_start = i;
                    let mut bd = 0i32;
                    while i < chars.len() {
                        match chars[i] {
                            '(' | '[' | '{' => bd += 1,
                            ')' | ']' | '}' if bd > 0 => bd -= 1,
                            '>' | '|' if bd == 0 => break,
                            _ => {}
                        }
                        i += 1;
                    }
                    let body: String = chars[body_start..i].iter().collect();
                    result.push_str(body.trim());
                    result.push(')');
                    continue;
                }
                // Check for >= (leave as-is, it's a comparison)
                if i + 1 < chars.len() && chars[i + 1] == '=' {
                    result.push('>');
                    result.push('=');
                    i += 2;
                    continue;
                }
                // Single > at depth 0 → pipe ONLY when space-delimited: ` > `
                // Bare > without spaces is a comparison operator (e.g., x>0)
                let has_space_before = !result.is_empty() && result.ends_with(' ');
                let has_space_after = i + 1 < chars.len() && chars[i + 1] == ' ';
                if has_space_before && has_space_after {
                    result.push_str("| ");
                    i += 1;
                    // Skip whitespace
                    while i < chars.len() && chars[i] == ' ' {
                        i += 1;
                    }
                    continue;
                }
                // Otherwise, it's a comparison — leave as-is
                result.push('>');
            }
            '|' if depth == 0 => {
                // Check for field projection: |.accessor → | map(fn(__) => __.accessor)
                let mut peek = i + 1;
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
                            _ => {
                                j += 1;
                            }
                        }
                    }
                    let accessor: String = chars[acc_start..j].iter().collect();
                    if !result.ends_with(' ') {
                        result.push(' ');
                    }
                    result.push_str(&format!("| map(fn(__) => __{})", accessor));
                    i = j;
                    continue;
                }

                // v2 pipe — normalize spacing: a|b → a | b
                if !result.ends_with(' ') {
                    result.push(' ');
                }
                result.push('|');
                result.push(' ');
                i += 1;
                while i < chars.len() && chars[i] == ' ' {
                    i += 1;
                }
                continue;
            }
            _ => result.push(chars[i]),
        }
        i += 1;
    }

    result
}

/// Expand assignments at statement level:
///   `x:=expr` → `let mut x = expr`
///   `x=expr`  → `let x = expr`  (if x is a simple identifier)
fn expand_assignments(s: &str) -> String {
    let trimmed = s.trim();

    // Check for walrus `:=` assignment (mutable)
    if let Some(pos) = trimmed.find(":=") {
        let lhs = trimmed[..pos].trim();
        if is_simple_identifier(lhs) {
            let rhs = trimmed[pos + 2..].trim();
            return format!("let mut {} = {}", lhs, rhs);
        }
    }

    // Check for `=` assignment (but not `==`, `!=`, `<=`, `>=`, `=>`)
    // Only if the line starts with an identifier followed by `=`
    let chars: Vec<char> = trimmed.chars().collect();
    let mut i = 0;

    // Collect potential identifier
    if i < chars.len() && (chars[i].is_alphabetic() || chars[i] == '_') {
        while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
        // Skip whitespace
        while i < chars.len() && chars[i] == ' ' {
            i += 1;
        }

        // Check for = but not ==, =>, !=
        if i < chars.len()
            && chars[i] == '='
            && (i + 1 >= chars.len() || (chars[i + 1] != '=' && chars[i + 1] != '>'))
        {
            let lhs: String = chars[..i].iter().collect();
            let lhs = lhs.trim();
            if is_simple_identifier(lhs) {
                let rhs: String = chars[i + 1..].iter().collect();
                let rhs = rhs.trim();
                // Don't expand if there's already a `let` keyword
                if !trimmed.starts_with("let ") {
                    return format!("let {} = {}", lhs, rhs);
                }
            }
        }
    }

    s.to_string()
}

/// Expand match shorthand: ?val{A=>"x",B=>"y"} → match val { A => "x", B => "y" }.
/// Retired from the transpile pipeline — `?` is now parsed natively by the
/// grammar. Kept for reference.
#[allow(dead_code)]
fn expand_match(s: &str) -> String {
    let mut result = s.to_string();

    // Repeatedly find and expand ?scrutinee{arms} anywhere in the string
    loop {
        let chars: Vec<char> = result.chars().collect();
        let mut found = false;

        let mut i = 0;
        let mut in_string = false;
        while i < chars.len() {
            if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                in_string = !in_string;
                i += 1;
                continue;
            }
            if in_string {
                i += 1;
                continue;
            }
            if chars[i] == '?' {
                let q_pos = i;
                let mut j = i + 1;

                // Collect scrutinee until {
                let scrut_start = j;
                while j < chars.len() && chars[j] != '{' {
                    j += 1;
                }
                if j >= chars.len() {
                    i += 1;
                    continue;
                }
                let scrutinee: String = chars[scrut_start..j].iter().collect();
                if scrutinee.trim().is_empty() {
                    i += 1;
                    continue;
                }

                // Collect the entire body from { to matching }
                let body_start = j;
                j += 1; // skip {
                let mut depth = 1;
                while j < chars.len() && depth > 0 {
                    match chars[j] {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                if depth != 0 {
                    i += 1;
                    continue;
                }
                let body: String = chars[body_start..j].iter().collect();

                let replacement = format!("match {} {}", scrutinee.trim(), body);
                let before: String = chars[..q_pos].iter().collect();
                let after: String = chars[j..].iter().collect();
                result = format!("{}{}{}", before, replacement, after);
                found = true;
                break;
            }
            i += 1;
        }

        if !found {
            break;
        }
    }

    result
}

/// Expand try/catch: !{expr}{"fallback"} → try { expr } catch e { "fallback" }
fn expand_try_catch(s: &str) -> String {
    let mut result = s.to_string();

    // Repeatedly find and expand !{expr}{fallback} anywhere in the string
    loop {
        let chars: Vec<char> = result.chars().collect();
        let mut found = false;

        let mut i = 0;
        let mut in_string = false;
        while i < chars.len() {
            if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                in_string = !in_string;
                i += 1;
                continue;
            }
            if in_string {
                i += 1;
                continue;
            }
            if chars[i] == '!' && i + 1 < chars.len() && chars[i + 1] == '{' {
                let bang_pos = i;
                let mut j = i + 2; // skip !{
                let mut depth = 1;
                let try_start = j;
                while j < chars.len() && depth > 0 {
                    match chars[j] {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                if depth != 0 {
                    i += 1;
                    continue;
                }
                let try_body: String = chars[try_start..j - 1].iter().collect();

                // Find optional catch body
                let replacement = if j < chars.len() && chars[j] == '{' {
                    j += 1; // skip {
                    depth = 1;
                    let catch_start = j;
                    while j < chars.len() && depth > 0 {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    if depth != 0 {
                        i += 1;
                        continue;
                    }
                    let catch_body: String = chars[catch_start..j - 1].iter().collect();
                    format!(
                        "try {{ {} }} catch e {{ {} }}",
                        try_body.trim(),
                        catch_body.trim()
                    )
                } else {
                    format!("try {{ {} }} catch e {{ null }}", try_body.trim())
                };

                let before: String = chars[..bang_pos].iter().collect();
                let after: String = chars[j..].iter().collect();
                result = format!("{}{}{}", before, replacement, after);
                found = true;
                break;
            }
            i += 1;
        }

        if !found {
            break;
        }
    }

    result
}

/// Expand conditional shorthand: ^cond{then}{else} → if cond { then } else { else }
fn expand_conditional(s: &str) -> String {
    let mut result = s.to_string();

    // Repeatedly find and expand ^cond{then}{else} anywhere in the string
    loop {
        let chars: Vec<char> = result.chars().collect();
        let mut found = false;

        // Scan for ^ outside string literals
        let mut i = 0;
        let mut in_string = false;
        while i < chars.len() {
            if chars[i] == '"' && (i == 0 || chars[i - 1] != '\\') {
                in_string = !in_string;
                i += 1;
                continue;
            }
            if in_string {
                i += 1;
                continue;
            }
            if chars[i] == '^' {
                // Check this is a conditional ^ (next non-space char is not empty, and there's a { ahead)
                let caret_pos = i;
                let mut j = i + 1;

                // Collect condition until {
                let cond_start = j;
                while j < chars.len() && chars[j] != '{' {
                    j += 1;
                }
                if j >= chars.len() {
                    i += 1;
                    continue;
                }
                let condition: String = chars[cond_start..j].iter().collect();
                if condition.trim().is_empty() {
                    i += 1;
                    continue;
                }

                // Collect then-body between { }
                j += 1; // skip {
                let mut depth = 1;
                let then_start = j;
                while j < chars.len() && depth > 0 {
                    match chars[j] {
                        '{' => depth += 1,
                        '}' => depth -= 1,
                        _ => {}
                    }
                    j += 1;
                }
                if depth != 0 {
                    i += 1;
                    continue;
                }
                let then_body: String = chars[then_start..j - 1].iter().collect();

                // Check for else body
                let replacement = if j < chars.len() && chars[j] == '{' {
                    j += 1; // skip {
                    depth = 1;
                    let else_start = j;
                    while j < chars.len() && depth > 0 {
                        match chars[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    if depth != 0 {
                        i += 1;
                        continue;
                    }
                    let else_body: String = chars[else_start..j - 1].iter().collect();
                    format!(
                        "match ({}) {{ true => ({}), _ => ({}) }}",
                        condition.trim(),
                        then_body.trim(),
                        else_body.trim()
                    )
                } else {
                    format!(
                        "match ({}) {{ true => ({}), _ => null }}",
                        condition.trim(),
                        then_body.trim()
                    )
                };

                // Replace the ^cond{then}{else} span with the expansion
                let before: String = chars[..caret_pos].iter().collect();
                let after: String = chars[j..].iter().collect();
                result = format!("{}{}{}", before, replacement, after);
                found = true;
                break;
            }
            i += 1;
        }

        if !found {
            break;
        }
    }

    result
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

    #[test]
    fn test_si_suffixes() {
        assert_eq!(expand_si_suffixes("1k"), "1000");
        assert_eq!(expand_si_suffixes("5M"), "5000000");
        assert_eq!(expand_si_suffixes("2G"), "2000000000");
        assert_eq!(expand_si_suffixes("42"), "42");
        // Don't expand inside strings
        assert_eq!(expand_si_suffixes("\"1k\""), "\"1k\"");
        // Don't expand when followed by alphanumeric
        assert_eq!(expand_si_suffixes("1key"), "1key");
    }

    #[test]
    fn test_lambda_expansion() {
        assert!(expand_lambdas(r"\x:x*2").contains("fn(x) => x*2"));
        assert!(expand_lambdas(r"\x,y:x+y").contains("fn(x, y) => x+y"));
        assert!(expand_lambdas(r"\.size>100").contains("fn(__) => __.size>100"));
    }

    #[test]
    fn test_module_sigil_expansion() {
        assert_eq!(expand_module_sigils("@f.r(\"p\")"), "file.r(\"p\")");
        assert_eq!(expand_module_sigils("@s.h()"), "sys.h()");
        assert_eq!(expand_module_sigils("@h.g(url)"), "http.g(url)");
        assert_eq!(expand_module_sigils("@ai(\"prompt\")"), "ai(\"prompt\")");
    }

    #[test]
    fn test_builtin_shorthands() {
        assert_eq!(expand_builtin_shorthands("#e \"hello\""), "echo(\"hello\")");
        assert_eq!(expand_builtin_shorthands("#l \".\""), "ls(\".\")");
        assert_eq!(expand_builtin_shorthands("#t 5"), "take(5)");
    }

    #[test]
    fn test_pipeline_expansion() {
        assert!(expand_pipelines("a > b").contains(" | "));
        // >= should NOT be expanded
        assert!(expand_pipelines("x >= 5").contains(">="));
    }

    #[test]
    fn test_assignment_expansion() {
        assert_eq!(expand_assignments("x=42").trim(), "let x = 42");
        assert_eq!(expand_assignments("x:=42").trim(), "let mut x = 42");
        // Don't expand comparisons
        assert_eq!(expand_assignments("x==42"), "x==42");
    }

    #[test]
    fn test_match_expansion() {
        assert!(expand_match("?val{A=>\"x\"}").starts_with("match val"));
    }

    #[test]
    fn test_try_catch_expansion() {
        let result = expand_try_catch("!{risky()}{\"fallback\"}");
        assert!(result.contains("try"));
        assert!(result.contains("catch"));
        assert!(result.contains("fallback"));
    }

    #[test]
    fn test_full_transpile() {
        let input = r#"; List large files
#l "./src" > #w \.size>1k > #m \.name
"#;
        let result = transpile_agentic_to_ae(input).unwrap();
        assert!(result.contains("ls("));
        assert!(result.contains("where("));
        assert!(result.contains("map("));
        assert!(result.contains("fn(__)"));
    }

    #[test]
    fn test_assignment_in_full() {
        let result = transpile_agentic_to_ae("x=42\n").unwrap();
        assert!(result.contains("let x = 42"));
    }

    #[test]
    fn test_mutable_assignment() {
        let result = transpile_agentic_to_ae("counter:=0\n").unwrap();
        assert!(result.contains("let mut counter = 0"));
    }

    #[test]
    fn test_complex_pipeline() {
        let result =
            transpile_agentic_to_ae("@h.g(\"https://api.com/data\") > @j.p(_) > #m \\.items\n")
                .unwrap();
        assert!(result.contains("http.get("), "got:\n{result}");
        assert!(result.contains("json.parse("), "got:\n{result}");
        assert!(result.contains("map("), "got:\n{result}");
    }

    // ── v2 Ultra-compressed tests ────────────────────────────────────

    #[test]
    fn test_tilde_lambda_named() {
        assert!(expand_lambdas("~x:x*2").contains("fn(x) => x*2"));
        assert!(expand_lambdas("~x,y:x+y").contains("fn(x, y) => x+y"));
    }

    #[test]
    fn test_tilde_lambda_implicit() {
        assert!(expand_lambdas("~.size>100").contains("fn(__) => __.size>100"));
    }

    #[test]
    fn test_tilde_not_in_identifier() {
        // a~b should NOT become a lambda (~ preceded by alphanumeric)
        let result = expand_lambdas("a~b");
        assert!(!result.contains("fn("), "got: {result}");
    }

    #[test]
    fn test_pipe_bar_spacing() {
        let result = expand_pipelines("a|b");
        assert_eq!(result, "a | b");
    }

    #[test]
    fn test_pipe_bar_already_spaced() {
        let result = expand_pipelines("a | b");
        assert_eq!(result, "a | b");
    }

    #[test]
    fn test_preprocess_bare_builtin_string_arg() {
        let result = preprocess_ultra("e\"hello\"");
        assert_eq!(result, "#e \"hello\"");
    }

    #[test]
    fn test_preprocess_bare_builtin_tilde_lambda() {
        let result = preprocess_ultra("w~.size>1k");
        assert_eq!(result, "#w ~.size>1k");
    }

    #[test]
    fn test_preprocess_bare_builtin_not_in_word() {
        // "let" should NOT match 'l' as a bare builtin
        let result = preprocess_ultra("let x = 42");
        assert!(!result.contains("#l"), "got: {result}");
    }

    #[test]
    fn test_preprocess_bare_module() {
        let result = preprocess_ultra("F.read(\"path\")");
        assert_eq!(result, "@f.read(\"path\")");
    }

    #[test]
    fn test_preprocess_bare_module_digraph() {
        let result = preprocess_ultra("DK.ps()");
        assert_eq!(result, "@dk.ps()");
    }

    #[test]
    fn test_preprocess_bare_module_not_in_word() {
        // "OK" followed by non-dot should NOT match
        let result = preprocess_ultra("OK");
        assert_eq!(result, "OK");
    }

    #[test]
    fn test_func_abbreviation() {
        assert_eq!(
            expand_func_abbreviations("file.r(\"p\")"),
            "file.read(\"p\")"
        );
        assert_eq!(expand_func_abbreviations("http.g(url)"), "http.get(url)");
        assert_eq!(expand_func_abbreviations("json.p(s)"), "json.parse(s)");
        // Multi-char function names should NOT be replaced
        assert_eq!(
            expand_func_abbreviations("file.read(\"p\")"),
            "file.read(\"p\")"
        );
    }

    #[test]
    fn test_func_abbreviation_sys() {
        assert_eq!(expand_func_abbreviations("sys.h()"), "sys.hostname()");
    }

    #[test]
    fn test_v2_full_pipeline() {
        let result = transpile_agentic_to_ae("e\"hello\"\n").unwrap();
        assert!(result.contains("echo(\"hello\")"), "got:\n{result}");
    }

    #[test]
    fn test_v2_ultra_pipeline() {
        let result = transpile_agentic_to_ae("l\"./src\"|w~.size>1k|m~.name\n").unwrap();
        assert!(result.contains("ls(\"./src\")"), "got:\n{result}");
        assert!(result.contains("where(fn(__) =>"), "got:\n{result}");
        assert!(result.contains("map(fn(__) =>"), "got:\n{result}");
        assert!(result.contains(" | "), "got:\n{result}");
    }

    #[test]
    fn test_v2_module_with_func_abbrev() {
        let result = transpile_agentic_to_ae("F.r(\"README.md\")\n").unwrap();
        assert!(
            result.contains("file.read(\"README.md\")"),
            "got:\n{result}"
        );
    }

    #[test]
    fn test_v2_mixed_pipeline() {
        let result =
            transpile_agentic_to_ae("H.g(\"https://api.com\")|J.p(resp)|m~.items\n").unwrap();
        assert!(result.contains("http.get("), "got:\n{result}");
        assert!(result.contains("json.parse("), "got:\n{result}");
        assert!(result.contains("map(fn(__) =>"), "got:\n{result}");
    }

    // ── v3 symbol→value tests ───────────────────────────────────────

    #[test]
    fn test_expand_symbols_true() {
        assert_eq!(expand_symbols("T"), "true");
    }

    #[test]
    fn test_expand_symbols_null() {
        assert_eq!(expand_symbols("N"), "null");
    }

    #[test]
    fn test_expand_symbols_not_in_ident() {
        // T/N inside identifiers should NOT expand
        assert_eq!(expand_symbols("TRUE"), "TRUE");
        assert_eq!(expand_symbols("NULL"), "NULL");
        assert_eq!(expand_symbols("xT"), "xT");
        assert_eq!(expand_symbols("Tx"), "Tx");
    }

    #[test]
    fn test_expand_symbols_not_before_dot() {
        // T. or N. should NOT expand (could be module ref)
        assert_eq!(expand_symbols("T.x"), "T.x");
        assert_eq!(expand_symbols("N.x"), "N.x");
    }

    #[test]
    fn test_expand_symbols_single_quotes() {
        assert_eq!(expand_symbols("'hello'"), "\"hello\"");
        assert_eq!(expand_symbols("'path/to/file'"), "\"path/to/file\"");
    }

    #[test]
    fn test_expand_symbols_backtick() {
        assert_eq!(expand_symbols("`uname -a`"), "sh(\"uname -a\")");
        assert_eq!(expand_symbols("`ls -la`"), "sh(\"ls -la\")");
    }

    #[test]
    fn test_expand_symbols_in_context() {
        // T and N in expressions with operators
        assert_eq!(expand_symbols("x=T"), "x=true");
        assert_eq!(expand_symbols("y=N"), "y=null");
        assert_eq!(expand_symbols("T,N"), "true,null");
    }

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
    fn test_preprocess_bare_path_relative() {
        let result = preprocess_ultra("l./src");
        assert!(result.contains("#l \"./src\""), "got: {result}");
    }

    #[test]
    fn test_preprocess_bare_path_absolute() {
        let result = preprocess_ultra("l/usr/bin");
        assert!(result.contains("#l \"/usr/bin\""), "got: {result}");
    }

    #[test]
    fn test_preprocess_bare_glob() {
        let result = preprocess_ultra("g*.rs");
        assert!(result.contains("#g \"*.rs\""), "got: {result}");
    }

    #[test]
    fn test_preprocess_dot_not_field() {
        // l.field should NOT auto-quote (dot before alpha = field access)
        let result = preprocess_ultra("l.field");
        assert!(!result.contains("#l \".field\""), "got: {result}");
    }

    #[test]
    fn test_preprocess_bare_path_in_pipeline() {
        let result = preprocess_ultra("l./src|w~.size>1k");
        assert!(result.contains("#l \"./src\""), "got: {result}");
        assert!(result.contains("#w ~.size>1k"), "got: {result}");
    }

    #[test]
    fn test_star_digit_not_path() {
        // x*2 should NOT be treated as a glob (it's multiplication)
        let result = preprocess_ultra("x*2");
        assert!(!result.contains("#x"), "got: {result}");
    }

    // ── v4 compactness + expandability tests ────────────────────────

    #[test]
    fn test_expand_symbols_env_var() {
        assert_eq!(expand_symbols("$HOME"), "sys.env(\"HOME\")");
        assert_eq!(expand_symbols("$PATH"), "sys.env(\"PATH\")");
        assert_eq!(expand_symbols("$MY_VAR"), "sys.env(\"MY_VAR\")");
    }

    #[test]
    fn test_expand_symbols_env_var_not_in_string() {
        // $VAR inside strings should NOT expand
        assert_eq!(expand_symbols("\"$HOME\""), "\"$HOME\"");
    }

    #[test]
    fn test_expand_symbols_env_var_in_context() {
        assert_eq!(expand_symbols("x=$HOME"), "x=sys.env(\"HOME\")");
    }

    #[test]
    fn test_expand_symbols_dollar_brace() {
        // ${...} is AetherShell string interpolation — pass through
        assert_eq!(expand_symbols("${x}"), "${x}");
    }

    #[test]
    fn test_field_projection_simple() {
        let result = expand_pipelines("a|.name");
        assert_eq!(result, "a | map(fn(__) => __.name)");
    }

    #[test]
    fn test_field_projection_chained() {
        let result = expand_pipelines("a|.data.items");
        assert_eq!(result, "a | map(fn(__) => __.data.items)");
    }

    #[test]
    fn test_field_projection_method() {
        let result = expand_pipelines("a|.trim()");
        assert_eq!(result, "a | map(fn(__) => __.trim())");
    }

    #[test]
    fn test_field_projection_in_pipeline() {
        let result = expand_pipelines("a|.name|b");
        assert!(result.contains("map(fn(__) => __.name)"), "got: {result}");
        assert!(result.ends_with("| b"), "got: {result}");
    }

    #[test]
    fn test_field_projection_not_bare_dot() {
        // "|b" should NOT trigger field projection (no dot)
        let result = expand_pipelines("a|b");
        assert_eq!(result, "a | b");
    }

    #[test]
    fn test_expand_conditional_simple() {
        let result = expand_conditional("^x>0{x*2}");
        assert_eq!(result, "match (x>0) { true => (x*2), _ => null }");
    }

    #[test]
    fn test_expand_conditional_with_else() {
        let result = expand_conditional("^x>0{x*2}{0}");
        assert_eq!(result, "match (x>0) { true => (x*2), _ => (0) }");
    }

    #[test]
    fn test_expand_conditional_not_caret() {
        // Non-^ lines should pass through
        assert_eq!(expand_conditional("x=42"), "x=42");
    }

    #[test]
    fn test_replace_standalone_basic() {
        assert_eq!(
            replace_standalone("fetch(url)", "fetch", "http.get"),
            "http.get(url)"
        );
    }

    #[test]
    fn test_replace_standalone_not_in_word() {
        // "fetching" should NOT match "fetch"
        assert_eq!(
            replace_standalone("fetching(url)", "fetch", "http.get"),
            "fetching(url)"
        );
    }

    #[test]
    fn test_replace_standalone_not_in_string() {
        assert_eq!(
            replace_standalone("\"fetch\" is good", "fetch", "http.get"),
            "\"fetch\" is good"
        );
    }

    #[test]
    fn test_preamble_def() {
        let input = "%def fetch H.g\nfetch(\"url\")\n";
        let ae = transpile_agentic_to_ae(input).unwrap();
        assert!(ae.contains("http.get(\"url\")"), "got:\n{ae}");
    }

    #[test]
    fn test_preamble_multiple_defs() {
        let input = "%def fetch H.g\n%def parse J.p\nfetch(\"url\")|parse(data)\n";
        let ae = transpile_agentic_to_ae(input).unwrap();
        assert!(ae.contains("http.get(\"url\")"), "got:\n{ae}");
        assert!(ae.contains("json.parse(data)"), "got:\n{ae}");
    }

    // ── Robustness / edge-case tests ────────────────────────────────

    #[test]
    fn test_single_quote_embedded_double() {
        // Embedded " inside single quotes must be escaped in output
        let result = expand_symbols(r#"'she said "hi"'"#);
        assert_eq!(result, r#""she said \"hi\"""#);
    }

    #[test]
    fn test_single_quote_no_embedded_double() {
        // Normal case: no embedded double quotes
        assert_eq!(expand_symbols("'hello'"), "\"hello\"");
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
        // Semicolons inside strings are NOT comments
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
    fn test_inline_comment_integration() {
        let ae = transpile_agentic_to_ae("x=42 ; my var\n").unwrap();
        assert!(ae.contains("let x = 42"), "got:\n{ae}");
        assert!(ae.contains("// my var"), "got:\n{ae}");
    }

    #[test]
    fn test_dollar_not_expanded_in_backtick() {
        // $ inside backticks should NOT be expanded (R03)
        let result = expand_symbols("`echo $HOME`");
        assert_eq!(result, "sh(\"echo $HOME\")");
    }

    #[test]
    fn test_dollar_not_expanded_in_string() {
        // $ inside double-quoted strings should NOT be expanded
        assert_eq!(expand_symbols("\"$HOME\""), "\"$HOME\"");
    }

    #[test]
    fn test_gt_bare_is_comparison() {
        // > without spaces is comparison, NOT pipe (R09)
        let result = expand_pipelines("x>0");
        assert_eq!(result, "x>0");
    }

    #[test]
    fn test_gt_spaced_is_pipe() {
        // > with spaces is pipe
        let result = expand_pipelines("a > b");
        assert!(result.contains(" | "), "got: {result}");
    }

    #[test]
    fn test_gte_preserved() {
        // >= is never a pipe (R10)
        assert_eq!(expand_pipelines("x >= 5"), "x >= 5");
    }

    #[test]
    fn test_fat_arrow_preserved() {
        // => is never a pipe (R10)
        assert!(expand_pipelines("A => \"x\"").contains("=>"));
    }

    #[test]
    fn test_double_gt_is_each() {
        let result = expand_pipelines("a >> f");
        assert!(result.contains("each(f)"), "got: {result}");
    }

    #[test]
    fn test_tilde_not_after_alpha() {
        // ~ after alphanumeric is NOT a lambda (R11, bitwise NOT)
        let result = expand_lambdas("a~b");
        assert!(!result.contains("fn("), "got: {result}");
    }

    #[test]
    fn test_assignment_not_on_comparison() {
        // == != <= >= => should NOT trigger assignment (R12)
        assert!(!expand_assignments("x==42").contains("let"));
        assert!(!expand_assignments("x!=42").contains("let"));
    }

    #[test]
    fn test_si_suffix_not_on_ident() {
        // 1key should NOT expand k (R19)
        assert_eq!(expand_si_suffixes("1key"), "1key");
    }

    #[test]
    fn test_nested_braces_conditional() {
        // Nested {} inside conditional body should work
        let result = expand_conditional("^x>0{{a:1}}");
        assert_eq!(result, "match (x>0) { true => ({a:1}), _ => null }");
    }

    #[test]
    fn test_nested_braces_try_catch() {
        let result = expand_try_catch("!{{a:1}}{\"err\"}");
        assert!(result.contains("{a:1}"), "got: {result}");
        assert!(result.contains("err"), "got: {result}");
    }

    #[test]
    fn test_multiple_env_vars() {
        let result = expand_symbols("$HOME/$USER");
        assert_eq!(result, "sys.env(\"HOME\")/sys.env(\"USER\")");
    }

    #[test]
    fn test_dollar_number_not_expanded() {
        // $1, $2 etc. (positional params) — not alpha, so NOT expanded
        assert_eq!(expand_symbols("$1"), "$1");
    }

    #[test]
    fn test_empty_line_passthrough() {
        let ae = transpile_agentic_to_ae("\n\n\n").unwrap();
        // Should just have the header and no content lines
        assert_eq!(ae.lines().count(), 1); // just the transpiler comment
    }

    #[test]
    fn test_bare_let_not_builtin() {
        // "let" starts with 'l' which is a builtin — but 'l' is followed by 'e' (alpha)
        // so it should NOT be treated as a bare builtin (R06)
        let result = preprocess_ultra("let x = 42");
        assert!(!result.contains("#l"), "got: {result}");
    }

    #[test]
    fn test_field_projection_double_pipe() {
        // Multiple field projections in a row
        let result = expand_pipelines("a|.name|.upper()");
        assert!(result.contains("map(fn(__) => __.name)"), "got: {result}");
        assert!(
            result.contains("map(fn(__) => __.upper())"),
            "got: {result}"
        );
    }

    #[test]
    fn test_ontology_completeness() {
        // Every ontology rule with a non-empty example_output should produce
        // the expected output when run through the full transpiler.
        for cat in ONTOLOGY {
            for rule in cat.rules {
                let (input, expected) = rule.example;
                if expected.is_empty() {
                    continue; // %def doesn't produce output
                }
                let ae = transpile_agentic_to_ae(&format!("{}\n", input)).unwrap();
                assert!(
                    ae.contains(expected),
                    "Ontology validation failed!\n  Category: {}\n  Pattern: {}\n  Input: {}\n  Expected output to contain: {}\n  Got: {}",
                    cat.name, rule.pattern, input, expected, ae
                );
            }
        }
    }

    #[test]
    fn test_ontology_has_all_categories() {
        // Ensure the ontology covers all pipeline stages
        let names: Vec<&str> = ONTOLOGY.iter().map(|c| c.name).collect();
        assert!(names.contains(&"Preprocessing"), "missing Preprocessing");
        assert!(names.contains(&"Symbols"), "missing Symbols");
        assert!(names.contains(&"SI Suffixes"), "missing SI Suffixes");
        assert!(names.contains(&"Lambdas"), "missing Lambdas");
        assert!(names.contains(&"Module Sigils"), "missing Module Sigils");
        assert!(
            names.contains(&"Function Abbreviations"),
            "missing Function Abbreviations"
        );
        assert!(
            names.contains(&"Builtin Shorthands"),
            "missing Builtin Shorthands"
        );
        assert!(names.contains(&"Pipelines"), "missing Pipelines");
        assert!(names.contains(&"Assignments"), "missing Assignments");
        assert!(names.contains(&"Match"), "missing Match");
        assert!(names.contains(&"Try/Catch"), "missing Try/Catch");
        assert!(names.contains(&"Conditional"), "missing Conditional");
        assert!(names.contains(&"Comments"), "missing Comments");
        assert!(names.contains(&"Preamble"), "missing Preamble");
    }

    #[test]
    fn test_ontology_describe_not_empty() {
        let desc = describe_ontology();
        assert!(desc.len() > 1000, "Ontology description too short");
        assert!(desc.contains("MODULE_MAP"), "missing MODULE_MAP section");
        assert!(
            desc.contains("BUILTIN_SHORT"),
            "missing BUILTIN_SHORT section"
        );
        assert!(desc.contains("FUNC_ABBREV"), "missing FUNC_ABBREV section");
        assert!(
            desc.contains("Conflict Resolution"),
            "missing conflict rules"
        );
    }

    #[test]
    fn test_reserved_chars_complete() {
        // All reserved chars that have special meaning should be listed
        let chars: Vec<char> = RESERVED_CHARS.iter().map(|(c, _, _)| *c).collect();
        assert!(chars.contains(&'|'));
        assert!(chars.contains(&'>'));
        assert!(chars.contains(&'^'));
        assert!(chars.contains(&'?'));
        assert!(chars.contains(&'!'));
        assert!(chars.contains(&'~'));
        assert!(chars.contains(&'$'));
        assert!(chars.contains(&'#'));
        assert!(chars.contains(&'@'));
        assert!(chars.contains(&'%'));
        assert!(chars.contains(&';'));
        assert!(chars.contains(&'='));
        assert!(chars.contains(&'T'));
        assert!(chars.contains(&'N'));
    }

    #[test]
    fn test_conflict_rules_numbered() {
        // All conflict rules should be numbered R01-R19
        for (i, rule) in CONFLICT_RULES.iter().enumerate() {
            let expected_prefix = format!("R{:02}", i + 1);
            assert!(
                rule.starts_with(&expected_prefix),
                "Rule {} should start with {}, got: {}",
                i,
                expected_prefix,
                rule
            );
        }
    }

    // ─── Auto-parens tests ─────────────────────────────────────────

    #[test]
    fn test_auto_parens_basic() {
        let result = expand_auto_parens(r#"file.read"path.txt""#);
        assert_eq!(result, r#"file.read("path.txt")"#);
    }

    #[test]
    fn test_auto_parens_with_existing_parens() {
        // Should NOT double-wrap when parens already present
        let result = expand_auto_parens(r#"file.read("path.txt")"#);
        assert_eq!(result, r#"file.read("path.txt")"#);
    }

    #[test]
    fn test_auto_parens_multiple_in_pipeline() {
        let result = expand_auto_parens(r#"file.read"a.txt" | http.get"url""#);
        assert_eq!(result, r#"file.read("a.txt") | http.get("url")"#);
    }

    #[test]
    fn test_auto_parens_no_string_arg() {
        // No auto-parens when followed by non-string
        let result = expand_auto_parens("file.read(x)");
        assert_eq!(result, "file.read(x)");
    }

    #[test]
    fn test_auto_parens_escaped_string() {
        let result = expand_auto_parens(r#"file.read"path with \"quotes\"""#);
        assert_eq!(result, r#"file.read("path with \"quotes\"")"#);
    }

    #[test]
    fn test_auto_parens_full_pipeline() {
        // End-to-end: F.r"p" → file.read("p")
        let ae = transpile_agentic_to_ae("F.r\"README.md\"\n").unwrap();
        assert!(ae.contains("file.read(\"README.md\")"), "got:\n{ae}");
    }

    // ─── For-each loop tests ───────────────────────────────────────

    #[test]
    fn test_for_each_basic() {
        let result = expand_for_each("*[1,2,3]~x:echo(x)");
        assert_eq!(result, "([1,2,3]) | each(fn(x) => echo(x))");
    }

    #[test]
    fn test_for_each_identifier() {
        let result = expand_for_each("*items~item:proc(item)");
        assert_eq!(result, "(items) | each(fn(item) => proc(item))");
    }

    #[test]
    fn test_for_each_backslash() {
        let result = expand_for_each(r"*items\x:echo(x)");
        assert_eq!(result, "(items) | each(fn(x) => echo(x))");
    }

    #[test]
    fn test_for_each_no_match() {
        // Not at line start — pass through
        let result = expand_for_each("x = 2 * 3");
        assert_eq!(result, "x = 2 * 3");
    }

    #[test]
    fn test_for_each_nested_brackets() {
        let result = expand_for_each("*arr.range(10)~i:echo(i)");
        assert_eq!(result, "(arr.range(10)) | each(fn(i) => echo(i))");
    }

    #[test]
    fn test_for_each_full_pipeline() {
        let ae = transpile_agentic_to_ae("*[1,2,3]~x:echo(x)\n").unwrap();
        assert!(
            ae.contains("([1,2,3]) | each(fn(x) => echo(x))"),
            "got:\n{ae}"
        );
    }

    // ─── New FUNC_ABBREV tests ─────────────────────────────────────

    #[test]
    fn test_func_abbrev_new_modules() {
        // a2a
        assert_eq!(
            expand_func_abbreviations("a2a.s(\"agent\", msg)"),
            "a2a.send(\"agent\", msg)"
        );
        // helm
        assert_eq!(expand_func_abbreviations("helm.l()"), "helm.list()");
        // terraform
        assert_eq!(
            expand_func_abbreviations("terraform.p()"),
            "terraform.plan()"
        );
        // npm
        assert_eq!(
            expand_func_abbreviations("npm.i(\"pkg\")"),
            "npm.install(\"pkg\")"
        );
        // go
        assert_eq!(expand_func_abbreviations("go.b()"), "go.build()");
        // trivy
        assert_eq!(
            expand_func_abbreviations("trivy.s(\"target\")"),
            "trivy.scan(\"target\")"
        );
    }

    #[test]
    fn test_func_abbrev_end_to_end() {
        // Full pipeline: HM.l() → helm.list()
        let ae = transpile_agentic_to_ae("HM.l()\n").unwrap();
        assert!(ae.contains("helm.list()"), "got:\n{ae}");
    }
}
