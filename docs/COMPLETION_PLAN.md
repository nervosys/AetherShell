# Completion plan

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

## Phase 0 — Blocked on the maintainer

- [ ] Push the amended `infra` commit (`git push --force-with-lease`); the agent sandbox refuses force-pushes.
- [ ] Decide the global git `user.email`; repositories without a local override inherit it.
- [ ] Provide a model API credential for E7.
- [ ] Check `hw_battery`, `hw_gpu`, `hw_sensors` and `sys_cpu_freq` on hardware that has a battery, GPU and sensors.
- [ ] Choose the release version. 173 argument arms and 84 stubs now refuse where they answered `false`, `[]` or a string, which argues for a major version.
- [ ] Choose where the response is published.

## Phase 1 — Finish the signature migration

- [ ] 1a. Declare the 156 real builtins that take no arguments, in tranches, read-only first. Builtins that change state need a disposable container, because gathering evidence means running them.
- [ ] 1b. Declare the ~700 builtins that take parameters, by reading each body; the E1/E2/E7 working set first. Never generate declarations from probe data.
- [ ] 1c. `discarded-args` below 5% of comparable builtins.

## Phase 2 — Remaining correctness gaps

- [x] Search on Windows: the `search_*` family walks in-process instead of shelling out to `find`/`grep`, and the parameterless three are declared. Checked against GNU `find`/`grep` with the same exclusions: six of six result sets identical, and the same counts on Windows as on Linux.
  - Found on the way: `search_symbols` had never matched anything (`|` is literal in a basic regex), and `find` treated `?`, `[...]` and a middle `*` as exact names.
- [x] `capabilities` on Windows: the refusal is final. Linux capability sets do not exist there; the E_UNIMPLEMENTED hint names `whoami /priv`. The Linux side was the broken one. It parsed `capsh --print`, never extracted the effective set, returned `uid` as a string, and failed on hosts without libcap's tools. It now decodes all five masks from /proc/self/status. `tests/capabilities_reads_the_kernel.rs` checks the result against the kernel. A manual check under `unshare -r`, where all 41 capabilities are held, decoded all 41.
- [x] Triage the remaining `silent-success` suspects. Of 51, a differential against the bare call split off the 7 that took `{unexpected: true}` as a name. Six were real: identifier lookups that stringified a record and reported the miss, plus `platform_require`, whose version check was a TODO. Reading the bare-call group found every git builtin ignoring git's exit status (`git_status()` outside a repository was `[]`, a clean tree) and six builtins that defaulted or answered `false`/`0`/`{}` for a missing or wrongly typed argument. All are fixed, with tests. `tests/exit_status_ratchet.rs` pins the rest of the class, which needs reading one body at a time: 51 builtins still never read an exit status, and 100 sites still report failure as `false`. Both counts may only fall.
- [x] Conventions. `env_venv` now returns null when no virtualenv is active. Null is what `env()` and `sys.env()` return for any unset variable; this is a breaking change for the release notes. `now` and `time` stay as two names for one clock: one implementation now, declared identically. `startup_list` answered `[]` ("nothing starts at boot") whenever its tool failed and on every other operating system. It now returns E_TOOL_FAILED with the tool's stderr, and E_UNIMPLEMENTED where nothing was inspected.
- [x] Tool detection: the `platform_*` family probed 20-60 tools one after another with no time limit, and recorded a failing tool's error line as its version. It now probes in parallel with a 5s bound per tool, reads the exit status, and answers "is it installed?" from a `PATH` index built once instead of a lookup per tool. Under WSL: `platform_tool_versions` 108s -> 1.6s, `platform_detect_tools` 14.5s -> 0.6s, matching `command -v` on all 49 tools it checks.
- [x] Harden `eval::call_lambda1`'s per-call pipe-input copy. Eight lambda-call and pipeline sites deep-copied the pipe input and then overwrote it. They now move it out with `take_input()` and put it back, which behaves identically on every path, early returns included. Measured on WSL with `map` and a nested-pipe `map` at 20k and 200k elements: no difference either way, and both linear. The copy was not the bottleneck in those shapes; this removes the quadratic case where a lambda runs with a large value still on the pipe.
- [x] Every implementation is discoverable. The ontology listed 1,052 builtins while 1,101 distinct implementations dispatch: four categories (AI, Cluster, Platform, Service; 112 builtins) were shadowed by same-named builtins in `ontology_describe`. It now lists 1,164, and every dispatchable function is reachable under at least one listed name. Pinned by `tests/every_category_enumerates.rs`.
- [ ] Reconcile "1,280+ builtins" in `AGENTS.md` with what is measurable: 1,433 callable names (aliases included), 1,101 distinct implementations, 1,164 listed by the ontology.
- [x] Classify `ai` correctly. A new probe, `network-egress.mjs`, traces connect(2) for every builtin under `--agent --policy strict` with `AETHER_MAX_NET=0`. It found six builtins classified `pure` that open connections anyway: `ai`, `agent`, `swarm`, `rlm_agent`, `ai_backends` and `mcp_client`. All six, plus their aliases `recursive_agent` and `mcp_connect`, are now `Network`, and the probe runs as a fourth CI gate.
- [x] `sh("git commit -m 'fix bug'")` split on every space, so the quoted message arrived as three arguments. It now uses POSIX word splitting with no expansions, and an unterminated quote is an E_BAD_ARG. `validate_http_url` returned bare prose, which reached agents as E_UNKNOWN. It now returns E_BAD_ARG for a malformed URL, E_POLICY_DENY for SSRF and E_NOT_FOUND for an unresolvable host.

## Phase 3 — Make it stay done

- [x] Run the probes in CI as gates. The `agentic-probes` job runs the uncoded, second-path and discarded-args probes; every gate was shown to fail on an injected regression before being trusted to pass. Its first runs found problems WSL could not: five uncoded builtins that only fail on a host where the tool is installed, and a hang. Green at `c64a7bd` across 1,164 builtins: 0 uncoded and 0 hung on both paths; `discarded_args_max` calibrated from the runner to 318.
- [x] Carry each probe's non-vacuity checks into CI, so a probe that stops testing anything fails the build (each exits 2 when it measured nothing, and the CI step fails on any non-zero exit).
- [x] Check every builtin name that collides with a Windows system tool. The builtins invoke 202 distinct programs; 13 share a name with a System32 executable. After the `find` fix, none reaches a Windows tool with different semantics: `timeout`, `cmd`, `nslookup`, `where`, `mount` and `net` are all behind a platform check (several as runtime `cfg!`), and the ungated `curl` and `tar` calls get real curl and bsdtar, which accept the same flags.
- [x] Fail the build on a stray control character in source (`tests/no_control_characters_in_source.rs`); a scripted edit had put a form feed into `signature.rs` and a backspace into a regex.

## Phase 4 — Response document

- [ ] Bring `docs/TYPED_SHELL_RESPONSE.md` up to date with the findings since `07752c5`.
- [ ] Re-run E1–E3 on a host with all four shells installed.
- [ ] Run E7 against a live model.
- [ ] Final pass: every number matches the command that produced it.

## Phase 5 — Release

- [ ] Version, CHANGELOG, and the counts in `AGENTS.md` and `llms.txt`.
- [ ] Publish to crates.io by hand; the repository has no Actions secrets, so the release pipeline has never published.
- [ ] Verify the published version against the registry, not the workflow.

Order: Phase 0 in parallel with everything; then Phase 2's Windows search,
Phase 3, 1a, Phase 4, Phase 5, with 1b last and longest. Phases 3 and 4 do not
wait on 1b.
