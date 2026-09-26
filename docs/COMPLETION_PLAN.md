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
- [ ] `capabilities` on Windows: implement, or record the refusal as final.
- [ ] Triage the remaining `silent-success` suspects.
- [ ] Conventions: `env_venv` empty string vs null; `now`/`time` duplication; `startup_list` on unsupported operating systems.
- [x] Tool detection: the `platform_*` family probed 20-60 tools one after another with no time limit, and recorded a failing tool's error line as its version. It now probes in parallel with a 5s bound per tool, reads the exit status, and answers "is it installed?" from a `PATH` index built once instead of a lookup per tool. Under WSL: `platform_tool_versions` 108s -> 1.6s, `platform_detect_tools` 14.5s -> 0.6s, matching `command -v` on all 49 tools it checks.
- [ ] Harden `eval::call_lambda1`'s per-call pipe-input copy (linear today, 9.3x over a 10x step).
- [x] Every implementation is discoverable. The ontology listed 1,052 builtins while 1,101 distinct implementations dispatch: four categories (AI, Cluster, Platform, Service; 112 builtins) were shadowed by same-named builtins in `ontology_describe`. It now lists 1,164, and every dispatchable function is reachable under at least one listed name. Pinned by `tests/every_category_enumerates.rs`.
- [ ] Reconcile "1,280+ builtins" in `AGENTS.md` with what is measurable: 1,433 callable names (aliases included), 1,101 distinct implementations, 1,164 listed by the ontology.
- [ ] Classify `ai` correctly: it calls external models and is currently `pure`, so the network budget does not apply to it.

## Phase 3 — Make it stay done

- [ ] Run the probes in CI as gates. The job exists and every gate has been shown to fail on an injected regression; its first run found 2 uncoded failures on the Ubuntu runner that do not occur under WSL (a different host has different tools, so different failure paths). Open until those are fixed and `discarded_args_max` is calibrated from the runner's own count.
- [ ] Carry each probe's non-vacuity checks into CI, so a probe that stops testing anything fails the build.
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
