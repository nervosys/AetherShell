# Handoff — 2026-08-24 (session 5 update: 2026-08-25 — §7.3 item 3 is DONE, and it was hiding a guard bypass)

> **Session 5 (2026-08-25): the last open engineering item turned out not to be
> an optimisation. Widening it required knowing what the dispatcher serves, and
> asking that question found a live authorisation bypass.**
>
> **The finding.** `safety::classified_effect` is a `match` on the builtin's
> *name*. Dispatch is by *implementation*: `BUILTIN_LOOKUP` aliases share a
> dispatch index (`sh` and `shell` are both 123), and a fallback `match` arm
> serves every literal on its left (`"vault-convert" | "vault_convert" =>
> bi_vault_convert(..)`). So one spelling of a builtin could be classified and
> the other not — and the unclassified spelling read as `Pure`.
> `centrally_enforced(Pure)` is false, so `guard_dispatch` returned `Ok` before
> any policy ran, and since the audit line covers only `WriteLocal`/`Network` the
> call left **no trace either**.
>
> Measured, not argued: **104 alias groups disagreed with themselves**, and
> **26 of them produced a different guard decision for the same implementation**.
> The demonstration, in agent mode, one call apart:
>
> ```
> lldb_run  -> ERR  {"approval": {...}, "builtin":"lldb_run", ...}     refused
> lldb      -> OK   Record({exit_code: 1, output: "(lldb) target create …"})
> ```
>
> Same dispatch index. The debugger ran. `tests/alias_guard_bypass.rs` is that
> sequence, and it is red without the fix (confirmed by reverting `effect_of` and
> watching `lldb` run again).
>
> **Not every split was exploitable, and the difference matters.** Some come from
> `SELF_GUARDED`, where the "allowed" spelling is allowed *because its own body
> guards it* — `sh`/`shell` is one of these, so the shell passthrough was never
> actually open. Those are defence in depth, not holes. The holes are the groups
> where the unclassified spelling reaches the implementation with nothing in
> front of it, which is what the `lldb` probe confirms directly.
>
> **The fix restores the design.** §5.3 of `docs/AGENTIC_FIRST_DESIGN.md`
> specified the effect table as "a side table keyed by `BUILTIN_LOOKUP` **index**";
> it was implemented keyed by name, and that divergence *is* the bug.
> `builtins::alias_groups()` now derives the equivalence classes from the
> dispatcher itself, and `effect_of` fills any unclassified spelling with the
> strictest classification a sibling carries. No hand-written alias list, so
> nothing to drift. **118 builtins changed class, every one of them in the strict
> direction** (58 → `ReadLocal`, 23 → `Exec`, 20 → `Network`, 15 → `WriteLocal`,
> 1 → `Destructive`, and `file_mkdir` `ReadLocal` → `WriteLocal`); nothing was
> weakened, which is asserted rather than eyeballed.
>
> **What made it findable.** The prerequisite session 4 named — *"a single source
> of truth for the names the dispatcher serves, in `src/`"* — is
> `builtins::FALLBACK_BUILTINS`, 116 `(name, function)` pairs mirroring the
> fallback `match`. It is hand-written **and** drift-proof:
> `tests/fallback_dispatch.rs` parses the arms out of `src/builtins.rs` and
> demands equality, names *and* the function each reaches, so an arm added
> without an entry fails the build. That mechanical check is the difference
> between a second source of truth and a second place to forget — and it is why
> this is not the drift the "catalog is a promise" invariant exists to prevent.
>
> **Three invariants that were half-blind now see both halves:**
>
> - `tests/effect_ratchet.rs` gated on `BUILTIN_LOOKUP.contains_key`, so **113
>   agent-callable names had never been examined** while `guard_dispatch`
>   enforced policy from `effect_of` for all of them. Now covered. It reports
>   zero violations across both halves — and `report_fallback_dispatch_coverage`
>   exists because that zero arriving the moment the set widened is exactly what a
>   *blind* check looks like. It prints the arithmetic: 116 arms, **116 bodies
>   resolved**, 0 unresolved.
> - `tests/effect_snapshot.txt` pinned `BUILTIN_LOOKUP` only — it now pins both
>   halves (1,310 → 1,423 entries). It was leaving out the part that had already
>   gone wrong.
> - `tests/effect_alias_agreement.rs` is new and holds every alias group to one
>   effect, so a classification added to one spelling and not its twin fails the
>   build instead of quietly opening a door.
>
> **§7.3 item 3 itself — DONE, and now sound rather than fail-safe.**
> `eval::is_effect_free` asks `builtins::is_dispatched` instead of
> `BUILTIN_LOOKUP`, so a `take` downstream of a fallback-half builtin
> (`from_json`, `select`, `group_by`, `to_csv`, …) short-circuits like any other:
> `pulled == 3` of 1000 where it was 1000. An undispatched name still blocks it —
> `false` means *unknown*, never *harmless* — asserted by a user-defined-function
> test that must still read all 20 elements. Both are red without the change.
>
> **Also verified this session:** `a16b3a2` — the handoff's last-pushed commit,
> which session 4 could only check as far as `afd6e8f` — is **17/17 green by
> SHA**, `Test (windows-latest)` included.
>
> **Measured this session, not inspected:** `aethershell` **1905 passed, 0 failed
> across 111 suites**; `agentic-eval` **83 passed, 0 failed**; doctests **7 ok / 1
> ignored / 0 failed**; `cargo clippy --workspace --all-targets --features native
> -- -D warnings` exit 0; `cargo fmt --all -- --check` exit 0. That was the state
> at `44ab51c` — `c7780c4` (the source of truth), `194a496` (the guard fix),
> `51f0532` (item 3) and the docs — **17/17 green by SHA**, `Lint` and
> `Test (windows-latest)` included.
>
> After the rest of session 5 (`142d60c`, `a006d40`, `8e24118`) the suite reads
> **1919 passed, 0 failed across 113 binaries**, doctests **7/0**, clippy and fmt
> both exit 0.
>
> Every number here is from the **committed** tree, and each test figure is one
> uninterrupted `cargo test --features native --no-fail-fast` at exit 0 — not a
> stitched-together set of partial runs.
>
> **A trap worth carrying forward, hit four times this session.** Getting that
> one clean run took five attempts, and *every* failure in between was the
> environment wearing a code failure's clothes:
>
> | Reading | What it actually was |
> |---|---|
> | 3 doctest failures, *crate `encoding_rs` required to be available in rlib format* | re-running the doctests alone: 7 ok / 0 failed |
> | `error[E0463]: can't find crate for aethershell` during clippy | clippy on the identical tree minutes later: exit 0 |
> | `EXIT=127` mid-suite | disk, exactly as §7.2 says — 358 `.pdb` files holding **49.1 GB**; purging restored it |
> | `STATUS_STACK_BUFFER_OVERRUN` (rustc ICE), twice | 39 concurrent `cargo`/`rustc` from other repos; clean once they drained to 2 |
>
> **A failure naming a missing rlib, an ICE, or exit 127 is an environment
> reading, not a result — re-run it before you write it down.** The converse
> matters just as much and is the harder discipline: a *green* run under those
> conditions is not trustworthy either, which is why the numbers above were only
> recorded once one run completed end to end with the machine quiet.
>
> **Environment note.** 36 stray `cargo.exe`/`rustc.exe` were running throughout,
> all from *other* repositories (cyb0rg, ami, chasm, irongate). §7.1's warning
> held: builds were slow, but nothing corrupted, and killing another session's
> work is not on the table. Check `tasklist` before believing a compiler error.

> **Session 5, second half: the roadmap in §5 item 4 is closed out — two done,
> one deliberately not, and the reasons are the useful part.**
>
> - **Lazy iterators (`142d60c`).** A whole-collection stage used to disqualify
>   the *whole* pipeline, so `xs | map(f) | take(3) | sort` called `f` a thousand
>   times to sort three elements. A barrier now ends the streamable region rather
>   than poisoning it: the prefix streams, the barrier gets exactly the
>   collection the eager evaluator would have handed it, `pulled` drops 1000 → 3.
>   Two of the new tests assert agreement against `eval_program` itself rather
>   than against a constant, and five of six are red without the change. The
>   effect rule survives the split — a `take` still may not abandon the source
>   when anything upstream acts — asserted with a barrier now sitting behind it,
>   because moving the barrier out of the streamed region must not smuggle that
>   check away.
> - **RBAC bridge (`a006d40`) — decided against, not skipped.** It looked like
>   plumbing and is a security decision: the older store holds
>   `(resource, actions)` and the newer holds capability strings, and only the
>   newer is read by `guard`. `rbac_grant` is `Privileged`; `role_grant` is
>   unclassified and allowed **because it confers nothing**. Bridging without
>   classifying `role_*` first would recreate `194a496`'s bug from the other
>   direction. `tests/rbac_legacy_store.rs` measures the inertness behaviourally
>   — `rm` refused, broadest legacy role granted, `rm` refused again — so anyone
>   who does bridge it is told what has to happen first.
> - **Transpiler retirement — untouched, and the blocker is why.** Finishing it
>   means extending the grammar over five cipher forms and disambiguating two
>   overloaded tokens. That is a language change needing its own session and a
>   decision about whether those forms are kept at all; doing it in the margins
>   of another task is how a grammar acquires accidents.
>
> **Adversarial pass over the session's own work (`77bde14`), which found a
> second bypass.** The effect classification fixed this morning only constrains
> anything if `guard_dispatch` is genuinely the single door.
> `call_with_input_inner`'s comment says it is. It was a comment: **128 `bi_*`
> implementations were `pub`**, so the body could be called directly, meeting no
> policy at all.
>
> The invariant worth stating is narrower than “nothing is pub”. For the four
> classes `centrally_enforced` gates, skipping the central guard means skipping
> the *only* guard — unless the body has its own, which `SELF_GUARDED` records.
> Twelve builtins failed that: seven `terraform_*`, four `ansible_*` (Exec) and
> `dd_copy` (Destructive). `terraform_destroy` **was** guarded, being one of the
> eight § 7 wired by hand, while `terraform_apply` beside it was not — which is
> how a hand-picked list of eight ages.
>
> Latent, not live: nothing in `src/` called a body directly, so every in-crate
> route still went through the door. The exposure was to consumers of the
> published library and to the next person to add a caller. Fixed by dropping
> `pub` from 114 of 128 so the **compiler** enforces the door; 14 stay public
> because tests call them (`bi_rm`/`bi_rmdir` deliberately, to exercise their own
> guards). `tests/one_door.rs` holds all of it, ordering included — policy
> consulted *after* the action is a log, not a gate.
>
> **Suite after this: 1924 passed, 0 failed across 114 binaries**, clippy and fmt
> exit 0.
>
> **Checked §5 item 2’s “two live vulnerabilities” rather than assuming (`c9ff0e7`).**
> The wording is ambiguous between “genuine” and “still unfixed”, and this session
> had already found three stale claims, so both were re-verified:
>
> - **Unauthenticated MCP builtin execution — fixed.** `McpServer::call_builtin`
>   goes through `builtins::call`, so `guard_dispatch` applies; `agent_api`
>   requires a bearer token on every route but `/health`.
> - **But the AI API had a live fail-open next to it.** `aimodel` assigned the
>   CLI `--require-api-key` flag straight over the config value. The flag is a
>   bare bool, `false` whenever absent — so `require_api_key = true` in a config
>   file was silently switched **off** by running `aimodel server`. Host, port and
>   CORS were overridden the same way, but those land on *more* restrictive
>   values; only this one failed open. Routes affected are not read-only: model
>   download, convert (spawns a converter), delete, storage cleanup. Now folded
>   one-way — a flag may add a requirement, never drop one.
> - Default posture is unchanged and defensible: `127.0.0.1`, no key. What was
>   missing is any coupling between “I made this reachable” and “I turned auth
>   on”, so a non-loopback bind without a key now warns (warns, not refuses — it
>   is legitimate behind a trusted boundary).
> - **SQL injection — audited, and half of it was live (`15fd176`).**
>   `safety::sql_identifier` exists and `db_sqlite_delete`/`_count` call it, with
>   a comment explaining that their WHERE clause is SQL by contract and the table
>   is not. `db_sqlite_insert` and `db_sqlite_update` were missed: they escape a
>   record’s *values* and interpolate its *keys* raw, and the sqlite3 CLI runs
>   every `;`-separated statement, so
>   `db_sqlite_insert(db, "victim", {"id) VALUES (1); DROP TABLE victim; --": 1})`
>   dropped the table. Demonstrated against a real database, checking
>   `sqlite_master` afterwards rather than the return value.
>
>   **The escaping that was present is what hid it** — a reader sees quote-doubling
>   on the values and stops looking. It was hand-rolled rather than `sql_literal`,
>   so it had also never inherited that helper’s NUL check. Both now share a
>   `sql_value` helper. Identifiers validated in `drop_table`, `create` and
>   `export_csv` too.
>
>   **A second live one, found by generalising instead of stopping (`927f52a`).**
>   `db_json_to_sqlite` is registered, and its column names come from the *file*:
>   a JSON object key of `x TEXT); DROP TABLE victim; --` closed the CREATE TABLE
>   and the CLI ran the rest. Importing an untrusted file was enough — no hostile
>   argument required. Its values were the third copy of the hand-rolled escape,
>   and so the third to lack the NUL check.
>
>   **`tests/sql_interpolation_ratchet.rs` is the durable part.**
>   `safety::ps_quote` solves this structurally — it returns a `PsLiteral` newtype
>   whose quotes are *included*, so a missed call site breaks the output instead
>   of failing silently; 72 uses, no drift. The SQL helpers return a plain
>   `String`, which is why three sites could be missed while the code around them
>   looked careful. The ratchet substitutes a source lint for that missing type:
>   every SQL-looking `format!` must interpolate helper calls, things built from
>   them, or an allowlisted name **with a reason**. Verified to catch a
>   reintroduced hole, not assumed to.
>
>   **~~Still open here~~ — CLOSED in session 6.** `db_sqlite_create_table`
>   interpolated the column *type* as written, deferred across three sessions
>   on the grounds that constraining a SQL type expression (`VARCHAR(255)`,
>   `DECIMAL(10,2)`, `NOT NULL`) is a decision rather than a drive-by fix.
>   The decision is now made and lives in `safety::sql_column_type`: a token
>   allowlist, not a grammar. Still unregistered, so nothing was exploitable —
>   what is removed is the precondition that registering it required a
>   security decision nobody had taken.
>
> **Suite after this: 1931 passed, 0 failed across 115 binaries**, clippy and fmt
> exit 0.
>
> **A hypothesis that did not reproduce (`902fa28`), recorded because a negative
> result is still a result.** `safety::reject_option_like` was called at three of
> eight `sqlite3` spawn sites — the same partial-application shape — and its own
> doc warns that several tools can be made to run a command through an
> option-like argument. Expected: command execution through an unguarded path.
> Measured against sqlite3 3.53.4:
>
> - a dot-command **does** execute from a single argv entry (`.system echo hi > f`
>   created the file) — which is what `reject_sqlite_dot_command` already guards
>   in the positions that take caller input;
> - a newline does **not** begin a second dot-command inside one argv entry, so
>   the chaining the attack needed does not exist;
> - a quote in `db_sqlite_backup`’s path reaches `.backup`’s option parser, which
>   is an integrity problem, not an execution one.
>
> **No RCE.** Guards added for consistency and defence in depth — described as
> that, not as a fix for a hole — plus a ratchet over all eight sites. Worth
> carrying forward: the partial-application heuristic found five real issues
> today and one false alarm, so it is a good place to *look*, not evidence on its
> own.
>
> **Suite: 1942 passed, 0 failed across 116 binaries.**
>
> **Command execution through `web.open_url` — found by taking the same question
> to a different family, and the one place today's heuristic was not the tool.**
> The recurring question of this session has been "where is a safety helper
> partially applied?" This one came from asking the more general form — *where is
> a value handed to something that parses it?* — of the shell-spawn sites rather
> than of a helper. There was no helper here to be partially applied; there was
> no gate at all.
>
> `bi_web_open_url`'s Windows branch ran:
>
> ```rust
> std::process::Command::new("cmd").args(["/C", "start", &url]).output()?
> ```
>
> It reads as a fixed command with the URL in an argument slot, which is why it
> survived review. It is not one. Rust quotes an argument only when it contains a
> space or a quote, and `cmd` splits its command line on `&`, so a URL with
> neither reaches `cmd` verbatim and everything after `&` runs as a second
> command. **Demonstrated, not theorised:** against this machine's `cmd.exe`,
> `http://example.com&echo.>marker.txt` created the file — through a plain `echo`
> in the same argv shape, and again through the real `start` form.
>
> The placement in the safety model is what makes it the worst finding of the
> session. `effect_of("web_open_url")` is `Network` by the `web_*` prefix rule,
> `Network` is not in `centrally_enforced()`, and the builtin was not in
> `SELF_GUARDED`. So in agent mode it was **default-allow, unapproved, unmetered,
> and outside the `AETHER_NET_ALLOW` egress allowlist** — a builtin advertised as
> "open a web page" was an unapproved `Exec`, and separately a clean exfiltration
> channel, since a URL carries whatever you put in its query string.
>
> **Why the obvious fix is the wrong one, and this is the part worth carrying
> forward.** The reflex is to refuse `&`. That cannot work: `&` is the
> query-string separator, so the dangerous character is *legal data* in exactly
> the values this builtin exists to accept. A blocklist that refuses it breaks
> `?a=1&b=2`; one that allows it leaves the hole open. When the metacharacter is
> also the payload, only a structural fix is available. The Windows branch now
> uses `rundll32 url.dll,FileProtocolHandler` — the same `ShellExecute` dispatch
> with no shell in front of it — and spawns rather than waits, which also fixes
> the old `.output()` blocking until the browser was closed.
>
> Three things were added, each doing a job the others cannot:
>
> - `guard_network("web_open_url", …)`, which the rest of the `web_*` family
>   already called, plus the `SELF_GUARDED` entry that goes with it — this is the
>   metering and the egress allowlist, not the injection.
> - `safety::reject_unsafe_url`, for what a shell-free launcher does *not* solve:
>   the scheme. `ShellExecute` and `xdg-open` dispatch on it, so `ms-msdt:` (the
>   Follina shape) reaches a registered handler that is not a browser. Allowlist
>   of `http https ftp ftps mailto file`; it may only shrink. It also refuses
>   control characters and a leading `-`, the latter because the macOS and Linux
>   branches pass the value positionally and `open -a <app>` launches an
>   arbitrary application.
> - `tests/shell_spawn_guard.rs`, the ratchet: **every site handing a value to
>   `sh -c`/`cmd /C` must gate on `Exec` first.** Two entries on the allowlist
>   (`nohup_run`, `xargs_exec`), each of which runs a caller's command by
>   contract and calls `guard_exec`. Verified red by restoring the old call site,
>   which named `bi_web_open_url` exactly.
>
> The scanner's first draft had a false positive — `bi_hw_sensors`'s literal
> `"which osx-cpu-temp && osx-cpu-temp"`, whose `&&` is inside the string — which
> is now one of the unit tests on the scanner itself.
>
> **Suite: 1958 passed, 0 failed across 118 binaries**, clippy `-D warnings` and
> fmt exit 0.
>
> **Revised tally on the heuristic:** six real issues, one false alarm. The
> generalisation is the useful part — *partially applied helper* was the shape
> that kept paying, but the question underneath it is **where does a value reach
> a parser**, and that one still had somewhere to go after the helper question
> ran dry.
>
> **Not closable from this seat, and both still open:** the crates.io token
> rotation (§5 item 1 — a human at <https://crates.io/settings/tokens>) and the
> external security review and penetration test (§5 item 2). Registering the 168
> unadvertised `bi_*` (§5 item 3) is a product call, not an engineering one.
>
> **A jail escape in `web.download`, and the reason it was invisible: a builtin
> can have two effects and the taxonomy carries one.** Same question again —
> where does a value reach something that parses it — this time asked of `curl`.
>
> `web_download`, `wget_download` and `web_upload_file` are classified `Network`
> by the prefix rule, call `guard_network` on the URL, and are listed in
> `SELF_GUARDED`. Every place a reader checks says *gated*. None of it touches
> the file. `guard_network` passes `Effect::Network` and `fs_paths: false`, and
> the jail in `safety::guard` fires only when the effect `is_filesystem()` **and**
> `fs_paths` is set — so in agent mode this held:
>
> ```text
> file.write   <outside-workspace>          → refused, OutsideWorkspace
> web.download <url> <outside-workspace>    → allowed
> ```
>
> Same jail, same path, opposite answers. The workspace jail is the containment
> story for agent mode, so a builtin that writes past it is not a lesser
> `file_write` — it is the way around it.
>
> Fixed with `guard_network_local_write`, which resolves the path
> workspace-relative (as `file_write` does) and guards it as `WriteLocal` with
> `fs_paths: true`. `tests/network_write_jail.rs` asserts the **agreement**
> between the two builtins rather than the refusal, so if the jail policy is ever
> loosened the test follows it instead of fossilising today's answer. Verified
> red by removing the guards: three of six tests failed, and the three that
> should not have.
>
> **`web_upload_file` was deliberately left alone**, and that is pinned as a
> decision rather than left as an oversight. The jail covers `WriteLocal` and
> `Destructive`; `ReadLocal` is unjailed by design, and `file.read` of an outside
> path followed by `web.post` is already an allowed pair — so jailing the upload
> would be a new policy invented at a call site. If reads are ever jailed,
> `is_filesystem()` is the one place to change, and the test says so.
>
> **`reject_option_like` moved into `guard_network`** rather than being added at
> fourteen call sites. `curl -K<file>` reads a config file that can set `output`
> and `url`, so an option-like "URL" turns a fetch into an arbitrary write; no
> legitimate URL starts with `-`. Putting it at the one door every network
> builtin already goes through means a site added later inherits it — the
> opposite of the partial-application shape that produced most of this session's
> findings.
>
> **Suite: 1964 passed, 0 failed across 119 binaries**, clippy `-D warnings` and
> fmt exit 0.
>
> **CI note.** The push of `ee5aa54..c541f6d` landed on `origin/master` and
> triggered **no workflow run at all** — `check-runs` stayed at 0 and the newest
> run remained `c1833a6`. This is the failure the `workflow_dispatch` trigger was
> added for on 2026-08-06 ("pushes landed but triggered no runs"), so it was
> dispatched by hand rather than treated as a green-by-absence. **A commit with
> no checks is not a passing commit**; verify by SHA.
>
> **Closing pass. Every engineering item in §5 that can be closed from this seat
> now is, and each is pinned by a ratchet rather than by prose.**
>
> **1. The label understated the effect — six more, and the rule that finds them.**
> `tests/effect_ratchet.rs` compares evidence against the label, but it only
> fires when the label is `Pure`: "acts while claiming to do nothing". A builtin
> that claims to do *something*, just not the thing it does, was invisible to it.
> Six were:
>
> | builtin | was | is | what the body does |
> |---|---|---|---|
> | `file_edit` | ReadLocal | WriteLocal | `fs::write` + `fs::rename` over the caller's path |
> | `file_insert` | ReadLocal | WriteLocal | same |
> | `file_patch` | ReadLocal | WriteLocal | same |
> | `file_backup` | ReadLocal | WriteLocal | `fs::copy` onto `<path><suffix>` |
> | `session_export` | ReadLocal | WriteLocal | `fs::write` to a caller-named path |
> | `file_move` | ReadLocal | **Destructive** | `fs::rename` — removes the source, can overwrite the destination |
>
> The jail keys on exactly this label: `safety::guard` applies the workspace check
> only when `effect.is_filesystem()`, i.e. `WriteLocal | Destructive`. So in agent
> mode `file.write` to a path outside the workspace was refused while
> `file.patch` on the *same path* rewrote it. Alias inheritance carried the fix to
> `edit_file`, `insert_lines`, `patch_file`, `text_edit`, `backup_file`,
> `move_file`, `mv`, `file_rename` — thirteen snapshot lines, all reviewed.
>
> **`mv` and `move_file` are now `Destructive`, which means agent mode asks for
> approval before a move.** That is a real change in what an agent may do without
> asking. It is the honest classification — `file_delete_lines` is already
> `Destructive` for altering a file in place, and moving one over another is not
> the smaller act — but it is a behaviour change, not just a relabelling.
>
> `tests/write_evidence.rs` is the durable part: **a body that writes must carry a
> label the jail keys on.** Stated against `is_filesystem()` rather than a
> severity ordering, because the jail is the control and the control's own
> predicate is the thing to assert.
>
> **2. Option injection, generalised and bounded.** `reject_option_like` went from
> 11 uses to 43, and — more usefully — the check moved *into* `guard_network`, so
> all fourteen `curl`/`wget` sites and any future one inherit it at the one door
> they already pass through. `tests/option_injection_ratchet.rs` covers the tools
> where a leading `-` buys **code execution** (`git --upload-pack`,
> `ssh -oProxyCommand`, `tar --use-compress-program`, `zip -TT`, `curl -K`,
> `openssl -engine`, `find -exec`) and deliberately not the other ~330 value-
> carrying sites, where a leading `-` is a bad argument rather than a foothold. A
> rule that flagged all 372 would be waived, not obeyed.
>
> Twelve allowlist entries, each a **decision**, not a waiver: a slot that is an
> option by contract (`git reset --hard`, `find -size -1M`), or one where the
> value is consumed as another option's argument and may legitimately start with
> `-` (a commit message, a password, an HMAC key, a `find -name` pattern).
>
> **3. `db_sqlite_create_table`'s column type — §5's one named "still open".**
> `safety::sql_column_type` is a token allowlist, not a parser: a type name with
> an optional `(N)`/`(N,M)` size, a short constraint vocabulary, numeric literals,
> and single-quoted literals with no interior quote. `CHECK(…)` and `REFERENCES`
> are refused with an error that points at the raw column-definition branch. The
> builtin is still unregistered, so nothing here was exploitable — which is why it
> kept being deferred. What the fix removes is a **precondition**: registering it
> used to require a security decision nobody had taken.
>
> **4. A latent bug in `tests/guard_enforcement.rs`, and the hole it was hiding.**
> Its body extractor skipped string literals but not char literals, so `'"'` read
> as opening a string and `'}'` as closing the function, and bodies ran on into
> their neighbours. With that fixed, `SELF_GUARDED` turned out to list
> **`platform_os_version` and `sys_info`, neither of which calls any guard** —
> they had only ever looked self-guarding because the extractor swallowed a
> neighbour's. `SELF_GUARDED` makes `guard_dispatch` return immediately, so a
> stale entry is an exemption. Both are `ReadLocal`, which is not centrally
> enforced, so **nothing was live** — it was a trap armed for whoever reclassified
> them upward. Both removed.
>
> The same bug bit the new scanner first, and that is the part worth carrying:
> `tests/write_evidence.rs` initially reported **nine** offenders, seven of them
> builtins containing no write at all (`sys_info`, `agent`, `project_name`,
> `project_version`, `platform_os_version`, `platform_machine_id`, `vm_info`) —
> extracted "bodies" of 250KB, 490KB and 1.6MB. The tempting fix was to allowlist
> all seven, which would have recorded a scanner bug as a set of deliberate
> exceptions. **The two real findings were sitting in the same list.**
>
> **Suite: 1981 passed, 0 failed across 121 binaries**, clippy `-D warnings` and
> fmt exit 0.
>
> **What is left, and it is not engineering.** §5 item 1 (crates.io token
> rotation) and §5 item 2 (external security review and penetration test) both
> need a human. §5 item 3 (registering the 168) is a product decision, and its one
> engineering blocker is now removed. §5.4's remaining roadmap entries are each
> closed or explicitly decided against with a test pinning the decision.
>
> **`mkdir` did nothing. It had never done anything.**
>
> `mkdir`, `mkdirp` and `file_mkdir` were all registered at `BUILTIN_DISPATCH`
> index 532. The comment above the reserved run read `533-539`; the first
> placeholder was at **532**. So all three resolved to `|_, _, _|
> Ok(Value::Null)` — returning a success value and creating nothing — while
> `bi_file_mkdir` sat in `builtins.rs` fully written, correctly classified
> `WriteLocal`, and referenced by nothing at all.
>
> A silent no-op is worse than a missing builtin: an unknown name fails loudly
> and the caller tries something else, while this one hands back success.
> `tests/catalog_reachability.rs` could not see it — it asks whether an advertised
> name *dispatches*, and this one did. The question nobody had asked is whether it
> dispatches to anything. `tests/no_stub_dispatch.rs` now asks it.
>
> It was found sideways: a workspace-jail test expected `mkdir` outside the
> workspace to be *refused* and got `Ok(Null)`. Nothing was looking for it.
>
> **The `WriteLocal` jail reaches 10 of 119 builtins, and a central fix was tried
> and reverted.** §5.3 promises `WriteLocal` is "jailed to workspace". The jail
> lives inside `safety::guard`; `guard_dispatch` returns early for any effect that
> is not centrally enforced (`Process | Destructive | Exec | Privileged`), so
> `guard` is never reached. A `WriteLocal` builtin is jailed only if it guards
> itself. Eight did. **Demonstrated:** `copy_file` overwrote a file outside the
> workspace — `success: true`, 8 bytes — that `file_write` was refused for.
>
> The obvious fix, running the jail over `existing_paths(&args)` in
> `guard_dispatch` exactly as the enforced branch does, was implemented, tested,
> and **reverted after measuring it**: it refuses `copy_file <outside-source>
> <inside-destination>`, which is copying a file *into* the workspace. Reading
> from outside is allowed by policy and the write lands inside the jail, so that
> is a false positive with no workaround — the exact failure the existing comment
> in `guard_dispatch` warns about. The two cases are indistinguishable from a
> central point: both are "a `WriteLocal` call naming an existing path outside the
> workspace". Only the call site knows which argument is the destination.
>
> So the jail stays at call sites. `file_copy`, `file_mkdir`, `tar_extract` and
> `zip_extract` gained one (destination only), taking it from 8 to 10, and
> `tests/writelocal_jail.rs` **measures the remainder** rather than assuming the
> call sites cover it. The reverted attempt is written into the code comment so
> the next person does not spend the afternoon rediscovering why it does not work.
>
> **A negative result: archive path traversal does not reproduce.** `tar_extract`
> and `zip_extract` looked like textbook zip-slip — the destination was unjailed
> and archive entries control paths. Measured on this machine with an archive
> containing `../escaped.txt`:
>
> - GNU tar 1.35 strips `../` from member names on creation and refuses it on
>   extraction;
> - Windows bsdtar refuses: `Path contains '..': Unknown error`, and nothing was
>   written outside the destination;
> - `Expand-Archive` refuses: `Can not process invalid archive entry
>   '../escaped_zip.txt'`.
>
> **No traversal.** The destination guard was added anyway, because an unjailed
> `-C` target is a real gap independent of what the archive contains.
>
> **The scanner was wrong, and that is the part to read.**
> `tests/no_stub_dispatch.rs`'s first parser counted only dispatch rows beginning
> with `|`. One row is a bare function reference:
>
> ```rust
>     bi_try_repair,                                  // 1140
> ```
>
> Every index after it came out one too low. The report said the last row was out
> of range and that **nine correct registrations** — `plan_diff`, `rm`, `rmdir`,
> `touch`, `cd` and the four `rbac_*` — each called its neighbour's
> implementation. That was one edit away from being "fixed", which would have
> broken `rm`, `rmdir`, `touch`, `cd` and the whole RBAC session family at once.
>
> What stopped it: `rm` calling `bi_rmdir` would have failed
> `tests/filesystem_removal.rs` on its first run, and it does not fail. **A
> scanner's output is a claim, and a claim that contradicts a passing test is the
> scanner's problem first.** The row count is now pinned against known-good
> indices at both ends of the table, so a missed row shape fails immediately
> instead of producing a plausible list of victims.
>
> That is twice in two passes — seven phantom "writers" in
> `tests/write_evidence.rs`, nine phantom "misregistrations" here. Both times the
> false report was longer than the true one, and both times the tempting fix would
> have made things worse. **When a new scanner's first run produces a long list,
> the list is evidence about the scanner.**
>
> **Suite: 1992 passed, 0 failed across 123 binaries**, clippy `-D warnings` and
> fmt exit 0.
>
> **The dead-code question, settled: 112 implementations deleted, 8 names wired.**
> §5 item 3 was "registering the 168 unadvertised `bi_*` is a product decision".
> The decision was made — *wire it if it should exist, delete it if it shouldn't*
> — and this is what that came to.
>
> **The criterion.** A family that is already *partly* registered is incomplete
> without its dead members, so those get wired. A family with **zero** registered
> members is speculative code nothing advertises, so it goes. Then two overrides,
> both of which delete regardless of family:
>
> * **duplicates of a registered spelling** — `input_multi_select` beside the
>   registered `input_multiselect`, `db_memory_get/set/store` beside `db_kv_*`,
>   `file_checksum` beside `crypto_hash_file`, `curl_exec` beside
>   `web_rest_api`, `crypto_key_generate` beside `crypto_generate_key`;
> * **stubs that return an excuse as a success value** — `input_hotkey` and
>   `input_history` each return a *string explaining they do not work*, and
>   `input_available`/`input_clear_buffer` return `Bool(true)` unconditionally.
>   That is the `mkdir` failure mode with a different spelling, and the fix for a
>   builtin that lies about succeeding is to not have it.
>
> **Wired (8 names, 7 rows):** `db_csv_query` + `db_csv_to_json` (one index — the
> second is an alias), `db_json_query`, `db_json_to_csv`, `clipboard_types`,
> `crypto_random_string`, `input_number`, `input_timeout`. Rows were **appended,
> never inserted**: a row added mid-table shifts every index after it and
> silently re-points every registration above — which is exactly how `mkdir` came
> to sit on a placeholder. The comment in the table now says so.
>
> **`builtins.rs` is 3,329 lines shorter.**
>
> ### Three corrections during the pass, each caught by something other than care
>
> **1. The first reachability model was wrong, and the compiler said so.** It
> asked "is this function named in the dispatch table?", which misses a function
> reached *through* a registered wrapper. Five were: `crypto_key_generate`,
> `db_sqlite_create`, `db_sqlite_export_csv`, `db_sqlite_import_csv` and
> `input_multi_select` are all live code behind registered names. The first build
> after deleting them failed with five `cannot find function` errors. Redone as a
> transitive walk from the dispatch roots: **120 truly dead**, not 125.
>
> This is the third scanner this session whose first answer was wrong. The
> difference here is that the compiler is a checker the scanner cannot argue
> with — which is the argument for deleting code and building, rather than
> reasoning about whether it is used.
>
> **2. A deleted function carried the only test of the FIPS gate.**
> `bi_file_checksum` was unreachable and duplicated `crypto_hash_file` — and it
> was what `file_checksum_honors_the_fips_gate` called, the only coverage
> `safety::require_fips_hash` has, and that gate is what the compliance claim
> rests on. The test now calls `bi_crypto_hash_file` instead (argument order is
> reversed between them). It is better coverage than before: it exercises the
> spelling a caller can actually reach.
>
> **3. Deleting dead code exposed three more stale `SELF_GUARDED` entries.**
> `curl_exec`, `lxc_exec` and `nohup_run` were listed as guarding themselves,
> with no implementation left to guard anything. With the two found earlier
> (`platform_os_version`, `sys_info`) that is **five stale exemptions** from
> `guard_dispatch` — an entry there makes the dispatcher return immediately, so a
> stale one is a standing exemption for a name nobody is checking.
>
> All three ratchets now carry the generalisation:
> **an allowlist entry naming a function that no longer exists fails the build.**
> A list that may only shrink has to actually shrink when the thing it describes
> is deleted, or it accumulates exceptions for nothing.
>
> ### Recorded rather than tidied away
>
> Three of the `reject_option_like` guards added earlier this session went to
> `bi_sftp_list`, `bi_sftp_get` and `bi_sftp_put` — a wholly unregistered family,
> now deleted. **About a tenth of the option-injection hardening was applied to
> code no caller could reach.** The guards were not wrong; the survey that chose
> where to put them asked "does this spawn a risky tool?" and never asked "can
> anyone call this?". Worth carrying: *reachability is a precondition for
> severity, and it is cheap to check first.*
>
> ### §5 item 1
>
> Closed at the repository owner's direction (2026-08-26). Not carried forward.


> **Session 4 (2026-08-25): the shell was healthy, everything was pushed, and CI
> is green by SHA for the first time in three sessions.**
>
> - **§7.3 item 1 — done.** `756e200..afd6e8f` pushed. Queried by SHA:
>   **17/17 checks green on `afd6e8f`, `Lint` among them.** The job that had been
>   red on `master` since `756e200` is fixed — confirmed against CI, not against
>   a local proxy for it.
> - **§7.3 item 2 — done**, as its own commit `a16b3a2`
>   `fix(eval): a non-array pipeline source ran twice, and the second run was a side effect`.
>   Pushed. The test counts the *effect*, not the value, because the value never
>   differed: confirmed red without the fix (log reads `"xx"`) and green with it
>   (`"x"`). A second test measures the streaming path against the eager
>   evaluator rather than against a constant.
> - **§7.3 item 3 — not started**, deliberately. Widening `is_effect_free` past
>   `BUILTIN_LOOKUP` is fail-safe as it stands: a missed optimisation, not a bug.
> - **§5 item 1 — still open, and still the urgent one.** The disclosed crates.io
>   tokens have not been rotated. That needs a human.
>
> **Measured this session, not inspected:** full workspace suite **1980 passed,
> 0 failed across 113 suites**, exit 0; doctests 7 ok / 1 ignored / 0 failed;
> `cargo clippy --workspace --all-targets --features native -- -D warnings`
> exit 0; `cargo fmt --all -- --check` exit 0.
>
> **New environment finding — see §7.1.** The first full-suite attempt died with
> `E0786 invalid metadata files for crate aethershell` and a run of rustc ICEs
> (`STATUS_STACK_BUFFER_OVERRUN`). It was **not** a code failure: ~17 stray
> `cargo.exe`/`rustc.exe` from *other* repositories (irongate, ami-server, iv)
> were building concurrently under other sessions. `cargo clean -p aethershell`
> (56 GiB reclaimed) plus a re-run passed cleanly. §6's "do not run two cargo
> invocations at once" extends **across repositories**, not just within one — and
> the symptom impersonates a compiler bug, so check `tasklist` before believing it.

> **Session 3 (2026-08-25): the shell came back and both open work items landed.**
> Sections 3 and 4 below are kept for the reasoning trail, but their *tasks* are
> finished. Two commits on `master`, neither pushed:
>
> - `0c4f6e2` `fix(a2a): the red Lint was one drain(..).collect(), invisible locally`
> - `afd6e8f` `feat(eval): take(n) stops the source — laziness that saves work, not just memory`
>
> **§3 answered in full.** The red Lint was `clippy::drain_collect` at
> `src/ai/a2a.rs:159` (exit 101, matching the CI annotation exactly). It was
> invisible locally because the local toolchain was one release behind CI's
> `stable` — clippy 0.1.97 is silent on it, 0.1.98 raises it. Fixed with
> `mem::take`; the exact CI invocation now exits 0.
>
> **§4 finished and verified, not merely inspected.** Every static prediction in
> the session-2 notes below held. Results: `tests/streaming.rs` 12/12; full
> workspace **1962 passed, 0 failed** across 113 suites; **16/16 doctests**;
> `cargo clippy --workspace --all-targets --features native -- -D warnings`
> clean; `cargo fmt --all -- --check` clean. Both `Remaining:` notes in
> `docs/AGENTIC_FIRST_DESIGN.md` (§6.3 bullet, phase 6) are updated.
>
> **Still not pushed.** CI has not seen either commit. Push and verify by SHA,
> the way §3 describes.
>
> **Read §7 before running anything.** The toolchain broke badly during this
> session and the machine is actively losing files under `~/.rustup`.

Written mid-task, because the session's shell died and the remaining work cannot
be verified from here. Read the **Unverified** section before trusting anything
in the working tree.

---

## 1. Why this document exists

Every `Bash`/`PowerShell` invocation in the session fails before the command runs:

```
EEXIST: file already exists, mkdir '…\c173091d-e3f6-4cc3-b5d6-1e33028ec907\tasks'
```

and that same path fails `stat` with `EPERM`. The session's task directory is in
a Windows *pending-delete* state — an earlier tool result reported that another
Claude Code process in this project deleted it during startup cleanup. Stopping
the session's background tasks did not clear it. It should resolve when the
other process exits or the session is restarted.

Consequence: **no compiling, no testing, no git** from the point the shell died.
File edits still work, so there is unverified work in the tree.

> **Session 2 update (2026-08-24): the blocker survived a session restart.**
> A new session (`fd3fc85e…`) still fails every `Bash`/`PowerShell` call with the
> same `EEXIST`, and the path it names is still the **old** session's
> (`c173091d…`) — so the shell subsystem is holding a stale task directory rather
> than creating its own. Writing into that directory with the `Write` tool fails
> `EPERM`, confirming the pending-delete state; it cannot be cleared from inside
> a session. **The other Claude Code process on this project must exit** (check
> Task Manager for stray `claude`/`node` processes) before a shell will work.
> Everything in §4's build-and-test list therefore remains undone. What session 2
> *could* do without a shell: answer §3 (see below — Lint is red) and statically
> verify the draft (see §4's new subsection).

---

## 2. State of the repository

| | |
|---|---|
| Branch | `master` |
| CI verified by SHA | `9bfd1b8` **17/17** (14 success, 3 always-skipped: CLA Check, Check Outdated Dependencies, Dependency Review) — this carries all of session 5's work including `142d60c` and `a006d40`. `44ab51c`, `2b91cf8` and `a16b3a2` were each 17/17 too |
| Unpushed | only the docs-only commit carrying this line. Every code change in session 5 is pushed and green by SHA |
| Version | 8.0.0 (published to crates.io; verified against the registry, not a green run) |
| Working tree | clean |

### Verified and pushed

Both commits below were formatted, clippy-clean, and passed the full local suite
(`cargo test --features native --no-fail-fast`) before pushing.

**`d286ae0` — `fix(modules): the whole rbac.* module pointed at builtins that do not exist`**

All seven aliases in `rbac_module()` named `rbac_*` builtins that no dispatcher
entry implements, so `rbac.create(...)` and `rbac.check(...)` answered
`unknown builtin` while the README documented them as working examples of the
access-control surface. The implementations existed the whole time under their
own names (`role_create`, `role_delete`, `role_grant`, `role_revoke`,
`check_permission`, `roles_list`, `user_roles`); the aliases now point at them.
`rbac.permissions` was withdrawn rather than allowlisted — nothing implements
it, and advertising a name is a promise. `tests/module_aliases.rs`'s debt list
shrank 71 → 64. Verified end to end: `rbac.check("alice","config.toml","write")`
returns `true` through the built binary.

**`756e200` — `feat(auth): an interactive login, and the privilege class that governed nothing`**

Four things, each measured before it was fixed.

1. **Interactive login** (the RBAC phase's remaining roadmap item).
   `auth::AuthManager` — registration, password verification, sessions, bearer
   tokens, API keys, its own audit trail — existed in full with *no caller
   anywhere in the crate*. The only route to a principal was
   `rbac_principal(id)`, which asserts an identity rather than proving one.
   Added `rbac_register` / `rbac_login` / `rbac_logout` / `rbac_session`
   (dispatch 1146–1149). Login verifies against the stored hash **before** it
   touches `set_principal`. With no password argument the pair prompt on the
   terminal and refuse to prompt when stdin is not one.

2. **A live privilege escalation.** `decide(Privileged, Agent)` is `Deny`, the
   strongest rule in the taxonomy, and *no builtin was classified `Privileged`
   at all* — the class read as coverage while governing nothing. Most
   privilege-shaped names (`sudo_exec`, `user_add`, `acl_set`, `fs_unmount`)
   are stubs performing no effect, so `Pure` is honest for them; measured, not
   assumed. `rbac_grant` and `rbac_principal` are not stubs, and an authorized
   principal *skips approval entirely*. `tests/privilege_escalation.rs` ran the
   sequence and was red: the agent granted itself `effect:*`, became that
   principal, and deleted a file it had been refused one call earlier.

3. **Unsalted password storage.** `register_user` stored `hash_key(password)` —
   bare SHA-256, right for a 256-bit random API key and wrong for a password.
   Now Argon2id with a per-password random salt (`auth::hash_password` /
   `verify_password`). New dependency: `argon2 0.5` (pulls `blake2`,
   `password-hash`). `cargo add` also re-resolved `socket2` 0.5→0.6 and some
   `windows-sys` versions in the lockfile as collateral.

4. **A username-enumeration timing oracle.** The unknown-user path returned
   before any hashing happened: **836 µs against 695 ms** for a wrong password.
   Closed with a decoy Argon2 verification; confirmed by deleting the decoy and
   watching the timing assertion go red.

Effect classifications now in `tests/effect_snapshot.txt`:
`rbac_grant` / `rbac_principal` / `rbac_login` / `rbac_register` → `Privileged`
(denied in agent mode); `rbac_logout` → `WriteLocal` (giving up authority cannot
escalate); `rbac_can` / `rbac_session` → `ReadLocal`. `rbac_principal`
self-guards so only its *setting* form is gated.

---

## 3. Loose end: CI on `756e200` — ANSWERED: Lint is red

Checked by SHA on 2026-08-24 (session 2) via the public checks API, since `gh`
was still unavailable:

```
GET /repos/nervosys/AetherShell/commits/756e200/check-runs
```

**`Lint` — `completed / failure`.** Everything else on that SHA is green:

| Check | Conclusion |
|---|---|
| Test (ubuntu / windows / macos-latest) | success (all three) |
| Build (x86_64-linux, x86_64-msvc, x86_64-darwin, aarch64-darwin) | success |
| WASM Build, Generate SBOM | success |
| Security Audit, Secret Scanning, Supply Chain Security, Security Summary | success |
| **Lint** | **failure** |
| Check Outdated Dependencies, CLA Check, Dependency Review | skipped |

So `756e200` is **behaviourally** sound — all five test jobs and every build
target passed on it. What failed is style/lint only.

The job's two annotations are all the detail the public API exposes:

- a Node 20 deprecation warning for `actions/checkout@v4` (noise, not the cause);
- `Process completed with exit code 101` — the Rust abort code, i.e. `cargo
  clippy -D warnings` (or a `cargo fmt --check` wrapper) rejected the tree.

`GET /actions/jobs/97603961189/logs` is **403 unauthenticated**, so the offending
lint cannot be named from here. Failing job:
<https://github.com/nervosys/AetherShell/actions/runs/32781371434/job/97603961189>

**Next session, first thing:** `cargo clippy --all-targets --features native`
locally and fix what it reports. Note this is a pre-existing failure on the
*pushed* commit — it is not caused by the uncommitted draft in §4, which CI has
never seen.

Local risk areas: the tests are Windows-CRLF sensitive in places,
and `tests/interactive_login.rs` has one **timing** assertion
(`an_unknown_username_costs_the_same_as_a_wrong_password`) that compares two
Argon2 verifications. Its band is loose (a ratio floor of 0.4 against a measured
830× gap), but it is the one test in the set that a heavily loaded runner could
in principle disturb.

---

## 4. Unverified work in the tree

`src/eval.rs` and `tests/streaming.rs` carry the next roadmap item — *"fully
lazy iterators end-to-end"*, the `Remaining:` note on the streaming bullet in
`docs/AGENTIC_FIRST_DESIGN.md` (§6.3) and on phase 6 of the table.

**It has never been compiled.** Treat it as a draft.

What it does:

- `take(n)` with a literal count is no longer a barrier that forces the eager
  fallback. It streams, and a satisfied `take` **abandons the source unread**,
  so `xs | map(f) | take(3)` calls `f` three times instead of once per element.
  The stage classifier is now `StageKind::{Elementwise, Prefix(n), Barrier}`.
- The early exit is withheld when any upstream stage reaches a builtin that is
  not `Pure` — walked by `is_effect_free`, which uses `safety::effect_of`, the
  classifier `tests/effect_ratchet.rs` holds to *"no builtin that acts is
  classified `Pure`"*. Skipping work the program asked for is not an
  optimisation. Such pipelines still stream; they just read to the end.
- New `StreamStats { emitted, pulled, streamed, short_circuited }` and
  `eval_stream_with_stats`. `pulled` exists because from the outside a lazy
  pipeline and a materialising one produce identical values in identical order —
  it is the only number that tells them apart. `eval_stream` delegates and keeps
  its signature, so `agent_api.rs`'s SSE route is untouched.
- Six new tests in `tests/streaming.rs` asserting on `pulled`, including a
  control (no `take` → every element pulled, so the main test cannot pass for
  the wrong reason), an effectful-stage case that must *not* short-circuit, and
  an equivalence check against the eager evaluator.

### To finish it

```bash
cargo build --features native            # it has never compiled; expect fixes
cargo test  --features native --test streaming
cargo fmt --all && cargo clippy --all-targets --features native
cargo test  --features native --no-fail-fast
```

Specific things to check, since none of them have been exercised:

1. `range(0, 1000)` — the tests assume it exists, returns an `Array`, and is
   half-open. If it is inclusive or absent, fix the tests, not the semantics.
2. `is_effect_free` requires the stage's callee (`map`/`where`/`filter`/`take`)
   to be in `BUILTIN_LOOKUP` **and** `effect_of` it to be `Pure`. If `map` is
   classified otherwise, no pipeline will ever short-circuit and
   `take_stops_pulling_the_source` will fail with `pulled == 1000`. That failure
   means the guard is working and the classification needs looking at — not that
   the guard should be loosened.
3. The pulled-count expectations encode "a satisfied take stops the source
   *before* the next element is read": `take(0)` → `pulled == 0`,
   `xs | take(2) | map(...)` → `pulled == 2`. If you change that ordering,
   change the numbers with it.
4. Confirm the existing `eval_stream_falls_back_for_whole_collection_stage` and
   the `/api/v1/stream/eval` route still behave — `sort` must still fall back.

### Static verification done in session 2 (still not compiled)

The shell never came back, so none of the commands above have been run. What
*could* be done without one — reading the draft against the code it calls — was,
and **all four listed assumptions hold**. This lowers the risk but does not
discharge it: nothing here substitutes for `cargo build`.

1. **`range` — confirmed exactly as assumed.** `builtins.rs:11564-11594`:
   `range(n)` → `0..n`, `range(start, end)` → `start..end`, both **half-open**
   (`while current < end`), pushing `Value::Int` into a `Value::Array`. So
   `range(0, 1000)` is 1000 elements, `0..=999`. The tests need no change.
2. **`map` is registered and `Pure` — the predicted failure will not happen.**
   `map`/`where`/`take`/`sort` are in `BUILTIN_LOOKUP` (`builtins.rs:65-74`).
   None is named in `safety.rs`, so `classified_effect` returns `None` and
   `effect_of` falls through to `Pure` — which is what `is_effect_free` needs, so
   `take_stops_pulling_the_source` should **not** fail with `pulled == 1000`.
   Note *how* it holds: by `effect_of`'s `unwrap_or(Effect::Pure)` default, not
   by an affirmative classification. That default is exactly what
   `tests/effect_ratchet.rs` guards — it keys off `effect_of` (line 565), not
   `classified_effect`, so a registered builtin that acts while merely
   *defaulting* to `Pure` still fails the ratchet. The draft's doc comment is
   accurate. (`filter` is classified `Elementwise` by `stage_kind` but is **not**
   in `BUILTIN_LOOKUP`; a `filter` stage would fail at eval as an unknown builtin
   either way, so this is cosmetic — but the name is in the classifier promising
   something the dispatcher does not serve.)
3. **The pulled-counts are self-consistent.** Hand-traced all six new tests
   against `try_stream_pipeline`; each reaches its asserted `pulled`,
   `emitted`, `streamed` and `short_circuited`. `take(0)` breaks on the first
   iteration *before* `pulled += 1` → `pulled == 0`; `xs | take(2) | map(...)` →
   `pulled == 2`. The effectful case resolves correctly: `file_exists` hits the
   `n.starts_with("file_")` arm at `safety.rs:887` → `ReadLocal`, so the test's
   own `assert_ne!(effect_of("file_exists"), Pure)` guard passes,
   `is_effect_free` rejects the stage, `may_abandon` is false, and all 20
   elements are pulled while `take` still caps emission at 2.
4. **`sort` still falls back.** `stage_kind` returns `Barrier` for it, and the
   `Barrier` check (`eval.rs:258`) runs *before* the source is evaluated, so
   `eval_stream_falls_back_for_whole_collection_stage` is unaffected.

Compile surface also checked by hand: every `Expr` variant and field name the
draft matches on exists in `ast.rs` (`Lambda{body}`, `Binary{left,right}`,
`Unary{expr}`, `MemberAccess{object}`, `Call{callee,args,named}`, `Pipe`),
`Stmt::Expr(Expr)` exists, `Effect` derives `PartialEq`, `BUILTIN_LOOKUP` is a
`HashMap<&'static str, usize>` so `contains_key(name.as_str())` is fine,
`env.input()`/`set_input` have the assumed signatures, and the test file's
`eval_program(&stmts, &mut env) -> Result<Value>` matches `eval.rs:46`. No
type error was found by inspection — which is not the same as there being none.

Two things inspection flagged that the draft does not mention:

- **A non-array source is evaluated twice.** `try_stream_pipeline` evaluates the
  source (`eval_expr(cur, env)`, `eval.rs:262`) and only *then* returns
  `Ok(None)` if it is not a `Value::Array`; the caller's fallback
  (`eval_stmt(last, env)`) re-evaluates the whole final statement, source
  included. So `read_file("x") | map(f)` reads the file twice, and an effectful
  scalar source duplicates its effect — reachable from the `/api/v1/stream/eval`
  route. The `Barrier` path does not have this problem (it returns before
  evaluating anything). This looks **pre-existing** rather than introduced by the
  draft — the eager-fallback shape predates it — but it should be confirmed
  against `git diff` and fixed by hoisting the evaluated source into the
  fallback instead of recomputing it. Deliberately **not** fixed here: it cannot
  be compiled, and stacking a second unverified edit on top of the first is how
  green-but-wrong happens.
- `stages.iter().map(|s| stage_kind(s))` and `.all(|s| is_effect_free(s))` pass
  `&&Expr` to a `&Expr` parameter. It compiles by deref coercion, but
  `clippy::redundant_closure` may fire on the shorthand — worth watching given
  Lint is already red (§3).

Then update the two `Remaining:` notes in `docs/AGENTIC_FIRST_DESIGN.md` (the
streaming bullet in §6.3 and phase 6 in the table) and commit. Suggested
subject: `feat(eval): take(n) stops the source — laziness that saves work, not just memory`.

If you would rather not carry a draft, `git checkout -- src/eval.rs tests/streaming.rs`
loses only this item; nothing else depends on it.

---

## 5. Open items carried forward

Items 1-3 and 5 predate the 2026-08-26 sessions; item 4 was raised by the audit
on that date. None is blocked on the shell.

1. **~~Token rotation~~ — CLOSED at the repository owner's direction
   (2026-08-26).** Not carried forward.

2. **External security review and penetration test — the one item still
   open, and the only one that needs someone outside this repository.**
   These are the acceptance criteria for the CRITICAL items in the tracker,
   so those counts are deliberately left unflipped.

   **What to hand a reviewer.** The session notes above are the scoping
   document: every finding is recorded with the method that found it, the
   measurement that confirmed it, and the ratchet that now pins it —
   including the two that did **not** reproduce, which are the ones most
   worth re-testing independently.

   **Where an outside eye is most likely to pay.** Every finding here came
   from one question — *where does a value reach something that parses it?*
   That is a productive lens and a narrow one. It says nothing about
   time-of-check/time-of-use, the approval-token lifecycle, the audit
   chain's resistance to a hostile writer, concurrency between the jail
   check and the write, or whether the effect taxonomy is the right model
   at all. **The boundary of the method is the best guess at where the
   remaining bugs are.**

3. **~~168 unregistered `bi_*` implementations~~ — DECIDED and DONE
   (2026-08-26).** The instruction was *wire it if it should exist, delete it
   if it shouldn't*. Measured by transitive reachability from the dispatch
   table rather than by name: **120 were truly dead**. Of those, **8 names**
   (7 rows) were wired — the members of families that were already partly
   registered and were neither duplicates nor stubs — and **112 were
   deleted**. `builtins.rs` is 3,329 lines shorter. See the session note
   above for the criterion and for the three corrections the pass needed.

4. **Seven open findings from the 2026-08-26 audit** (`docs/security/SECURITY_AUDIT_2026-08-26.md`), mapped to CWE, MITRE
   ATT&CK, NIST FIPS and CMMC 2.0. None is fixed; all are recorded with the
   evidence grade that was actually reached.

   | ID | Severity | Evidence | Summary |
   |---|---|---|---|
   | AS-2026-01 | High | **Demonstrated** | `approve()` is self-callable, so in agent mode the `approve` policy class is `allow` against an adversarial or injected agent |
   | AS-2026-02 | High | Structural | the audit log sits inside the jail it audits and its chain is unkeyed |
   | AS-2026-03 | Medium | Structural | passwords reach `openssl` on the command line (3 sites) |
   | AS-2026-04 | Medium | Structural | `crypto_encrypt` is AES-256-CBC with no integrity |
   | AS-2026-05 | Medium | Measured | `crypto_uuid`'s fallback is a v4-labelled clock, zero bits of randomness |
   | AS-2026-06 | Low | Measured | modulo bias in `crypto_random_string`, 0.0045 bits/char |
   | AS-2026-07 | Info | Verified | the FIPS gate covers hashes only; `FIPS_140-2_COMPLIANCE.md` names a superseded standard |

   **AS-2026-01 and AS-2026-02 are design decisions, not patches.** Both turn
   on which threat model is in scope — an agent that *errs* versus one that is
   *adversarial or prompt-injected*. The options are written up in the audit;
   choosing between them is the owner's call, and it changes the fix.

   Dependency position at the same commit: **zero vulnerabilities**, nine
   informational RUSTSEC advisories.

5. **Roadmap, remaining.**
   - ~~Fully lazy iterators end-to-end~~ — **done as far as it goes** (session 5,
     `142d60c`). A whole-collection stage now *ends* the streamable region
     instead of disqualifying the pipeline, so `xs | map(f) | take(3) | sort`
     pulls 3 rather than 1000. What is left is not "unfinished", it is where the
     idea stops paying: a pipeline whose **first** stage is a barrier has nothing
     to stream ahead of it and a buffer to pay, and a non-array source has
     nothing to stream by definition. Both deliberately still take the eager
     path, and tests pin that.
   - ~~Transpiler retirement to a shim~~ — **this bullet was stale, and session 5
     repeated it before checking.** It named `expand_lambdas`/`expand_pipelines`
     as the blocker. Those functions do not exist: Phase 5 deleted all 14
     `expand_*`/`preprocess_ultra` functions when it replaced the 10-pass
     pipeline with a single left-to-right `scan`, and §4.3 of the design doc has
     recorded that as **COMPLETE** since. The phase-5 table row said "in
     progress, 2 passes retired" the whole time, and its other listed remainder —
     boundary type-checking (§8) — is marked ✅ done there too.

     What is genuinely left is the *aspiration* in §4.3: that the transpiler
     become a thin shim emitting canonical AST. §4.3 itself recommends deferring
     it, measure-first — "high-risk, large-effort, low-value on a deprecated
     surface" — and that verdict still holds. `tests/grammar_vs_transpiler.rs`
     now pins the division mechanically so this cannot go stale again: which
     forms the grammar has adopted, which are still transpiler-only, and that
     none of the deleted passes has come back.

     **The one thing worth knowing before anyone tries it:** the test asserts on
     *meaning*, not parseability, because `>` is the awkward case.
     `[1,2,3] > len()` parses fine — as a greater-than comparison. It is a valid
     program that means something different on each surface, which is worse than
     a syntax error and invisible to a parse check. Deleting the transpiler
     without a grammar production for `>`-as-pipe would silently change what
     existing `.aeg` means, rather than failing loudly.
   - ~~Optionally bridge the older `RBAC_ROLES` registry into `RbacManager`~~ —
     **decided against, deliberately** (session 5, `a006d40`), and the decision
     is now pinned by `tests/rbac_legacy_store.rs`. The two stores are not two
     copies of one model: the older one holds `(resource, actions)` pairs and
     answers `rbac.check`, the newer holds capability strings and is the only one
     `guard` reads. The separation is load-bearing — `rbac_grant` is `Privileged`
     because it writes where the guard reads, while `role_grant` is unclassified
     and allowed *because it confers nothing*. **Bridging without classifying
     `role_*` first would give the agent surface an ungated spelling of a denied
     operation**, the same shape as the alias bypass in `194a496`. If it is ever
     done, classify `role_create`/`role_grant`/`role_revoke` in the same commit.

---

## 6. How this repository expects to be worked on

Carried from the project's own notes, because it explains the shape of
everything above:

- **Measure early and often, trust nothing.** The failure mode here is
  green-but-wrong, and a blind check reports zero. Before claiming a fix, make
  the test fail without it — the timing oracle, the privilege escalation and the
  `rm` bug were all confirmed that way.
- **Verify against the thing, not the report.** Check crates.io for a
  publication, query CI *by SHA*, read the parsed YAML rather than the file you
  wrote, and run the built binary — not only the library tests.
- **The catalog is a promise.** A name that is advertised and not served is
  worse than one that is missing from both: an agent reads the catalog and
  believes it. `tests/catalog_reachability.rs`, `tests/module_aliases.rs` and
  `tests/effect_snapshot.txt` exist to keep that gap at zero, and their
  allowlists should only ever shrink.
- **Disk is chronically full** (~70–100 GB free of 3.7 TB). A full `cargo test`
  has exhausted it before and produced a bogus `E0463`; purging `.pdb` files
  reclaims space. Do not run two `cargo` invocations at once — a concurrent
  build deleting an `rlib` mid-run produces
  `extern location for aethershell does not exist`, which looks like a code
  failure and is not.

---

## 7. Session 3 findings — read before running anything

### 7.1 The build environment is actively hostile (new, and the biggest risk)

`rustup update stable` — run to close §3, since a local clippy is only a CI
proxy when it matches CI's release — **failed mid-flight and destroyed the
toolchain.** The recovery took most of the session. What is now known:

- The update died on `Access is denied` deleting a temp file, leaving the
  toolchain with no manifest. After that every `cargo`/`rustc` call is a rustup
  proxy blocking on the install lock; **47 accumulated and deadlocked against
  the installer.** Killing them costs the half-written components.
- Each subsequent `rustup` operation's *rollback strips a different component*,
  so repair attempts oscillate: cargo present/rustc gone, then the reverse.
  **Do not stub missing `.pdb` files** to get past `detected conflict` — that
  corrupts the manifest further and cost two extra cycles here.
- **What actually worked:** kill every cargo/rustc/rustup process →
  `Remove-Item -Recurse -Force` the toolchain dir *and* `~/.rustup/downloads`
  *and* `~/.rustup/update-hashes` → one
  `rustup toolchain install stable --profile minimal --force` → then add
  `clippy` and `rustfmt`.
- **Files under `~/.rustup` disappear on their own.** `.partial` downloads
  vanish between write and rename (`os error 2`), and `ls ~/.rustup/toolchains`
  returns *different sets on consecutive calls*. The whole `stable` directory
  read as absent immediately after 1962 tests had run with it, while
  `rustdoc --version` answered normally. **Never conclude a toolchain is gone
  from a directory listing — re-check with the binary.** This is the same
  disappearing-file signature as the pending-delete shell blocker in §1, which
  suggests one cause, most likely an AV/cleanup agent. **A Defender exclusion
  for `%USERPROFILE%\.rustup` and `%USERPROFILE%\.cargo` is the fix to try.**
- Current state: cargo/rustc/clippy/rustfmt/rustdoc all work at **1.98.0**.
  `rust-docs` never installed (irrelevant to build/test/lint). Several
  previously-installed toolchains (`1.75`–`1.90`, `nightly`, `esp`) could not be
  confirmed present at session end — but given the unreliable enumeration, **do
  not assume they are gone without checking**.

### 7.2 Two verification traps hit this session

1. **Never pipe `cargo test` through `tail`/`head`.** `$?` becomes the *pager's*
   exit code. A run piped to `tail -120` reported "exit 0" and 91 tests while
   cargo's real status was hidden and the `aethershell` suites were silently
   discarded. Redirect to a log and echo `$?` on the next line. This is exactly
   the green-but-wrong shape this repo keeps warning about — it was caught only
   because 91 tests is visibly not 1650.
2. **`EXIT=127` mid-suite is disk, not mystery.** The full run took free space
   70 GB → 31 GB and died inside `ai_swarm.rs` at 774 tests. `target/debug` held
   **41.5 GB of `.pdb` across 976 files** — present even under
   `CARGO_PROFILE_DEV_DEBUG=0`, because earlier builds wrote them. Purging
   restored 70 GB and the identical re-run completed all 113 suites. Check `df`
   and purge `.pdb` before re-running.

### 7.3 Open, and worth doing next — items 1 and 2 are DONE (session 4)

1. ~~**Push the two commits and verify CI by SHA.**~~ **DONE (session 4):** pushed
   `756e200..afd6e8f`; 17/17 checks green on `afd6e8f`, `Lint` included. Original
   note follows. — Nothing has been pushed. The
   Lint fix is the one that matters — `master` has been red on it.
2. ~~**A non-array pipeline source is evaluated twice.**~~ **DONE (session 4):**
   fixed in `a16b3a2` exactly as prescribed below — the already-evaluated source
   is pushed through the stages as one batch instead of being discarded — with
   two tests that count the effect. Original note follows. Confirmed pre-existing, not
   introduced by the `take` work. `git show afd6e8f:src/eval.rs` has
   `let src = eval_expr(cur, env)?;` immediately followed by `_ => return
   Ok(None)`; the caller then eager-evaluates the whole statement. So
   `read_file("x") | map(f)` reads the file **twice**, reachable through
   `/api/v1/stream/eval`. A second side effect is a real bug, not a slow path.
   The fix is to stop discarding the already-evaluated source: push it through
   the stages as a single batch (exactly what the eager evaluator does for a
   pipe chain) and emit the result, instead of returning `Ok(None)`. It is
   compilable now — this was deferred only because the earlier session had no
   shell. **Do it as its own commit, with a test that counts the reads.**
3. ~~**`is_effect_free` gates on `BUILTIN_LOOKUP.contains_key`.**~~ **DONE
   (session 5).** The prerequisite this note asks for — a single source of truth
   in `src/` for the names the dispatcher serves — is `builtins::FALLBACK_BUILTINS`
   plus `is_dispatched`, kept honest by `tests/fallback_dispatch.rs` parsing the
   match arms rather than by anyone remembering. Building it also found that the
   same blind spot was load-bearing for *safety*, not just for this optimisation:
   see the session 5 note at the top. Original note follows.

   **`is_effect_free` gates on `BUILTIN_LOOKUP.contains_key`, which the
   comprehensive-match builtins fail.** Names dispatched through the fallback
   `match` in `builtins.rs` (`select`, `foreach`, `group_by`, `from_json`, …)
   are therefore treated as effectful, so a `take` downstream of them withholds
   its short-circuit. **Fail-safe — more work, never wrong work** — so it is a
   missed optimisation, not a bug. Worth widening only with a test that proves
   the wider set is still sound.

   **Session 4 looked into doing it and did not.** There is no enumeration of the
   fallback `match`'s arms anywhere in `src/`: the match is a few hundred literal
   arms ending in `_ => unknown_builtin(..)`, and the only list that exists is
   `SERVED_BY_FALLBACK` in `tests/catalog_reachability.rs` — hand-maintained,
   deliberately partial (only what the ontology advertises), and in `tests/`
   where `src/` cannot reach it. Widening therefore means either a second
   hand-maintained name list — the drift the "catalog is a promise" invariant
   exists to prevent — or treating unregistered names as `Pure`, which is
   unsound, since an unregistered name may be a user-defined function whose body
   this walk cannot see. **The prerequisite is a single source of truth for
   "names the dispatcher serves" in `src/`, not a change to `is_effect_free`.**
   Until then the fail-safe default is the right answer.
   (Settled along the way: bare `filter(...)` is in *neither* `BUILTIN_LOOKUP`
   nor the comprehensive match, so it is not callable at all —
   `modules.rs:522` serves only the `arr.filter` member form. The `"filter"`
   arm in `stage_kind` is inert, inherited verbatim from the old whitelist. The
   "catalog is a promise" invariant is **not** violated.)
