# Security audit — 2026-11-04

White-box review of master at `3e0f9f0` (release candidate `68711fb`). Four
read-only reviews covered network servers, agent containment, injection, and
secrets, plugins and protocols. The serious findings were then run against a
debug build.

**Evidence grades.** *Demonstrated*: executed and observed, on Linux (Debian 13,
`ae-disposable`), in agent mode with no approvals unless stated. *Structural*:
established by reading the code, not run.

**Not a certification or a penetration test.** Nothing was run on Windows or
macOS. Findings that rest on a review's "sound" verdict were not re-read.

## Summary

All findings below were fixed on 2026-11-05 except for the residual risks listed
under "After the fixes". Findings marked Demonstrated were rerun on Linux after
the fixes, and each attempt was refused.

| ID | Severity | Evidence | Summary |
|---|---|---|---|
| AS-2026-16 | Critical | Demonstrated | `env_set` steers child programs, so an agent runs code with no approval |
| AS-2026-17 | High | Demonstrated | `db_sqlite_to_csv` runs any SQL as WriteLocal, including writes outside the workspace |
| AS-2026-18 | High | Demonstrated | `cd` in `ish -b` moves the jail root unless the workspace is pinned |
| AS-2026-19 | High | Demonstrated | `cd` moves the audit log to a new chain, with no marker |
| AS-2026-20 | High | Demonstrated | MCP HTTP OS tools skip the effect gate and the jail |
| AS-2026-21 | High | Demonstrated | `proc_env` returns secrets that `env` redacts |
| AS-2026-22 | High | Demonstrated | `IRONGATE_URL` sends the gate key and prompts to a host the agent chose |
| AS-2026-23 | Medium | Demonstrated | Writes through a symlink escape the jail |
| AS-2026-24 | Medium | Demonstrated | SSRF check missing from `web_*`, and bypassed by IPv4-mapped IPv6 |
| AS-2026-25 | Medium | Structural | `aimodel` daemon has no default auth and trusts a self-set header |
| AS-2026-26 | Medium | Structural | Marketplace names reach file paths; packages are unsigned |
| AS-2026-27 | Low | Demonstrated | `file_replace` is classed ReadLocal, so `undo` cannot revert it |
| AS-2026-28 | Low | Structural | Smaller items, listed below |

## AS-2026-16: `env_set` steers child programs (Critical)

`refuse_gate_env` (`src/builtins.rs:13937`) refuses only `IRONSHELL_*` names, and
`child_command` passes everything else to children. `git_status` and `git_diff`
are ReadLocal, so they need no approval. `exec` and `sh` were refused in the
same run.

Both of these ran a command with no approval:

- **Planted program:** write `<ws>/git`, `fs_chmod` it, `env_set("PATH", "<ws>:...")`,
  then `git_status()`.
- **Text only:** write a git config with a `textconv` command and a
  `.gitattributes`, `env_set("GIT_CONFIG_GLOBAL", ...)`, then `git_diff()`. This
  needs no executable bit, so it probably works on Windows too (not run there).

`GIT_EXTERNAL_DIFF` did not run; `--no-ext-diff` holds. `GIT_SSH_COMMAND` needs
`git_fetch`, which asks for approval.

## AS-2026-17: arbitrary SQL as WriteLocal (High)

`db_sqlite_to_csv` passes text starting with `SELECT` straight to the `sqlite3`
tool (`src/builtins.rs:44423`), and it is classed WriteLocal. With no approval:

- `SELECT 1; DROP TABLE users` dropped the table.
- `SELECT writefile('<outside>', 'x')` wrote a file outside the workspace.
- `SELECT 1; ATTACH '<outside>' AS e; CREATE TABLE e.t(x)` created a database
  outside the workspace.

A write to a shell startup file would turn this into code execution.
`db_sqlite_update` (WHERE clause) and `db_sqlite_create_table` (raw column text)
take the same route by code reading; they were not run. `db_sqlite_exec` is
correctly classed Exec. `sqlite3 -safe` refused `writefile` and `ATTACH` when
tried by hand on 3.46.1.

## AS-2026-18: `cd` removes the jail in `ish -b` (High)

`workspace_root()` (`src/safety.rs:2701`) is the live working directory unless
`IRONSHELL_WORKSPACE` is set, and the POSIX `cd` (`src/posix.rs:1604`) has no
guard. `cd <outside>; echo x > <outside>/f` wrote the file. The same write without
`cd` got `E_OUTSIDE_WORKSPACE`, and so did the `cd` form with the workspace set.

## AS-2026-19: `cd` moves the audit log (High)

`audit_path()` (`src/safety.rs:3238`) follows the same root. After `mkdir("sub")`
and `cd("sub")`, records went to a new `sub/.ironshell/audit.log` with its own
chain. The original log has no record of the later writes and shows nothing wrong.

## AS-2026-20: MCP HTTP OS tools skip the gate (High)

`handle_call_tool` (`src/mcp.rs:1556`) runs a registered OS tool through
`execute_tool`, which has no guard. It takes arguments in declared order without
checking them. In agent mode, a request to the `curl` tool with `method: "-o"`
wrote to a path outside the workspace. This needs the bearer token. MCP stdio
routes through the gate and is not affected by code reading.

## AS-2026-21: `proc_env` returns secrets (High)

`bi_proc_env` (`src/builtins.rs:26262`) has no `gate_env_secret`. For a fake
`FAKE_API_KEY`, `env()` and `env_vars()` gave `[REDACTED:FAKE_API_KEY]`, while
`proc_env(<own pid>).FAKE_API_KEY` gave the value. Sending it anywhere still needs
network approval. The branch for other processes has no gate either, by code
reading.

## AS-2026-22: `IRONGATE_URL` redirects the key (High)

`env_set` can change `IRONGATE_URL`, which `gate_url()` reads (`src/model_plane.rs:54`).
`ai()` needs approval, but the approval names only the prompt, not the destination.
Once approved, the listener received `Authorization: Bearer <IRONGATE_API_KEY>`
and the prompt.

## AS-2026-23: writes through symlinks (Medium)

`lexical_abs` (`src/safety.rs:3041`) removes `..` before symlinks are resolved.
With `<ws>/lnk` pointing outside, `file_write("<ws>/lnk/../f")` and
`echo x > lnk/../f` both landed outside the workspace. In `ish -b`, a redirect to a
dangling link created the file outside too. The link has to exist already: a
review found an agent can't create one itself, so the likely route is a malicious
repository.

## AS-2026-24: SSRF (Medium)

Network calls need approval, so these apply after approval. `http_get` refused
`127.0.0.1` and `169.254.169.254`. But:

- `web_fetch("http://127.0.0.1:...")` reached the listener; the `web_*` builtins
  do not check their first URL.
- `http_get` and `web_fetch` both reached it at `[::ffff:127.0.0.1]`, because
  `is_internal_ip` (`src/security.rs:1178`) does not unwrap mapped addresses. By
  code reading, this also lets a redirect from a public page through.
- By code reading, the check resolves the name separately from the fetch, so DNS
  rebinding is possible. Not tested.

## AS-2026-25: `aimodel` daemon (Medium, not run)

It binds to loopback by default, with no API key required. Mutating routes accept
any request carrying `X-IronShell-Client` (`src/ai_api/server.rs:1089`), and there
is no Host check, so DNS rebinding could reach them from a browser. Reachable
routes download from any URL to disk, delete models, and spend provider keys.
Binding beyond loopback only warns.

## AS-2026-26: marketplace (Medium, not run)

`name` from the request reaches `cache_dir.join(name)` (`src/marketplace.rs:403`)
and `remove_dir_all`, unvalidated. Reaching it needs a registry reply for such a
name. Packages have no signatures, the digest comes from the same registry, MD5 is
accepted, and `packages.rs` never checks its checksum. Nothing runs installed
content automatically.

## AS-2026-27: `file_replace` is not undoable (Low)

The `file_` prefix rule (`src/safety.rs:1227`) classes `file_replace` as ReadLocal,
so it is not journalled. After `file_replace`, `undo()` did not restore the file.
The jail still applies. `AGENTS.md` recommends this builtin for edits.

## AS-2026-28: smaller items (Low, not run)

- `aimodel`: rate limit and TLS settings are never read; API key comparison is not
  constant-time; `--api-key` on argv and key prefixes printed; `config.toml` saved
  with default permissions; `/v1/storage/stats` shows directories without auth.
- Agent API WebSocket and MCP HTTP have no admission limit; MCP HTTP has no timeout.
- `zip_extract` and `tar_extract` have no byte budget.
- `nc_listen` and `tcp_listen` can bind all interfaces in agent mode.
- Redaction misses `?key=` in URLs and `hf_` keys; `is_secret_name` omits `*_PAT`,
  `*_CREDS`, `*_AUTH`.
- `env::set_var` can race `getenv` on Agent API threads; `eprintln!` in `pre_exec`.
- VS Code: workspace settings choose the binary, which receives the full environment;
  a crafted filename reaches `terminal.sendText`.
- `dtolnay/rust-toolchain@master` in `ci.yml`; other actions are pinned to tags, not
  commits.
- `md5`, `sha1`, `sha2` and `hmac` crates are used directly, against the IronCrypto
  policy.

## Corrections to the reviews

Two reviews said network calls run without approval, based on `decide()`. In the
runs, `http_get`, `web_fetch`, `git_fetch` and `ai` all returned `E_NEEDS_APPROVAL`.
This lowers the exfiltration and SSRF findings. `decide()`'s table does not match
what happens, and why was not traced.

## Dependencies

`cargo audit`: no vulnerabilities in `Cargo.lock`, 10 warnings (unmaintained crates;
`lru` unsound, through `ratatui`). `cargo deny`: advisories, bans and sources pass,
and warn that the ignore for `RUSTSEC-2026-0429` is unused, because the offline
database lacks that advisory. The ignore is live: without it `cargo audit` reports
`zip` 0.6.6 and 1.1.4. The Security Audit workflow passed on
the candidate (run 41961598759).

## After the fixes

Risks that remain, and why:

- **Packages are unsigned.** No publisher keys or registry support exist, so a digest
  detects corruption only. The docs say so.
- **A check and its use are separate.** A link swapped between the jail's check and the
  write can still redirect it. The address pin has not been shown to stop DNS rebinding,
  because that needs a name whose answer changes; only its syntax and fallback were tested.
- **Approved calls are trusted.** An approved MCP OS tool is not jailed: approval is the
  control. Raw TCP tools (`nc`, `socat`, `nmap`), git remotes and the browser openers have
  no internal-address check; all of them ask for approval first.
- **Library callers don't pin.** Only the binary fixes the root. A library caller gets a
  marker in the audit log when the path changes, but its jail still follows the working
  directory.
- **Declared archive sizes can lie.** The tool deadline still bounds extraction, and
  bzip2, xz or zstd archives without a decompressor on PATH aren't counted.
- **`env::set_var` can race C code calling `getenv`** on Agent API threads. It wasn't
  changed: std's own environment functions lock, and the rest needs a design of its own.
- **MD5 and SHA-1 remain** as user-requested legacy hashes gated by FIPS mode, and as
  session directory names, because IronCrypto has no equivalent. Changing the directory
  names would orphan existing sessions.
- **Generated tokens are still printed at startup.** That is how an operator gets them,
  and a parent process capturing stdout will see them.
- **A false refusal remains:** `<ws>/x/.git/../notes` is refused as git control, because
  that check still matches the path as spelled. It fails closed. The same refusal for
  `.ironshell/../notes.txt` was fixed on 2026-11-06.

Also found, not a security problem, and fixed on 2026-11-06: the MCP HTTP `curl` tool
passed values as bare arguments, so ordinary calls failed. It now maps them to flags and
takes only HTTP(S) URLs.

## Gaps

- The review's repo-wide search for hard-coded secrets was blocked by a permission
  check and was not retried. The workflow's Secret Scanning job passed.
- No reproduction on Windows or macOS.
- `aimodel`, marketplace and the items marked "not run" rest on code reading as
  findings. Their fixes have tests, except the VS Code changes, which were not compiled
  locally. Tests for `aimodel`'s redirect hops and key output were added on 2026-11-06.
  `aimodel keys` uses the OS credential store and is not tested.
- AS-2026-28 said `aimodel` printed key prefixes. On reading the code again, the 8-character
  prefix is kept in the serialized key record, and no print statement shows it.
  Whether any route returns that record was not checked.
