# AetherShell completion plan

Updated 2026-10-02. Target: **13.0.0**, selected for the stricter builtin contracts.

Completion means a validated, installable release with a declared contract for
every dispatchable builtin, honest unsupported-feature reporting, reproducible
performance evidence, and verified distribution artifacts. Future roadmap ideas
have their own milestones; this checklist governs the current release.

## Current checkpoint

| Area | Verified state | Remaining gate |
| --- | --- | --- |
| Signature migration | 583/1,171 implementations; 708/1,437 callable names | 588 implementations and 729 callable names |
| Latest targeted Windows validation | 731 unique tests across nine suites passed | Validate the complete final tree |
| Latest Windows workspace checkpoint | 2,653 passed across 186 suites; four ignored | Rerun after all remaining changes |
| Evaluator optimization | Implemented and measured; module throughput 3.65x, reduce 2.27x in the recorded Windows comparison | Final-tree performance and platform checks |
| Release preflight | Three Python tests; eight simulated checks on each of PowerShell 5.1 and 7 | Validate final metadata, credentials and actual publication |
| Package versions | Root and LSP remain 12.0.2 | Update the release metadata together |
| Publication | No 13.0.0 tag or publication performed in this preparation | Approved publication and independent verification |

Evidence: [release preparation](release/13.0.0.md),
[performance report](../benches/agentic/results/perf-2026-09-29-followup.md), and
`cargo run --example signature_inventory`. Performance figures are workload-
and host-specific; they exclude process startup and transport overhead.

The 2026-10-01 checkpoint, before the latest crypto migration, passed: Rust 1.88 workspace check with the root
lockfile, library check without default features, locked offline wasm32 check,
and the CI browser packaging command. The separate web lockfile was stale at
root version 1.6.0 and has been refreshed offline for the current manifest.
The generated browser package now includes a byte-identical repository LICENSE.
These are checkpoint results; stage 4 still requires validation after migration.
Non-native builds retain an existing unused Env-method warning.

The four ignored tests are recorded rather than counted as passes: HTTP example
03 requires network access; AI example 05 requires a configured provider; TUI/MCP
example 22 requires suitable servers; the security sanitization doctest is marked
ignore and contains illustrative pseudocode. Provider/server checks remain open.

## 1. Confirm release scope and unsupported features

- [x] Select 13.0.0 for the compatibility changes.
- [x] Preserve the implemented evaluator latency and throughput improvements.
- [x] Make a release-scope register: implemented, explicitly unsupported, or deferred, with evidence and an owner for each unresolved item.
- [ ] Review placeholder operations, including accounts, groups, ACLs, mounting, watching and elevated execution; approve which remain explicitly unsupported for 13.0.0.
- [x] Resolve the DQN training decision: implement and test real weight updates, or retain the explicit unsupported contract and document that limitation.
- [x] Separate future roadmap features from release blockers. Reconcile the stale version, metrics and open-issue sections in ROADMAP.md using current evidence.

**Gate:** every advertised capability has an implementation or an explicit,
accepted limitation. No placeholder is described as successful work.

### Release-scope register

Owners below are responsibility roles, not assignments to named people. All open
items remain release blockers unless explicitly listed as deferred.

| Area | 13.0.0 disposition | Evidence and remaining work | Owner |
| --- | --- | --- | --- |
| Evaluator optimization | Implemented; retain | Recorded comparison linked above; final-tree measurements pending | Performance maintainer |
| Strict builtin signatures | Partially implemented; blocker | Live inventory: 583 implementations covered, 588 pending; review bodies and aliases | Runtime maintainer |
| Native password hashing | Implemented | Salted, bounded PHC generation/verification; Windows tests passed, platform matrix pending | Runtime maintainer |
| Ownership, modes and filesystem mutation | Partially implemented; blocker | Strict contracts and owned-fixture tests; native Unix checks pending | Runtime and platform maintainers |
| Accounts, groups, ACL mutation, mount/unmount, watch/unwatch, elevated execution, scheduler mutation | Explicitly unsupported for this release | Source bodies and refusal contracts produce E_UNIMPLEMENTED; no successful mutation claimed | Runtime maintainer |
| DQN weight training | Explicitly unsupported for this release; real trainer deferred | src/rl.rs train computes a local Q-value without updating weights; rl_dqn_step refuses before the batch threshold mutates state; tests/dqn_contracts.rs witnesses refusal | RL maintainer |
| Other placeholder operations | Audit pending; blocker | Enumerate through remaining source-reviewed catalogue migration; do not infer support from registration | Runtime maintainer |
| Platform/features/packaging | Validation pending; blocker | Complete stages 4 and 6 on final commit, including MSRV and supported feature builds | Platform and release maintainers |
| Provider-backed E7 evidence | Validation pending; blocker | Requires configured test provider; record model, prompt and corpus versions | Evaluation maintainer |
| 13.0.0 publication | Pending final readiness and explicit publication authorization | Stage 7; no tag or publication performed | Release maintainer |
| Future product expansion | Deferred beyond 13.0.0 | Historical roadmap ideas do not turn unsupported implementations into working features | Feature maintainers |

Retaining the DQN refusal is the release implementation decision: a functioning
trainer needs a separate weight-update implementation and tests demonstrating
learning. Its existing API must not claim that training occurred. These scope
choices require accurate user-facing release limitations and do not waive the
remaining catalogue or platform gates.

## 2. Finish all builtin contracts — the critical path

- [x] Add declaration uniqueness, alias inheritance and repeatable coverage inventory.
- [x] Cover the E1/E2/E7 working set with declared contracts.
- [ ] Finish the 588 remaining implementations, grouped by module and actual behavior. Refresh the inventory after every tranche.
- [x] Migrate sudo_check: strict input rejection, native Unix effective UID, checked Windows subprocess status and boolean parsing, and coded failures; Windows checks passed, native Unix validation remains a platform gate.
- [ ] Read each body before authoring its declaration; record direct-call, pipeline, optional-argument and alias semantics without generating contracts from probes.
- [ ] Reject surplus arguments, wrong types, conflicting input and invalid options before effects or expensive work begin.
- [ ] Add executable examples for each declaration, witnessing optional parameters both absent and present. Use owned fixtures for mutations and explicit machine-dependent checks where necessary.
- [ ] Check each return shape and refusal code on all applicable platforms; require unsupported behavior to remain explicit.
- [ ] Complete containment, audit identity, effect classification, budget and transaction checks for effectful contracts. Refuse transaction use where rollback cannot restore the operation.
- [ ] Reach full coverage for both implementations and callable names, using the final inventory totals rather than assuming the current denominator is fixed.
- [ ] Remove interim undeclared-builtin assertions only after the inventory has no uncovered implementations or aliases.

**Gate:** zero uncovered implementations and callable names; declaration examples
and malformed-call regressions pass without broad skip lists masking failures.

## 3. Close correctness and security gaps

- [x] Fix legacy password hash/verification consistency across platforms, including fresh salts, supported formats and FIPS behavior; test success, wrong passwords and malformed hashes.
- [ ] Complete cron_list output validation: Windows currently accepts scalar JSON as an empty list and incomplete task objects as records; review Unix environment assignments, special schedules and repeated whitespace. The current scheduling tranche enforces call shape only for this already-declared query.
- [ ] Review remaining unchecked subprocess exits and false/zero/empty-result failure paths; lower the ratchet counts only with source review and meaningful tests.
- [ ] Verify that unsupported, absent-tool, failed-tool, invalid-input and policy refusals remain distinct, coded outcomes.
- [ ] Complete native Unix validation for ownership, modes, execute bits, symlink targets, dangling links and audit aliases. Validate Windows privilege and read-only behavior separately.
- [ ] Verify bounded reads, allocations, edit batches, output sizes and time limits for the remaining contracts.
- [ ] Verify atomic publication, owned temporary-file cleanup, preflight refusal without mutation, and transaction rollback for supported filesystem changes.
- [ ] Recheck agent policy enforcement, effect provenance, aliases, audit protection and network restrictions after the catalogue is fully migrated.

**Gate:** no open correctness or security defect can turn an unsuccessful operation
into apparent success, bypass its effect gate, or make a documented mutation unsafe.

## 4. Validate the complete release tree

- [ ] Run cargo fmt --all -- --check and git diff --check.
- [ ] Run cargo test --workspace --no-fail-fast on the complete final tree; preserve logs and account for every ignored test.
- [ ] Temporarily clear inherited LC_ALL for the known locale-sensitive Windows check, restore it afterward, and record the environment adjustment.
- [ ] Run cargo clippy --workspace --all-targets -- -D warnings.
- [ ] Build ae, aimodel and the LSP in supported release configurations.
- [ ] Validate the Rust 1.88 MSRV and the supported feature combinations, including optional local inference and WASM where advertised.
- [ ] Run Linux, macOS and Windows CI, including the declared supported architectures and packaging targets.
- [ ] Validate hardware, GUI, clipboard, service and other environment-dependent contracts on suitable hosts or disposable environments; document unavailable coverage.
- [ ] Run uncoded-failure, second-path, discarded-argument, silent-success and network-egress probes in isolated environments; verify their non-vacuity checks and final thresholds.
- [ ] Confirm CI gates execute the intended tests and fail on representative injected regressions.

**Gate:** all required jobs pass on the exact release commit. A historical pass,
a targeted suite or a started workflow does not satisfy this gate.

## 5. Recheck performance and agentic evidence

- [x] Record paired evaluator measurements and ownership/binding regression evidence.
- [ ] Rebuild baseline and final optimized executables, identify them by commit/hash, and rerun the paired performance probe after compilation is complete.
- [ ] Check module calls, map/filter/reduce, nested records and the recorded neutral or slower cases; investigate regressions before setting final acceptance thresholds.
- [ ] Measure CLI cold startup and Agent API latency/throughput separately if claiming improvements for those paths; keep in-process results labeled accurately.
- [ ] Rerun the applicable agentic corpora with pinned tools and reproducible fixtures. Reconcile every claim against its command and result artifact.
- [ ] Finish E7 with an authorized model credential, or clearly mark live-model evidence as unavailable without treating it as measured.
- [ ] Finalize docs/TYPED_SHELL_RESPONSE.md and choose its publication destination.

**Gate:** every performance and correctness claim is traceable to final evidence;
no speedup is inferred from a refactor alone.

## 6. Prepare versions, documentation and packages

- [ ] Update root and LSP manifests, lockfiles, SDK/extension/package metadata and API/OpenAPI versions to the intended release versions together.
- [ ] Finalize CHANGELOG.md and release/13.0.0.md with breaking-call examples, migration guidance, supported platforms and accepted limitations.
- [ ] Reconcile README, AGENTS.md, llms.txt, llms-full.txt, ROADMAP.md and generated discovery metadata with the final inventory and feature register.
- [ ] Update installation, upgrade and troubleshooting instructions; verify every advertised channel's actual package name and expected version.
- [ ] Run scripts/test_release_version.py and scripts/test-release-preflight.ps1 under PowerShell 5.1 and 7.
- [ ] Inspect package file lists, license/source inclusion, artifact names and platform coverage; build and smoke-test the packages intended for publication.
- [ ] Check credentials only for channels included in this release, including crates.io and any chosen SDK or extension registries; never record secrets in the plan or logs.
- [ ] Prepare a concrete release commit, release notes, artifact checklist and recovery procedure for approval.

**Gate:** metadata agrees, preflight passes, and the release candidate is reviewable
without tagging or publishing it first.

## 7. Authorize, publish and verify

- [ ] Obtain explicit approval for the release push/tag, public release and each registry publication included in the release.
- [ ] Publish the approved release commit and tag; verify that the tag identifies the validated commit.
- [ ] Monitor every build and upload to completion. The current workflow creates a public release before binary jobs finish.
- [ ] After the authorized tag exists, compute its real source-archive SHA-256 and update the Homebrew formula; do not invent a prepublication checksum.
- [ ] Publish the chosen registry packages and independently verify their versions and artifacts in the registries.
- [ ] Download the actual release assets and test clean installation on Windows, Linux and macOS, including the supported architectures and channels.
- [ ] Smoke-test ae --version, representative typed pipelines, the Agent API, SDK/LSP integration, upgrades and uninstall instructions.
- [ ] Verify published checksums, documentation links, notes and artifact completeness; resolve discrepancies before declaring the release finished.

**Gate:** users can install the published 13.0.0 artifacts and reproduce the
documented behavior. Workflow initiation alone is insufficient evidence.

## Working checklist for each migration tranche

- [ ] Read the implementation and all aliases; identify ignored input and hidden failure paths.
- [ ] Author the declaration and safe executable examples.
- [ ] Implement strict validation and coded refusals; add meaningful regression coverage.
- [ ] Run the relevant contract, effect, containment and transaction checks.
- [ ] Run formatting and Clippy for the affected code; broaden tests when changes or failures justify it.
- [ ] Refresh coverage, record actual results and remaining limitations, then make a local commit.

No automatic push, tag or publication is part of a migration tranche.

## Dependencies and completion decision

Work in this order: scope decisions → complete contracts and correctness fixes →
final platform/CI validation → final performance and documentation evidence →
release candidate → approved publication → clean-install verification.
Preparation of fixtures, CI environments, documentation structure and credentials
can proceed while the migration continues. Final checks follow the last relevant
change. Check an item only when its evidence is recorded; reopen affected gates
when the release tree changes.

The principal execution dependency is the remaining contract review. External
inputs are supported-platform/hardware access, an authorized model credential
for E7, credentials for selected publication channels, and final release approval.
There is no completion-date commitment until these dependencies and the remaining
migration effort are resolved.

## Historical plan and evidence — reference only

The current checklists above govern 13.0.0. The material below preserves earlier
investigations and checkpoints. Its historical totals and unchecked tasks require
re-triage before action; they are not additional current release requirements.


2026-10-01 update: the maintainer selected **13.0.0** for the stricter contracts.
The source-reviewed migration currently has 569 declarations covering 569 of
1,171 implementations and 694 of 1,437 callable names, measured with
`cargo run --example signature_inventory`. The migration remains incomplete.
The latest intermediate Windows workspace check passed 2,633 tests across 186
suites, with four ignored, after the file-copy, line-deletion, hard-link,
permission-mode and structured-edit tranches. Release preflight passed three
Python tests and eight simulated checks under each of PowerShell 7 and 5.1.
Final-tree and cross-platform release validation remain outstanding.
Current preparation and release blockers are tracked in
[`release/13.0.0.md`](release/13.0.0.md). Older figures below are historical baselines.

The work that began as a benchmark response to *Is Bash All You Need?* and
*Testing if bash is all you need* is finished when:

- the response document is published with freshly measured numbers,
- every builtin declares its arguments and refuses calls it cannot honour,
- no failure reaches an agent as a plain value at exit 0,
- the probes run in CI, so none of that can quietly regress, and
- a release ships that people can actually install.

Every number below is measured, and each task is ticked only when the command
that proves it has been run. Baseline at `b799289`:

| Measure | Value |
| --- | --- |
| Builtins declared | 113 of 1,052 |
| Uncoded failures, first and second failure path | 0 and 0 |
| Stubs that now refuse (`E_UNIMPLEMENTED`) | 84 |
| Comparable builtins that discard extra arguments | 245 of 267 (91.8%) |

#### Phase 0 — Blocked on the maintainer

- [ ] Push the amended `infra` commit (`git push --force-with-lease`); the agent sandbox refuses force-pushes.
- [ ] Decide the global git `user.email`; repositories without a local override inherit it.
- [ ] Provide a model API credential for E7.
- [ ] Check `hw_battery`, `hw_gpu`, `hw_sensors` and `sys_cpu_freq` on hardware that has a battery, GPU and sensors.
- [x] Choose the release version: the maintainer selected 13.0.0 for the stricter contracts on 2026-09-29.
- [ ] Choose where the response is published.

#### Phase 1 — Finish the signature migration

- [ ] 1a. Declare the 156 real builtins that take no arguments, in tranches, read-only first. Builtins that change state need a disposable container, because gathering evidence means running them. Progress: 187 declarations (was 116). The fourth tranche added 71 read-only parameterless builtins, each type measured identically on Linux and Windows. Measuring them turned up four defects, all fixed: `whoami()` listed every account; `platform_cpu_freq` and `platform_libcpp` changed type by OS; and the narrowness harness leaked AETHER_MAX_NET=0 into later tests. Still undeclared among the parameterless ones: state-changing builtins (container needed), hardware and GUI builtins, and `docs_*`/`diag_config`, whose answer depends on the working directory. 2026-09-27: 14 more declared (docs_*, diag_config and config_reload in the machine-dependent tier; clear, governor_reset, session_start, telemetry_*, ui_clear strict). Reading them found `fs_watch` answering "watch_id_stub", `platform_memory_free` assuming 4 KB pages (a quarter of the truth on Apple Silicon), `sys_users` answering [] on failure and `config_reload` uncoded; all fixed. Still undeclared: state-changing (container needed), hw_battery/hw_gpu/hw_sensors (Phase 0 hardware check), GUI and clipboard.
- [ ] 1b. Declare the ~700 builtins that take parameters, by reading each body; the E1/E2/E7 working set first. Never generate declarations from probe data. Progress: the working set is done. Every builtin the E1/E2 corpora or the E7 prompts use is declared (`each`, `echo`, `git_branch` were the last). Declaring them found that `each` swallowed its action's errors and that `echo` dropped its pipe input. Later tranches, 2026-09-26, took declarations from 190 to 283, each body read and each example measured. On the runner, discarded args fell from 231 to 136 of 343. Reading the bodies found defects worse than discarded arguments, all fixed with tests: `md5()` returned SHA-256, and an unknown hash algorithm silently became SHA-256; `proc_kill("name")` signalled pid 0 (the shell's own group) and `proc_kill(-1)` everything; `db_sqlite_count` always returned 0; `hex_decode` panicked; `role_create` dropped malformed permissions; `touch` did not touch; 13 `monitor_*` builtins ran programs while classified Pure, two of them running `sh -c`, which the effect ratchet missed because it matched names instead of dispatch rows; and six stubs returned advice as their answer. What remains is mostly environment-dependent (package managers, objdump/readelf, systemd, GUI, interactive input). Their examples cannot give the same answer on all three CI systems. The designed escape is the example test's `NEEDS_THE_WORLD` skip list, which keeps the call-shape check but not the result check; using it at scale is a judgement call. Resolved ("Proceed", 2026-09-26) with a stricter tier than the skip list: `MACHINE_DEPENDENT` in tests/declared_signatures.rs runs every such example and requires the declared return type or a code meaning absent-here (E_TOOL_MISSING, E_TOOL_FAILED, E_UNIMPLEMENTED, E_NOT_FOUND, E_NO_UI, E_IO); E_BAD_ARG or an uncoded failure still fails. Each entry names what it depends on.
- [x] 1c. `discarded-args` below 5% of comparable builtins. Done 2026-09-28: 6 of 126 (4.8%) on two CI runs of 8a7af92, down from 57 of 175 (a figure earlier reported against a stale denominator of 343). The remaining six are audit_verify (its bare form needs an active audit log), clipboard_history (reads the user's clipboard), platform_db_import/init/store (write the user's platform store) and strings. The probe was made deterministic on the way: time-varying answers are rechecked, and HashMap-ordered listings (syntax KB, plugins) are sorted.
- [x] The effect ratchet (tests/effect_ratchet.rs) follows plain function calls, not method calls, so a builtin that reaches a process through `obj.method(...)` reads as Pure. That is how `mcp_call` ran catalogue programs (cp, make, deno, ...) unguarded in agent mode until 2026-09-27. It should resolve method calls whose name is unambiguous across the crate, or report builtins whose bodies construct a type that owns a spawn site. Done: a method call is followed when the receiver's type is stated in the body (`let s = Type::...`, `let s: Type`, or `self`). Following every same-named method reported five false HTTP callers, and following crate-unique names mapped `HashMap::remove` to `SyntaxKB::remove` (29 false reports), so neither is done. It found one real case, `config_set`, now WriteLocal.
- [x] `rlm_stats()` with no argument returns constants (0 spawned, max_depth 5, ...) presented as "statistics from the last run"; nothing tracks runs. `examples/24_recursive_agents.ae` relies on it. It needs a real source of run statistics, or it should refuse with the example reworked around the record an RLM run returns. Done in ad3e03f: runs are recorded and it answers {runs, last_run}.

#### Phase 2 — Remaining correctness gaps

- [x] Search on Windows: the `search_*` family walks in-process instead of shelling out to `find`/`grep`, and the parameterless three are declared. Checked against GNU `find`/`grep` with the same exclusions: six of six result sets identical, and the same counts on Windows as on Linux.
  - Found on the way: `search_symbols` had never matched anything (`|` is literal in a basic regex), and `find` treated `?`, `[...]` and a middle `*` as exact names.
- [x] `capabilities` on Windows: the refusal is final. Linux capability sets do not exist there; the E_UNIMPLEMENTED hint names `whoami /priv`. The Linux side was the broken one. It parsed `capsh --print`, never extracted the effective set, returned `uid` as a string, and failed on hosts without libcap's tools. It now decodes all five masks from /proc/self/status. `tests/capabilities_reads_the_kernel.rs` checks the result against the kernel. A manual check under `unshare -r`, where all 41 capabilities are held, decoded all 41.
- [x] Triage the remaining `silent-success` suspects. Of 51, a differential against the bare call split off the 7 that took `{unexpected: true}` as a name. Six were real: identifier lookups that stringified a record and reported the miss, plus `platform_require`, whose version check was a TODO. Reading the bare-call group found every git builtin ignoring git's exit status (`git_status()` outside a repository was `[]`, a clean tree) and six builtins that defaulted or answered `false`/`0`/`{}` for a missing or wrongly typed argument. All are fixed, with tests. `tests/exit_status_ratchet.rs` pins the rest of the class, which needs reading one body at a time: 51 builtins still never read an exit status, and 100 sites still report failure as `false`. Both counts may only fall.
- [x] Conventions. `env_venv` now returns null when no virtualenv is active. Null is what `env()` and `sys.env()` return for any unset variable; this is a breaking change for the release notes. `now` and `time` stay as two names for one clock: one implementation now, declared identically. `startup_list` answered `[]` ("nothing starts at boot") whenever its tool failed and on every other operating system. It now returns E_TOOL_FAILED with the tool's stderr, and E_UNIMPLEMENTED where nothing was inspected.
- [x] Tool detection: the `platform_*` family probed 20-60 tools one after another with no time limit, and recorded a failing tool's error line as its version. It now probes in parallel with a 5s bound per tool, reads the exit status, and answers "is it installed?" from a `PATH` index built once instead of a lookup per tool. Under WSL: `platform_tool_versions` 108s -> 1.6s, `platform_detect_tools` 14.5s -> 0.6s, matching `command -v` on all 49 tools it checks.
- [x] Harden `eval::call_lambda1`'s per-call pipe-input copy. Eight lambda-call and pipeline sites deep-copied the pipe input and then overwrote it. They now move it out with `take_input()` and put it back, which behaves identically on every path, early returns included. Measured on WSL with `map` and a nested-pipe `map` at 20k and 200k elements: no difference either way, and both linear. The copy was not the bottleneck in those shapes; this removes the quadratic case where a lambda runs with a large value still on the pipe.
- [x] Every implementation is discoverable. The ontology listed 1,052 builtins while 1,101 distinct implementations dispatch: four categories (AI, Cluster, Platform, Service; 112 builtins) were shadowed by same-named builtins in `ontology_describe`. It now lists 1,164, and every dispatchable function is reachable under at least one listed name. Pinned by `tests/every_category_enumerates.rs`.
- [x] Reconcile "1,280+ builtins". Five figures were in circulation: 1,280+, 1,100+, 1,300+ and 1,301 builtins, in 106 or 108 modules. `tests/documented_builtin_count.rs` now computes them: 1,164 listed by the ontology, 1,433 callable names counting aliases, and 1,164 distinct implementations. Every page advertises "1,100+" in 108 modules, and the test fails if a page drifts. The README's effect coverage is updated to the current 799 of 1,318.
- [x] Classify `ai` correctly. A new probe, `network-egress.mjs`, traces connect(2) for every builtin under `--agent --policy strict` with `AETHER_MAX_NET=0`. It found six builtins classified `pure` that open connections anyway: `ai`, `agent`, `swarm`, `rlm_agent`, `ai_backends` and `mcp_client`. All six, plus their aliases `recursive_agent` and `mcp_connect`, are now `Network`, and the probe runs as a fourth CI gate.
- [x] `sh("git commit -m 'fix bug'")` split on every space, so the quoted message arrived as three arguments. It now uses POSIX word splitting with no expansions, and an unterminated quote is an E_BAD_ARG. `validate_http_url` returned bare prose, which reached agents as E_UNKNOWN. It now returns E_BAD_ARG for a malformed URL, E_POLICY_DENY for SSRF and E_NOT_FOUND for an unresolvable host.

#### Phase 3 — Make it stay done

- [x] Run the probes in CI as gates. The `agentic-probes` job runs the uncoded, second-path and discarded-args probes; every gate was shown to fail on an injected regression before being trusted to pass. Its first runs found problems WSL could not: five uncoded builtins that only fail on a host where the tool is installed, and a hang. Green at `c64a7bd` across 1,164 builtins: 0 uncoded and 0 hung on both paths; `discarded_args_max` calibrated from the runner to 318.
- [x] Carry each probe's non-vacuity checks into CI, so a probe that stops testing anything fails the build (each exits 2 when it measured nothing, and the CI step fails on any non-zero exit).
- [x] Check every builtin name that collides with a Windows system tool. The builtins invoke 202 distinct programs; 13 share a name with a System32 executable. After the `find` fix, none reaches a Windows tool with different semantics: `timeout`, `cmd`, `nslookup`, `where`, `mount` and `net` are all behind a platform check (several as runtime `cfg!`), and the ungated `curl` and `tar` calls get real curl and bsdtar, which accept the same flags.
- [x] Fail the build on a stray control character in source (`tests/no_control_characters_in_source.rs`); a scripted edit had put a form feed into `signature.rs` and a backspace into a regex.

#### Phase 4 — Response document

- [x] Bring `docs/TYPED_SHELL_RESPONSE.md` up to date with the findings since `07752c5`. A new part of section 7, "What the catalogue probes found", covers the four probes and their first and current CI figures, egress, silent answers, the stubs, OS-dependent types and discovery. The probes are added to "Reproducing". Every figure comes from a CI log, a probe run or a commit message; the final pass re-checks them.
- [x] Re-run E1–E3 on a host with all four shells installed. Done on WSL, 2026-09-25, with pinned PowerShell 7.6.6 and nushell 0.115.1 unpacked into ~/tools, against a release build. Every engine was correct and byte-stable. The re-run exposed three harness defects, all fixed. The oracle was a pasted table attributed to an `oracle.mjs` that was never committed, so a fresh corpus scored all eight engines wrong; `oracle.mjs` now computes it, and it agrees with all eight engines. `prepare.mjs` broke under cmd.exe on the `&` in its URL. `report.mjs` built a relative manifest path on Linux. E2 was re-run from a clone on the Linux filesystem, because /mnt/c made every engine 5–17× slower. The response is updated throughout: section 3 table, section 4 (now 3.0×), section 5 table, and the README.
- [ ] Run E7 against a live model.
- [ ] Final pass: every number matches the command that produced it.

#### Phase 5 — Release

- [ ] Version, CHANGELOG, and the counts in `AGENTS.md` and `llms.txt`.
- [ ] Publish to crates.io by hand; the repository has no Actions secrets, so the release pipeline has never published.
- [ ] Verify the published version against the registry, not the workflow.

Order: Phase 0 in parallel with everything; then Phase 2's Windows search,
Phase 3, 1a, Phase 4, Phase 5, with 1b last and longest. Phases 3 and 4 do not
wait on 1b.
