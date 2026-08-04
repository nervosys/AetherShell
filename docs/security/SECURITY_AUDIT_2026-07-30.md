# Security Audit — 2026-07-30, updated 2026-08-04

Scope: the `master` tree, covering the `aethershell`, `aethershell-lsp` and
`agentic-eval` crates, the browser extension, and the CI and release workflows.
The first pass reviewed `2d8b969`; the second and third passes (2026-08-04, findings 6–10)
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
| 10 | Argument injection into PowerShell and archivers | CWE-78 / CWE-88 | **High** | Fixed |
| 1 | `quinn-proto` remote memory exhaustion | RUSTSEC-2026-0185 | High (7.5) | Fixed |
| 2 | Unimplemented crypto builtins reported success | CWE-347 / CWE-311 | High | Fixed |
| 4 | `rbac.*`, `perm.acl_*`, `sso.*` advertised but unimplemented | CWE-1104 | Medium | Documented, not implemented |
| 9 | MCP servers adopted by port convention, unauthenticated | CWE-306 | Medium | Documented, not fixed |
| 8 | `static mut` PRNG state mutated without synchronization | CWE-362 | Low | Fixed |
| 3 | Committed WASM leaked a developer's username | CWE-532 | Low | Fixed (tree only) |
| 5 | Builtin registry split hides names from agent discovery | Correctness | Low | Documented |

Nothing in this audit indicates a compromise or data exposure to a third party.
Finding 3 concerns one developer's username, published in a public repository.

The table is ordered by severity; the sections below are numbered in discovery
order, so the later passes (6–10) are written up first.

Findings 6, 7 and 10 are the significant ones, and all were reachable in the
*intended* configuration rather than an unusual one. They compound: an
unauthenticated `/api/v1/eval` (6) reaching an exec surface that agent mode did
not actually gate (7), with two further routes to execution that bypassed even
that gate (10), means the hardening a deployment believed it had from
`AETHER_MODE=agent` would not have constrained a drive-by request.

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
program can still be *told* to run a command by an argument, and 20 such sites
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

## 10. Argument injection into PowerShell and archivers (CWE-78, CWE-88)

Found in the third pass, while reviewing the fixed-program `Command::new` sites
that finding 7 explicitly did not cover. Two distinct defects, both verified by
execution.

### 10a. PowerShell command injection (CWE-78, Windows)

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

**Residual.** This closes single-quoted PowerShell interpolation and the
positional-path class. Double-quoted PowerShell interpolation — where `$` and
backtick also expand — was reviewed and the sites found either use fixed strings
or already backtick-escape, but that escaping is ad-hoc rather than centralized
and would benefit from the same treatment.

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
  runs commands, and interpolation into a PowerShell literal (finding 10) — and
  20 were fixed. They were **not** exhaustively reviewed program by program: the
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
6. **Add a `TimeoutLayer` to the Agent API** — currently an authenticated
   caller can hold a worker indefinitely.
