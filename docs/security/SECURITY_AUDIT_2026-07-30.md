# Security Audit — 2026-07-30, updated 2026-08-06

Scope: the `master` tree, covering the `aethershell`, `aethershell-lsp` and
`agentic-eval` crates, the browser extension, and the CI and release workflows.
The first pass reviewed `2d8b969`; the second and third passes (2026-08-04, findings 6–11)
reviewed `30e3586` and reached the surfaces the first pass did not: the HTTP
API, the process-execution gate, native plugin loading, `unsafe`, MCP
discovery, deserialization of untrusted input, and argument handling at the
fixed-program `Command::new` sites.

Method: `cargo audit` (1173 RUSTSEC advisories), `cargo deny check` (advisories,
bans, licenses, sources), targeted source review of the cryptographic, policy,
audit, HTTP and process-spawning surfaces, review of all 31 `unsafe` blocks, and
a repository-wide scan for credentials and personal data. Findings were fixed in
this pass unless recorded otherwise below.

Frameworks requested and addressed: CVE/RUSTSEC, MITRE ATT&CK, NIST FIPS, and
CMMC 2.0.

---

## Summary

| # | Finding | Class | Severity | Status |
|---|---------|-------|----------|--------|
| 6 | Agent API executed code for unauthenticated callers | CWE-306 / CWE-352 | **Critical** | Fixed |
| 7 | Exec gate covered the name `sh`, not the capability | CWE-184 / CWE-693 | **High** | Fixed |
| 10 | Argument injection into PowerShell, AppleScript and archivers | CWE-78 / CWE-88 | **High** | Fixed |
| 11 | sqlite3 dot-commands and tmux exec paths ungated | CWE-77 | **High** | Fixed |
| 12 | Docs directed users to an unclaimed PyPI package name | CWE-494 | **High** | Fixed (name registered 2026-08-07) |
| 13 | Recursion aborted the process; usable depth was ~30 | CWE-674 | Medium | Fixed (large stack + depth limit) |
| 1 | `quinn-proto` remote memory exhaustion | RUSTSEC-2026-0185 | High (7.5) | Fixed |
| 2 | Unimplemented crypto builtins reported success | CWE-347 / CWE-311 | High | Fixed |
| 4 | `rbac.*`, `perm.acl_*`, `sso.*` advertised but unimplemented | CWE-1104 | Medium | Documented, not implemented |
| 9 | MCP servers adopted by port convention, unauthenticated | CWE-306 | Medium | Documented, not fixed |
| 8 | `static mut` PRNG state mutated without synchronization | CWE-362 | Low | Fixed |
| 3 | Committed WASM leaked a developer's username | CWE-532 | Low | Fixed (tree only) |
| 5 | Builtin registry split hides names from agent discovery | Correctness | Low | Documented |

Nothing in this audit indicates a compromise or data exposure to a third party.
Finding 3 concerns one developer's username, published in a public repository.

Finding 12 was the only item with a window a third party could close first — the
documentation directed users to an unregistered PyPI name, so every reader
following the install instructions was one attacker-registration away from
running arbitrary code. **Closed 2026-08-07**: `aethershell` 1.5.0 is published
and the name is claimed (§12a).

No finding in this audit is now open. What remains (§"Open decisions") is
policy and design work, not unpatched defects.

The table is ordered by severity; the sections below are numbered in discovery
order, so the later passes (6–11) are written up first.

Findings 6, 7, 10 and 11 are the significant ones, and all were reachable in
the *intended* configuration rather than an unusual one. They compound: an
unauthenticated `/api/v1/eval` (6) reaching an exec surface that agent mode did
not actually gate (7), with five further routes to execution that bypassed even
that gate (10, 11), means the hardening a deployment believed it had from
`AETHER_MODE=agent` would not have constrained a drive-by request.

A pattern worth naming: 7, 10 and 11 were each found by testing an assumption
the *previous* finding had rested on. Reasoning about which programs are
dangerous produced two wrong answers before enumeration produced a defensible
one.

The pattern held through a fifth iteration (10g, 2026-08-06): a second lint,
written to close 10f's stated residual, found three further live injections of a
shape the *first* lint could not match. Each layer has now found something every
prior layer missed. Read the green state as "no defect the current five methods
can see", not as absence.

And a sixth (6a, same day), in a different form: the *obvious* fix for the Agent
API's missing deadline — mounting a `TimeoutLayer` — does nothing, because the
handlers never yield to it. Only running the assertion before the fix revealed
that. The recurring lesson across every finding in this audit is the same one:
a change that reviews as correct is not evidence that it works.

---

## 6. Agent API executed code for unauthenticated callers (CWE-306, CWE-352)

The most serious finding of the three passes.

`agent_api::server` mounted `POST /api/v1/eval` — documented as "evaluate raw
AetherShell code" — with **no authentication on any route**. `enable_cors`
defaults to `true`, which layered:

```rust
CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
```

on top. Binding loopback is not a boundary against a browser: any web page the
user visited while the server was running could preflight successfully and POST
to `127.0.0.1:3002`. That is drive-by remote code execution on the user's
machine, triggered by visiting a page, not a local-only convenience.

Every route except `/health` now requires `Authorization: Bearer <token>`,
enforced by `axum::middleware::from_fn` applied *before* the CORS layer so it
wraps every route. `/health` stays open so liveness probes keep working, which
is why the exemption is itself tested.

The token comes from `--token`, else `AETHER_API_TOKEN`, else one is generated
from 32 random bytes and printed at startup. There is deliberately no
configuration that disables authentication. `AETHER_API_TOKEN` is the documented
path because an argv token is readable by every other process on the host.

Comparison is constant-time, so the token cannot be recovered a byte at a time
by timing the 401. Binding a non-loopback address now warns that the token is
the only thing in front of an arbitrary-code evaluator.

`tests/agent_api_auth.rs` drives the real router over a real TCP socket rather
than a reimplementation of the middleware: no token, three wrong tokens
(including a *prefix* of the real one, which catches a comparison that stops at
the shorter length), the correct token, and `/health` without one.

**Residual risk.** A bearer token over plain HTTP is readable by anything on the
path. Non-loopback deployments need TLS; the warning says so, but nothing
enforces it. There is no token rotation, revocation, or per-client identity —
this is one shared secret, appropriate for a local agent bridge and not for a
multi-tenant service.

### 6a. The request deadline, and why mounting the layer was not the fix

Authentication turned "anyone can run code here" into "an authenticated caller
can run code here". That left availability: no route had a deadline, and
`/api/v1/eval` evaluates arbitrary code, so holding a worker forever was a
one-line POST. Listed as open item 6 for the maintainer until 2026-08-06.

The obvious fix is a `TimeoutLayer`, and the obvious fix is **wrong on its own**
— which is worth recording, because mounting it produces a diff that reviews as
correct and changes nothing.

`process_request` is synchronous and was called directly from the `async`
handlers. Tower's timeout races the deadline against the inner future *within
the same poll*: if the inner call never yields, the timeout branch is never
reached. A handler that blocks its worker for twenty seconds is therefore
immune to a one-second deadline mounted directly above it.

This was **measured, not reasoned about**. The test asserting a 408 was written
before the handler change and failed: the client hit its own 15-second timeout
while the server never responded. That failure is the evidence the layer alone
was decorative; without running it, the mounted layer would have been recorded
as a fix.

The four execution handlers now run via `tokio::task::spawn_blocking`, so the
handler future yields, the deadline fires, the async worker is freed, and the
caller gets 408.

**Bounded honestly.** Dropping a `spawn_blocking` handle does not cancel the
closure. A wedged evaluation keeps a blocking-pool thread until it finishes on
its own. So the HTTP deadline alone converts *one request wedges the server*
into *the server keeps answering while leaked threads accumulate* against a
bounded pool (512 threads by default). It is a real improvement and it is not a
bound on evaluation.

### 6b. The interpreter-level deadline

The residual above is now largely closed. `safety::enter_deadline` sets a
per-thread limit that `eval_expr` checks, so evaluation stops itself rather than
running until it happens to finish.

Three details carry the design:

- **The language has no loop constructs.** Unbounded work arrives as recursion
  or as large data, both of which pass through `eval_expr`, so one check covers
  it. This was worth confirming rather than assuming — a check placed in a loop
  evaluator would have covered nothing, because there is no loop evaluator.
- **The clock is sampled, not read every node.** A counter reads
  `Instant::now()` every 1024 steps. With no deadline set — the REPL, scripts,
  every test — the check is one thread-local read.

  Stated precisely, because this document is otherwise strict about it: the
  sampling is a **precaution, not a measured optimisation**. No before/after
  benchmark was run. A debug-build attempt was abandoned as useless — the
  workload took 327 s per run and debug timings distort relative costs — and a
  release build was not available (it exhausted the disk). The reasoning is that
  a clock read costs tens of nanoseconds on the interpreter's hot path and the
  deadline needs no per-node resolution; that is a judgement, not a measurement,
  and it should be measured before anyone tunes the interval.
- **The guard restores rather than clears.** These threads are pooled and
  reused. A deadline left set would make the *next* request on that thread fail
  instantly, which is a worse bug than the one being fixed. There is a test for
  precisely that, and another for nesting.

The Agent API gives the interpreter 90% of the HTTP timeout, so evaluation stops
itself before the connection is torn away and the caller gets an error that says
what happened instead of a bare 408.

**Still not bounded: builtins already blocked in a syscall.** `sleep 3600`, a
subprocess wait, a large network read — none of these return to the interpreter
to be asked, so none can be interrupted. The gap is narrower than before, not
gone. A `sleep`-based denial is still available to an authenticated caller.

**Verified by disabling the fix.** `tests/eval_deadline.rs` asserts that a long
evaluation stops near its deadline. To confirm the test discriminates rather
than passing for free, the `check_deadline()` call was commented out and the
test re-run: it hung until killed at 400 seconds, versus passing in 26 with the
check in place. Given that this audit has now recorded three separate changes
that reviewed as correct and did nothing, a passing test is not evidence until
its failing form has been seen.

**A related gap, found while testing this and not fixed.** There is no recursion
depth limit. `let f = fn(x) => f(x)` grows the Rust stack until the process
aborts on stack overflow — which the deadline cannot catch, because the abort is
not a `Result`. That is a crash rather than a hang, and it deserves its own
treatment (a depth counter in `call_lambda`).

The SSE `/api/v1/stream/*` routes and the WebSocket are exempt by construction:
the deadline is applied to the timed router before the long-lived routes are
merged in. Both properties are tested — that the deadline fires, and that it
does not touch the streaming routes — so neither can be lost silently.

**A sixth detection method, and the same lesson.** Findings 10a–10g came from
reading, testing, two lints and a type check. This one came from *executing the
assertion before the fix* and watching it fail for a reason the code review
would not have surfaced. Writing the failing test first is what distinguished
"layer mounted" from "deadline enforced".

**Breaking change.** Existing API clients must send the header.

## 7. The exec gate covered the name `sh`, not the capability (CWE-184, CWE-693)

`bi_sh` was the **only** builtin in the tree that called `safety::guard` with
`Effect::Exec` — 14 guard calls in total against 668 `Command::new` sites. The
exec control was therefore a denylist of exactly one *name*, while several other
builtins hand a caller-controlled string straight to a shell:

```rust
Command::new("timeout").args([&secs, "sh", "-c", &command])   // bi_timeout_cmd
Command::new("sh").args(["-c", &full_cmd])                     // bi_xargs_exec
Command::new(&cmd).args(&spawn_args)                           // bi_proc_spawn
```

This was demonstrated rather than inferred. In agent mode, with `sh` disabled
outright — the intended hardened configuration, since `sh` refuses unless
`AETHER_ALLOW_SH=true` — the following ran to completion:

```
sh          => Err("sh() is disabled for security…")
timeout_cmd => Ok({exit_code: 0, …})     and the marker file existed afterwards
```

No approval prompt, and no `exec`-classified audit entry, so the hash-chained
log recorded the wrong thing as well. An agent denied `sh` had nine other names
for the same capability: `timeout`, `xargs`, `proc.spawn`, `nohup`, `strace`,
`ltrace`, `perf.stat`, `perf.record`, `lxc.exec`.

`safety::guard_exec` is now the single choke point for "the argument *is* a
command", applied to all nine. `safety::effect_of` classifies the same set as
`Effect::Exec`, because it feeds `agent_api`'s dynamic discovery — which was
telling agents that `timeout("rm -rf /")` was a `pure` call.

`tests/exec_gate_coverage.rs` asserts the property that matters — after a gated
call, *the side effect does not exist* — rather than any error string, and pins
`effect_of` against the `guard_exec` call sites, since drift between those two
lists is exactly what reopens this. Human mode is default-allow by design and a
test holds that line.

**Scope of the fix, stated precisely.** This closes the *arbitrary command*
class. It does not gate the ~650 remaining `Command::new` sites that invoke a
fixed program with caller-supplied data arguments (`ping -c 4 $host`,
`helm status $release`).

Those were assumed to be a materially weaker class, on the reasoning that they
cannot run an attacker-chosen program. **That assumption was wrong**, and
finding 10 is the result of testing it rather than resting on it: a fixed
program can still be *told* to run a command by an argument, and 43 such sites
were reachable. The remaining sites in this class pass data to programs with no
known command-executing flag, which is a weaker claim than "safe" and is why
this stays on the open list.

**Breaking change.** Agent-mode scripts using these nine now need approval.

## 8. `static mut` PRNG state mutated without synchronization (CWE-362)

`neural::rand_f64` and `evolution::rand_f64` mutated a `static mut` from an
`unsafe` block with no synchronization. Builtins are reachable from the
multi-threaded agent API server, so two concurrent requests were a data race —
undefined behaviour irrespective of how benign the values are.

Both are now `AtomicU64` with `Relaxed` ordering: the state orders nothing else,
and a single-threaded caller sees the identical sequence, so `seed_rng`
reproducibility is preserved. Both are now documented as non-cryptographic,
because an LCG named `rand_f64` is a tempting thing to reach for. They are used
only for network weights and mutation rates; no key, token or salt derives from
them, which was checked rather than assumed.

## 10. Argument injection into PowerShell, AppleScript and archivers (CWE-78, CWE-88)

Found in the third pass, while reviewing the fixed-program `Command::new` sites
that finding 7 explicitly did not cover. Three defects, each verified by
execution — and 10c only because the first write-up of 10a made an untested
claim that the rest of the class was safe.

### 10a. Single-quoted PowerShell command injection (CWE-78, Windows)

Windows builtins build commands by interpolating values into single-quoted
PowerShell literals:

```rust
format!("Start-Service '{}'", name)
```

A single-quoted PowerShell string ends at the first `'`, so a value containing
one closes the literal and everything after it is executed. Demonstrated with a
service name of `x'; New-Item -ItemType File -Path '<tmp>' -Force; '`, which
created the file. Re-run after the fix, the same payload is treated as an
ordinary string and no file appears.

This was **not** limited to one builtin. 17 sites interpolated caller-controlled
strings into PowerShell: service control (start/stop/restart/set), Hyper-V
(create/delete/start/stop/restart/status/snapshot/clone), `Get-EventLog`,
`Get-LocalGroupMember`, `Get-FileHash`, `Get-ItemProperty` (registry), three
`NetFirewallRule` operations, `Set-Clipboard`, `Compress-Archive`,
`Expand-Archive` and `ZipFile::OpenRead`. Escaping was inconsistent rather than
absent — two sites already doubled quotes correctly, which is why the defect
survived: the pattern looked handled.

Severity is high but Windows-only, and it bypasses everything else: an agent
denied `sh` and denied the nine exec builtins from finding 7 could still reach
arbitrary execution through `service.start`.

`safety::ps_quote` is now the single escaping point. It returns the value
*including* its surrounding quotes, so callers interpolate `{}` rather than
`'{}'` — a missed call site is then a PowerShell syntax error rather than a
silently unquoted value. It is documented as single-quoted-context only, since a
double-quoted PowerShell string also expands `$` and backtick.

### 10b. Option injection into archivers (CWE-88)

`tar -cvf <archive> <files>` passed a caller-supplied file list with no `--`
separator. GNU tar parses a "file" named `--use-compress-program=sh -c '…'` as
an option and runs it; Info-ZIP's `-TT` sets the command used to test an
archive; `zip <archive> <files>` took the archive name as the *first
positional*, so that was injectable too.

Like 10a, these had no policy gate, so they bypassed the `Effect::Exec`
approval added by finding 7 the same day.

`safety::reject_option_like` refuses positional path arguments beginning with
`-`, and `--` is passed to `tar` as defence in depth. Refusing is the check that
does not depend on the tool's own parser; the error names `./-name` as the way
to reach a file genuinely called that.

`tests/argument_injection.rs` covers both: the escaping rule (including that it
alters nothing but quotes), the exact payload proven to execute, the archiver
option payloads, and that ordinary paths still pass.

### 10c. Double-quoted PowerShell and AppleScript interpolation (CWE-78)

**A correction.** The first write-up of finding 10 claimed the double-quoted
PowerShell sites "use fixed strings or already backtick-escape". That was
asserted from reading and was wrong. Tested, it fails:

A *double*-quoted PowerShell string expands `$`, so `$(command)` executes with
no quote character in the payload at all. The sites that escaped `"` as `` `" ``
— `crypto.hash`, `crypto.hmac`, `base64_encode`/`decode` — stopped nothing,
and several more escaped nothing whatsoever: 21 sites across GUI window
control (`FindWindow`), screenshot paths, toast notifications, dialog titles
and descriptions, `Read-Host` prompts, and password generation.

Demonstrated: `[Convert]::ToBase64String(…GetBytes("$(New-Item …)"))` created
the file. After the fix the same input is base64-encoded as literal text.

All 21 now interpolate through `ps_quote`, i.e. into a **single**-quoted
literal, which removes expansion entirely rather than trying to enumerate the
metacharacters that need escaping. That is why the fix is single-quoting rather
than a better double-quote escaper.

The two macOS `osascript` sites (`display notification`, `display dialog`) have
the same shape: AppleScript literals are double-quoted and escape with a
backslash, so an unescaped `"` closes the literal and `" & (do shell script
"…") & "` runs a command. `safety::applescript_quote` escapes backslash *first*,
then the quote — the other order is undone by the payload.

### 10d. The lint that found what three manual passes missed

The residual noted above — that nothing *mechanically* prevented a future site
from interpolating directly — was closed with
`tests/no_raw_shell_interpolation.rs`, which scans for the shape of the bug
(a quoted `{}` on a line that looks like a PowerShell or AppleScript command).

It failed on first run, flagging **six sites that 10a and 10c had both missed**:

```
Resolve-DnsName '{}'                              ×2  (hostname, ip)
[System.Windows.Forms.SendKeys]::SendWait("{}")   ×3  (key, combo, escaped)
[System.Windows.Forms.MessageBox]::Show("{}","{}")×1  (message, title)
```

All six were live injection vectors, and all six survived three passes of
careful manual review by the same reviewer who wrote the earlier findings. That
is the strongest evidence in this document for a specific claim: **on a codebase
this size, reading does not find this class reliably, and a mechanical check
does.** The finding is not really the six sites; it is that they existed after
the class had supposedly been closed twice.

The lint is heuristic and carries an `ALLOWED` list for error messages that
legitimately contain `'{}'`. It has a companion test asserting it still fires on
the pre-fix shapes, because a lint that cannot fail reads as coverage while
providing none.

### 10e. The type now records that quoting happened

The residual left by 10d — that a lint catches the *shape* but not the
semantics — is closed. `ps_quote` and `applescript_quote` no longer return
`String`; they return `PsLiteral` and `AppleScriptLiteral`, newtypes with a
private field that nothing outside `safety` can construct.

The refactor was almost free, because both render through `Display`: every
existing `format!("… {}", ps_quote(&v))` site compiled unchanged, and the
compiler surfaced exactly two places that had relied on the `String` (a
`Vec::join` in each zip builtin). That is the argument for doing it this way
round — the compiler enumerated the call sites, which is the thing manual review
demonstrably failed at three times in this document.

Neither type implements `From<String>` or `Deref<Target = str>`. Either would
let an unescaped value stand in for an escaped one, which is the entire property
being bought.

**What is now defended, and how.** Three independent layers, in increasing
order of strength:

1. The escapers themselves (10a, 10c) — correct behaviour.
2. `tests/no_raw_shell_interpolation.rs` (10d) — catches the textual shape of a
   new raw interpolation, including in code the type system never sees.
3. The newtypes (10e) — a `String` cannot be passed where a quoted literal is
   required.

### 10f. The macro, and the two sites only a type check could find

The residual above — that `format!` accepts anything `Display`, so a bare
`String` still compiles — is now closed. `ps_script!` and `applescript!` bind
every argument through a sealed `PsArg`/`AppleScriptArg` trait before
formatting, so a `String` is a compile error naming the argument. 56 PowerShell
sites and 2 AppleScript sites were converted mechanically, with the
transformation verified by normalising macro names and diffing: nothing changed
but the call.

`PsArg` is implemented for exactly three things, and the reasoning for each
matters:

- `PsLiteral` — escaped by construction.
- Integers — no PowerShell metacharacter has a numeric representation.
- `&'static str` — a compile-time literal cannot be caller data. This covers the
  common `match algo { "sha256" => "SHA256", … }` shape. (`String::leak` could
  forge one, but that is a deliberate act, not an accident.)

Not implemented for `String` or a borrowed `&str`: that is how caller data
arrives.

**The macro immediately found two injection sites that nothing else could
have.** Both interpolate *unquoted*, so the 10d lint — which looks for quoted
placeholders — is blind to them by construction, and three manual passes had
missed them:

```
New-VM -MemoryStartupBytes {} -NewVHDSizeBytes {}   vm.create(name, memory, disk)
New-NetFirewallRule -LocalPort {}                   firewall.allow(port)
```

Neither can be quoted — `-MemoryStartupBytes '4GB'` is not
`-MemoryStartupBytes 4GB`, because `4GB` is a numeric literal — so
`safety::ps_bare_number` validates them instead against a whitelist of digits,
at most one decimal point, and an optional `KB`/`MB`/`GB`/`TB`/`PB` suffix.

**Four defects, four different detection methods.** 10a and 10b came from
reading. 10c came from testing an assertion that reading had produced and got
wrong. 10d came from a lint that fired on six sites reading had missed twice.
10f came from a type check, on two sites the lint could not see. No single
method found this class; that is the durable lesson, and it is why all three
layers are kept rather than the newest one replacing the others.

**Remaining residual.** `ps_script!` is enforced only where it is used — a new
site can still call `format!` directly. The 10d lint is what catches that, and
it remains a heuristic. Making the macro mandatory would need a lint rule
rejecting `format!` in any expression reaching a `Command::new("powershell")`,
which is a dataflow question a text scan cannot answer.

### 10g. Making the macro mandatory, and four more live injections

The 10f residual — that the macro is only enforced where it is already used — is
now closed by a second lint,
`powershell_commands_with_values_use_the_checked_macro`. It approximates the
dataflow question 10f called unanswerable by a text scan, and the approximation
is deliberately crude: any line that looks like a PowerShell or AppleScript
command, contains a `{}`, and sits inside a `format!` rather than `ps_script!`
is an offender. It over-flags, and `ALLOWED` absorbs the false positives. That
is the correct trade here — a missed site is an injection, a false positive is
an annoyance.

It flagged 21 sites. Seventeen were numeric interpolations that only needed
routing through the macro. **Three were live injections, and a fourth was
hand-escaped rather than helper-escaped:**

| Builtin | Fragment | Status before |
| --- | --- | --- |
| `net.ip_addresses` | `Get-NetIPAddress … -like '*{}*'` | unescaped |
| `net.adapters` | `Get-NetAdapter … -like '*{}*'` | unescaped |
| `timeout` (Windows) | `Start-Process … -ArgumentList '/C {}'` | unescaped |
| `log.search` | `Get-WinEvent … -like '*{}*'` | hand-escaped |

An interface name or command containing `'` terminates the string and the rest
executes. `timeout` is the worst of the four: the injected text lands in a
`cmd /C` argument list, so it needs no PowerShell knowledge to exploit.

**Why four detection layers had all missed them.** The 10d lint matches the
exact shapes `'{}'` and `"{}"`. These *embed* the placeholder in a larger quoted
string — `'*{}*'`, `'/C {}'` — so the substring never appears. The lint had
looked like coverage for this class while being blind to its most common shape.
`is_suspect` now pairs single quotes around a placeholder generally, with the
four shapes above as regression assertions. It is restricted to single quotes on
purpose: a `"` on these lines is usually the Rust literal's own delimiter, so
pairing across it would flag the correct unquoted `-Id {}` numeric form.

All four are fixed by `ps_quote` over the full pattern — `ps_quote(&format!("*{}*", v))`
— which escapes the value and supplies the quotes, leaving the wildcards outside
the escaped span where they belong.

The type check also rejected one further site, `dmesg`, which passed a
pre-stringified `count` into `-MaxEvents {}`. That one was safe in fact — the
value is either a stringified `Int` or the literal `"50"` — but the type cannot
see that, and it should not have to. It is now carried as an `i64`.

**Five defects, five detection methods.** This is the fifth consecutive time a
new method found what every previous method had missed: reading, then a test of
reading's conclusion, then a lint, then a type check, now a second lint aimed at
the first lint's blind spot. The pattern has not yet broken. That is the
strongest available evidence that this class is not exhausted, and it argues
against reading the current green state as proof of absence.

## 11. Three more exec paths found by enumerating programs, not reasoning (CWE-77)

Findings 7 and 10 were both produced by *assuming* a class was safe and being
wrong. So this pass enumerated instead: all 647 literal `Command::new("…")`
sites reduce to **216 distinct programs**, and each was considered for whether
it can be made to run a command by an argument.

That found three more, all reachable with no `Effect::Exec` gate:

**`sqlite3` dot-commands.** `sqlite3 <db> "<sql>"` accepts the CLI's own
dot-commands in the SQL position, and `.system` / `.shell` run programs.
Verified: `sqlite3 db ".system cmd /c echo … > file"` created the file. So
`db.sqlite_query`, `db.sqlite_exec` and `db.sqlite_export_csv` were arbitrary
execution wearing a database API. `safety::reject_sqlite_dot_command` refuses
them — dot-commands are a feature of the shell, not of SQL, so nothing
expressible in SQL is lost. The `db_path` argument is also the first positional
and is now passed through `reject_option_like`, since a leading `-` would reach
sqlite3's own option parser.

**`tmux new-session -d -s <name> <cmd>`.** The trailing argument is the command
tmux runs for the session — `sh -c` under another name. Now gated.

**`tmux send-keys -t <target> <keys> Enter`.** Types a string into a live shell
and presses return. That is execution in every sense that matters; the only
difference from `sh` is that the process belongs to someone else. Now gated.

Both tmux builtins are added to `effect_of`, and to the test that pins that list
against the `guard_exec` call sites.

**What this pass did *not* do.** 216 programs were considered against known
command-execution mechanisms — `-exec`, `-e`, `--use-compress-program`,
`ProxyCommand`, dot-commands, `run-shell`, and the like. That is a review
against a list of mechanisms I know about, which is not a proof that no other
mechanism exists in any of those 216 tools. It is a stronger claim than the two
that preceded it and were wrong, but it is not a guarantee.

Sites checked and found sound include: `git` (its command-executing options —
`-c core.sshCommand`, `--upload-pack`, `--exec-path` — must precede the
subcommand, and all 32 sites fix the subcommand first, so a caller value can
never reach that position); `find` (all five sites use fixed `-name`/`-type`/
`-size` predicates, never `-exec`); `wmic` (all six use `get`, never
`process call create`); and `curl`, `openssl`, `systemctl`, `journalctl`,
`ps`, `ip` and `netstat`, where caller values land in value positions after a
fixed flag.

## 13. Recursion aborts the process, and the usable depth is ~30 (CWE-674)

Found 2026-08-07 while testing finding 6b's deadline, and it is worse than the
"add a depth limit" note that finding originally carried.

```
let f = fn(x) => f(x)
f(1)
```

```
thread 'main' (39616) has overflowed its stack
```

The process **aborts**. The 6b deadline cannot catch this: a stack overflow is
not a `Result`, so nothing unwinds and no error is returned. For the Agent API
this is a denial of service that costs one request — worse than the hang 6b
addressed, because the whole server dies rather than one thread being tied up.

**The depth is the surprising part.** Bisected on Windows, debug build:

| Depth | Result |
| --- | --- |
| 30 | ok |
| 40 | **stack overflow** |

That is roughly 30 KB of stack per recursive call — `eval_expr` is a large match
and, unoptimised, its frame appears to carry locals for every arm. On Windows
the main thread gets a 1 MB stack by default, which is consistent with ~30
frames.

**Why a depth limit alone is not the fix.** The obvious remedy is a counter in
`call_lambda`, and on its own it does not work:

- A limit low enough to fire before overflow on this platform (~25) makes
  ordinary recursive programs fail. That is not a safety feature, it is a
  broken interpreter.
- A limit high enough to be usable (say 1000) never fires before the stack
  does, so the process still aborts. It would look like a fix and prevent
  nothing — the exact failure mode this audit has recorded four times already.

### 13a. Fixed: a large stack first, then the limit

This was initially recorded as needing design work and left open. That was
over-cautious — the two halves together are a contained change, and both are now
in place.

**The stack.** `safety::with_eval_stack` runs evaluation on a thread with a
256 MB stack, and `main` is a thin wrapper around it, so the REPL, scripts and
`-c` all get it. The Agent API's tokio runtime sets `thread_stack_size` to the
same value — necessary and easy to miss, because evaluation there happens on
tokio's own `spawn_blocking` workers, which would otherwise keep the default
stack and overflow at a few dozen frames no matter what `main` does. The stack
is *reserved* address space, not committed memory, so the size costs nothing
until used.

**The limit.** `safety::MAX_CALL_DEPTH` is 2000, enforced through an RAII guard
so the depth unwinds even when an inner call returns `Err`.

**Measured after the change**, same machine and debug profile:

| Depth | Before | After |
| --- | --- | --- |
| 40 | **stack overflow** | ok |
| 500 | stack overflow | ok |
| 1900 | stack overflow | ok |
| 2100 | stack overflow | **refused cleanly** |
| unbounded | **process abort** | **refused cleanly** |

Usable depth went from ~35 to 1900+, and the abort became a catchable error.

**Two things this got wrong on the way, both worth recording.** The guard was
first placed in `builtins::call_lambda` — which is not on this path. `eval.rs`
has four more entry points (`call_lambda0/1/2/_n`), and with only the first
guarded, `f(2500)` still returned a result: the limit was never consulted. It
looked correct and did nothing, again. All five are now guarded.

Then the test asserting deep recursion overflowed anyway, because a default test
thread has ~2 MB and the limit needs ~60 MB to be reachable — the stack beat the
limit, which is exactly the failure the large stack exists to prevent.

**Remaining constraint, and it is a real one.** The depth limit is only safe
when paired with the large stack. A library consumer calling
`eval::eval_program` on an ordinary thread gets the limit but not the stack, so
deep recursion still aborts before the limit can refuse it. Embedders must use
`safety::with_eval_stack` or set an equivalent `stack_size` themselves. Reducing
`eval_expr`'s frame size (boxing large match arms) would raise the ceiling for
everyone and remains worth doing.

**Measured and not measured.** The depths above are Windows, debug, this
machine. Release builds have smaller frames and Linux/macOS main threads
conventionally get 8 MB rather than 1 MB, so the real ceiling elsewhere is
higher — but *how much* higher was not measured, and the abort happens on every
platform regardless. Do not read "~30" as the number to tune against.

**Severity: Medium.** Trivially triggered and it kills the process, but it needs
an authenticated caller on the Agent API, and locally it is a crash of the
user's own shell rather than a privilege boundary being crossed.

## 12. Documentation directs users to an unclaimed package name (CWE-494, supply chain)

Found 2026-08-06 while checking why the release workflow's publish steps never
appear to do anything.

`docs/api/PYTHON_SDK.md`, `docs/book/src/api/python-sdk.md` and
`integrations/python/README.md` all instruct users to run:

```sh
pip install aethershell
```

**`aethershell` is not registered on PyPI.** Nor is `aether-shell`, nor either
name on npm — all four checked and returning 404 at the time of writing. The
package has never been published: the `publish-pypi` and `publish-npm` jobs in
`release.yml` both carry `continue-on-error: true`, so they have failed silently
on every release since they were added, and nothing surfaced it.

**Confirmed by execution, 2026-08-07.** The v4.0.0 release ran while this
finding was being written, which allowed the mechanism to be observed directly
rather than inferred. The `Publish Python SDK to PyPI` job reported
**`completed/success`** through the API — every step green, including
`Publish to PyPI` — and PyPI still had no package afterwards. The job log shows
what actually happened:

```
##[end-action id=__pypa_gh-action-pypi-publish.__self;outcome=failure;conclusion=failure]
* environment: `MISSING`
See https://docs.pypi.org/trusted-publishers/troubleshooting/ for more help.
```

Trusted publishing was never configured on PyPI for this repository, so the
upload is rejected. `continue-on-error: true` then rewrites the failed step's
`conclusion` to `success`, and the job and the whole run inherit it.

The npm job on the same release behaved identically — `completed/success`, and
nothing published. Its cause is different and simpler:

```
NODE_AUTH_TOKEN:                     <- empty; a populated secret renders as ***
npm error code ENEEDAUTH
npm error need auth This command requires you to be logged in to https://registry.npmjs.org/
```

The `NPM_TOKEN` secret has never been set. So the two publish jobs fail for two
unrelated reasons — one missing OIDC configuration, one missing secret — and
both surface identically as green. That is the point worth carrying: the
suppression does not just hide *a* failure, it makes every distinct failure
indistinguishable from success.

**Correction, and it narrows the finding.** An earlier draft of this section
treated npm as carrying the same user-facing risk as PyPI. It does not, and the
difference is worth stating precisely because it changes what is urgent:

- **PyPI is the live exposure.** `integrations/python/pyproject.toml` declares
  `name = "aethershell"`, the documentation told users to `pip install
  aethershell`, and that name is unregistered. Docs pointing at an unclaimed
  name is the whole vulnerability.
- **npm is a broken job, not an exposure.** No documentation anywhere in the
  repository tells anyone to `npm install` this project — checked. So no user is
  being directed at an unclaimed npm name, and the severity there is
  "distribution channel silently absent", not CWE-494.

Checking that also surfaced a plain bug. The artifact actually published is
`web/pkg/package.json`, generated by wasm-pack, which declares
`name = "aether_wasm"`. The hand-maintained `web/package.json` declares
`@nervosys/aethershell`. Those are three different names across two files and
the docs, and none of them is registered. Whoever fixes the npm job should
decide which name is intended before setting a token, or the first successful
publish will claim the wrong one.

The verification steps added alongside this finding read the name and version
out of the published artifact rather than assuming them, for exactly this
reason: hardcoding `aethershell` would have produced a check that reports on a
package the workflow never publishes.

That is three layers of the same illusion stacked:

1. `release.yml` contains a publish step, so **reading it looks correct**.
2. `continue-on-error` rewrites the failure, so **the step reports green**.
3. The job and run inherit that, so **"did the release succeed?" answers yes**.

Every check available from inside the repository says this works. Only asking
PyPI shows that nothing has ever been published.

The risk is not that the command fails. It is that **anyone may register the
name**, and `pip install` executes code from the package at install time
(`setup.py`, or a build backend hook). An attacker who claims `aethershell`
gains arbitrary code execution on the machine of every user who follows this
project's own documentation — with the project's docs serving as the delivery
mechanism and the user having no reason to suspect anything.

This is the same shape as finding 4: a capability advertised but not
implemented. It is more urgent, because the gap between "advertised" and
"implemented" is a namespace that a third party can occupy at will.

**Severity: High.** Trivial to exploit, needs no access to this project or its
infrastructure, and the documentation actively drives victims to it.

### 12a. Resolved: the name is registered

**`aethershell` 1.5.0 was published to PyPI on 2026-08-07**, closing the
squatting window. Verified beyond the upload's own success message — the
registry reports the package, and `pip install aethershell` in a clean
virtualenv installs and imports it.

The sdist was scanned before upload, on the same basis as the crate tarball
(21 files, no username, no host paths, no credential-shaped strings), because
PyPI does not allow a release to be deleted — only yanked.

One thing this nearly shipped: the SDK's `README.md` is the package's PyPI
long description, and it had been edited earlier the same day to warn *"do not
run `pip install aethershell`."* Publishing then would have made that warning
the package's front page. Caught by inspecting the built artifact rather than
the source tree.

**Also corrected here: `CARGO_REGISTRY_TOKEN` was never set on this repository
either.** An earlier note in `release.yml` claimed the crates.io publish step
"demonstrably works — crates.io has the published versions". That reasoning was
wrong. `gh secret list` returns *no secrets at all* for `nervosys/AetherShell`,
and the v4.1.0 run's environment shows `CARGO_REGISTRY_TOKEN:` empty. The
published versions exist because they were pushed manually from a local token,
not by CI. So all three publish jobs have never worked, for three unrelated
reasons — missing OIDC config, missing `NPM_TOKEN`, missing
`CARGO_REGISTRY_TOKEN` — and the suppression made all three look identical to
success. This was a fourth instance of the same illusion, authored while
documenting the first three.

**Remaining work, none of it urgent now that the name is claimed:**

1. Set `CARGO_REGISTRY_TOKEN` and `NPM_TOKEN` on the repository, or configure
   PyPI trusted publishing, so releases stop depending on a maintainer's local
   credentials.
2. Decide the npm package name (`aether_wasm` vs `@nervosys/aethershell`) before
   setting a token, or the first success claims the wrong one.
3. Consider registering `aether-shell` on PyPI as typo protection.

**Original remediation notes**, retained for the record:

1. **Register `aethershell` on PyPI now**, even as a placeholder. This closes
   the window regardless of what is decided about the SDKs, and it is the only
   step here that a third party can pre-empt. Consider `aether-shell` too, as
   the obvious typo target. npm is *not* urgent — no documentation directs
   anyone there — but claiming the intended name is cheap insurance.
2. **Then** decide whether to publish the SDKs for real or remove the install
   instructions. Both are defensible; leaving the docs as they are is not.
3. **Fix the two credentials, which are broken in two different ways.** PyPI
   needs *trusted publishing* configured for `nervosys/AetherShell` — the
   observed failure is `environment: MISSING`, so the OIDC claim matches no
   configured publisher. npm needs the **`NPM_TOKEN` secret set at all** — it is
   currently empty, and npm fails with `ENEEDAUTH`.
4. **Remove `continue-on-error: true` from the publish jobs**, or at minimum
   have them fail loudly. Error suppression is what let this run undetected
   across every release — and, worse, what made the failure report as success
   at every level the repository can see. If the suppression must stay (the
   crates.io step has a legitimate reason for it), add a verification step that
   queries the registry afterwards and fails on a missing version. A publish
   step that cannot tell you whether it published is not a publish step.

Note the version drift that made this visible: the Rust crate is at 4.0.0 while
`integrations/python/pyproject.toml` says `1.5.0` and `web/package.json` says
`0.2.0`. Had the publish ever succeeded, the version would have moved.

**Detection method, for the record:** this is the seventh distinct method in
this audit — checking whether a *thing the build claims to do* actually
happened, by querying the outside world rather than reading the workflow.
Reading `release.yml` shows a publish step and looks correct.

## 9. MCP servers are adopted by port convention, unauthenticated (CWE-306)

`detect_mcp_servers_uncached` probes seven hardcoded `http://localhost` ports
(3001–3005, 8080, 8081) and adopts as a tool provider anything that answers
`GET /mcp/v1/tools` with a parseable list of tool schemas.

That contract is narrow enough that an unrelated listener will not match by
accident, which bounds the severity. But there is no allowlist, no
authentication of the server, no TLS, and no operator confirmation: any local
process that implements one path becomes a tool source for the agent. On a
shared or multi-user host, or after any other local compromise, that is a
tool-poisoning foothold.

Discovered tool *descriptions* do not currently flow into a model prompt — this
was checked, and it is the difference between "a trust-boundary weakness" and a
prompt-injection vulnerability. If that changes, this becomes considerably more
serious, so it is recorded now.

Noted rather than fixed: an allowlist or explicit configuration is a product
decision about how MCP discovery should work, not an audit change.

One incidental interaction worth recording: `3002` is in the probe list *and* is
the Agent API's default port, so AetherShell would probe itself. Since finding 6
the API answers `401` to an unauthenticated `GET /mcp/v1/tools`, so it is no
longer adopted — the auth fix closed a self-detection collision as a side
effect.

## 1. CVE / advisory posture — RUSTSEC-2026-0185

`cargo audit` reported one vulnerability: unbounded out-of-order stream
reassembly in `quinn-proto` 0.11.14 permitting remote memory exhaustion
(CVSS 7.5). Updated to 0.11.16.

Exploitability in AetherShell was nil rather than merely low. `quinn` appears in
`Cargo.lock` only as an optional dependency of `reqwest`'s `http3` feature,
which is not enabled; `cargo tree -i quinn-proto --target all --all-features`
reports "nothing to print", i.e. it is never compiled into any shipped artifact.
It was updated regardless so that a clean lockfile cannot mask a future real
exposure.

`cargo audit` now exits 0. Ten findings remain as accepted warnings
(`derivative`, `instant`, `number_prefix`, `paste`, `proc-macro-error`
unmaintained; `anyhow`, `lru`, `memmap2`, `rand` ×2 unsound). These are
transitive, previously reviewed, and allowlisted. `cargo deny check` passes all
four gates.

## 2. Unimplemented crypto builtins reported success (CWE-347, CWE-311)

The most serious finding of the first pass, and not dependency-related.

`eval::is_truthy` maps `Value::Str(s)` to `!s.is_empty()` and `Value::Error` to
`false`. Seven cryptographic builtins were stubs that returned an explanatory
*string*, which is therefore truthy and indistinguishable from success:

```
if crypto.verify_signature(sig, data, key) { deploy() }   # branch always taken
```

A script gating on signature or certificate verification proceeded with nothing
verified. `crypto.encrypt` expressed the same defect differently: it returned
the sentence `"Encryption requires OpenSSL"` where the caller expects
ciphertext, so persisting the result stored prose in place of an encrypted
secret, silently.

This was not limited to unusual configurations. `encrypt`, `decrypt` and
`verify_cert` shell out to the `openssl` CLI under `#[cfg(unix)]` only, so on
Windows the stub was the *only* path; on Unix, any `openssl` failure fell
through to it.

All seven now return `Err` with the `E_UNIMPLEMENTED` code, matching the
existing `E_*` convention. `tests/crypto_fail_closed.rs` asserts the property
that matters — that these never yield a truthy value — rather than exact
strings, so a genuine implementation satisfies the test unchanged.

## 3. Committed build artifact leaked a developer's username (CWE-532)

`integrations/browser-extension/wasm/` held committed `wasm-bindgen` output
embedding absolute build paths, and hence the building developer's username, in
panic-location strings shipped to anyone loading the extension:

```
C:\Users\<user>\.cargo\registry\src\...\serde_json-1.0.149\src\error.rs
C:\Users\<user>\.rustup\toolchains\stable-x86_64-pc-windows-msvc\...
```

These are build artifacts: `build.ps1` copies them out of `web/pkg` (correctly
untracked) and `build.ps1 -Clean` deletes the directory wholesale. The extension
README already lists building the WASM as installation step 1 and labels the
directory "(generated)", so removing them breaks nothing that was documented to
work. Removed and gitignored.

`build.ps1` now also passes `--remap-path-prefix` via `CARGO_ENCODED_RUSTFLAGS`
so a future local build cannot reintroduce the leak. Cargo's `[profile]
trim-paths` would be tidier but is still unstable as of Cargo 1.97.1. Verified
empirically on a scratch crate with a registry dependency: three
username-bearing paths before, zero after.

**Incomplete.** This fixes the working tree and future builds. The artifact
remains in git history and is still fetchable from prior commits. Scrubbing it
requires a history rewrite and force-push, which was not performed.

`.mailmap` also maps a personal address to the organisation address. That is
what a mailmap is for, and both addresses are already present in commit
metadata, so it exposes nothing new — noted for completeness. It is excluded
from the published crate.

No hardcoded credentials were found. Matches for `sk-`, `ghp_`, `AKIA`, PEM
blocks and JWTs are all fixtures belonging to the redaction machinery itself,
whose behaviour is covered by `tests/secret_hygiene.rs` (4/4 passing): secret
shapes and secret-named fields are scrubbed from agent output, and secrets never
reach the hash-chained audit log.

## 4. Advertised access-control surface is unimplemented (CWE-1104)

`modules.rs` maps user-facing names to builtin names, and nothing verified the
targets existed. 80 of 919 aliases pointed at builtins that do not exist. Nine
were naming drift and were corrected (`crypto.verify`, `crypto.key_generate`,
six `gui.window_*`, `input.multi_select`).

The remaining 71 are genuinely unimplemented and are now pinned in
`tests/module_aliases.rs` as an allowlist that can only shrink. Among them:

```
rbac_check  rbac_permissions  rbac_revoke  rbac_roles  rbac_user_roles
rbac_create_role  rbac_delete_role
perm_acl_get  perm_acl_set  perm_owner_get  perm_owner_set
sso_providers  sso_refresh  sso_user_info
```

**AetherShell advertises a role-based access control, ACL and SSO surface that
it does not implement.** Calling them yields `unknown builtin`, so the failure
is *closed* and this is not itself a vulnerability — but any deployment that
believes it is enforcing authorization through `rbac.check` is enforcing
nothing. This was documented rather than implemented; implementing it is a
product decision, not an audit fix.

The access controls that *do* exist and are exercised by tests are the policy
gates: `E_POLICY_DENY`, `E_NEEDS_APPROVAL`, `E_EGRESS_DENIED`,
`E_OUTSIDE_WORKSPACE`, `E_BUDGET_EXCEEDED` (`tests/safety.rs` 13/13,
`tests/security_user_simulation.rs` 16/16).

## 5. Builtin registry split

Dispatch has two layers: `BUILTIN_LOOKUP` (name → index → function-pointer
table) and a trailing fallback `match`. Only the first is public, so `agent`,
`ai` and `swarm` dispatch correctly yet are invisible to anything enumerating
`BUILTIN_LOOKUP` — including `agent_api`'s dynamic discovery, whose purpose is
telling an agent what it may call. Documented in `tests/module_aliases.rs`;
unifying the two is deferred.

---

## Reviewed and found sound

Recorded so a later pass knows these were examined rather than skipped.

**Native plugin loading** (`plugins.rs`, 13 of the 31 `unsafe` blocks). A native
plugin executes arbitrary machine code in-process, so the gate matters more than
the FFI. `Plugin::load` calls `authorize_load` first, and `libloading::Library::
new` appears at exactly one site, so the gate cannot be bypassed by another
path. Policy: `AETHER_PLUGINS=off` is a kill switch, agent mode is default-deny,
and `AETHER_PLUGIN_ALLOW` allowlists directories. The allowlist check uses
`Path::starts_with`, which is component-wise — so `/opt/plugins-evil` does *not*
match an `/opt/plugins` root, the bug this idiom usually has. Both the plugin
path and each allow root are canonicalized first, defeating `..` traversal.

One weakness, not fixed: a non-existent allow root falls back to a literal
string comparison rather than failing. That only weakens a misconfiguration
(a root that does not exist matches nothing useful either way), so it is noted
rather than treated as a finding.

**The remaining 18 `unsafe` blocks.** `agent.rs` (5) uses `pre_exec` +
`setrlimit` to bound CPU, address space and file size on Unix — sandbox
hardening, and the standard idiom for it. `builtins.rs` (4), `os_tools.rs`,
`providers/platform.rs` are `libc::geteuid`/`getuid`/`getgid` calls, trivially
sound. `external_tools.rs` is a `libc::kill` on a pid the same function spawned.
Only `neural.rs`/`evolution.rs` were unsound (finding 8).

**Deserialization of untrusted input.** The Agent API's 50 `Json` extractors
rely on axum's default body limit — `DEFAULT_LIMIT = 2_097_152` in axum-core
0.4.5, applied whenever no `DefaultBodyLimit` is set, and nothing in the tree
sets one — so an oversized body is rejected before allocation.
`serde_json`'s default 128-level recursion limit bounds deeply nested input, so
a nesting bomb cannot overflow the stack. No `unsafe` deserialization, no
`bincode`/`rmp` over untrusted bytes, and no `serde` type that executes on
deserialize.

The gap is that the server sets **no request timeout**: a slow client, or an
`/api/v1/eval` body that runs indefinitely, occupies a worker. Since finding 6
that requires a valid token, which reduces it to an authenticated-caller
denial of service — worth a `TimeoutLayer`, not treated as a finding.

**Published artifact contents.** `cargo package --list` is 273 files with no
`.wasm`, `.js`, `.pdb`, `.env`, `.pem` or key material, and `.mailmap` — the one
file carrying a personal address — is excluded. The working tree contains no
occurrence of the developer's username; the two remaining `C:\Users\` strings
are a `<you>` placeholder in a comment and a synthetic `ada` test fixture.

## NIST FIPS

`AETHER_FIPS` gates non-approved hash algorithms. `require_fips_hash` rejects
MD5 and SHA-1 with `E_FIPS_DISALLOWED`; the SHA-2 family passes. All four
weak-hash entry points are gated, and the two remaining MD5 call sites are
deliberate and correctly conditioned:

- `marketplace.rs` accepts a 32-hex legacy MD5 package checksum only when
  `!fips_enabled()`, preferring SHA-256 (64-hex) — documented in place.
- `persistence.rs` likewise skips MD5 comparison under FIPS.

The existing doc comment already draws the distinction that matters, and it is
worth repeating: this enforces *approved-algorithm-only* at the application
layer. It does **not** make the underlying crypto a FIPS-140-validated module.
Any claim of FIPS 140-2/140-3 validation would be unsupported.

Separately, package integrity in `marketplace.rs` rests on a checksum served by
the same registry that serves the payload, so it detects corruption, not a
malicious or compromised registry — independent of hash strength. Signature
verification against a pinned key would be required for that, and
`crypto.verify_signature` is (per finding 2) unimplemented.

## MITRE ATT&CK

| Technique | Relevance | Control |
|---|---|---|
| T1059 Command and Scripting Interpreter | AetherShell *is* an interpreter; 668 `Command::new` sites in `builtins.rs` | Effect classification (`safety::effect_of`), policy gates, approval prompts, hash-chained audit log. Until finding 7, the gate covered one builtin name; it now covers all ten arbitrary-command builtins. Fixed-program sites remain ungated |
| T1190 Exploit Public-Facing Application | Agent API HTTP listener evaluating arbitrary code | Bearer token on every route but `/health` (finding 6); loopback default; non-loopback bind warns |
| T1195 Supply Chain Compromise | Agent packages fetched from a registry | Checksum verified; **no signature verification** (findings 2, 4) — residual risk |
| T1552 Unsecured Credentials | Secrets in env/output/logs | Redaction of secret shapes and names; env reads gated in agent mode; secrets excluded from the audit log (`tests/secret_hygiene.rs`) |
| T1027 Obfuscated Files or Information | Homograph/invisible-character path spoofing | `is_deceptive_char` rejects invisible and bidi characters in `validate_safe_path` (CWE-1007) |
| T1005 Data from Local System | Filesystem access by an agent | `E_OUTSIDE_WORKSPACE` workspace confinement |
| T1071 Application Layer Protocol | Network egress by an agent | `E_EGRESS_DENIED` egress policy |

## CMMC 2.0

Assessed as a software component, not an accredited enclave; CMMC applies to
organisations, so this maps practice families to implemented controls only.

| Family | Implemented | Gap |
|---|---|---|
| AU (Audit & Accountability) | Hash-chained audit log, verifiable, secrets redacted before persistence | `audit_stream`, `audit_retention` unimplemented — no retention enforcement |
| AC (Access Control) | Workspace confinement, egress policy, approval gates, budget limits, exec gate over all arbitrary-command builtins (finding 7) | `rbac.*` and `perm.acl_*` unimplemented (finding 4); ~650 fixed-program `Command::new` sites ungated |
| IA (Identification & Authentication) | Keyring-backed credential storage; bearer token on the Agent API (finding 6) | `sso.*` unimplemented; the API token is a single shared secret with no rotation, revocation or per-client identity |
| SC (System & Communications Protection) | TLS via rustls by default | `crypto.encrypt`/`sign` unimplemented; now fail closed rather than silently (finding 2) |
| SI (System & Information Integrity) | `cargo audit`/`cargo deny` in CI; advisories clean | Package integrity lacks signature verification |

---

## Limitations

- Static and test-based review, with targeted dynamic probes for findings 6 and
  7. No fuzzing, no sustained penetration testing, no formal threat model.
- **Two passes found two critical/high issues in surfaces the previous pass had
  not reached.** The first pass concentrated on dependencies, crypto and
  secrets, and reported clean; the HTTP listener and the exec gate were where
  the real problems were. The third pass then found a High in a class the second
  pass had explicitly written off as weaker. Treat the current result as "no
  further findings in the surfaces examined", not as an assurance that the
  codebase is clean — each pass so far has found something the previous one
  reasoned its way past.
- MCP trust boundaries were reviewed at the *discovery* layer (finding 9). The
  handling of tool *results* returned by an MCP server — where they are
  interpolated, and whether any reach an exec path — was not traced end to end.
- The fixed-program `Command::new` sites were reviewed for the two injection
  mechanisms known to reach execution — option injection into a program that
  runs commands, and interpolation into a PowerShell or AppleScript literal
  (finding 10) — and 43 were fixed. They were **not** exhaustively reviewed program by program: the
  remaining sites pass data to programs with no *known* command-executing flag,
  which is weaker than a demonstration that none exists. Finding 10 exists
  because this class was assumed benign in the previous pass, so the assumption
  should not be made a second time.
- The 71 unimplemented aliases were enumerated, not individually reviewed for
  what a caller might assume of them.
- FIPS assessment covers algorithm restriction only. No validated cryptographic
  module is present or claimed.
- Finding 3's history exposure is unresolved by design; it needs an explicit
  decision to rewrite history and force-push.
- Verification ran on Windows with the 1.97 toolchain. The Unix-only `openssl`
  paths in the crypto builtins, the `setrlimit` sandbox in `agent.rs`, and the
  Linux-only `strace`/`perf` builtins were reviewed by reading, not executed.

## Open decisions for the maintainer

Not audit fixes; each needs an explicit call.

0. ~~**Register `aethershell` on PyPI**~~ — done 2026-08-07 (§12a); the SDK is
   published and `pip install aethershell` works. What replaces it is
   housekeeping: **set `CARGO_REGISTRY_TOKEN` and `NPM_TOKEN` on the
   repository** (it currently has *no* Actions secrets at all, so every publish
   job has always failed silently and releases depend on a maintainer's local
   credentials), settle the npm package name before setting that token, and
   consider claiming `aether-shell` as typo protection.

1. **Scrub the leaked WASM from git history** (finding 3) — requires a history
   rewrite and force-push. Currently the artifact is still fetchable from prior
   commits.
2. **Implement or withdraw `rbac.*`, `perm.acl_*`, `sso.*`** (finding 4).
   Advertising an unimplemented access-control surface is worse than not
   advertising one.
3. **Gate or review the fixed-program `Command::new` sites** (finding 7).
4. **Decide how MCP servers should be trusted** (finding 9) — an allowlist or
   explicit configuration, rather than adoption by port convention.
5. **Delete the published `nervosys/aethershell` container images** on Docker
   Hub and ghcr.io, now that Docker is no longer a distribution channel.
6. ~~**Add a `TimeoutLayer` to the Agent API**~~ — done in 4.0.0 (§6a), though
   not by adding a `TimeoutLayer`, which on its own does nothing here.
   ~~**A deadline checked inside the interpreter**~~ — done (§6b): `eval_expr`
   now stops runaway recursion and large computations. What remains is narrower:
   a builtin already blocked in a syscall (`sleep`, subprocess wait, network
   read) never returns to be asked, so it cannot be interrupted.
7. ~~**Give evaluation an explicit large stack, then limit recursion depth**~~ —
   done (finding 13a). What remains is smaller: **reduce `eval_expr`'s frame
   size** by boxing large match arms, which raises the ceiling for everyone
   including embedders who evaluate on an ordinary thread and therefore get the
   depth limit without the stack.

## Decisions taken, 2026-08-06

**Every version other than 3.0.1 is now yanked, on both crates.** The eight
pre-audit releases (`1.7.3`–`1.6.0`, `0.3.1`–`0.2.0`) each predate finding 6,
so `cargo install aethershell --version 1.7.3` served an agent API that
executed code for unauthenticated callers. Leaving them installable while
`2.0.0`–`3.0.0` were yanked for strictly lesser defects was inconsistent in
the wrong direction. Yanking does not alter any existing `Cargo.lock`; it only
removes these versions from new resolution, and it is reversible with
`cargo unyank`. The same reasoning was applied to `aethershell-lsp`, whose
pre-audit versions depend on the vulnerable `aethershell`.

**The crates.io publishing token is knowingly unrotated.** It was exposed in a
maintainer's terminal session on 2026-08-05 — an `echo` using `${VAR:+…}${VAR:-…}`
printed the value rather than a yes/no — and has been used for twelve
publishes since. The maintainer has weighed this and is deferring rotation. It
is recorded here rather than left implicit: anyone with that value can publish
or yank arbitrary versions of both crates. Rotation is at
`crates.io/settings/tokens`, and the new value must also replace the
`CARGO_REGISTRY_TOKEN` GitHub Actions secret.
