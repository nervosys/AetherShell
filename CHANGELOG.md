# Changelog

All notable changes to AetherShell will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing yet.

## [11.0.2] - 2026-09-01

An integrity fix for the audit log, in the configuration the defaults produce.
The failure mode was silent: a torn record makes the log unparseable, and a
tamper alarm that fires whenever two agents run trains people to ignore the
real ones.


### Fixed — the audit log under concurrent writers

Found by probing the case the default configuration produces: in agent mode the
log is `<workspace>/.ae/audit.log`, so two agents in one workspace share it. The
chain is per-process state, so they interleave two independent chains.

- **Records could be torn.** `writeln!` emits the content and the newline as
  separate writes, and `O_APPEND` only guarantees atomicity *per write*, so two
  entries could land on one line and the log stopped being valid JSON —
  observed as `JSONDecodeError: Extra data: line 1 column 401`. Each record is
  now built with its terminator and issued in one write. A log that fails to
  verify still tells you something happened; a log that cannot be parsed tells
  you nothing.
- **Ordinary concurrency raised false tamper alarms.** Each process's tail
  check treated the other's appends as evidence of rewriting: 33
  `tamper-detected` markers in a single 80-operation run. An alarm that fires
  whenever two agents run is one nobody reads. Entries now carry a per-process
  `writer` id, and a differing writer is recognised as concurrency rather than
  tampering. That id is inside the signed core, so it cannot be used to launder
  a rewrite: relabelling an entry's writer breaks its hash and the log still
  reports tampering.
- **Verification blamed the wrong thing.** A shared log reported `broken chain
  link`, which reads as an attack. It now says the log interleaves entries from
  N writers and names the two remedies — a log per process, or one shared
  append-only `AETHER_AUDIT_SINK`.

**Not claimed:** that a shared log forms a single verifiable chain. It does not,
and cannot without cross-process locking. What is asserted is that it stays
parseable, stays quiet, and explains itself — and that a single writer still
produces one clean chain with contiguous sequence numbers.

### Fixed — the AI chapters documented an API the shell does not have

Publishing the book made its content something a reader could act on, so it was
audited against the running binary rather than read. Both AI chapters described
things that do not exist.

- **Agents are not callable objects.** Every example in `ai/agents.md` was built
  on `let coder = agent("You are a Python expert")` followed by
  `coder("Write a function…")`. A bound value is not callable — `x("hi")`
  answers `unknown builtin: x` — and `agent` takes a *goal*, not a persona, and
  returns a string. The "Conversation Memory" section claimed agents remember
  across calls; `run_sync` builds a fresh dialogue every time, so a second call
  knows nothing about the first. The option record was wrong too: `model`,
  `temperature`, `max_iterations`, `context_length` and `timeout` are read by
  nothing. `agent`/`swarm` read exactly `goal`, `tools`, `max_steps`,
  `dry_run`. The chapter is rewritten against the real signature.
- **`swarm` does not create a swarm.** `ai::agents::swarm::run_sync` is one
  line: it calls the single-agent `run_sync`. The `Swarm` struct, its
  blackboard and both coordinators are never constructed by any builtin, and
  `ai/swarms.md` described all of it as working behaviour. It now says what the
  builtin does and marks the engine as present in the library but unreachable
  from the shell.
- **An empty tools array was documented as "allow all tools (use with
  caution)".** It is the opposite: `resolve_many` resolves names one at a time,
  so `[]` gives the agent no tools at all. The advice inverted a permission
  boundary.
- **The 512 MB memory limit is Unix-only.** `ai/swarms.md` listed it among the
  sandbox guarantees without qualification. `configure_sandbox` on Windows is a
  documented no-op with a TODO for Job Objects, so Windows gets the 30-second
  timeout and the 10 MB output cap and nothing more. Verified alongside it and
  left standing: the 4,000-character prompt cap, metacharacter rejection, the
  10/minute and 5/minute rate limits, and `sh` gated behind `AETHER_ALLOW_SH`.
- **`mcp_server_start` takes a configuration record**, not the URL string the
  chapter passed it.
- **`AGENT_ALLOW_CMDS` is read once.** Both chapters implied `set_env` could
  change the allowlist mid-session. The configuration is built on first use, so
  it must be exported before `ae` starts.

`tests/book_builtins_are_real.rs` is the ratchet: every underscored identifier
called in an `aethershell` fence must be a name the source quotes, unless the
same example defines it. Scope is deliberately narrow — bare words like `map`
or `person` are as likely to be an example's own variable, and a rule that
fires on those gets switched off rather than obeyed. Verified against the
original defect: restoring `agent_reset(coder)` fails it. One placeholder,
`risky_operation()` in `language/errors.md`, became a real call rather than an
exemption.

`documented_env_is_real` now covers `AGENT_*` as well as `AETHER_*`, on both
sides — the scanner and the source index. `AGENT_ALLOW_CMDS` is real, but until
now nothing outside the `AETHER_` prefix was checked at all, so an invented
`AGENT_*` setting would have passed.

### Fixed — the front pages, and two dead community links

- **`introduction.md` and `getting-started/quick-start.md`** — the first two
  pages of the book — carried the same callable-agent pattern, plus
  `read("app.rs")` (`read` is not a builtin; `cat` is) and an `ai` config key
  `context` that `bi_ai` never reads. The "Features at a Glance" table
  advertised a workflow engine with "MapReduce, Saga, Pipeline, Fan-Out
  patterns" that is not reachable, swarms and coordinators that are not
  constructed, and TUI "image rendering" that does not exist.
- **`tui/multimodal.md` contradicted `tui/guide.md`** in the same book: one
  said the TUI "can display images … directly in the terminal", the other
  already carried the correction that inline rendering is absent. The chapter
  now describes what it does — classify and reference files — and links the
  correction.
- **The Discord invite is invalid and the Twitter handle does not exist.**
  `discord.gg/aethershell` returns `Unknown Invite` (code 10006) from the
  Discord API, and `twitter.com/AetherShell` returns 404. Both were advertised
  in the README, which ships inside the published crate, and in the book. They
  are replaced with the documentation site and the issue tracker.

### Fixed — the documentation book was never built or published

The only open issue on the repository asks whether AetherShell has
documentation. It has 47 chapters of it, under `docs/book`, and there was no
way for anyone to find out.

- **The book could not be built.** `book.toml` still declared `multilingual`,
  `use-hierarchical-outline` and `git-repository-icon = "fa-github"` — three
  keys mdBook has removed — plus an `additional-js` naming a `highlight.js`
  that does not exist, and an `additional-css` path resolved relative to the
  book root while `custom.css` sat in `src/`. Any one of them aborts the
  render. Nothing had run mdBook against this book, so all four accumulated
  unseen.
- **Two chapters pointed at nothing.** `SUMMARY.md` linked `./changelog.md` and
  `./faq.md`; the files are in `appendix/`. `create-missing = true` meant
  mdBook quietly invented empty files for both rather than reporting the
  broken links, so the failure mode was two blank pages. That setting is now
  `false`. `tui/guide.md` existed but was in no chapter list at all.
- **`agent_reset` is not a builtin.** `ai/agents.md` documented it for clearing
  an agent's memory. The shell answers `error[E_UNKNOWN_BUILTIN]: unknown
  builtin: agent_reset`. The section now says what is actually true: memory is
  bound to the agent, so a new agent is how you start over.
- **`ai/tools.md` was listed but never written.** Now written, against the real
  builtins: `tool_list`, `tool_search`, `tool_info`, `tool_schema` and
  `tool_exec`/`tool_execute`, with the catalogue's real numbers (198 tools; 131
  `Safe`, 49 `Caution`, 14 `Dangerous`, 4 `Critical`) and the real refusal
  message for a gated tool. `ai/workflows.md` was listed too and is now
  dropped: `src/workflows.rs` declares fourteen `workflow_*` builtins (plus two
  circuit-breaker ones) in a `workflow_builtins()` whose only caller is its own
  unit test — which asserts the list has at least ten entries and names three of
  them, and so passes whether or not the shell has ever registered one. It has
  not: `workflow_templates()` at the prompt answers `unknown builtin`.
  Documenting them would have been the same mistake as the settings the shell
  never read.
- **The book carried its own changelog, stuck at v0.3.0.** `appendix/changelog.md`
  held a hand-written copy of v0.1.0 to v0.3.0 with v0.3.0 marked "(Current)",
  eleven major versions out of date. It now links `CHANGELOG.md` and the
  releases page instead of duplicating them.
- **Pages served one file.** The workflow uploaded `website/` and nothing else,
  and both "Documentation" links on that page — and every one in the README —
  pointed at `docs/TUI_GUIDE.md`. The book is now built in CI and published at
  <https://nervosys.github.io/AetherShell/book/>, and the workflow runs when
  `docs/book/**` changes.

`tests/book_is_publishable.rs` holds the line: every `SUMMARY` entry must
resolve to a non-empty file, no chapter may be stranded outside the summary,
`create-missing` must stay `false`, the removed mdBook keys must not return,
`additional-css` must name a file that exists, and the README must link the
published book.

### Fixed — `help("file")` promised documentation that does not exist

`bi_help` takes no arguments. The README's `help("file") # Documentation for
file module` described a per-module lookup the shell has never had; passing any
argument prints the same full list. The README now says what it does.

### Changed — debug info for local builds

`.cargo/config.toml` sets `[profile.dev] debug = "line-tables-only"`. The
default writes a full PDB per binary and this workspace builds 141 test
binaries: 387 PDB files holding 40.5 GB of a 49 GB `target/debug`, which
surfaces as `LNK1318`, `LNK1140` or `os error 112` — none of which names disk
space. Backtraces keep their line numbers; use `CARGO_PROFILE_DEV_DEBUG=2` when
you need variable inspection. It lives in `.cargo/config.toml` rather than
`Cargo.toml` so it applies to this repository without shipping in the package.

### Fixed — the Homebrew formula was four releases behind

- `Formula/aethershell.rb` still pinned `v10.0.0`, so
  `brew install nervosys/tap/aethershell` built a version nobody was shipping.
  It had drifted before — `v0.2.0`, a literal `PLACEHOLDER_SHA256`, and
  Apache-2.0 declared for AGPL-3.0-or-later code — and fixing it by hand twice
  is the signal it needs a check rather than a third fix.

  Now pinned to the release tag with its real digest, and `published_contracts`
  gained two ratchets: the url must name the current crate version with a real
  64-character digest, and the declared licence must match `Cargo.toml`. A
  formula tells someone else how to obtain this software, so it is a published
  contract like the openapi spec and gets the same treatment.

### Testing

- **`tests/audit_concurrency.rs`** — two real processes writing one log, with an
  assertion that they genuinely interleave, so the suite cannot pass because
  they happened to run sequentially. Verified red: 33 false alarms without the
  writer-aware tail check.

- **`tests/gate_routes.rs`** — the safety gate must fire whatever syntactic
  route reaches a destructive builtin. `tests/one_door.rs` pins this
  structurally; this pins it behaviourally from the language side: direct call,
  lambda, `map`, captured binding, returned closure and `try`/`catch` are each
  refused, and the file survives. Added when closure capture went in, because
  capture carries a value from one scope into a call that runs later, which is
  the shape a gate bypass would take. It does not bypass — but that is worth a
  test rather than an assumption.

  Includes a **non-vacuity check**, which is the part that matters. The first
  draft of this file used `file_delete` and every case passed while proving
  nothing: `file_delete` is *classified* Destructive in `safety.rs` but is not a
  builtin, so the gate fired on the name before anything resolved it and the
  file survived because the call did not exist. The suite now grants approval
  and requires the file to actually be deleted; if that fails, every refusal
  asserted elsewhere in the file is meaningless.

  Two notes recorded in the file itself: 15 of the 606 classified names are not
  in the dispatcher, which is defensible as defence in depth — an unclassified
  builtin would default to ungated `Pure` — but means "the gate fired" is not
  evidence a builtin exists. And `"path" | rm` is deliberately not among the
  routes: `rm` takes its path positionally and rejects piped input before any
  guard runs, so asserting the gate fires there would assert something untrue
  about a call that cannot happen.

## [11.0.1] - 2026-09-01

A crash fix. Found by adversarial probing rather than by a report or a
failing test, which is the only reason it was found at all — nothing in the
suite had ever handed the shell input it did not choose.


### Fixed — a crash found by adversarial probing

- **Deeply nested input overflowed the stack and aborted the process.** The
  parser is recursive descent, so nesting in the input became nesting on the
  native stack, and nothing bounded it. At roughly 15,000 levels:

  ```text
  thread 'aether-eval' has overflowed its stack
  ```

  A crash, not an error, reachable by anyone who can hand the shell a script —
  which for an agentic shell is the ordinary case. The evaluator had guarded
  call depth at 2,000 since long before; the parser guarded nothing.

  It had three separate sources, and closing it took three passes:

  1. **Bracket nesting** (`(((…`, `[[[…`, `{a:{a:…`) — bounded by a depth
     counter in the expression parser.
  2. **Prefix chains** (`----1`, `!!!!true`, `await await …`) — `parse_unary`
     recurses into itself without passing through that counter, so the first
     guard never fired for them.
  3. **Operator and postfix chains** (`x.f.f.f…`, `1 + 1 + 1…`, `a | f | f…`) —
     these parse *iteratively*, so the parser's own stack was never at risk.
     They built a tree tens of thousands deep that overflowed whatever walked
     it next, which put the failure a long way from its cause.

  The limit is 512 levels: far above anything hand-written or generated in
  practice, far below where the stack gives out. Exceeding it is a parse error,
  which a caller can handle.

### Testing

- **`tests/hostile_input.rs`** — the shell must never abort and must always
  terminate on input it did not choose. Covers nesting depth, prefix and
  operator chains, unbalanced brackets, malformed and adversarial bytes (NUL,
  control characters, RTL override, BOM, 500 KB literals), and pathological
  interpolation — alongside cases asserting that ordinary programs still run,
  because a depth limit that rejected real code would be worse than the crash.
  Verified red: five of its tests report `Crashed("stack overflow")` against an
  unguarded parser.

## [11.0.0] - 2026-09-01

**Major because `Lambda` gained a public field.** `value::Lambda` and
`value::AsyncLambda` now carry `captured`, so any code constructing one with a
struct literal must add it — this repository's own tests and benchmark did.
That is the break a downstream consumer hits, and it is the whole of it: no
function was removed and no behaviour a working script relied on has changed.

Everything else here is a fix. The headline: **closures now capture**, so
currying works instead of silently producing `null`; and **an error handler no
longer reads a stale value** when its variable name is already taken.

### Security — second audit (`docs/security/SECURITY_AUDIT_2026-09-01.md`)

An audit of everything added since 10.0.0: ~790 new lines, three new
cryptographic dependencies and a parser change, some of it written in the same
session that fixed the first audit's findings. Five findings, three fixed.

- **AS-2026-08 — the cipher chain had one non-approved primitive.** AES-256-GCM
  was chosen so the cipher would be FIPS-approved, but key derivation stayed
  Argon2id, which is not. Under `AETHER_FIPS` the key is now derived with
  PBKDF2-HMAC-SHA256 (SP 800-132) at 600,000 iterations and the ciphertext
  carries the tag `AE1F`. Argon2id remains the default: it is the better choice
  against offline cracking, and this is a trade a FIPS deployment makes
  deliberately rather than one imposed on everyone. **Decrypt reads the KDF
  from the envelope, never from the ambient mode**, so turning the mode on or
  off never strands existing ciphertext.
- **AS-2026-09 — the envelope version was not authenticated.** Both versions
  used the same AAD, so a swapped tag was rejected only because the two KDFs
  produce different keys — a consequence, not a property. Each version now has
  its own AAD, making the rejection structural. `AE1`'s AAD is unchanged: 10.0.0
  shipped ciphertext authenticated under exactly those bytes.
- **AS-2026-11 — the audit sink was outside the jail's protection.**
  `is_audit_artifact` knew only about the log, so a sink placed inside the
  workspace was an ordinary writable file. It is now an audit artifact too.

### Added

- **`AETHER_AUDIT_SINK`** mirrors every audit entry to an append-only
  destination — a FIFO drained by a collector, a WORM mount, a path where the
  shell has append but not write. This is the supported mitigation for the one
  residue of AS-2026-02: in-process code holds the chain key and can forge with
  it, and no in-process scheme fixes that. The integrity comes from what is
  behind the path, not from the shell; the sink is byte-identical to the log,
  so it verifies with the same chain check.

### Fixed

- **Two tracked `test-scripts/` files never ran**, failing the same two ways
  the examples did: a multi-statement lambda body, and a binding over a module
  name. `tests/shipped_scripts_parse.rs` now covers that directory too.
- **A shipped example was excluded from the repository by `.gitignore`.** The
  `*_test.ae` pattern, listed under "Temporary files", was unanchored and also
  swallowed `examples/99_comprehensive_test.ae` — a numbered example that
  therefore reached nobody and that no CI run ever executed. Anchored to the
  repository root.

- **AS-2026-13 — lambdas captured nothing, so currying silently failed.**
  `fn(factor) => fn(x) => x * factor` lost `factor` the moment the outer call
  returned. Arithmetic errored on the resulting `Null`; **concatenation did
  not** — `"v: " + f` produced `"v: null"`, a wrong answer with no error.

  A lambda now captures the free variables of its body as a
  `BTreeMap<String, Value>`, which keeps `Serialize`, `Deserialize` and
  `PartialEq` intact where an environment handle would have broken them. Only
  names already bound are captured, so a lambda referring to a binding
  introduced later still resolves dynamically; parameters win over captures;
  and captures are restored after the call.

- **AS-2026-14 — capturing by value would have made `let mut` updates
  invisible.** Caught while testing the fix above, before it shipped: a
  snapshot of a mutable binding changes what existing scripts do. `let mut`
  bindings are not captured. This needed a distinction `Env` did not draw —
  `set_var_unchecked` marks every internal binding mutable, including lambda
  parameters — so user-declared mutability is now tracked separately.

- **AS-2026-15 — `catch e` silently failed to bind when the name was taken.**
  Pre-existing, and unrelated to the above:

  ```text
  let e = "outer"
  try { throw "boom" } catch e { e }     # -> "outer"
  ```

  The binding used `set_var`, which refuses to overwrite an immutable variable,
  and the error was discarded — so the handler read whatever was there before.
  An error handler returning a stale value is worse than one that fails
  loudly. The catch variable is now installed like a lambda parameter and the
  previous binding restored.

- **AS-2026-10 — the string limit now applies to every string operation**, not
  just repetition, so `a + a` can no longer walk past it.

### Known limitations

- **No global allocation bound.** No single string can exceed 8 MiB, but total
  memory is not bounded: an array of many strings, or a deep recursion each
  level of which allocates, is limited only by the call-depth cap. Measured,
  and true before this release too.
- **The browser (wasm) evaluator does not implement closure capture.**
  `src/wasm.rs` is a separate interpreter from the native `eval.rs`, so
  currying behaves there as it did everywhere before this release.
- **An undefined variable interpolates as `null`** (AS-2026-12), consistent
  with the language's existing treatment of undefined names. A misspelled
  *field* or *function* is loud; a misspelled variable is not.

## [10.1.0] - 2026-09-01

Everything here was found by **running what we ship** — the binary, the 57
example scripts, the Homebrew formula — rather than by reading it. Nothing in
this release was reported by a user; all of it had been shipping.

If you are on 10.0.0, upgrade: it contains a crash and writes escape codes into
pipes.

### Fixed — crashes and data loss

- **Printing long non-ASCII text crashed the shell.** `value::pretty::truncate`
  sliced on a *byte* index, so any output long enough to be elided whose 80th
  byte fell inside a multi-byte character aborted the evaluator:
  `end byte index 80 is not a char boundary; it is inside '─'`. Five shipped
  examples did it, because a row of box-drawing characters is the obvious way
  to draw a banner; emoji and CJK did it too. Truncation now counts characters,
  which also makes the limit mean what it says — 80 bytes of CJK is 26 glyphs.
- **`print` truncated output at 80 characters and discarded the rest.** A
  shell's `print` is how a script produces output; it now renders the whole
  value. `debug` and `trace` keep the inline cap, which is what it was for.
- **ANSI escape codes were written into pipes and files.** The colour decision
  consulted `config.colors.enabled` and never asked where the output was going,
  so `ae -c '1 + 2' > out` wrote `\x1b[38;2;180;142;173m3\x1b[39m` and
  `n=$(ae -c '1 + 2')` captured escapes. There is now one policy — `NO_COLOR`,
  then `FORCE_COLOR`/`CLICOLOR_FORCE`, then whether the stream is a terminal,
  then the config — asked separately for stdout and stderr, because the two are
  redirected independently.
- **String interpolation leaked Rust's `Debug`.** `"x: ${r}"` produced
  `x: Record({"a": Int(1)})`. A hole that failed to *parse* was also emitted as
  literal text, so `"${msg.from}"` printed itself and read as success.

### Fixed — the parser could change what a program means

- **A statement could swallow the statement after it.** The parser has two
  loops that slurp space-separated word-call arguments, and only one checked
  that the arguments were on the same line as the callee. So

  ```text
  let hi = 1
  hi
  print("second")
  ```

  parsed as `hi(print, "second")`. The silent form was worse than the loud one:
  with a one-argument call the program simply did something else, and with two
  the error was `expected ')'` pointing at a comma on a line the author had no
  reason to suspect. Word-calls still take arguments on their own line — that
  property is pinned by its own test.

### Added — primitives the language was missing

Each of these was assumed by shipped examples and did not exist.

- **`str(v)` / `to_string(v)`** — there was no way to convert a value to a
  string at all. 41 example sites called a `str()` that has never existed. It
  uses the same renderer as `print` and `${…}`, so the three agree.
- **`"=" * 50`** repeats a string — how every example draws a rule, previously
  `unsupported op`. Capped at 8 MiB so a typo cannot allocate gigabytes.
- **`"hit: " + true`** — `Str + Int` and `Str + Float` were special-cased while
  `Bool`, `Null`, `Array` and `Record` fell through to an error. Any value now
  concatenates with a string, rendering as the user would see it.
- **Async lambdas can see their enclosing scope.** `await` evaluated the body
  in a fresh `Env`, so an async lambda could not call a function defined beside
  it. Plain lambdas bind into the caller's environment, which is why this went
  unnoticed. Parameters are saved and restored, so a parameter name no longer
  clobbers an outer binding.

### Fixed — diagnostics

- **`let user = 1` said "Cannot reassign immutable variable"**, sending you to
  look for an earlier `let` that does not exist. `user` is a module name; the
  message now says so and names the fix.

### Fixed — packaging and licensing

- **The Homebrew formula declared Apache-2.0 for AGPL-3.0-or-later code** —
  permissive versus copyleft, not a typo. `web/package.json` said MIT and an
  example plugin said Apache-2.0. Every declared licence in the repository is
  now AGPL-3.0-or-later.
- **The formula could not have installed anyone**: it pinned `v0.2.0`, carried
  a literal `PLACEHOLDER_SHA256`, ran `ae completions` (no such subcommand —
  that aborts the install), and its test asserted a `--json` flag that does not
  exist. Every remaining assertion in it was executed against the real binary
  before being written down.

### Examples

**All 57 shipped scripts now run; 25 of them failed before.** Seven used
`/* */` block comments the parser has never accepted. Others called APIs that
never existed (`a2a_create_bus`, `nanda_coordinator`, `agents_run_sync`,
`mcp_server_start` used as an object), used `--model` flag syntax the parser
does not have, `filter` for `where`, `http` for `http.json_post`, `group` given
a lambda instead of a property name, or bound variables over module names. The
six that need an AI provider or a running server now detect that and skip with
an explanation instead of failing partway through.

`docs/EXAMPLES_TEST_REPORT.md` recorded 68% of examples broken in October 2025,
at v0.1.0. Ten major versions later nobody had acted on it.

### Testing

Six new files, each verified red before green: `shipped_scripts_parse`,
`unicode_does_not_panic`, `output_is_pipeable`, `statement_boundaries`,
`language_primitives`, and `vscode_extension_agreement`. Four of the five
statement-boundary tests fail without the parser guard, and the one that passes
is "word-calls still work" — the property the fix must not break.

Suite: **2,138 passing across 137 binaries**, 38 VS Code extension tests,
57/57 shipped scripts running, clippy `-D warnings` and fmt clean, green on
Linux, macOS, Windows and wasm32.

## [10.0.0] - 2026-08-31

Closes the two findings 9.0.0 left open on purpose. Both fixes are breaking, and
that is why they waited for a major release rather than being slipped into a
patch: one changes a ciphertext format, the other changes what
`verify_audit` will accept.

### Breaking

- **`crypto.encrypt` produces a new, authenticated format.** Output is now
  `AE1.<salt>.<nonce>.<body>` — AES-256-GCM with an Argon2id-derived key —
  instead of `openssl enc -aes-256-cbc` base64. Ciphertext written by 9.x and
  earlier **is refused by default** with `E_DECRYPT_UNAUTHENTICATED`; set
  `AETHER_CRYPTO_LEGACY_DECRYPT=1` to read it, then re-encrypt.
  **This is deliberately loud.** Accepting the old format silently would hand an
  attacker a downgrade: strip the `AE1.` envelope and its tag from a modern
  ciphertext and the unauthenticated path would take it without complaint. The
  refusal names the variable, so recovering genuinely old data is one step.
- **`verify_audit` refuses a chain whose keying does not match the verifier.** A
  keyed verifier rejects unkeyed entries, and an unkeyed verifier now reports —
  rather than silently accepts — a log whose entries are keyed. Both directions
  are downgrades if allowed to pass.
- **`safety::truthy_env` is now public**, and `safety::verify_audit_with`,
  `safety::audit_key` are new.

### Security

- **AS-2026-04 — `crypto.encrypt` was unauthenticated (CWE-353, CWE-327).**
  CBC is malleable: a modified ciphertext decrypted to modified plaintext and
  the caller could not tell. `openssl enc` cannot do AEAD at all — 3.5.7 answers
  `enc: AEAD ciphers not supported` — so the cipher moved in-process to
  AES-256-GCM, key derived by Argon2id, salt and nonce fresh per message.
  Poly1305 verifies the tag before any plaintext is released, so "it decrypted"
  finally means "it was not modified".

  Three things came free with dropping the subprocess: **AS-2026-03 no longer
  applies to these builtins** (no child, so no password handed to a child
  environment); `crypto.encrypt` **works on Windows**, where the `#[cfg(unix)]`
  openssl path had been returning `E_UNIMPLEMENTED`; and there is no longer an
  external CLI whose version determines whether the shell can encrypt.

- **AS-2026-02 — the audit chain was unkeyed (CWE-345, CWE-732).** A chain
  anyone can recompute is evidence against corruption, not against an author:
  the audited party could truncate the log and rewrite it end to end with a
  fresh, internally consistent chain that verified clean. The chain is now
  HMAC-SHA256 when a key is configured via `AETHER_AUDIT_KEY` (hex or raw,
  32-byte minimum) or `AETHER_AUDIT_KEY_FILE`.

  The key is read **once** and then removed from the process environment, so no
  spawned child inherits it. That is load-bearing rather than tidy: an approved
  `Exec` that could read the key could forge with it, which would leave the
  chain exactly as unkeyed as before. Each entry carries a `key_id` so rotation
  reads as rotation rather than as tampering, and the `mac` label lives inside
  the tagged core so it cannot be relabelled away.

  Keying is **opt-in** — with no key configured the chain stays plain SHA-256.
  A control that needs somewhere to put a key cannot invent that somewhere on
  the operator's behalf, and a key stored next to the log would be theatre.

- **Audit-log tampering is now detected at the next append**, not only at the
  next offline verify. The audit layer compares the file's tail against what
  this process last wrote and, when they diverge, writes a chained
  `audit_chain / tamper-detected` entry recording the hash the chain was
  expected to continue from. A truncation by an approved `Exec` — which no jail
  rule can prevent — therefore leaves a permanent marker instead of a
  clean-looking file. The tail read is bounded to 64 KiB, so it stays O(1) in
  log size.

### Fixed

- **`chacha20` 0.10.1 was yanked** and reached the tree through `rand`, which
  backs `crypto.uuid` and the API-key generator. Moved to 0.10.2.

### Known limitations

- **Nothing in-process can defend the audit log against in-process code.**
  Whatever key this process appends with, it can forge with. Keying defends
  against anyone with only *write* access — an approved `Exec`, an offline edit.
  Defending against the shell itself needs an append-only sink it cannot
  rewrite (remote syslog, a WORM mount), which is a deployment decision.
- **One primitive in the new cipher chain is not FIPS-approved: the Argon2id
  KDF.** AES-256-GCM was chosen over the more usual ChaCha20-Poly1305 precisely
  so the cipher stays approved; the KDF is a considered trade of certification
  for resistance to offline password cracking, unchanged from what `auth.rs`
  already used. `docs/security/FIPS_140-2_COMPLIANCE.md` now states this instead
  of claiming approved algorithms throughout.
- **The `AETHER_FIPS` gate still covers hashes only** — ciphers, key derivation
  and DRBGs are not gated by it.

### Testing

Sixteen new assertions across two files. `tests/crypto_authenticated.rs` covers
the round trip, modification of each envelope field, the wrong password,
salt/nonce freshness, the envelope-strip downgrade, and availability on every
platform. `tests/audit_chain_keyed.rs` builds the exact forgery the finding
described, asserts it *does* pass an unkeyed verifier — so the test cannot
quietly become vacuous — and then that a keyed one refuses it.

## [9.0.0] - 2026-08-27

**Major, and the reason is concrete rather than ceremonial:** 114 `pub fn` were
removed from the library surface, and two documented agent-mode workflows changed
shape. Both are breaking under semver.

### Breaking

- **`approve()` is refused in agent mode.** §7.2 documented the loop as
  `E_NEEDS_APPROVAL` → `approve(token)` → re-run, and that loop is right for a
  human at a REPL and wrong for an agent: the agent holds the token it was just
  refused with. Demonstrated end to end, then closed. Human mode is untouched;
  the out-of-band `AETHER_APPROVE` / `AETHER_APPROVE_ALL` channel, set by
  whoever launches the agent, still works.
  **Consequence: `plan`/`apply` is no longer self-service for an agent.**
- **`mv`, `move_file`, `file_move` and `file_rename` are `Destructive`**, so
  agent mode asks for approval before a move. `fs::rename` removes the source and
  can overwrite the destination; `file_delete_lines` was already `Destructive`
  for altering a file in place, and moving one over another is not the smaller
  act.
- **114 `pub fn` removed** from the library. The builtin bodies are reached
  through the dispatcher; exporting them let callers bypass the gate.
- **112 unreachable builtin implementations deleted** (`vm_*`, `wsl_*`,
  `virsh_*`, `firewall_*`, `hyperv_*`, `container_*` and others). Measured by
  transitive reachability from the dispatch table: none was registered, so none
  was callable. `builtins.rs` is 3,395 lines shorter.
- **`AGENTS.md` no longer advertises six modules that never existed.**
  `vm.list()`, `hyperv.list()`, `virsh.list()`, `wsl.exec()`,
  `firewall.rules()` and `container.ps()` had always returned "unknown builtin";
  the deletion exposed that rather than causing it. The matching transpiler
  shorthands, module sigils and PowerShell cmdlet mappings went with them.

### Security

Eleven issues found by asking one question of the codebase — *where does a value
reach something that parses it?* — and seven more from a framework audit against
CWE, MITRE ATT&CK, NIST FIPS and CMMC 2.0 (`docs/security/SECURITY_AUDIT_2026-08-26.md`).

- **Command execution through `web.open_url` (CWE-78).** The Windows branch ran
  `cmd /C start <url>`; `cmd` splits on `&`, and Rust quotes an argument only
  when it contains a space or a quote. `http://example.com&echo.>marker.txt`
  created the file. A blocklist cannot fix this — `&` is the query-string
  separator, so the dangerous character is legal data — so the shell was removed
  from the path entirely.
- **`web.open_url` was an unapproved `Exec`** classified `Network`: default-allow
  in agent mode, unmetered, and outside the `AETHER_NET_ALLOW` egress allowlist.
- **Three workspace-jail escapes (CWE-22, CWE-732).** `web.download` and
  `wget_download` wrote past the jail; six builtins (`file_edit`, `file_insert`,
  `file_patch`, `file_backup`, `session_export`, `file_move`) wrote outside it
  under a *read* label; `copy_file` overwrote files the jail refused `file_write`.
- **Option injection (CWE-88)** across `git`, `ssh`, `scp`, `tar`, `zip`,
  `curl`, `openssl` and `find`. `reject_option_like` went from 11 call sites to
  43, and the check moved *into* `guard_network` so every network builtin
  inherits it at the one door they already pass through.
- **An agent could approve its own denied call (CWE-863).** See Breaking.
- **The audit log lived inside the jail it audits (CWE-345, CWE-732).** It
  defaults to `<workspace>/.ae/audit.log` — the region the agent may write — and
  the chain is unkeyed, so it could be rewritten end to end with a consistent
  chain that `audit_verify()` accepts. The jail now refuses guarded writes to the
  log and its directory. **Partial:** an approved `Exec` can still reach it and
  the chain remains unkeyed; keying it needs key management and stays open.
- **Secrets on helper-process command lines (CWE-214).** `crypto_password_hash`
  now reads from stdin; `crypto_encrypt`/`crypto_decrypt` pass through the child
  environment. `/proc/<pid>/environ` is owner-readable where `cmdline` is
  world-readable.
- **`crypto_uuid`'s fallback was a clock wearing a v4 label (CWE-330).**
  Measured: three of five groups constant, **zero bits of randomness**. Now uses
  the same CSPRNG as the API-key path.
- **Modulo bias in `crypto_random_string` (CWE-1241).** Measured at 0.0045 bits
  per character; now rejection sampling on both platforms.
- **`crypto_decrypt` reported a decryption failure as `E_UNIMPLEMENTED`** —
  telling a caller with a tampered ciphertext that openssl was not installed. For
  an unauthenticated cipher that rejection is the only detection channel there
  is. Now `E_DECRYPT_FAILED`.
- **`sql_column_type`** validates the `CREATE TABLE` column type, closing the
  last unvalidated SQL interpolation.

### Fixed

- **`mkdir` never created anything.** `mkdir`, `mkdirp` and `file_mkdir` were
  registered at dispatch index 532 while the reserved placeholder run began
  there — an off-by-one in a comment — so all three returned `Ok(Null)` and did
  nothing, while `bi_file_mkdir` sat written and referenced by nothing.
- **Five stale `SELF_GUARDED` entries** exempted names from `guard_dispatch`
  whose implementations no longer guarded anything.
- **A dangling module alias**: `input.number` resolved to nothing.

### Known limitations

- **`crypto_encrypt` provides confidentiality, not integrity** (AS-2026-04).
  OpenSSL 3.5.7 answers `enc: AEAD ciphers not supported`, so `-aes-256-gcm` is
  unreachable through this path. The alternatives are a dependency that breaks
  every existing ciphertext, or a hand-rolled construction. Deferred to a release
  where the format break can be versioned and announced.
- **The FIPS gate covers hashes only.** Verified complete for those — three of
  three call sites, three independent ways — but ciphers, key derivation and
  DRBGs are not gated by `AETHER_FIPS`, and the cryptography is delegated to the
  host, so any validated-module boundary is the operating system's.

### Testing

Fourteen new mechanical ratchets, each verified red before green, covering:
shell-spawn gating, option injection, write-evidence versus effect label, the
workspace jail for downloads and for `WriteLocal`, placeholder dispatch rows,
approval self-grant, and audit-log tampering. Suite: **2,003 passing across 125
binaries**, clippy `-D warnings` and fmt clean, green on Linux, macOS and Windows.


---

### Also in this release — the earlier remediation-tracker audit

This work was sitting under `[Unreleased]` and ships as part of 9.0.0.

Security work found by auditing `docs/security/REMEDIATION_TRACKER.md` against
the code. Two of its fifteen itemised findings were live vulnerabilities; both
were marked NOT STARTED and both were accurate.

### Security

- **SQL injection in the kv store (CWE-89).** `db_kv_*` built SQL with
  `format!`, so a key could rewrite the query. Demonstrated, not theorised:
  `db_kv_get(db, "x' OR '1'='1")` returned another key's value, and
  `db_kv_delete(db, "z'; DELETE FROM kv; --")` emptied the table. `db_kv_set`
  already escaped the *value* and not the key — the technique was known and
  applied to one of two interpolations on the same line. Fixed with
  `safety::sql_literal` / `sql_identifier`.
- **The MCP HTTP server executed builtins unauthenticated.** `POST
  /mcp/v1/tools/:name/execute` runs any builtin, there was no authentication,
  and CORS defaulted on. A page on another origin read `C:/Windows/win.ini` off
  the disk. Now requires a bearer token on every route except `/health`,
  minted and printed at startup; CORS defaults off.
- **RUSTSEC-2026-0258** (`h2`, unbounded empty DATA frames) was present twice
  and `cargo audit || true` had never reported it. Removed by updating one
  instance and dropping `jsonschema`'s default `resolve-http` feature, which
  pulled the other through reqwest 0.11 → hyper 0.14.
- **A non-loopback bind is now refused** unless `AETHER_ALLOW_REMOTE_BIND=true`.
- **Security headers** on both HTTP servers: `nosniff`, `X-Frame-Options`, CSP,
  `no-referrer`, `no-store`.

### Changed

- `cargo audit`, `cargo deny` and the SBOM job can now fail the build. Every
  scanner previously ended in `|| true`, so Security Audit reported green
  regardless of findings. gitleaks stays non-blocking pending a git-history
  triage, which the workflow now states.
- `effect_of` distinguishes a classification from its fall-through, surfaced as
  `effect_declared`. No builtin was explicitly classified `Pure`, so
  `x-effect: pure` meant "unclassified" for all of them.
- 55 builtins that read local state (`ls`, `cat`, `grep`, `env_*`, …) are now
  `ReadLocal` rather than `Pure`. No policy change — `Pure` and `ReadLocal` are
  both Allow — but `Pure` also claims referential transparency, which invited
  an agent to cache or reorder them.

### Fixed

- A test race in the global handle store that had CI red on Windows.
- `ls.path`'s field example was only true on the machine that captured it.
- Test isolation: 3 unguarded env-mutating tests and 2 needless `unsafe` blocks.

### Breaking

- The MCP HTTP server requires `Authorization: Bearer <token>`.
- Binding a non-loopback address requires `AETHER_ALLOW_REMOTE_BIND`.
- `db_sqlite_delete` / `_count` / `_create_table` / `_to_json` reject table
  names that are not plain identifiers.
- The `jsonschema` feature no longer resolves remote `$ref` URLs.

## [8.0.0] - 2026-08-17

A major version because `ai_convert_model` changes shape. See **Changed**.

### Added

- **The NERVOSYS stack: AI traffic routes through IronGate.** AetherShell is a
  frontend; deciding which model serves a request is a routing problem, and
  routing belongs to [IronGate](https://github.com/nervosys/IronGate), which
  reaches [IronWorks](https://github.com/nervosys/IronWorks) for local inference
  and IronWorks in turn reaches [IronVault](https://crates.io/crates/ironvault)
  for weights.

  - `ProviderType::IronGate` / `Provider::IronGate` — scheme `irongate:` with
    aliases `gate:` and `iron:`. OpenAI-compatible on the wire, so it needs no
    adapter of its own; what differs is where the request goes next. Tools,
    streaming and vision are all declared, because the gateway's IR carries them
    and it negotiates capability per target.
  - `AETHER_AI=irongate` routes every AI call through it. With `AETHER_AI`
    **unset**, a reachable gateway is now used rather than erroring — the probe
    carries a 2s timeout, so the existing "no provider configured" message still
    arrives promptly when nothing is running.
  - Backend detection probes the gateway **first**, so `AETHER_AI=auto` prefers
    routing over a direct connection that would bypass every budget ceiling and
    circuit breaker the operator configured.
  - `ai_gateway()` reports the gateway's state and the per-target circuit
    detail behind it. An unreachable gateway returns `reachable: false` rather
    than an error, so `ai_gateway().reachable` is writable.
  - `IRONGATE_URL`, `IRONGATE_MODEL` (default `auto`), `IRONGATE_API_KEY`.
    The default endpoint matches IronGate's own `[server] port = 7700`.

- **IronVault is the model store and conversion module.** `vault_models()`,
  `vault_conversions()`, `vault_convert({name, to_format, quantization?,
  output?, validate?})`, reached through the `iv` CLI and `IRONVAULT_BIN`.

  The CLI rather than the crate on purpose: `ironvault` declares
  `rust-version = "1.89"` against AetherShell's 1.88, so taking it as a
  dependency would raise this project's MSRV for every user, including those who
  never touch a model file — and pull a second AES/Argon2/tokio stack into a
  shell binary.

- `ai_local_generate` falls back to the gateway when no in-process backend can
  serve the handle. In this stack "local inference" *means* IronWorks behind
  IronGate, and the default build compiles neither `candle` nor `onnx`, so that
  call previously could only fail. Both paths now report a `backend` field
  naming the engine that actually answered.

### Changed

- **BREAKING: `ai_convert_model` no longer accepts the source/target path form,
  and no longer reports success for a file copy.** Every branch of the in-tree
  converter was `fs::copy` — its own source said *"Simulate conversion by
  copying file"* — after which it returned `success: true` with a checksum of
  the unchanged bytes. A conversion that silently does nothing is worse than one
  that fails.

  Conversion is IronVault's job in this stack, and IronVault addresses models by
  vault name rather than file path, so there is no correct automatic
  translation. The path form now returns an error naming the replacement:
  `iv add <path> --name <name>` then `ai_convert_model({name, to_format})`.
  `ai_supported_conversions()` likewise answers from the vault instead of an
  in-tree table that advertised capability the shell did not have.

- `ai_gateway`, `vault_models`, `vault_conversions`, `vault_convert` and
  `ai_convert_model` are classified in `effect_of` — `Network` for the gateway
  probe, `Exec` for everything that spawns `iv`. The effect ratchet reads
  `Command::new` in `model_plane::vault_run` and would refuse anything weaker;
  `tests/model_plane.rs` asserts the classifications directly rather than trusting
  the lint's silence.

### Fixed

- **Python SDK 1.5.1** — the version on PyPI does not work. `aethershell 1.5.0`
  was uploaded on 2026-08-10, two days before the flag fix, so the published
  wheel still calls `ae -e <code> --json`; `clap` rejects `-e`, and every
  `eval()` raises `RuntimeError`. Verified by downloading the wheel and reading
  `__init__.py` rather than reasoning from dates.

  PyPI does not permit replacing a release, so this bumps the SDK to **1.5.1**
  (`pyproject.toml`, `__version__`, docs). The rebuilt wheel was checked the
  same way before committing: `-c`/`--deterministic` present, `-e`/`--json`
  absent.

  Publishing it still requires either a **trusted publisher** on PyPI (the
  workflow already uses `pypa/gh-action-pypi-publish` with `id-token: write`, so
  no API token is needed — configure the publisher and the job stops failing) or
  a manual `twine upload`.

  Related: the package name is at least safely owned — `aethershell` on PyPI is
  registered to `contact@nervosys.ai` and points at this repository. The
  supply-chain exposure noted in `release.yml` (docs advertising an unclaimed
  name that anyone could take, with `pip` executing package code at install
  time) is therefore closed.

## [7.4.0] - 2026-08-12

### Added

- **`sort_by` accepts a field name**, not only a lambda:
  `sort_by("size")` alongside `sort_by(fn(r) => r.size)`, and it composes with
  the existing direction argument — `sort_by("size", "desc")`.

  Added after hitting it while using the shell as an agent. The string form is
  what most callers reach for first, and it previously failed *twice over*: the
  argument was silently ignored (only `"desc"`/`"descending"` were recognised)
  and the call then errored for want of a lambda. One wasted round-trip on a
  call that reads as obviously correct.

  The no-key error now names both accepted forms, so an agent that guesses wrong
  learns the call from the failure rather than from a second attempt.

  Limitation, asserted in a test rather than left implicit: `"desc"` and
  `"descending"` stay reserved as the direction, so a field genuinely named
  `desc` must use the lambda form. A missing field yields a `Null` key and sorts
  rather than failing the call, so a partial dataset still works.

## [7.3.3] - 2026-08-12

### Fixed

- **The audit trail redacted the token each entry was about.** 7.3.2 fixed the
  value layer but `redact_json`, which scrubs audit entries at write time, had
  the identical two flaws — so every record of a guarded call read
  `needs_approval` with `detail.token = "[REDACTED]"`. An audit entry that
  cannot say *which* approval was requested cannot be correlated with the grant
  that followed it, which is most of what an audit log is for.

  The rule now lives in one place, `safety::is_capability_token`, used by both
  redaction layers, with a test asserting they agree. Fixing one layer and not
  its sibling is how they came to disagree in the first place — the same mistake
  as classifying by name instead of by evidence.

  Audit entries now record `{"token":"apv_…"}`; a genuine `ghp_…` under
  `auth_token`, a `password`, and secrets nested in containers are still blanked,
  and numeric counts pass through.

  Fourth defect found by driving the shell as an agent, and a direct consequence
  of the previous fix being incomplete.

## [7.3.2] - 2026-08-11

### Fixed

- **Redaction was destroying the agent's own data.** `safety::is_secret_name`
  matches substrings, so `TOKEN` also matched `full_tokens`, `compact_tokens`,
  `page_tokens` and `tokens_in`/`tokens_out`. Those hold **counts**, and blanking
  them made the token-economy surface — the thing this project measures itself
  by — report `[REDACTED]` to agents instead of numbers. `digest()` was the
  clearest casualty: its whole purpose is comparing full against compact size.

- **`plan()` returned an unusable token.** The same rule blanked its `token`
  field while `hint` printed the real `apl_…` alongside it. A caller reading the
  machine-readable field could not complete the documented plan/apply flow at
  all; it would have had to scrape the hint string.

  The fix is narrow and in two parts: a secret is a **string**, so a number under
  a secret-sounding name is left alone; and `apv_`/`apl_` values are
  **capabilities the agent is required to echo back**, not credentials to hide.
  Everything else is unchanged — `tests/redaction_scope.rs` pins both
  directions, including a genuine `ghp_…` credential under a field literally
  named `auth_token`, and a secret nested inside a container.

  Found by driving the shell as an agent. Three defects have now come from using
  it (this, the approval-token collision in 7.3.1, and a journal entry recorded
  for a call the jail refused) against none from reading it.

## [7.3.1] - 2026-08-11

### Security

- **An approval token authorised calls it was never issued for.** The token is a
  hash of the approval descriptor, so anything distinguishing two calls must be
  inside it — but `guard_dispatch` (added in 7.1.0) passed only the **string**
  arguments. `git_clean`'s only argument is a bool, so `git_clean(true)` — a dry
  run that prints what it *would* remove — and `git_clean(false)`, which deletes
  untracked files, hashed to the same `apv_…`. Approving the harmless preview
  silently granted the destructive call, the exact inverse of what
  content-binding exists to guarantee.

  Every argument is now bound into the descriptor, typed
  (`blast_radius.args = [{"Bool": true}]`). Two regression tests cover it: that
  the two tokens differ, and that a granted token admits only the call it was
  issued for.

  Affects 7.1.0–7.3.0 in agent mode. Any builtin whose behaviour is selected by
  a non-string argument was exposed; `git_clean` is the clearest case. Human
  mode was never gated and is unaffected.

  Found by driving the shell as an agent rather than by review — the tokens were
  visibly identical in two consecutive error responses.

### Notes

- The fix initially appeared **not to work**: the rebuilt binary reported the
  same colliding tokens. The binary was 27 minutes older than the sources —
  cargo's fingerprint cache had been corrupted by earlier disk-exhaustion
  failures and was silently skipping the rebuild, so every check was testing
  stale code. Clearing `target/debug/.fingerprint/aethershell-*` restored
  correct builds. Worth knowing on this machine: a test result is only evidence
  about the binary that actually ran.

## [7.3.0] - 2026-08-11

Three limits that had been *stated* rather than fixed. Two were closed by
measurement; the third was closed by using the shell as an agent, which found
two defects nothing else had.

### Changed

- **The ratchet reads the whole crate, not one file.** It scanned
  `builtins.rs` alone, so any effect reached through `security`, `os_tools` or
  another module was invisible — `Command::new` appears in six other modules,
  `fs::write` in ten. "No evidence" meant "no evidence in one file". It now
  reads every `.rs` under `src/` at test time, so a module added later is
  covered without anyone remembering. Six genuinely-acting builtins were found
  and classified: `fs_link`, `fs_symlink`, `git_ignore`, `perm_set`
  (`WriteLocal`), `platform_has_network`, `platform_machine_id` (`ReadLocal`).
  Coverage: **607 classified, 53% falling through** (was 601 / 54%).

- **Follow depth was measured rather than argued.** Depths 2, 3, 4 and 5 all
  report zero outstanding violations, so nothing hides deeper here. Raised to 4
  regardless, since it costs a fraction of a second and removes the question.

- **`x-returns` can now carry an example value** where a field's *format* is
  surprising (`shapes::FIELD_EXAMPLES`), and every example is verified against
  what the builtin really returns.

### Fixed

- **A refused call no longer enters the journal.** A `file_write` blocked by the
  workspace jail still recorded an entry, so `undo()` answered
  `complete: false, skipped: 1` about an operation that never ran — an honest
  report of a fictional entry, which would lead an agent to believe something
  was left unreversed.

- **Four defects in the lint itself**, each caught by disbelieving a result
  rather than by review:

  - a bare `symlink` marker matched the *field* `allow_symlinks`, reporting
    `ls`, `cat`, `head`, `tail` and `read_text` as creating symbolic links;
  - reading the whole tree multiplied name collisions, so delegation through a
    name defined in several modules resolved arbitrarily — ambiguous names are
    now refused rather than guessed, and the count (265) is reported;
  - string literals were scanned as code, so `bi_help`'s help text containing
    `| join("-")` made `help` "open a datagram socket";
  - **and the dangerous one:** blanking strings without handling character
    literals meant `'"'` — 20 occurrences in `builtins.rs` — opened a phantom
    string and blanked the code after it. The violation count fell from 6 to 3
    and *looked like progress*; `platform_machine_id` had simply become
    invisible. Four canary tests now assert that known-acting code is still
    seen, because a blind lint reports zero and zero is indistinguishable from
    success.

### Notes

Two findings came from driving the real binary as an agent, and neither was
reachable by reading code:

- **A type is not a format.** `ls` declares `ext:str`, which is true, and
  `where(fn(f) => f.ext == "rs")` still matched nothing — the value is `".rs"`.
  The filter did not error; it returned an empty set, the worst failure mode
  available, because an empty result is a plausible answer.
- Argument shapes are still discovered by failing: `sort_by("size")` returns an
  actionable error, but only after the call is made.

## [7.2.0] - 2026-08-11

Debt paydown, measured at each step. Three of the items below were found by
checking a result rather than believing it, and two were live bugs rather than
debt.

### Fixed

- **The Python SDK could never have worked.** `AetherRuntime.eval` ran
  `ae -e <code> --json`. The binary takes `-c/--command` and has no `--json`, so
  clap rejected the call and *every* `eval()` raised `RuntimeError`. Fixed to
  `ae -c <code> --deterministic`, verified by calling it, and
  `tests/sdk_contract.rs` now runs the binary with the flags read out of the SDK
  source so they cannot drift apart again.

- **Handles and the journal were useless to SDK users.** Both were
  process-lifetime, but the SDK spawns a fresh process per call — so every
  handle id was unresolvable and every journal empty. `undo` would have reported
  `0 restored, complete: true`: succeeding at nothing, the exact failure the
  journal exists to prevent. Both are now persisted to a session directory keyed
  by workspace (`AETHER_SESSION`/`AETHER_SESSION_DIR` override), and proven by
  **spawning the real `ae` binary twice** — clearing a static would only show
  that the loader runs.

  Persistence uses serde's representation, not `Value::to_json`: the latter
  renders `Uri` as a bare string, so a round-trip returns `Str` and the
  losslessness guarantee would have been quietly broken. Checked before being
  relied on.

### Changed

- **The effect ratchet now follows delegation.** It read only `bi_*` bodies, so
  a builtin handing its side effect to `cloud_run_cmd`, `kubectl_text` or
  another builtin was invisible — 209 of them. Following calls two levels deep
  and reading helper bodies found **116 registered builtins acting while
  classified `Pure`**; all are now classified from what their helper actually
  runs, with a builtin that delegates to another inheriting its effect.

  | | before | after |
  |---|---|---|
  | classified | 485 | **601** |
  | fall through to `Pure` | 63% | **54%** |

  Getting a trustworthy number took three fixes to the lint itself. It indexed a
  `&str` by byte offset (the source contains `…`, so it panicked); it read
  `BTreeMap::new()` as a call to a free function named `new`; and — the one that
  mattered — it matched `fn` inside a **comment**, binding `// fn json_to_value(…)`
  to the next real function's body, `bi_rm`, which deletes files. That phantom
  replaced the genuine entry and the lint reported `json_parse` as a builtin
  that deletes files. Trusting the first count would have "fixed" it by
  misclassifying a pure function. `the_lint_does_not_read_comments_as_code`
  pins that case.

- **`WriteLocal` and `Network` are now audited centrally** in agent mode. They
  still decide `Allow` — but "allowed" and "unobserved" are different things,
  and these previously left no trace at all.

- **The workspace jail applies at the dispatcher**, for arguments that
  demonstrably *are* paths: a string that resolves to an existing file or
  directory is a path by observation, not by guesswork. Everything else
  (subcommands, container names, SQL) is left to the hand-written call sites.
  The asymmetry is deliberate — a missed jail check is a gap those call sites
  still cover, while a false one refuses a legitimate call with no workaround.

### Added

- **Polymorphic return shapes.** Refusing to describe `first`, `last`, `take`,
  `unique`, `reverse` and `values` left the most-used combinators undocumented,
  yet their shapes are not unknown — only *relative*. The notation gained one
  variable, `T`, bound to the first argument's element type, taking proven
  shapes from **11 to 17**.

  The proof inverts for these: a fixed shape is proven by probes **agreeing**, a
  relative one by probes **disagreeing exactly as `T` predicts**, with a test
  that `first` really does vary with its input — otherwise `T` would be a
  misleading way to say something fixed.

  `sum` stays undeclared. It yields `int` for integers and `float` for floats,
  which is a promotion rule rather than the element type, and `T` would be a
  plausible-looking lie. A test pins that decision so the table is not
  "completed" later.

## [7.1.0] - 2026-08-11

### Changed

- **Effect classification is now enforced, not merely advertised.**
  `effect_of` describes an operation's danger; `guard` is what acts on it. Only
  **51 of ~1,300** builtins ever reached `guard`, so 6.0.0's classification of
  306 process-spawning builtins improved what the ontology *told* an agent
  without changing what the shell would *let* one do. An agent that read
  `x-effect` and respected it was protected; an agent that simply called the
  tool was not — `git_clean` was labelled `Destructive` and still deleted
  untracked files unguarded.

  `safety::guard_dispatch` closes that at `call_with_input_inner`, the one place
  every builtin already passes through. Roughly 90 builtins in the `Process`,
  `Destructive` and `Exec` classes now meet the policy table, with an approval
  path and an audit entry where they previously had neither.

  Scoped deliberately:

  - `WriteLocal` and `Network` are **excluded**. Both already decide `Allow` in
    agent mode, so central guarding would double their audit and governor
    accounting without changing one decision.
  - The workspace **jail stays at hand-written call sites**, which know their
    real targets. A central point cannot tell which string arguments are paths,
    and jailing anything path-shaped would reject legitimate calls.
  - Read-only builtins stay ungated, so exploration is untaxed.

- **`safety::SELF_GUARDED`** lists the 52 builtins that enforce policy
  themselves, skipped centrally so one action is not admitted twice.
  `tests/guard_enforcement.rs` derives the same set from the source and fails on
  any disagreement, so the list cannot drift as call sites change.

### Fixed

- **`apply` was double-gated by the first cut of central enforcement.** It gates
  a whole plan on one plan-derived token, jails each operation and snapshots
  into a transaction — but it never calls `guard`, so a detector looking only
  for `guard(` omitted it. Guarding it generically as `Exec` demanded a second,
  unrelated token and returned before the code that hands back the plan token,
  turning a working approval flow into a dead end. Caught by
  `tests/transactions.rs`; the detector now reads for approval checks as well as
  guards, and a regression test names the case.

- **CI formatting.** 7.0.0's new modules and tests were never run through
  `cargo fmt`, so `cargo fmt --check` failed on master while the suite and
  clippy were green. The published crate was unaffected — the failure was
  cosmetic — but `fmt` is now part of the local gate rather than something only
  CI checked.

## [7.0.0] - 2026-08-11

Three features aimed at the agent *loop* rather than the individual call: how
many round-trips it takes to get a call right, how much of the world lands in
the context window, and what happens when a step was wrong.

### Added

- **Return shapes (`x-returns`).** The agent surface described inputs
  (`json_schema`) and danger (`x-effect`) but never the *result*, so an agent
  meeting one of ~1,300 unfamiliar builtins had to run it to learn the shape and
  only then write the pipeline it wanted. `crate::shapes` advertises the shape
  ahead of the call — `ls` is `array<record{ext:str,is_dir:bool,…}>` — which is
  enough to compose `ls(".") | where(fn(f) => f.is_dir) | select("name")`
  correctly the first time.

  **Only proven shapes are advertised.** Declaring a shape from a name is the
  reasoning that produced 28 misclassified effects and then 306 more, so
  `tests/return_shapes.rs` requires every entry to be reproduced by *calling*
  the builtin, and to agree across probes with **different input types**. That
  second rule immediately caught five builtins a single probe would have
  mis-declared: `values`, `first`, `unique` and `reverse` return whatever they
  were handed, and `sum` yields `int` or `float` depending on its operands. They
  are polymorphic, this notation cannot yet say so, and silence is the honest
  answer — so 11 shapes ship rather than 16 wrong ones.

- **Result handles.** A result too large to be worth sending whole now stays
  server-side and is rendered as a reference — id, shape, item count, a short
  preview, and the call that narrows it down. The agent composes against it
  (`handle("h1") | where(…) | take(5)`) and only the small answer crosses back.

  | rows | whole | handle | |
  |---|---|---|---|
  | 400 | 2,975 tokens | 113 tokens | **26.3×** |
  | 2,000 | 15,709 tokens | 115 tokens | **136.6×** |

  Real cl100k. The ratio grows with the payload because the summary is
  constant-size, so no single multiplier describes it — which is why both rows
  are given. This is the lever AECON's ~2× could not reach: the cheapest tokens
  are the ones never sent.

  The value is kept **whole**, not summarised: `handle(id)` returns exactly what
  was computed. The preview states how many items it omits, because a preview
  that reads like a complete result is how silent truncation misleads. An opaque
  blob (a single huge string) is deliberately *not* handled — a reference the
  agent cannot narrow down trades a cost for a dead end. `AETHER_HANDLE_BYTES=0`
  restores whole results.

- **Reversible sessions.** Before any `WriteLocal` or `Destructive` call, the
  prior contents of the files it might touch are captured; `undo(n)` puts them
  back, `journal()` shows what was recorded. This is hooked at
  `call_with_input_inner` — the one boundary every builtin passes through, and
  already the single boundary for error structuring — so it covers every builtin
  instead of ~300 call sites.

  The motivation is that 6.0.0 bought safety with friction: 166 builtins now
  stop and ask, and every approval is a round-trip. Reversibility buys the same
  safety more cheaply, because a write that can be undone need not be prevented.

  **`undo` reports what it could not reverse.** A network call cannot be
  unsent, a directory tree is not captured, an oversized file is not held — each
  is journalled as an explicit irreversible entry, and `complete: false` comes
  back whenever any remain. A partial undo that claims success is worse than no
  undo, since it converts a recoverable situation into one where the user
  believes they have already recovered; `tests/reversible_sessions.rs` asserts
  exactly that case.

  Agent-mode only, so the human REPL is unchanged and pays no I/O.
  `AETHER_JOURNAL=on|off` forces it either way.

### Changed

- **BREAKING (agent surface):** a result over `AETHER_HANDLE_BYTES` (default
  2,048 rendered bytes) is delivered as a handle rather than as data. Consumers
  that expected whole results for large payloads must either follow the handle
  or set `AETHER_HANDLE_BYTES=0`. Human output is untouched.

## [6.0.0] - 2026-08-11

### Changed

- **All 306 ratchet-listed builtins are now classified, and the baseline is empty.**
  5.2.0 shipped `tests/effect_ratchet.rs`, which found 306 builtins that construct an
  OS process while `safety::effect_of` returned `Pure` — meaning `guard()` and the
  agent-facing ontology both described them as side-effect-free. Each has been given
  an effect derived from the argv its body actually builds:

  | Effect | n | Examples |
  |---|---|---|
  | `ReadLocal` | 140 | `git_status`, `pkg_list`, `platform_cpu`, `hw_gpu` |
  | `Exec` | 73 | `pytest_run`, `cargo_run`, `db_sqlite_query`, `eslint_check` |
  | `WriteLocal` | 52 | `tar_extract`, `black_format`, `ssh_keygen`, `diag_fix` |
  | `Network` | 24 | `git_push`, `gh_pr`, `skopeo_copy`, `rustup_update` |
  | `Process` | 13 | `svc_start`, `gui_close_window`, `tmux_attach` |
  | `Destructive` | 4 | `git_clean`, `git_reset`, `session_rollback`, `dd_copy` |

  The tiering rule is recorded in `effect_of` so it can be applied to the next builtin:
  irrecoverable deletion → `Destructive`; contacts another host → `Network`; process or
  window lifecycle → `Process`; writes a file or rewrites source → `WriteLocal`; only
  reads *and* executes no caller- or project-supplied code → `ReadLocal`; anything else
  → `Exec`. An unclear argv resolves to `Exec`, never `ReadLocal` — a false "dangerous"
  costs one approval, a false "safe" cannot be taken back.

  **This changes agent-mode behaviour.** 166 builtins that previously ran unguarded now
  meet the policy table, and those classified `Destructive` require an approval token;
  `AETHER_POLICY=permissive` restores the old behaviour wholesale. The remaining 140 are
  the read-only wrappers an agent leans on while exploring, and keep zero friction.

  Three of these were not merely unclassified but actively dangerous while advertised as
  pure: `git_clean -d` deletes untracked files, `session_rollback` is `git reset --hard`,
  and `dd_copy` can overwrite a block device. Two more are worth knowing about:
  `db_sqlite_query` passes caller SQL to the `sqlite3` binary, so `query` is a name and
  not a constraint; and `diag_fix`/`refactor_remove_unused` run `cargo fix --allow-dirty`,
  rewriting sources with the safety net explicitly off.

  A first pass at sizing this split by *name* suffix predicted 72 read-only rather than
  the actual 140 — wrong by half, in the direction that would have produced needless
  approval prompts. The tiers above come from the argv, which is the same discipline
  the ratchet itself enforces.

## [5.2.0] - 2026-08-11

### Added

- **`@nest`: record-valued columns are expanded into dotted columns.** A nested cell
  previously serialized as whole JSON on *every row*, keys included — the exact cost
  this format exists to remove, and unreachable by any column pass. Expanding it into
  `meta.region` / `artifact.path` subjects each leaf to `@const`, `@dict`,
  `@prefix`/`@suffix` and `@same` like any other column. On a 30-row API-shaped
  payload: **583 → 258 tokens, a 2.26× reduction** (real cl100k, identical rows).

  Only records are expanded, so an array cell stays a single atom and the cell grammar
  stays flat. A top-level key is expanded only when nothing already occupies its dotted
  namespace, so a literal `user.id` column is never shadowed by a nested `user` record;
  an empty record is left whole rather than flattened to nothing; and a field that is
  not a record in every row is left alone.

- **Typed argument failures.** `bad_arg` already computed *expected* and *got* and then
  buried them in an English message. They are now structured fields on `SafetyError`,
  emitted as a pair in the error JSON and surfaced by `diagnose`, so an agent repairing
  an `E_BAD_ARG` does not have to parse prose to learn what shape was wanted.

- **An idempotency signal, distinct from error retryability.** `ErrorCode::retryable`
  describes the *error*; `Effect::retry_safe_by_class` and `safety::idempotent` describe
  the *operation*. A network timeout is a retryable error, but re-issuing the request
  behind it may duplicate the effect — an agent needs both facts, and conflating them is
  how duplicate side effects happen. Conservative by construction: only `Pure` and
  `ReadLocal` are safe by class and everything else opts in explicitly, because a false
  "unsafe" costs one stalled retry while a false "safe" cannot be taken back.

- **An effect ratchet that reads bodies, not names** (`tests/effect_ratchet.rs`).
  `effect_coverage.rs` audits names that advertise a side effect; this one flags any
  builtin whose body constructs a process, writes a file or opens a socket while
  `effect_of` returns `Pure`. Name-based reasoning is what produced the original
  misclassifications, so this checks the evidence instead.

  It found **306** such builtins — overwhelmingly wrappers around external developer
  tooling (`pytest_run`, `eslint_check`, `go_build`, `skopeo_copy`), an order of
  magnitude beyond the 28 the name lint caught. Reclassifying them is a behavioural
  change in agent mode and belongs to the maintainer, so this lands as a ratchet rather
  than a gate: `tests/effect_ratchet_baseline.txt` records the debt and **may only
  shrink**. A newly added builtin that acts while `Pure` fails the build — the mechanism
  that was missing while those 306 accumulated — and a baseline entry that no longer
  violates must be deleted rather than left to rot.

- **Cross-turn prefix stability is now measured** (`tests/prefix_stability.rs`), along
  with a test that a reordered result forfeits most of its prefix — which is what the
  determinism guarantee actually buys.

### Fixed

- **Non-scalar cells never round-tripped.** A `Record` or `Array` cell rendered as bare
  JSON via `aecon_atom`'s catch-all, and the decoder could not distinguish that from a
  string that merely looks like JSON — so every composite cell decoded back as `Str`.
  The format documented itself as "lossless and reversible"; for nested data it was not.
  A string opening with `{` or `[` is now JSON-quoted, reserving a bare `{`/`[` cell for
  a genuine composite, exactly as `""` already reserves the empty string. Found by
  round-trip tests written for `@nest`.

- **The prompt-cache figure was modelled, not measured.** The README gave *a 90%-stable
  prefix is ~4.1× cheaper over 20 turns*. Measured on a realistic 20-turn poll — 20 rows,
  one field changing per turn — it is **61.8% mean stability and 2.12× cheaper** at a 0.1
  cached-token rate. The gap is structural rather than a rounding error: a row that
  changes early truncates every byte after it, so stability depends on *where* the change
  lands, not only how much changed. Corrected to the measured figure with its method.

## [5.1.0] - 2026-08-10

### Added

- **Two more AECON factoring passes.** `@suffix` factors a shared *trailing* run
  out of a string column — file extensions, domain tails, id suffixes — the
  mirror of the existing `@prefix`, and composes with it on the same column. The
  suffix is searched in the residue the prefix leaves, so the two runs can never
  overlap, even on a value they consume entirely. `@same` elides a cell identical
  to the one above it in run-structured columns: an empty cell is an unambiguous
  sentinel because a genuinely empty string renders JSON-quoted (`""`).
  `@const` already covered columns constant across *every* row; `@same` covers the
  runs `@const` cannot see, which is the shape of any sorted or grouped result.

  Both are reversed by `aecon_decode` — the load-bearing test for each is the
  exact round-trip.

- **Per-column encoding is now chosen by exact character cost.** Raw, `@dict` and
  `@prefix`/`@suffix` are each costed as they will actually be emitted — including
  the `@same` elision that runs afterwards — and the cheapest wins. This replaces
  two hand-tuned dictionary heuristics (`d <= rows/2`, `avg_len >= 3`) that
  mis-fired in both directions: they declined dictionaries that would have paid on
  long values, and, because `@dict` was tried first and won by default, they took
  columns that prefix/suffix factoring would have compressed much harder. A table
  with nothing to factor now emits no metadata lines at all.

  Together the three changes take a 30-row grouped listing with paths from **265
  to 153 tokens — 42% fewer** (real cl100k, identical rows, encoder before vs.
  after).

### Fixed

- **A tab inside a value silently corrupted its own dictionary.** `@dict` emits its
  distinct values tab-separated but accepted any `Value::Str`, so a value containing
  a tab split into two dictionary entries and every row referencing it decoded to
  the wrong string — `"a\tbcd"` came back as `"a"`. Lossy, silent, and reachable
  from ordinary data. Dictionary eligibility is now bare-safe strings only, matching
  what `@prefix`/`@suffix` already required.

- **A row factored away to nothing was silently dropped.** A single-column table
  whose value was entirely consumed by `@prefix` rendered as a bare empty line,
  and the decoder skipped every empty line — losing the row with no error. This
  predates `@suffix` (`@prefix` alone reproduces it) and was found by a
  round-trip test written for the new pass. Trailing blank lines are still
  treated as formatting; interior ones are now read as rows.

- **The release verifiers checked the wrong thing.** Added on 2026-08-07 to stop
  `continue-on-error` from masking failed publishes, they asked the registry
  whether the version was present — which proves the *state*, not that the run
  did anything. The v5.0.0 release showed why that is not enough. The crates.io
  job's log reads:

  ```
  CARGO_REGISTRY_TOKEN:
  error: crate aethershell@5.0.0 already exists on crates.io index
  ##[error]Process completed with exit code 101
  ```

  …and the verifier then printed `✅ 5.0.0 is published` — true, but only because
  a maintainer had published it by hand minutes earlier. So the step reported
  green over a publish that had exited 101: the exact failure mode it was written
  to catch, reproduced one level up.

  All three verifiers (crates.io, npm, PyPI) now capture the publish step's `id`
  and report two facts separately — *did this run publish* and *is it on the
  registry* — failing the job when the first is `false`. `continue-on-error` stays
  so the rest of the release proceeds, but the outcome is no longer rewritten to
  success.

- **Confirmed fixed:** the `draft: false` change from 4.1.0 works. v5.0.0 produced
  exactly one published release with 9 assets and release notes, and there are now
  **0 orphan drafts** (18 had accumulated between v0.3.0 and v4.0.0).

## [5.0.0] - 2026-08-10

Major version because agent-mode behaviour changes in ways that will break
existing callers. See **Breaking changes** below before upgrading.

### Breaking changes

1. **Nine builtins now require approval in agent mode.** `ssh_exec`,
   `docker_exec`, `podman_exec`, `k8s_exec`, `tool_exec`, `rlm_spawn`,
   `terraform_destroy`, `cloud_instance_destroy` and `db_sqlite_drop_table` were
   ungated; they now refuse with `E_NEEDS_APPROVAL` unless approved. An agent-mode
   script calling any of them will fail where it previously ran. Grant a
   session-scoped RBAC permission, approve per-call with `approve(token)`, or run
   outside agent mode. **Human mode is unchanged.**

2. **`remote_exec` and `cloud_deploy` no longer claim success they never had.**
   `remote_exec` returned `status: "executed"` and `cloud_deploy` returned
   `status: "deployed"`; both are stubs that perform no action. They now return
   `status: "simulated"` with `simulated: true`. Code branching on the old strings
   was branching on a fiction, but it will still need updating.

3. **Every builtin failure is now a structured error.** Failures that previously
   surfaced as bare prose now carry a stable code (`E_UNKNOWN` at minimum). In
   agent mode an uncaught error renders as JSON rather than a sentence. Callers
   matching on error *text* should match on `error.code` instead.

4. **`effect_of` reclassified 61 builtins** away from `Pure`, including the whole
   `web_*` family and every package installer. Anything consuming `x-effect` from
   the ontology, or the effect class via the MCP tool specs, will see different
   values — accurate ones, but different.

### Added — self-healing (agentic-first §9, §11)

The design claimed a self-correcting loop *"falls out of"* structured errors.
It does not fall out; the inference holds only if failures actually carry codes,
suggestions are real, repair context is cheap, and a failed attempt leaves no
debris. Each of those is now built and asserted rather than assumed.

- **Every builtin failure now carries a stable code.** `builtins::call_with_input`
  routes all errors through `safety::ensure_structured`: an error with a specific
  code passes through untouched, anything else becomes `E_UNKNOWN` with its
  original message preserved verbatim and `retryable: false`. Previously ~879
  `anyhow!`/`bail!` sites in `builtins.rs` reached the agent as prose with nothing
  to branch on, against ~520 sites using the structured helpers. New codes:
  `E_UNKNOWN_BUILTIN`, `E_UNKNOWN`; `ErrorCode::retryable()` is now the single
  definition of which failures a repair loop should act on.

- **The same boundary fills in a missing `builtin` name.** `safety::arg_err` takes
  only a message, so its `builtin` field was empty at ~490 call sites — meaning
  `diagnose` could not look up a signature for the majority of `E_BAD_ARG`
  failures, the exact population it serves. The call site is the one place that
  knows the name, so it is filled there rather than at hundreds of sites.

- **`did_you_mean` on `E_UNKNOWN_BUILTIN`** — up to three nearest real names from
  a bounded Levenshtein scan of the live `BUILTIN_LOOKUP` table, ordered
  deterministically, **omitted entirely when nothing is close**. Replaces a
  suggester that searched a hardcoded list of 16 names out of 1,100+ and fell back
  to `"ls, cat, grep"` — a confident wrong answer, which costs a retrying agent a
  full round trip to learn nothing.

- **`diagnose(error)`** (dispatch 1139) — the minimal repair context for a failed
  call: code, `retryable`, hint, suggestions, and the offending builtin's signature
  and effect class. Never costs more than a full `ontology_describe`, and about
  half on richly-documented builtins (map 82 vs 206 tokens, http_get 91 vs 170;
  thin definitions like grep 54 vs 67 save little, which is the honest shape of a
  disclosure win). Named `diagnose` because `explain` was already taken.

- **`try_repair(code)`** (dispatch 1140) — does not invent a fix; makes retrying
  *safe*. Brackets evaluation in a unique named savepoint and rolls back to it on
  failure, so attempt N+1 starts from the state attempt N did rather than from a
  half-applied batch. Returns the structured error alongside `restored`, and leaves
  an enclosing transaction's earlier work intact.

- **`agentic_eval::repair`** — a reusable harness that measures repair rate by
  *replaying* the corrected call, and scores a `MisleadingError` (stable code,
  confident hint, suggestion that does not work) distinctly from an honest dead
  end. An error can look actionable at every structural layer and still send the
  agent the wrong way; only re-running separates the two.

- **§11's self-correction metric is filled in.** It read `≥X%` for as long as the
  document existed. Measured (`tests/repair_rate.rs`): **8/8 (100%)** of plausible
  misspellings repaired by a model-free strategy, **0 uncoded failures** across a
  13-case mixed corpus including real runtime failures. The figure is a floor (no
  model involved) and deliberately scoped — wrong-argument failures are
  diagnosable but not mechanically repairable, and a test pins that down so the
  headline cannot be read as "all failures are repairable".

- **`did_you_mean` now covers module functions too.** `file.read`/`str.upper`
  resolve as *record fields*, not through `BUILTIN_LOOKUP` — and a dotted module
  path is what a model actually writes. `file.raed(…)` previously dead-ended on
  the prose "field 'raed' not found in record": no code, no candidates. It is now
  `E_UNKNOWN_FIELD` with `did_you_mean: ["read"]`, suggested from the record's own
  keys, which also covers ordinary record typos.

Full workspace suite green: 90 suites, 1770 tests, 0 failures.

### Security

- **28 builtins that name a side effect were classified `Effect::Pure`.** §12 of
  the agentic-first design listed effect-tagging 1,100+ builtins as unfinished
  labour and proposed a lint; the lint now exists (`tests/effect_coverage.rs`) and
  found the risk was not hypothetical. Among the 28: `ssh_exec`, `sudo_exec`,
  `remote_exec`, `docker_exec`, `k8s_exec`, `kubectl_exec`, `kubectl_delete`,
  `terraform_destroy`, `cloud_instance_destroy`, `db_sqlite_drop_table`. Because
  `Pure` is the fall-through class, each was advertised to agents as
  side-effect-free through the ontology's `x-effect` annotation, and would have
  been allowed outright by `guard()`. All 28 are now classified; 2 were genuine
  false positives (`platform_has_sudo` is a `which` lookup, `platform_shell_type`
  an env read) and are allow-listed with that reason recorded.

- **Those builtins are now actually gated, not just re-labelled.** A tag only
  changes what is advertised; `guard()` must be called at the site for anything to
  be enforced. Guards were wired into the eight that genuinely act — `ssh_exec`,
  `docker_exec`, `podman_exec`, `k8s_exec`, `tool_exec`, `rlm_spawn`,
  `terraform_destroy`, `cloud_instance_destroy`, `db_sqlite_drop_table` — so in
  agent mode they now refuse with `E_NEEDS_APPROVAL` and are written to the audit
  log. `terraform destroy -auto-approve` is the sharpest case: it does not prompt
  on its own, so approval either happens at the guard or nowhere. **Human mode is
  unchanged.** `kubectl_delete`/`kubectl_exec` needed nothing — they are aliases of
  the already-guarded `k8s_*`.

  **Reading each body before wiring a guard corrected four of the lint's own
  results.** `sudo_exec` returns "use sudo directly in terminal"; `watchexec_run`
  returns a suggested invocation; `env_shell` reads `$SHELL`; `remote_exec` is a
  stub whose own comment says *"Simulate remote execution"*. All execute nothing,
  and all had been tagged `Exec` **from the name alone** — the exact error the lint
  exists to catch, committed while fixing it. A name-based lint nominates suspects;
  it does not convict. Each acquittal is allow-listed with its verified reason.

  **What this still does not fix:** **1,183 of 1,301 builtins (91%) fall through to
  `Pure`**, and the lint only sees names that advertise a side effect — a dangerous
  builtin with an innocuous name remains invisible to it. `db_sqlite_exec` is
  classified `Exec` but deliberately left unguarded, since gating it would put
  every sqlite *read* (including `db_kv_get`) behind approval. Flipping the default
  to a restrictive class would gate ~1,000 builtins at once: a product decision.

- **Package installers were classified `Pure`.** Broadening the lint past
  exec/delete names to egress and persistence surfaced 29 more, including
  `npm_install`, `yarn_install`, `pnpm_install`, `bun_install`, `pipx_install`,
  `poetry_install`, `pkg_install`, `asdf_install`, `helm_install`,
  `marketplace_install` and `pre_commit_install` — each shells out to a package
  manager that fetches remote code and runs its install scripts. That is the
  supply-chain surface (CWE-494), and `effect_of` was reporting
  `npm_install("anything")` as side-effect-free. Now `Exec`;
  `helm_uninstall`/`marketplace_uninstall` are `Destructive`.

- **The `web_*` family was gated at runtime but advertised as `Pure`.** Every
  `web_*` fetch already routes through `guard_network` with `Effect::Network`, but
  `effect_of("web_post")` returned `Pure` because the Network arm only matched
  `http*`/`net_`/`nc_`. The control was correct; the label an agent reads was not.
  They now agree. Also classified: `scp_upload`/`scp_download`/`wget_download`/
  `marketplace_publish` as `Network`, and `write_file`/`write_json`/`text_write`/
  `save_json`/`gui_dialog_file_save`/`fs_mount` as `WriteLocal`.

- **A third lint pass** added privilege/service-control names
  (`chmod`/`chown`/`restart`/`deploy`/`service`) and classified the eight it
  surfaced: `svc_restart` and `k8s_rollout_restart` as `Process` (restarting a
  service interrupts whatever it was doing), `chmod`/`fs_chmod`/`fs_chown` as
  `WriteLocal`, and `k8s_deployments`/`k8s_services` as `Network` — they are reads,
  but *remote* ones that ship credentials to a cluster endpoint.

  Effect coverage after three passes: **1,122 of 1,301 (86%) fall through to
  `Pure`**, down from 1,183; classified builtins 118 → 179.

### Fixed

- **`remote_exec` claimed to have executed commands it never ran.** It returned
  `status: "executed"` from a stub whose own comment reads *"Simulate remote
  execution (in real impl would use SSH/RPC)"*. An agent acting on that would
  believe a service was restarted or a deploy done. It now reports
  `status: "simulated"` with an explicit `simulated: true` and an output string
  saying the command was not run.

- **`cloud_deploy` fabricated deployments the same way.** It minted a UUID and
  returned `status: "deployed"` with a timestamp, while containing no HTTP client
  and spawning no process — it contacts no cloud provider at all. Found by the
  third lint pass. Now `status: "simulated"` with `simulated: true` and a `note`
  saying no deployment was created.

  **The documentation was worse than the code.** `docs/book/src/advanced/distributed.md`
  showed `remote_exec` returning a real result (`# 15`), and the rolling-deploy
  runbook in `docs/book/src/examples/devops.md` used it to
  `systemctl restart aethershell` on every node — a deploy in which every node
  reports success without ever being restarted. The runbook now uses `ssh_exec`,
  which really executes and is approval-gated in agent mode.
- **`aethershell` is now registered on PyPI, closing finding 12.** The SDK is
  published as `aethershell` 1.5.0 and `pip install aethershell` works —
  verified in a clean virtualenv, not from the upload's own success message.
  The name can no longer be claimed by a third party, which was the actual
  vulnerability: the docs told users to install it, `pip install` executes
  package code, and nobody owned the name.

  The sdist was scanned before upload (21 files, no username, host paths or
  credential-shaped strings) because PyPI releases can be yanked but never
  deleted. That inspection also caught something the source tree did not show:
  the SDK `README.md` is the package's PyPI long description, and it still
  carried the "**do not run `pip install aethershell`**" warning added earlier
  the same day. Publishing then would have made that warning the package's
  front page.

- **Correction: `CARGO_REGISTRY_TOKEN` was never set on this repository
  either.** A comment in `release.yml` claimed the crates.io publish step
  "demonstrably works — crates.io has the published versions". Wrong reasoning:
  the repository has *no* Actions secrets at all, the v4.1.0 run shows
  `CARGO_REGISTRY_TOKEN:` empty, and those versions exist because they were
  published manually from a local token. All three publish jobs have always
  failed, for three unrelated reasons, and the suppression made every one look
  like success. Corrected in place.

### Changed
- Python SDK install instructions point at PyPI again, and note that the SDK
  versions independently of the shell (SDK 1.5.0 against shell 4.1.0).

## [4.1.0] - 2026-08-07

### Security
- **The Agent API's deadline now reaches the interpreter, so evaluation stops
  itself.** 4.0.0's request deadline freed the connection and the async worker
  but could not stop the work: dropping a `spawn_blocking` handle does not
  cancel the closure, so a wedged evaluation kept a blocking-pool thread until
  it returned on its own. `safety::enter_deadline` sets a per-thread limit that
  `eval_expr` checks.

  Three details carry it. The language has **no loop constructs** — unbounded
  work is recursion or large data, both of which pass through `eval_expr`, so
  one check covers it (worth confirming: a check placed in a loop evaluator
  would have covered nothing). The clock is **sampled every 1024 steps** rather
  than read per AST node, so with no deadline set — the REPL, scripts, every
  test — the check is one thread-local read. And the guard **restores rather
  than clears**, because these threads are pooled: a deadline left set would
  make the next request on that thread fail instantly, a worse bug than the one
  being fixed. Both properties are tested.

  **Still not bounded:** a builtin already blocked in a syscall — `sleep 3600`,
  a subprocess wait, a network read — never returns to the interpreter to be
  asked. The gap is narrower, not gone.

  **Verified by disabling it.** The test asserts a long evaluation stops near
  its deadline; with `check_deadline()` commented out it hung until killed at
  400 s, against 26 s passing. Three changes in this cycle reviewed as correct
  and did nothing, so a passing test is not evidence until its failing form has
  been seen.

- **Recursion no longer aborts the process, and usable depth went from ~35 to
  1900+ (CWE-674).** `let f = fn(x) => f(x)` overflowed the stack and killed the
  process. The new evaluation deadline could not catch it, because a stack
  overflow does not unwind.

  A depth counter alone would not have fixed it: low enough to fire before the
  stack does (~25) rejects ordinary recursive programs, high enough to be usable
  never fires. So the stack came first. `safety::with_eval_stack` runs
  evaluation on a 256 MB stack (reserved address space, not committed memory, so
  it costs nothing until used) and `main` is a thin wrapper around it. The Agent
  API's tokio runtime sets `thread_stack_size` to match — easy to miss and
  necessary, because evaluation there happens on tokio's own `spawn_blocking`
  workers, which would otherwise keep the default stack regardless of what
  `main` does. `safety::MAX_CALL_DEPTH` (2000) is then enforced through an RAII
  guard, so depth unwinds even when an inner call returns `Err`.

  Measured, debug profile, same machine:

  | Depth | Before | After |
  | --- | --- | --- |
  | 40 | **stack overflow** | ok |
  | 1900 | stack overflow | ok |
  | 2100 | stack overflow | **refused cleanly** |
  | unbounded | **process abort** | **refused cleanly** |

  Two wrong turns worth recording. The guard went first into
  `builtins::call_lambda`, which is not on this path — `eval.rs` has four more
  entry points, and with only the first guarded `f(2500)` still returned a
  result. It looked correct and did nothing. Then the test asserting deep
  recursion overflowed anyway, because a default test thread has ~2 MB while the
  limit needs ~60 MB to be reachable: the stack beat the limit, which is the
  exact failure the large stack exists to prevent.

  **Constraint on the fix:** the depth limit is only safe with the large stack.
  An embedder calling `eval::eval_program` on an ordinary thread gets the limit
  but not the stack, so deep recursion still aborts first. Use
  `safety::with_eval_stack` or set an equivalent `stack_size`.

- **Docs no longer direct users to an unclaimed package name (CWE-494).**
  `docs/api/PYTHON_SDK.md`, the book, and the SDK README all told readers to run
  `pip install aethershell`. That package **is not on PyPI, and the name is
  unregistered** — so the command installs whatever a third party has uploaded
  under it, and `pip install` executes package code at install time. The
  project's own documentation was the delivery mechanism.

  Found by checking whether a thing the build *claims* to do actually happened,
  rather than by reading the workflow. The giveaway was version drift — the
  crate is at 4.0.0 while `integrations/python/pyproject.toml` says 1.5.0 and
  `web/package.json` says 0.2.0. Had a publish ever landed, those would have
  moved.

  The v4.0.0 release then confirmed the mechanism by executing it. The
  `Publish Python SDK to PyPI` job reported **`completed/success`** — every step
  green — and PyPI still had no package. The log shows
  `outcome=failure;conclusion=failure` with `environment: MISSING`: trusted
  publishing was never configured, and `continue-on-error: true` rewrites the
  failed step to `success`, which the job and run inherit.

  The npm job on the same release behaved identically — `completed/success`,
  registry still 404 — for an entirely unrelated reason: `NODE_AUTH_TOKEN` was
  empty (`ENEEDAUTH`), because the `NPM_TOKEN` secret has never been set. Two
  publish jobs, two unrelated causes, both indistinguishable from success.

  Three layers of the same illusion: the workflow *contains* a publish step so
  reading it looks right; the suppression makes the step green; the job and run
  inherit that. Every signal available from inside the repository says this
  works. Only asking the registries shows it never has.

  All install instructions now point at the repository and carry a warning. The
  suppressed publish steps are annotated with what they actually do.

  Each publish job now has a verification step that queries the registry
  afterwards and reports the answer in the run summary, because a publish step
  that cannot tell you whether it published is not a publish step. The checks
  read the package name and version out of the artifact actually published
  rather than assuming them — necessary, since the npm artifact is named
  `aether_wasm`, not `aethershell`.

  **Scope, stated precisely.** PyPI is the live exposure: `pyproject.toml`
  declares `aethershell`, the docs pointed users at it, and the name is
  unregistered. npm is a broken job rather than an exposure — no documentation
  directs anyone to `npm install` this project, so nobody is being sent to an
  unclaimed name there.

  **Not fixed here, and needs the maintainer's registry account: register
  `aethershell` on PyPI.** This is the only open item in the audit with a
  window a third party can close for you. See finding 12 in
  `docs/security/SECURITY_AUDIT_2026-07-30.md`.

## [4.0.0] - 2026-08-06

### Security
- **The Agent API now enforces a request deadline — and, more to the point, the
  deadline actually fires.** Closes the last open code item from the audit: an
  authenticated caller could hold a worker indefinitely, and since
  `/api/v1/eval` evaluates arbitrary code, a wedged request was a one-line POST.

  A `TimeoutLayer` alone would have been decorative. `process_request` is
  synchronous and was called directly from the `async` handlers, so it pinned an
  async worker for the whole evaluation and never yielded. Tower races the
  deadline against the inner future *within the same poll*, so the timeout
  branch was never reached. Measured rather than assumed: with the layer
  mounted and a 1-second deadline, a 20-second request was not interrupted at
  all — the test asserting 408 failed before the handlers were changed.

  The four execution handlers now run on the blocking pool, which lets the
  deadline fire, frees the async worker, and returns 408.

  **What this does not do**, stated plainly: dropping a `spawn_blocking` handle
  does not cancel the closure, so a wedged evaluation keeps a blocking-pool
  thread until it finishes on its own. This converts "one request wedges the
  server" into "the server keeps answering while leaked threads accumulate"
  against a bounded (512-thread default) pool. Actually interrupting evaluation
  needs a deadline checked inside the interpreter loop, which remains open.

  The SSE `/api/v1/stream/*` routes and the WebSocket are deliberately exempt —
  they are long-lived by design, and a per-request deadline would sever them
  rather than protect anything. There is a test for that too, so the exemption
  cannot be silently lost.

### Added
- `ae agent-api serve --request-timeout <secs>` (default `300`). `0` disables
  the deadline and reintroduces the exhaustion above; prefer raising it.

### Changed
- **BREAKING: `AgentApiConfig` gained a `request_timeout_secs` field.** The
  struct has public fields and is not `#[non_exhaustive]`, so any downstream
  struct-literal construction stops compiling — this project's own integration
  test did. That is a breaking change to a published API regardless of how
  small the addition is, hence the major version. Semver is about the contract,
  not the size of the diff.

## [3.0.1] - 2026-08-06

### Security
- **Three more live PowerShell injection sites (CWE-78), found by a second lint
  aimed at the first lint's blind spot.** 3.0.0 closed the "a new site can call
  `format!` instead of `ps_script!`" residual only by convention. This release
  adds `powershell_commands_with_values_use_the_checked_macro`, which flags any
  shell-shaped line containing `{}` that sits inside a `format!`.

  It flagged 21 sites. Seventeen were numeric interpolations needing only the
  macro. Three were exploitable, and a fourth was hand-escaped rather than
  helper-escaped:

  | Builtin | Fragment | Before |
  | --- | --- | --- |
  | `net.ip_addresses` | `Get-NetIPAddress … -like '*{}*'` | unescaped |
  | `net.adapters` | `Get-NetAdapter … -like '*{}*'` | unescaped |
  | `timeout` (Windows) | `Start-Process … -ArgumentList '/C {}'` | unescaped |
  | `log.search` | `Get-WinEvent … -like '*{}*'` | hand-escaped |

  An interface name or command containing `'` terminates the string and the
  remainder executes. `timeout` is the most serious: the injected text lands in
  a `cmd /C` argument list, requiring no PowerShell knowledge to exploit.

  The 2.0.4 lint missed all four because it matches the exact shapes `'{}'` and
  `"{}"`, while these *embed* the placeholder in a larger quoted string. It had
  read as coverage for this class while being blind to its most common shape.
  `is_suspect` now pairs single quotes around a placeholder generally, with all
  four shapes as regression assertions, and asserts that the correct unquoted
  numeric form (`-Id {}`) stays unflagged.

  Fixed by escaping the whole pattern — `ps_quote(&format!("*{}*", v))` — so the
  wildcards sit outside the escaped span.

- Five defects in this class, five detection methods, none of which found what
  the others did. See `docs/security/SECURITY_AUDIT_2026-07-30.md` §10g.

### Changed
- `dmesg` carries its Windows event count as an `i64` rather than a
  pre-stringified value. The site was safe in fact but not by type; `ps_script!`
  rejected it, which is the check working as intended.

## [3.0.0] - 2026-08-05

### Security
- **Two more injection sites, found by a type check that nothing else could
  see.** `ps_script!` and `applescript!` now accept only pre-escaped literals
  (a sealed `PsArg` trait: `PsLiteral`, integers, and `&'static str` — never
  `String` or a borrowed `&str`), so passing caller data into a shell command
  is a compile error naming the argument. 56 PowerShell and 2 AppleScript
  sites were converted.

  Converting them immediately surfaced two live vectors that interpolate
  **unquoted** — `New-VM -MemoryStartupBytes {}` (`vm.create`) and
  `New-NetFirewallRule -LocalPort {}` (`firewall.allow`). The 2.0.4 lint is
  blind to these by construction, since it looks for *quoted* placeholders,
  and three manual passes had missed them. Neither can be quoted (`4GB` must
  stay a numeric literal), so `safety::ps_bare_number` validates them against
  digits plus an optional size suffix.

  Four defects in this class, found four different ways: reading, testing an
  assertion reading had got wrong, a lint, and a type check. All three defence
  layers are kept, because each caught what the others could not.

### Changed
- **`safety::ps_quote` and `safety::applescript_quote` return newtypes**
  (`PsLiteral`, `AppleScriptLiteral`) rather than `String`, so the *type*
  records that escaping happened. Both have private fields, so nothing outside
  `safety` can construct one, and neither implements `From<String>` or
  `Deref<Target = str>` — either would let an unescaped value stand in for an
  escaped one.

  Both render through `Display`, so every `format!("… {}", ps_quote(&v))` call
  site was unaffected; the compiler surfaced exactly two places that had relied
  on the `String` (a `Vec::join` in each zip builtin). Doing it this way round
  means the compiler enumerates the call sites, which is what manual review
  failed at three times in this cycle.

  This is a breaking change to public API — `safety::ps_quote` ships in 2.0.4 —
  hence the major version, even though the practical blast radius is nil: the
  helper is a day old, has no known downstream consumers, and the common use
  `format!("… {}", ps_quote(v))` is unaffected because the newtype implements
  `Display`. Semver is about the contract, not the download count.

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

[Unreleased]: https://github.com/nervosys/AetherShell/compare/v8.0.0...HEAD
[8.0.0]: https://github.com/nervosys/AetherShell/compare/v7.4.0...v8.0.0
[5.0.0]: https://github.com/nervosys/AetherShell/compare/v4.1.0...v5.0.0
[1.2.0]: https://github.com/nervosys/AetherShell/compare/v0.3.1...v1.2.0
[0.3.1]: https://github.com/nervosys/AetherShell/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/nervosys/AetherShell/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/nervosys/AetherShell/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/nervosys/AetherShell/releases/tag/v0.1.0
