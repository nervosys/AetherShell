# Changelog

All notable changes to AetherShell will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.0.4] - 2026-08-04

### Security
- **Six more PowerShell injection sites, found by a lint rather than by
  reading (CWE-78).** 2.0.1 and 2.0.2 closed this class by hand. This release
  adds `tests/no_raw_shell_interpolation.rs`, which scans the source for the
  *shape* of the bug — a quoted `{}` placeholder on a line that looks like a
  PowerShell or AppleScript command.

  It failed on its first run, flagging six sites that both earlier passes had
  missed: `Resolve-DnsName '{}'` (×2, `net.dns_lookup` and reverse lookup),
  `SendKeys::SendWait("{}")` (×3), and `MessageBox::Show("{}", "{}")`. All six
  were live injection vectors. All six survived three rounds of careful manual
  review.

  That is the point worth taking from this release: on a codebase with ~117
  PowerShell call sites, reading does not find this class reliably and a
  mechanical check does. The lint has a companion test asserting it still fires
  on the pre-fix shapes, because a lint that cannot fail reads as coverage
  while providing none.

## [2.0.3] - 2026-08-04

### Security
- **`sqlite3` dot-commands and two `tmux` exec paths were ungated (CWE-77).**
  Findings 7 and 10 were each produced by assuming a class of `Command::new`
  sites was safe and being wrong. So rather than reason a third time, all 647
  literal sites were reduced to their **216 distinct programs** and each was
  checked for a way to make it run a command. Three more turned up:

  - `sqlite3 <db> "<sql>"` accepts the CLI's own dot-commands where SQL is
    expected, and `.system` / `.shell` run programs. Verified:
    `sqlite3 db ".system cmd /c echo … > file"` created the file — so
    `db.sqlite_query`, `db.sqlite_exec` and `db.sqlite_export_csv` were
    arbitrary execution wearing a database API. `reject_sqlite_dot_command`
    refuses them; dot-commands belong to the shell, not to SQL, so no SQL is
    lost. `db_path` is the first positional and now goes through
    `reject_option_like` too.
  - `tmux new-session -d -s <name> <cmd>` runs `<cmd>` — `sh -c` renamed.
  - `tmux send-keys -t <target> <keys> Enter` types into a live shell and
    presses return.

  Both tmux paths now take `guard_exec` and are classified `Effect::Exec`.

  Checked and sound: `git` (its command-executing options must precede the
  subcommand, and all 32 sites fix the subcommand first), `find` (fixed
  predicates, never `-exec`), `wmic` (`get` only, never `process call
  create`), and the programs where caller values land after a fixed flag.

  This is a review against command-execution mechanisms that are known, across
  216 tools — stronger than the two assumptions that preceded it, but not a
  proof that no other mechanism exists.

## [2.0.2] - 2026-08-04

### Security
- **Double-quoted PowerShell and AppleScript injection (CWE-78).** Completes
  2.0.1. That release fixed the single-quoted PowerShell sites and asserted the
  double-quoted ones were safe because they escape `"`. **That assertion was
  made from reading and was wrong.**

  A double-quoted PowerShell string expands `$`, so `$(command)` executes with
  no quote character in the payload at all — escaping `"` as `` `" `` stops
  nothing. Verified: `crypto.base64_encode("$(New-Item …)")` created the file.
  21 sites were affected — GUI window control, screenshot paths, toast
  notifications, dialog titles, `Read-Host` prompts, password generation,
  `crypto.hash`, `crypto.hmac`, and base64 encode/decode.

  All 21 now interpolate through `safety::ps_quote` into a **single**-quoted
  literal, which removes expansion outright rather than trying to enumerate
  which metacharacters need escaping.

  The two macOS `osascript` sites (`display notification`, `display dialog`)
  had the same shape — an unescaped `"` closes an AppleScript literal, after
  which `" & (do shell script "…") & "` runs. `safety::applescript_quote`
  escapes backslash first, then the quote; the other order is undone by the
  payload.

  If you are on 2.0.0 or 2.0.1, upgrade. 2.0.0 additionally lacks the 2.0.1
  fixes.

## [2.0.1] - 2026-08-04

### Security
- **Argument injection into PowerShell and archivers (CWE-78, CWE-88).** Found
  while reviewing the fixed-program `Command::new` sites that 2.0.0's exec gate
  deliberately did not cover. Both defects bypassed that gate entirely: an agent
  denied `sh` and denied the nine exec builtins could still reach arbitrary
  execution.

  *PowerShell (Windows).* 17 builtins built commands by interpolating
  caller-controlled values into single-quoted PowerShell literals — service
  control, Hyper-V, `Get-EventLog`, `Get-LocalGroupMember`, `Get-FileHash`,
  registry reads, firewall rules, clipboard, and the zip builtins. A value
  containing `'` closed the literal and the rest executed. Verified: a service
  name of `x'; New-Item -ItemType File -Path '<tmp>' -Force; '` created the
  file; the same payload after the fix is treated as a string.
  `safety::ps_quote` is now the single escaping point, and it returns the value
  *with* its quotes so a missed call site is a syntax error rather than a
  silently unquoted value.

  *Archivers.* `tar -cvf <archive> <files>` passed a caller-supplied file list
  with no `--`, so a "file" named `--use-compress-program=sh -c '…'` ran that
  command; Info-ZIP's `-TT` is equivalent; and `zip` took the archive name as
  the first positional. `safety::reject_option_like` now refuses positional path
  arguments beginning with `-` (pass `./-name` for a file genuinely so named),
  and `--` is passed to `tar` as well.

  Escaping was previously inconsistent rather than absent — two sites already
  doubled quotes correctly, which is why the defect survived review: the pattern
  looked handled.

## [2.0.0] - 2026-08-04

A major version because three changes break existing callers, all of them
consequences of closing security findings. Read this section before upgrading:

- **HTTP API clients must now send `Authorization: Bearer <token>`** or every
  request returns 401.
- **Library consumers constructing `AgentApiConfig` with struct-literal syntax
  will not compile** — it gained an `auth_token` field.
- **Agent-mode scripts calling `timeout`, `xargs`, `proc.spawn`, `nohup`,
  `strace`, `ltrace`, `perf.stat`, `perf.record` or `lxc.exec` now require
  approval**, the same as `sh` always did.

Human/REPL mode is unchanged.

### Security
- **The Agent API now requires a bearer token.** `POST /api/v1/eval`
  ("evaluate raw code") was mounted with no authentication on *any* route,
  while `enable_cors` defaults to true and layered
  `allow_origin(Any) / allow_methods(Any) / allow_headers(Any)` on top. Any web
  page visited while the server was running could preflight successfully and
  POST to the user's loopback interface — drive-by remote code execution.

  Every route except `/health` now requires `Authorization: Bearer <token>`,
  compared in constant time. The token comes from `--token`, else
  `AETHER_API_TOKEN`, else one is generated and printed at startup; there is no
  way to disable authentication. **Breaking** for existing API clients: they
  must now send the header. See `docs/api/AGENT_API.md`.

- **The exec gate covers the capability, not just the name `sh`.** `bi_sh` was
  the only builtin that gated on `Effect::Exec`, so in agent mode — with `sh`
  disabled outright, the intended hardened configuration — `timeout`, `xargs`,
  `proc.spawn`, `nohup`, `strace`, `ltrace`, `perf.stat`, `perf.record` and
  `lxc.exec` still ran arbitrary commands with no approval prompt and no
  exec-classified audit entry. Confirmed by execution, not by reading:
  `timeout(5, "touch <marker>")` returned 0 and created the file.

  All nine now route through `safety::guard_exec`, and `effect_of` classifies
  them as `Effect::Exec` so the agent API stops advertising them as
  side-effect free. Human mode is unchanged. **Breaking** for agent-mode
  scripts that used these without approval.

- **`static mut` PRNG state replaced with atomics** in `neural` and
  `evolution`. Reachable from the multi-threaded API server, so concurrent
  calls were a data race. Sequence and `seed_rng` reproducibility unchanged.

### Removed
- **Docker is no longer a distribution channel.** The `Dockerfile` and the
  Docker workflow have been deleted, along with the `docker pull` /
  `docker run` instructions on the website, in `ROADMAP.md`, and in the
  productization plan. No image is published for new releases; install via
  the release binaries, Homebrew, crates.io, npm, or PyPI instead.

  The workflow had failed on *every* run since 2026-02-11 and only ever ran on
  `v*` tags, so it broke unnoticed for five months. Three separate faults were
  found and fixed before the channel was dropped — missing cargo target stubs
  in the dependency-cache layer (twice, in both build steps), and a `rust:1.75`
  base image that can no longer parse a dependency tree which has moved to
  edition 2024. The first fix was confirmed; the rest are moot now.

  AetherShell's *support for* Docker is unaffected — the `docker`, `podman`,
  and `container` builtins, the `docker-compose` wrapper, and running MCP
  servers in containers all remain.

### Changed
- **Declared MSRV corrected from 1.75 to 1.88.** `rust-version = "1.75"` in all
  three manifests had become false: dependencies now declare `rust-version` up
  to 1.88, and several (`base64ct`, `bcrypt`, `pbkdf2`, `time`, `url`, `home`)
  have moved to edition 2024, which Cargo 1.75 rejects outright. 1.88 is the
  floor implied by the dependency tree; it is not verified by a build on 1.88
  itself, as there is no MSRV job in CI.

## [1.7.3] - 2026-07-29

### Fixed
- **`aarch64-unknown-linux-gnu` release builds now link.** With the dependency
  blockers cleared in 1.7.1/1.7.2, every aarch64 crate compiled — and then the
  link step failed:

  ```
  rust-lld: error: symbols.o is incompatible with elf64-x86-64
  ```

  Cargo defaults to the host `cc` for every target. The release workflow has
  been installing `gcc-aarch64-linux-gnu` all along, but nothing told Cargo to
  use it. A `.cargo/config.toml` now maps the target to
  `aarch64-linux-gnu-gcc`.

  That file is deliberately **excluded from the published crate**: on a native
  aarch64 host, `cargo install aethershell` would otherwise pick it up and
  demand a cross toolchain that such a machine has no reason to have.

  This completes the cross-compilation work — the release matrix builds all
  seven targets for the first time since at least v1.3.1.

## [1.7.2] - 2026-07-29

### Removed
- **`rodio` (and with it `cpal`, `alsa`, `alsa-sys`, `symphonia`).** The
  dependency was declared and pulled into the `native` feature, but never
  imported anywhere — the only references were two comments saying "would use
  rodio". It pulled `alsa-sys`, whose build script needs ALSA headers for the
  *target* architecture, and after the TLS fix it was the **last** thing
  preventing the `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-gnu`
  release builds from compiling. Verified absent from both targets with
  `cargo tree --target <triple> -i alsa-sys`.

  No functionality is lost, because none existed.

### Fixed
- **`AudioPlayer` no longer claims to succeed at nothing.** `play()` printed
  "🎵 Playing audio…" and returned `Ok(())` without playing anything, and
  `stop()` did the same — indistinguishable from success to a caller or an
  agent. Both now return an explicit "not implemented" error, matching how the
  unsupported `gui.*` builtins behave.

## [1.7.1] - 2026-07-29

### Changed
- **TLS now uses rustls rather than the platform's native TLS**, fixing every
  cross-compiled release build. `reqwest` already requested `rustls-tls`, but
  `default-features` was never disabled — so reqwest's default `default-tls`
  (native-tls) was compiled *as well*, pulling `openssl-sys`, whose build
  script needs OpenSSL headers for the **target** architecture. That is why the
  `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-gnu` release jobs had
  failed on every tagged release since at least v1.3.1.

  `openssl-sys` is now absent from every target (verified with
  `cargo tree --target <triple> -i openssl-sys`). reqwest's other three default
  features (`charset`, `http2`, `macos-system-configuration`) are re-added
  explicitly, so this is purely a TLS-backend change.

  **Behavioral note:** rustls uses its own bundled root store and does *not*
  consult the operating system trust store. If you depend on an enterprise CA
  installed in the OS, build with the new opt-in feature:

  ```
  cargo build --features vendored-tls
  ```

  which restores native-tls with OpenSSL compiled from vendored source — so it
  still cross-compiles, unlike the previous configuration.

## [1.7.0] - 2026-07-29

### Added
- **Prompt styles** (`src/prompt.rs`) — `[prompt] style` selects `classic`,
  `fish`, `powerline` (oh-my-posh style), `pure`, or `custom`. fish-style path
  abbreviation (`~/d/n/AetherShell`), powerline segment blocks with per-segment
  colors, right-aligned and transient prompts, and a two-line mode. Git branch
  is read straight from `.git/HEAD`, so the prompt costs no subprocess; the
  optional `show_git_dirty` is the only path that shells out. See
  [docs/PROMPT_GUIDE.md](docs/PROMPT_GUIDE.md).
- **`PromptConfig::format` is now actually rendered.** The `{cwd}`,
  `{git_branch}`, `{user}`, `{host}`, `{time}`, `{status}`, `{symbol}`
  placeholders have been documented in the shipped config since the config
  system landed, but nothing expanded them — the REPL printed a hard-coded
  `æ❯`. They work now, along with new `{full_cwd}`, `{duration}`, `{newline}`.
- **fish-style line editing** (`src/line_editor.rs`) — history-backed ghost-text
  autosuggestions (→ / Ctrl-F to accept), abbreviations expanded on space or
  Enter, history recall that restores your in-progress draft, and emacs-style
  cursor/word keys. Falls back to plain reads when stdin is not a TTY. History
  now persists to `$XDG_DATA_HOME/aether/history`.
- **Agentic syntax v3: bare-dot implicit lambda.** In pipe position the `~` is
  implied for lambda-taking builtins — `|w.size>1k` ≡ `|w~.size>1k`. Measured
  **23.7% fewer tokens on predicate pipelines** (real cl100k BPE); reproduce
  with `cargo run -p agentic-eval --example sigil_audit --features real-tokens`.
  Restricted to pipe position so `m.name` at statement start remains a field
  access on a variable named `m`.
- **agentic-eval `sigil_audit` example** — measures every agentic construct
  against real cl100k/o200k BPE and tests candidate alternative encodings, so
  sigil choices are made on data rather than intuition.

### Changed
- **MCP tool inputs are now validated against their JSON Schema**
  (`ai::mcp::validate_against_schema`). Previously `validate_input` accepted
  everything with a `TODO: Implement full JSONSchema validation`. All violations
  are reported at once, so an LLM repairing its own tool call needs one retry
  rather than several.
- **The swarm `Router` policy now routes.** It previously always selected agent
  0; it now scores each agent's declared capability surface (system prompt plus
  tool names, tools weighted double) against the latest blackboard message, and
  avoids letting one agent monopolize the swarm. Deterministic and LLM-free —
  routing runs every tick.

### Changed
- **`from-yaml` / `to-yaml` are now a real YAML implementation** (`src/yaml.rs`).
  The previous version split each line on the first `:` into one flat record,
  which silently mis-parsed almost every real document: nesting collapsed,
  sequences vanished entirely (a `- item` line has no colon, so it was dropped),
  `#` comments were kept as part of the value, and `---` became a `{"---": ""}`
  entry. `to-yaml` emitted values unquoted, so any string containing `: `
  produced output that would not read back.

  The replacement handles nested mappings, sequences, quoted scalars with
  escapes, comments, document markers, and JSON-style flow collections — and
  **fails loudly** on what it does not implement (anchors, aliases, merge keys,
  block scalars, tags, multi-document streams), naming the construct. Duplicate
  keys are an error rather than silent last-wins. Emitted output is quoted
  wherever required, so `to-yaml` then `from-yaml` round-trips.

  For a shell whose pitch is deterministic typed output, silently returning
  wrong data is the worst available behavior — an agent cannot detect it. No new
  dependency: `serde_yaml` was deprecated upstream in 2024 and would have
  introduced an unmaintained advisory into the `cargo-deny` gate.

  Both builtins now also accept a positional argument, matching `from-json`.

### Security
- **Invisible and bidi-override characters are rejected in paths**
  (`validate_safe_path`, CWE-1007). A zero-width space or RTL override makes two
  paths render identically while resolving to different files — so an agent
  approving `config.toml` could be handed `config\u{200B}.toml`. Rejected rather
  than stripped, since silently rewriting a path means the caller acts on a file
  it did not name. Ordinary non-ASCII (`é`, `日`) is unaffected. Found by
  strengthening a test that previously asserted `is_ok() || is_err()`.

### Fixed
- **`agent api serve` panicked on a hostname bind address.** `SocketAddr` parses
  literal addresses only, so a config with `host = "localhost"` aborted the
  server via `.parse().unwrap()`. It now reports the problem and suggests a
  literal IP.
- **Vacuous test assertions replaced with real properties.** Thirteen assertions
  of the form `assert!(x.is_ok() || x.is_err())` /
  `assert!(v.is_empty() || !v.is_empty())` could never fail. They now assert
  what the tests were actually for: detected MCP servers are well-formed, model
  URIs are *understood* even when the call fails for lack of a key, failures
  carry actionable messages, agent trace steps record a thought or a command,
  and MCP detection does not change AI backend selection.
- **The interactive REPL was never reaching `repl.rs`.** `main.rs` carried a
  second, inline REPL — added "to avoid extra wires" — that printed a
  hard-coded `ae> ` prompt and `{:?}`-formatted values. `ae` now delegates to
  the real REPL, so prompt configuration, history, and value rendering apply.
- **Intermittent test failure in `tests/advanced_ai.rs`.** The RAG index,
  knowledge graph, and semantic cache are process-global, and 28 of the 29 tests
  in that file populate or clear them; run in parallel, one test's
  `semantic_cache_clear` landed between another's put and get, turning an
  expected hit into a miss. All tests in the file now take a shared,
  poison-tolerant `store_lock()`.
- **Intermittent test failure in `security::tests::test_path_validation_basic`**
  (failed in roughly half of full-workspace runs). `validate_safe_path` consults
  `safety::current_mode()`, which reads the process-global `AETHER_MODE`; the
  ~10 `safety` tests that set `AETHER_MODE=agent` were serialized behind a
  module-private `ENV_LOCK` that the `security` test could not take, so a
  concurrent safety test switched the workspace jail on and made even `"."`
  fail validation. `ENV_LOCK` is now crate-visible (`safety::ENV_LOCK`) and both
  modules take it.
- `providers::platform::test_platform_detection` asserted
  `caps.full_shell || !caps.full_shell`, a tautology that could never fail. It
  now checks detection stability and the desktop/mobile capability invariants.
- **CI is green again on all three platforms.** It had failed on every push to
  master since 2026-06-11 (8 consecutive runs), for three independent reasons:
  - `cargo fmt --check` failed on 19 `agentic-eval` files.
  - `cargo clippy -D warnings` failed on a `clippy::correctness` error, plus 63
    style lints that fire *only* inside `#[cfg(not(target_os = "windows"))]`
    blocks — invisible to a Windows checkout, which is why they accumulated.
    One (`unused_mut`) is fixed; the rest are baselined in `lib.rs` alongside
    the existing deferred-cleanup allows, with a note on how to clear them from
    a Linux checkout. None has a correctness component.
  - Every test in `tests/examples_smoke.rs` failed on Linux and macOS: the file
    hard-coded `target/debug/ae.exe`, so the binary was never found off Windows.
    It now uses `env!("CARGO_BIN_EXE_ae")`, which also fixes `--release` and
    custom `CARGO_TARGET_DIR` runs.
  - **The crate did not compile on macOS at all.** Twelve `gui.*` builtins had
    Windows and Linux branches and no third arm, so on macOS the function body
    yielded `()` where `Result<Value>` was required (12 × E0308). Each now
    returns an explicit "not implemented on this platform" error — an error
    rather than `Ok(false)`, which would conflate "this platform can't" with
    "the window wasn't found".
  - **The WASM build was broken** by three non-exhaustive matches: `Value` grew
    a `Builtin` variant that the non-native `Display` impl and two `wasm.rs`
    formatters never handled. Reproduce a fix locally with
    `cd web && cargo check --target wasm32-unknown-unknown` — unlike the
    Linux-only lints, this one *is* checkable from any host.
- Crate documentation claimed "430+ built-in functions across 50 modules" and
  "MCP (130+ tools)" on the docs.rs front page; the real figures are 1,284
  builtins across 108 modules, and MCP defaults to a three-tool compact
  discovery surface. A test now pins the module count so it cannot drift again.

## [1.6.0] - 2026-06-04

### Added
- **agentic-eval 0.8.0** — the bundled evaluation library gains a `vms` module:
  a curated benchmark of VM/sandbox systems (AetherVM, Firecracker, Cloud
  Hypervisor, gVisor, Kata Containers, QEMU/KVM, Docker) for agentic AI use,
  scored on agent-native axes (start-latency, density, isolation, snapshotting,
  agent-control). See `crates/agentic-eval/CHANGELOG.md`.

### Security
Hardening from a security audit (CVE / NIST FIPS / MITRE ATT&CK / CMMC 2.0):
- **0 dependency CVEs** — patched `quinn-proto` (HIGH QUIC DoS), `rustls-webpki`
  (4 TLS cert-path-validation flaws), and `tar` (symlink chmod / PAX); repaired the
  `cargo-deny` supply-chain gate (was unparseable under cargo-deny ≥0.18, and denied
  the project's own AGPL license).
- **SHA-256 integrity** (was MD5, collision-broken) for checkpoint/state integrity
  (`persistence.rs`) and package-download verification (`marketplace.rs`); legacy MD5
  digests still read for backward compatibility, never written.
- **Native plugin loader gated** — `DynamicPlugin::load` is now default-deny in agent
  mode (`AETHER_MODE=agent`) unless allowlisted via `AETHER_PLUGIN_ALLOW=<dirs>`;
  `AETHER_PLUGINS=off` is a kill switch. Closes a native-code-execution surface
  (ATT&CK T1129/T1574).
- **Network egress allowlist** — `AETHER_NET_ALLOW=<hosts>` restricts all network
  builtins to allowed hosts/subdomains (`E_EGRESS_DENIED` otherwise); opt-in, so
  default behavior is unchanged. Anti-exfiltration control (ATT&CK T1041).
- **FIPS-strict mode** — `AETHER_FIPS=1` enforces approved-algorithms-only: the hash
  builtins reject MD5/SHA-1 (`E_FIPS_DISALLOWED`) and integrity verification fails
  closed on legacy MD5 digests (only SHA-256 accepted). Crypto/FIPS posture and the
  remaining path to a FIPS-140-*validated* build documented in
  `docs/security/CRYPTO_AND_FIPS.md`.

## [1.5.0] - 2026-06-02

Token-efficiency release. Backward-compatible; all additions plus one default change
(compact MCP discovery, opt out with `AETHER_MCP_TOOLS=all`).

### Added
- **AECON `@prefix` lever** — a fourth output-compression lever alongside
  `@const`/`@dict`/`@delta`: string columns whose values share a leading run (paths,
  URIs, prefixed ids) emit the shared prefix once in a `@prefix col: …` line and
  strip it from every row. Lossless (round-trip tested), deterministic (char-based
  gate, no tokenizer/float), agent-mode only, never on `@dict` columns. Measured
  44–69% fewer tokens on path-heavy listings (`examples/prefix_gain.rs`).
- **Measurement examples** — `examples/prefix_gain.rs` (the `@prefix` saving),
  `examples/standing_context.rs` (catalog overhead an agent pays per turn), and a
  four-axis composite scorecard (0–10) in `examples/shell_agentic_eval.rs`.

### Changed
- **Compact MCP tool discovery by default** — `tools/list` now exposes a 3-tool
  discovery surface (`ontology_manifest`, `ontology_describe`, and an `aether`
  invoke meta-tool) instead of all ~1085 builtins, cutting the MCP `tools/list`
  payload from ~49k to ~239 tokens (≈206×) of standing context per session. Effect
  gating is unchanged — `aether` routes through `call_builtin`, so destructive ops
  are still approval-gated. `AETHER_MCP_TOOLS=all` restores the flat per-builtin
  `x-effect` listing.

## [1.4.0] - 2026-06-02

Agentic safety, reliability, and tooling release. Backward-compatible; all additions.
Headlines: **secret hygiene** and **resource governors** (agent-mode blast-radius
bounds); **structured `E_BAD_ARG`** for every argument/arity error plus rich human
error rendering; a **strict stdio JSON-RPC MCP transport**; **nested transactions**
and **plan/apply `copy`/`move`**; **RBAC startup config**; **streaming evaluation**;
a **cross-tokenizer benchmark** (cl100k + o200k); and a new standalone
**`agentic-eval`** crate (four-axis program evaluation) applied to the real engine.

### Changed
- **`agentic-eval` refined to 0.3.0** — pluggable tokenizer: `tokens::evaluate_with`
  and `rank_with` take any `Fn(&str) -> usize`, so a host (e.g. AetherShell) flows its
  own exact tokenizer through the cost model instead of hand-building `AgentCost`.
  Added `AgentCost::total_standing_per_turn` (no-prompt-caching upper bound, vs the
  caching-amortized `total_over` default) and `safety::assess_safety_named` (score
  from operation names + a classifier closure). The AetherShell integration test now
  flows through `evaluate_with`/`assess_safety_named`. Doctests added; Clippy-clean
  across all feature combinations.
- **`agentic-eval` refined to 0.2.0** — ergonomic crate-root re-exports
  (`agentic_eval::Model`/`Program`/`Effect`/…), `Display` for every report type
  (printable summaries), an optional `serde` feature deriving `Serialize` on all
  report/config types (machine-readable output), `Model::from_name` (CLI/config
  parsing, parity with `Effect::from_name`), `tokens::rank` (N-way generalization of
  `compare`), `Evaluation` `with_*` builders, and a more faithful heuristic that
  splits `snake_case` subwords (`file_read` ≈ 2 tokens). Clippy-clean across all
  feature combinations; 27 tests with `serde`.

### Added
- **New crate `agentic-eval`** (`crates/agentic-eval`) — a standalone, reusable
  library for evaluating how well a *program* serves an agentic AI system across
  four axes, each with tests: **token efficiency** (standing-context + input +
  output + retry cost under OpenAI GPT-4 `cl100k`, GPT-4o `o200k`, and a documented
  Claude approximation; exact with `--features real-tokens`, heuristic otherwise),
  **determinism** (byte-stability of output across runs), **reliability** (pass rate
  + structured/actionable-failure rate over representative invocations), and
  **safety** (blast-radius gating score from a program's declared effects under an
  agent policy). Execution-agnostic (closures/effects supplied by the caller),
  zero heavy deps by default. Includes an `evaluate` example and 22 tests.
- **`agentic-eval` applied to AetherShell's real engine (§4.0c).**
  `examples/agentic_eval.rs` + `tests/agentic_eval_integration.rs` wire the library
  to AetherShell's actual tokenizer (`est_token_count`), evaluator, canonical
  renderer, and `safety::effect_of`, scoring the shipped engine on all four axes:
  legible wins token efficiency over a session (~1.1k vs ~6.1k), `render_canonical`
  is byte-stable, wrong-typed args surface as actionable `E_BAD_ARG`, and dangerous
  builtins are blast-radius bounded (grade A) under the agent policy. `agentic-eval`
  is a dev-dependency; `Effect::from_name` maps host effect names into the taxonomy.
- **Cross-tokenizer benchmark check (§4/Phase 1).** `examples/token_bench.rs` now
  re-runs the §4 cipher-vs-legible criterion under a second real BPE (GPT-4o
  `o200k_base`) in addition to `cl100k_base`, and the legible-first verdict holds
  under both (standing-context ~11× under each) — confirming the result isn't
  cl100k-specific. Corpus broadened from 10 to 13 tasks. (Anthropic ships no offline
  Claude tokenizer crate, so `o200k_base` serves as the cross-provider proxy.)
- **Streaming evaluation (§6.3).** New `eval::eval_stream(code, env, on_item)`
  produces results incrementally instead of materializing the whole value first.
  For a final pipeline `source | map/where/filter…` over an array source, each
  element is pushed through the stage chain one at a time and emitted as soon as it
  survives — true stage-by-stage streaming, lazy per element. Each stage is driven
  through the same pipe mechanism the eager evaluator uses (no second map/where
  implementation; `eval_expr`/`eval_program` untouched). Whole-collection stages
  (sort/take/reduce/uniq) or non-array sources fall back to eager evaluation, still
  emitting elements after, so correctness holds for every input. The
  `/api/v1/stream/eval` SSE route now emits `chunk` events as items are produced.
- **MCP stdio transport (§9).** `ae [--agent] mcp stdio` runs a strict JSON-RPC 2.0
  MCP server over stdin/stdout (the canonical MCP transport) — `initialize`,
  `tools/list`, `tools/call`, `ping` — exposing every builtin as a tool routed
  through the safety model (policy/jail/approval/audit). `McpServer::serve_stdio`
  drives the loop; `McpServer::handle_rpc` is the unit-tested per-request dispatch.
- **Nested transactions (§9).** `tx_begin` now nests: a second begin while a
  transaction is active pushes a child frame (SQL nested-transaction semantics). A
  child `tx_commit` folds its changes into the parent (nothing is durable until the
  outermost commit); a child `tx_rollback` reverts only the child's operations and
  leaves the parent open. Each frame captures its own pre-image, so inner rollback
  is correct even when an outer frame touched the same path; outer rollback still
  undoes everything. `tx_begin`/`tx_status` report nesting `depth`. `apply` now
  nests cleanly inside an outer transaction instead of erroring.
- **RBAC startup config (§13).** `RbacManager::from_config_str` parses a TOML
  config (roles with permissions + inheritance; principals with role assignments +
  direct grants), and `safety::init_rbac_from_env()` (called from `main` at boot)
  loads it from `AETHER_RBAC_CONFIG` (or `<workspace>/.ae/rbac.toml`), installs the
  manager, and sets the acting principal (`AETHER_PRINCIPAL`, else the config's
  `principal`). The authorization model can now be configured from a file at
  startup, not just via in-shell `rbac_*` calls.
- **Plan/Apply `copy` and `move` ops (§9).** `plan`/`apply` now support `copy` and
  `move` operations alongside `write`/`append`/`rm`/`mkdir`; each takes a `dest` (or
  `to`) destination path. Both endpoints are workspace-jailed and snapshotted, so a
  copy/move participates in the atomic transaction and rolls back cleanly on failure.

### Changed
- **Boundary type-checking (§8) — reliability.** The shared argument-extraction
  helpers (`expect_string`/`expect_int`/`expect_array`/`need_lambda`, ~90 call
  sites) now emit a structured, catchable `E_BAD_ARG` (`safety::bad_arg`) naming
  both the expected and the actual type, instead of ad-hoc `anyhow!` prose. A
  wrong-typed argument to any builtin using them is now branchable by agents
  (caught by try/catch as `{error:{code,message,hint,…}}`) for self-correction.
  The arity (missing-arg) counterpart `arg(builtin, args, idx, expected)` gives the
  same structured `E_BAD_ARG`; the core agent-facing verbs (`map`/`where`/`reduce`/
  `take`/`call`/`agent`/`swarm`/`mcp_call`) now use it. `type_builtin_call` static
  inference also gained shape-preserving array transforms (`sort`/`uniq`/`take`/
  `head`/`tail`) and `len`/`wc` → `Int`. **All ~490 remaining prose arity/usage
  errors across the builtins (plus the evaluator's lambda-arity checks) are now
  structured `E_BAD_ARG`** via `safety::arg_err(message)` — message text preserved,
  error type upgraded, so every argument/arity failure is branchable and renders as
  legible prose for humans. Non-argument errors (API-response parsing, feature
  gating, URL validation, workflow definitions) were intentionally left untouched.
- **Richer human error rendering (§8) — legibility.** The human REPL unpacks an
  uncaught `SafetyError` into legible prose — `error[CODE]: message`, an indented
  `hint:` line, and (for approvable actions) the exact `AETHER_APPROVE=…` re-run
  incantation — instead of printing the raw JSON. Agent mode still emits the JSON
  so the structured fields survive for programmatic branching.

### Added
- **Resource governors (§7.6)** — a per-run blast-radius envelope enforced at the
  `guard()` chokepoint, agent-mode only, all limits opt-in via env (unset =
  unlimited). A breach returns the structured, non-retryable `E_BUDGET_EXCEEDED`
  so an agent stops instead of looping:
  - `AETHER_MAX_OPS` — total guarded operations per run.
  - `AETHER_MAX_FILES` — filesystem ops (WriteLocal + Destructive).
  - `AETHER_MAX_PROCS` — process/exec ops (Process + Exec).
  - `AETHER_MAX_NET` — network egress ops (Network). Enforced by routing **every
    egress builtin** through `guard()` with the `Network` effect via the
    `guard_network` helper — `http_get`, `curl_exec`, `wget_download`, and the full
    `web_*` fetch family (`web_fetch`/`web_get`/`web_post`/`web_json_get`/
    `web_json_post`/`web_scrape`/`web_download`/`web_headers`/`web_cookies`/
    `web_form_submit`/`web_upload_file`/`web_rest_api`/`web_graphql`/`web_check_url`;
    `web_robots_txt`/`web_sitemap` inherit via delegation) — which also brings every
    network call under the hash-chained audit log.
  - `AETHER_TIMEOUT_MS` — wall-clock budget since the first guarded op.

  New builtins `governor_status()` (counts/limits/elapsed, also folded into
  `safety_status()`) and `governor_reset()` (start a fresh envelope). New
  `E_BUDGET_EXCEEDED` safety error code.
- **Secret hygiene (§7.6)** — two deterministic defenses keep credentials out of
  the agent's context window and the audit log, opt-out via `AETHER_REDACT=off`:
  - **Shape redaction**: known secret *shapes* — provider key prefixes
    (`sk-`/`sk-ant-`, `ghp_`/`gho_`…, `xox*-`, `AIza…`, Stripe `sk_live`/`rk_test`),
    AWS access-key ids (`AKIA…`), JWTs, PEM `PRIVATE KEY` blocks,
    `scheme://user:password@host` URL credentials (password only), and
    `key=secret`/`key: secret` assignment forms — are replaced with `[REDACTED]`
    on the **agent render path** and in **every audit entry** before it is hashed
    and persisted. Ordinary text is unchanged byte-for-byte; the hash chain still
    verifies over redacted content.
  - **Env name gating**: in agent mode, reading a secret-*named* env var (`*_KEY`,
    `*TOKEN*`, `*SECRET*`, `*PASSWORD*`, … — `KEY` alone excluded) via
    `env`/`sys.env`/`env.var`/`env.vars` returns an opaque `[REDACTED:NAME]` handle
    instead of the value, unless `AETHER_SECRETS=allow`. Human mode returns the
    value (legibility).
- New env vars: `AETHER_REDACT=off` (disable all redaction) and
  `AETHER_SECRETS=allow` (permit clear reads of secret-named env vars in agent
  mode).

## [1.3.1] - 2026-06-01

Patch: workspace-jail/path-resolution correctness and security fixes for agent
mode. No API changes.

### Fixed
- `file_write` and `file_append` are now jailed to the workspace in agent mode
  (the `WriteLocal` effect guard, matching `rm`/`rmdir`): a write to an absolute
  path outside the workspace is rejected with `E_OUTSIDE_WORKSPACE`. Allowed by
  policy (no approval prompt) — only the workspace containment is enforced. Human
  mode is unaffected.
- In a **jailed context** (agent mode or an explicit `AETHER_WORKSPACE`), relative
  paths passed to effecting builtins (`file_write`/`file_append`/`rm`/`rmdir`) now
  resolve against the **workspace root** instead of the process CWD. This closes a
  soundness gap where a relative-path write could escape the workspace yet pass the
  jail check (when CWD ≠ workspace), and keeps the write, the jail check, and the
  transaction journal in agreement. Human mode is unchanged (relative → CWD).
- Path access is no longer sandboxed to the project directory in **human mode** —
  `ae` now behaves like a normal interactive shell (read/list/write any path).
  The workspace jail applies only in **agent mode** (`--agent`/`AETHER_MODE=agent`)
  or when a workspace/allowed-base-dir is explicitly configured. Always-on hygiene
  (null-byte, length, blocked-pattern, symlink checks) is unchanged.

## [1.3.0] - 2026-05-31

Agentic-first release: AetherShell is now optimized end-to-end for AI agents —
token-efficient structured output, a real safety/transaction model, and a unified
single-pass agentic parser. Backward-compatible; all additions, measured with the
real GPT-4 cl100k tokenizer.

### Added
- **AECON (Aether Compact Object Notation)** — compact structured output: a
  header-once tabular form with three deterministic, gated compression levers:
  constant-column factoring (`@const`), dictionary encoding for low-cardinality
  string columns (`@dict`), and delta encoding for large slowly-varying integer
  columns (`@delta`). ~2.8× fewer output tokens than POSIX shells and ~2.6× vs
  PowerShell's parseable JSON on realistic tabular results.
- **Lossless reversibility** — `aecon_decode` reverses the tabular form; optional
  `@type` tags make numeric-looking strings and integral floats round-trip exactly.
- **Agent-mode default rendering** — under `AETHER_MODE=agent`, results render as
  compact AECON automatically across the CLI, HTTP Agent API, and MCP server.
- **Token-economy builtins** — `aecon`, `aecon_decode`, `pick` (source-side field
  projection), `budget` (token-bounded paging with a cursor), `digest`
  (constant-token structural summary), `canonical` (deterministic JSON), `tokens`
  (cl100k estimate), and `ontology_manifest`/`ontology_describe` (progressive
  ontology disclosure).
- **`--deterministic` / `AE_DETERMINISTIC`** — render results as canonical,
  byte-stable JSON for snapshot tests, content-addressable caching, and diffs.
- **Safety model** — an effect taxonomy (Pure → Privileged), a
  capability→policy→approval→audit pipeline, content-bound approval tokens, a
  workspace jail, a hash-chained tamper-evident audit log, RBAC, structured `E_*`
  errors, and `safety_status` introspection. CLI flags `--agent`/`--workspace`/
  `--policy`.
- **Filesystem transactions & checkpoints** — `tx_begin`/`tx_commit`/`tx_rollback`/
  `tx_status` with **named savepoints** (`tx_savepoint`/`tx_rollback_to`) for
  partial rollback. Covers file writes/appends, deletes, recursive directory
  trees, sqlite databases, and the key-value store. No conventional shell offers
  this.
- **`plan` / `apply`** — Terraform-style declarative destructive batches: a
  reviewable typed plan plus a content-bound approval token, applied atomically
  inside a transaction with automatic rollback on any failure.
- **Persistent key-value store** — `db_kv_get`/`set`/`delete`/`keys`/`store`,
  transactional via the sqlite snapshot chokepoint.
- **Grammar additions** — native `|.field` projection, SI numeric suffixes
  (`1k`/`1M`/`1G`), `~x: body` lambdas, `?val{arms}` match, and
  `if cond { … } else { … }` expressions.
- **MCP server** now exposes builtins as effect-tagged tools; the **HTTP Agent
  API** gained per-call token accounting and real chunked streaming.
- **Cross-shell token benchmark** (`cargo run --example shell_bench --features
  real-tokens`) comparing AetherShell to Bash/Zsh/Fish/Nushell/PowerShell.
- **Real GPT-4 cl100k tokenizer** behind `--features real-tokens` for exact,
  authoritative token measurement (heuristic otherwise).

### Changed
- **Agentic `.aeg` transpiler rewritten as a single tokenizing pass** (Phase 5),
  retiring the former 10-pass text-substitution pipeline (~2,000 lines removed).
  Terse tokens can no longer be mis-expanded by pass ordering.
- Measure-first verdict committed the agent surface to **legible-first** syntax;
  the `.aeg` cipher remains an opt-in legacy surface.

### Fixed
- Zero-warning build restored across all targets.
- Two stale PowerShell transpiler integration tests (now assert the emitted
  `file.read`/`proc.list`); an `AETHER_MODE` env-var test race serialized.

## [1.2.0] - 2026-02-15

### Added
- Package registry client connecting marketplace builtins to packages.nervosys.ai
- Remote search, install, publish, update, and yank for community packages
- Tarball download and extraction for registry packages (flate2 + tar)
- Auth token support via `AETHER_REGISTRY_TOKEN` / `AETHERSHELL_TOKEN` env vars
- Expanded Candle backend with transformer architectures (Llama, Mistral, Phi)

### Changed
- Version bump from 0.3.1 to 1.2.0 across all project files and metadata
- Marketplace builtins now try remote registry first with local fallback

### Fixed
- Security test race condition — Mutex serialization for `configure_command_security()` tests
- Flaky `friendly_user_allowed_commands` test now stable (5/5 consecutive runs)

## [0.3.1] - 2026-02-10

### Changed
- License changed to AGPL-3.0-or-later with commercial dual-license
- Crate package size optimized from 10.1 MiB to 733 KiB compressed

### Added
- Contributor License Agreement (CLA) with CI enforcement
- GitHub Linguist submission package (samples, grammar, guide)
- README badges for CI, downloads, VS Code marketplace
- Windows Terminal profile fragment (`editors/windows-terminal/`)

### Fixed
- All 88 compiler warnings resolved (zero-warning build)
- Python SDK license corrected to AGPL-3.0
- Unused `SchemaFormat` import in binary crate

## [0.3.0] - 2026-01-30

### Added
- Implicit match scrutinee in lambda bodies
- Python SDK (`integrations/python/`)
- GitHub Actions CI/CD (ci.yml, release.yml, docker.yml, security-audit.yml)
- CLA check workflow
- Bash transpiler improvements

## [0.2.0] - 2026-01-15

### Added - Plugin System
- Plugin architecture with dynamic TOML manifest loading
- 7 plugin builtins: `plugins()`, `plugin_info()`, `plugin_enable()`, `plugin_disable()`, `plugin_load()`, `plugin_unload()`, `plugin_categories()`
- Example plugins (hello-plugin, math-utils, string-utils)
- Built-in file handlers (JSON, CSV, TOML)

### Added - Language Features
- Async/await syntax (`async fn(x) => expr`, `await expr`)
- Try/catch/throw error handling
- Conditional compilation (`#[cfg(platform)]`, `#[cfg(feature = "name")]`)
- Module visibility (pub/private, export, re-export)
- Package management (import syntax, aether.toml, registry)
- N-ary lambda support (3+ parameters)
- Zero-parameter lambdas
- Standard library (7 modules in lib/)
- Platform detection builtins
- Runtime feature flags
- Debugging tools (debug, trace, assert, inspect)
- Error recovery with multi-error reporting

### Added - Enterprise
- RBAC (role_create, role_grant, check_permission)
- Audit logging (audit_log, audit_query, audit_export)
- SSO integration (sso_init, sso_auth, sso_validate)
- Compliance reporting

### Added - AI
- RAG (Retrieval-Augmented Generation)
- Knowledge graphs
- Semantic caching
- Fine-tuning management

### Added - Infrastructure
- LSP server (aethershell-lsp crate with tower-lsp)
- VS Code extension v0.2.0 (hover, symbols, folding)
- Distributed computing builtins (cluster, job scheduling)
- 5 benchmark suites (parser, eval, pipeline, builtin, MCP)
- WASM support with browser REPL
- Distribution: Homebrew, Docker, npm, browser extension

## [0.1.0] - 2025-10-22

### Added - Core Language Features
- **Typed Functional Language**: Hindley-Milner type inference
- **Structured Data**: Records, Arrays, Tables (not text streams)
- **First-Class Functions**: Lambdas, closures, higher-order functions
- **Pattern Matching**: Match expressions with guards
- **Pipelines**: Typed data transformation pipelines
- **Mutable Variables**: `mut` keyword for mutable bindings
- **Dot Notation**: Field access on records (`record.field`)

### Added - AI Integration
- **Multi-Modal AI**: Native support for images, audio, and video
- **AI Function**: `ai(prompt)` and `ai(prompt, {images: [...], audio: [...], video: [...]})`
- **Multi-Agent System**: Agent creation, coordination, and swarms
- **Agent Protocols**:
  - MCP (Model Context Protocol) for tool integration
  - A2A (Agent-to-Agent) communication
  - NANDA (Negotiation And Dynamic Agents) consensus
- **AI Model Management**: OpenRouter-style API server
- **Local Model Support**: XDG-compliant storage and format conversion
- **Provider Support**: OpenAI, Anthropic, Ollama, and custom backends
- **LLM Backend Integration**: vLLM, TensorRT-LLM, SGLang, llama.cpp

### Added - Builtins
- **File Operations**: `read_text(path)`, `ls(path)`
- **Data Operations**: `map()`, `where()`, `reduce()`, `group()`
- **Type Operations**: `type_of(value)`, `keys(record)`, `len(value)`
- **HTTP**: `http_get(url)`, `http_post(url, data)`
- **Utilities**: `print()`, `range()`, `take()`, `first()`

### Added - Terminal UI
- **Interactive TUI**: Modern terminal interface (`ae tui`)
- **Media Viewer**: Display images, audio, video in terminal
- **Chat Interface**: Conversational AI with multimodal support
- **Agent Dashboard**: Monitor and control agent swarms
- **Media Selection**: Batch selection for multimodal queries

### Added - Infrastructure
- **Bash Compatibility**: Transpiler for running .sh scripts
- **REPL Mode**: Interactive shell with command history
- **Script Execution**: Run .ae files directly
- **XDG Compliance**: Standard config and data directories
- **Cross-Platform**: Windows, Linux, macOS support

### Added - AI Model CLI (`aimodel`)
- **Server Management**: Start/stop API server with OpenAI-compatible endpoints
- **Model Management**: List, search, download, remove models
- **Format Conversion**: Convert between GGUF, SafeTensors, PyTorch, ONNX, TensorFlow
- **Storage Management**: XDG-compliant local storage with statistics
- **Provider Configuration**: Manage API keys for OpenAI, Anthropic, etc.
- **Backend Management**: Auto-detect and configure vLLM, SGLang, TensorRT-LLM, llama.cpp
- **Alias System**: Create shortcuts for frequently used models

### Added - Examples
- `00_hello.ae`: Hello world and basic syntax
- `01_pipelines.ae`: Pipeline transformations
- `02_tables.ae`: Table operations and file system queries
- `03_http.ae`: HTTP requests with typed responses
- `04_match.ae`: Pattern matching examples
- `05_ai.ae`: AI integration basics
- `06_agent.ae`: Agent deployment
- `07_uri_types.ae`: URI type system
- `12_multi_agent_orchestration.ae`: Multi-agent architecture patterns
- `13_multimodal_ai.ae`: Multi-modal AI concepts
- `14_typed_pipelines.ae`: Type-safe functional programming
- `15_ai_protocols.ae`: MCP, A2A, NANDA demonstrations
- `16_mcp_servers.ae`: MCP server concepts
- `17_syntax_showcase.ae`: Core language features
- `18_mutable_variables.ae`: Mutable variable patterns
- `19_showcase.ae`: Pipeline showcase
- `98_dot_notation.ae`: Comprehensive dot notation examples
- `99_comprehensive_test.ae`: All features validation

### Added - Documentation
- Comprehensive README with feature overview
- Quick Reference guide for all syntax
- MCP Servers integration guide
- AI Protocols technical report
- Competitive analysis
- Project structure documentation
- Contributing guidelines
- Type system deep dive
- TUI usage guide

### Added - Testing
- 25+ unit tests for core language features
- Integration tests for AI functionality
- TUI component tests
- Multimodal AI backend tests
- Bash transpiler tests
- Example validation (18/18 passing)
- OS tools cross-platform tests

## Technical Details

### Language Design
- **Parser**: Hand-written recursive descent parser
- **Type System**: Hindley-Milner with type inference
- **Evaluator**: Tree-walking interpreter with environment chains
- **Values**: Sum type for Int, Float, String, Bool, Array, Record, Table, Lambda, Uri, Null
- **Error Handling**: Anyhow for ergonomic error propagation

### AI Architecture
- **Backend Trait**: `MultiModalLlmBackend` for provider abstraction
- **Message Format**: Unified message structure for all providers
- **Media Processing**: Image display (viuer), audio playback (rodio)
- **API Server**: Axum-based HTTP server with OpenAI-compatible endpoints
- **Format Conversion**: Support for 5+ model formats with quantization
- **Local Storage**: XDG Base Directory compliance

### TUI Implementation
- **Framework**: Ratatui for rendering
- **State Management**: Application state machine with tab navigation
- **Media Display**: Terminal-based image rendering and audio waveforms
- **Input Handling**: Crossterm for keyboard/mouse events

### Performance
- **Memory Safe**: Rust's ownership system prevents memory bugs
- **Zero Copy**: Efficient data handling where possible
- **Async Support**: Tokio runtime for concurrent operations
- **Lazy Evaluation**: Pipelines evaluate on-demand

## Breaking Changes

N/A - Initial release

## Migration Guide

N/A - Initial release

## Known Issues

- String multiplication (e.g., `"=" * 50`) not supported - use literal strings
- If statements only work in expression context (ternary) - use `match` for statement-level conditionals
- Pipeline ending with `| len` requires workaround - split into two statements
- Numeric array indexing (`array.0`) not supported - use `map` or functional patterns

## Future Plans

See [ROADMAP.md](ROADMAP.md) for upcoming features.

[Unreleased]: https://github.com/nervosys/AetherShell/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/nervosys/AetherShell/compare/v0.3.1...v1.2.0
[0.3.1]: https://github.com/nervosys/AetherShell/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/nervosys/AetherShell/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nervosys/AetherShell/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nervosys/AetherShell/releases/tag/v0.1.0
