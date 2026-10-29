# AetherShell completion plan

Updated 2026-10-02. Target: **13.0.0**, selected for the stricter builtin contracts.

Completion means a validated, installable release with a declared contract for
every dispatchable builtin, working implementations for placeholder operations, reproducible
performance evidence, and verified distribution artifacts. Future roadmap ideas
have their own milestones; this checklist governs the current release.

## Current checkpoint

| Area | Verified state | Remaining gate |
| --- | --- | --- |
| Signature migration | All 1,318 dispatch names declared (2026-10-20, BUILTIN_LOOKUP at run time) | Desktop paths of gui_* and clipboard_* unverified; final-tree validation |
| Latest targeted validation | Linux system-administration backends: 19 system-changing tests passed as root in the disposable distro; Windows and Linux each passed 16 suites, Clippy and (Windows) the Rust 1.88 check; prior 286-test filesystem-watch checkpoint retained | Validate the complete final tree |
| Latest Windows workspace checkpoint | 2,653 passed across 186 suites; four ignored | Rerun after all remaining changes |
| Process runners, stdin deadlocks and pipeline input | Fixed in commits 2a0d298 to 4d7756d (bounded runs kill the process tree; three stdin deadlocks fixed; pipeline stages get only their own input). Each commit's tests and the suites it touches passed on Windows and Linux, Clippy clean on both | Validate on the final tree; open runner items in section 3 |
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
- [ ] Implement placeholder operations, including accounts, groups, ACLs, mounting, watching, scheduler mutations and elevated execution. The user selected all unsupported builtins on 2026-10-02; refusal contracts do not complete this scope.
- [x] Implement and test DQN weight updates and actual learning; the user expanded implementation scope on 2026-10-02.
- [x] Separate future roadmap features from release blockers. Reconcile the stale version, metrics and open-issue sections in ROADMAP.md using current evidence.

**Gate:** every advertised capability has an implementation or an explicit,
accepted limitation. No placeholder is described as successful work.

### Release-scope register

Owners below are responsibility roles, not assignments to named people. All open
items remain release blockers unless explicitly listed as deferred.

| Area | 13.0.0 disposition | Evidence and remaining work | Owner |
| --- | --- | --- | --- |
| Evaluator optimization | Implemented; retain | Recorded comparison linked above; final-tree measurements pending | Performance maintainer |
| Strict builtin signatures | Implemented; platform validation open | All 1,318 dispatch names declared (BUILTIN_LOOKUP at run time, 2026-10-20). gui_* and clipboard_* are tested by refusal only; their desktop paths are unverified on every OS | Runtime maintainer |
| Native password hashing | Implemented | Salted, bounded PHC generation/verification; Windows tests passed, platform matrix pending | Runtime maintainer |
| Ownership, modes and filesystem mutation | Partially implemented; blocker | Strict contracts and owned-fixture tests; native Unix checks pending | Runtime and platform maintainers |
| Accounts, groups, ACL mutation, mount/unmount, elevated execution, scheduler mutation | Linux implemented and verified in the disposable distro; Windows/macOS open; blocker only for those platforms | Linux accounts, groups, ACLs, mounts, services, sudo_exec and apt backends are verified as root in the disposable distro; Windows and macOS refuse with E_UNIMPLEMENTED and their backends remain open. All scheduler mutation functions and fs_watch/fs_unwatch have native backends; owned-fixture/platform tests are required | Runtime maintainer |
| DQN weight training | Implemented; final platform/release validation open | Batch SGD updates weights/biases; finite-difference checks cover all seven activations; terminal targets, delayed synchronization, budgets and complete rollback on arithmetic failure are tested | RL maintainer |
| Other placeholder operations | Audit pending; blocker | Enumerate through remaining source-reviewed catalogue migration; do not infer support from registration | Runtime maintainer |
| Platform/features/packaging | Validation pending; blocker | Complete stages 4 and 6 on final commit, including MSRV and supported feature builds | Platform and release maintainers |
| Provider-backed E7 evidence | Validation pending; blocker | Requires configured test provider; record model, prompt and corpus versions | Evaluation maintainer |
| 13.0.0 publication | Pending final readiness and explicit publication authorization | Stage 7; no tag or publication performed | Release maintainer |
| Future product expansion | Deferred beyond 13.0.0 | Historical roadmap ideas do not turn unsupported implementations into working features | Feature maintainers |

The 2026-10-02 user instruction supersedes the earlier choice to release these
operations as refusal-only placeholders. Real DQN weight updates and working
placeholder operations are now required. Existing policy, containment, budgets
and rollback requirements remain in force; implement those semantics rather than
removing their guards.

### All-implementation work register — active scope

- [x] Confirm scope with the user: implement all unsupported builtins.
- [x] Implement docs_types and docs_signatures as pure Rust source extraction.
- [x] Fix cron_list query parsing and checked subprocess output; platform validation remains open.
- [ ] Classify the source candidates below into always-missing functionality, conditional platform support, and transaction/capability constraints; map helpers to callable operations.
- [x] Implement scheduler mutations with real backends, effect gates, validated inputs and isolated mutation tests.
  - [x] Implement cron_remove, cron_enable and cron_disable using native Windows task operations and exact-entry Unix crontab updates. Injected backend tests cover successful changes, duplicate refusal, preserved unrelated text, native failures and Windows wildcard refusal.
  - [x] Implement cron_add, at_schedule and at_remove; add Windows one-time task listing. Fixed registration code accepts UTF-8 JSON data on stdin and uses a limited interactive-user principal. Eight isolated tests passed, including all Windows trigger branches through process-local cmdlet stubs.
  - [x] Verify actual mutations in disposable Windows and Unix environments; backend injection is not a native-platform mutation witness. 2026-10-28: tests/windows_task_admin.rs passed 5/5 on a disposable GitHub windows-latest runner (run 40800454007): cron_add/disable/enable/remove and at_schedule/at_remove against the real Task Scheduler, each step checked with schtasks.exe /Query, plus missing names (E_TOOL_FAILED, nothing created), malformed calls and transaction refusals. tests/linux_task_admin.rs passed 3/3 as root in the ae-disposable distro (Debian cron and at installed there): the crontab is compared byte for byte with a seeded unrelated entry through add, disable, enable and remove, duplicates and missing entries are E_BAD_STATE with the table unchanged, and an at job is checked with atq and at -c through at_schedule and at_remove; root's crontab and the at queue were empty afterwards. macOS crontab has not been run.
- [ ] Implement services, users/groups, permissions/ACLs, mounting and watching with tested platform backends. (Linux; Windows/macOS open) 2026-10-28, Windows: svc_create/svc_delete (SCM), user_*/group_* (NetAPI) and pkg_install/remove/update/upgrade (winget) verified with real changes on a disposable GitHub windows-latest runner (run 40797023135, elevated runneradmin, winget 1.11.510): windows_service_admin 8/8, windows_account_admin 6/6, windows_winget_admin 7/7, each checked against an oracle other than the builtin (sc/NetUserGetInfo/LogonUser/net/winget list). The run first exposed a renamed built-in Administrator (the refusal test now finds accounts by SID) and winget list asking the unaccepted msstore source (list and uninstall now pass --source winget). Windows ACLs, mounting and watching, and every macOS backend, remain open.
- [x] Implement bounded native TCP connect/listen/send/recv/close with strict contracts and actual loopback transfers; Linux/macOS native validation remains open.
- [ ] Implement missing web/clipboard/UI/input operations.
- [ ] Implement missing package-manager operations with checked tool adapters. (pkg_list: apt, pacman and Windows Add/Remove Programs; dnf/yum (rpm) and brew are implemented with parser tests. On 2026-10-23, RPM 4.20.1 in the disposable distro answered list_rpm's exact query with `aeprobe-plain<TAB>1.2.3-4` and `aeprobe-epoch<TAB>7:2.0-1`, for two built packages, one with an epoch; that is the shape parse_rpm_rows reads. Not yet run end to end on a dnf/yum host, since detection picks apt there, and brew is untested outside the parser)
- [x] Implement source navigation, refactoring, search, tests, sessions and documentation operations.
- [ ] Implement DQN training and remaining crypto operations/legacy verification where supported by the algorithm policy.
  - [x] Implement DQN batch SGD with detached targets and real weights/bias updates.
  - [ ] Implement remaining crypto operations and permitted legacy verification.
- [ ] Complete long-running process/watch/server lifecycles with bounded cancellation and cleanup. 2026-10-26: a read-only survey listed what outlives a call. Bounded already: the run_* family (deadline, tree kill), test_watch, entr_watch/watchexec_run, iperf3_server, nc_listen, web_websocket, fs_watch (64 live, buffered events capped) and the TCP handles (128). Fixed: sleep keeps to the evaluation deadline, net_scan bounds each connect, the result-handle store stops at 4,096 (6891b2c, bd6cb10); WebSocket subscriptions stop on unsubscribe and disconnect, idle channels are pruned, streaming and WebSocket evaluation run on the blocking pool under the deadline, and the Agent API serves 64 requests at once (36baff4); workflow_cancel stops a run at the next step or 100 ms wait slice and it ends Cancelled (it had been overwritten with Completed), pause holds at step boundaries, and waits over one hour are refused before anything runs (635a172). Checked: on Windows under the 4 GiB cap, clippy all-targets clean, 801 library tests, result_handles, net_tranche_contracts, open_items_contracts, four_more_knowable_conditions, signature_migration, agent_api and declared_signatures 30/30; in ae-disposable as an unprivileged user, clippy clean, the sleep_deadline_, lifecycle_tests, workflows:: and handles:: unit tests and the same integration suites. workflow_execute(id, {wait: false}) starts the run on the process runtime (workers with the eval stack) and answers {id, status} at once, so the same caller can poll workflow_status and workflow_cancel it; mode, approvals and principal are process-wide, so a detached run is gated as a blocking one is, and the eval deadline is deliberately not inherited (workflow_builtins tests, 4 new). Limits: pause does not hold a wait already running; the 64-request limit queues without a time bound. proc_kill takes {tree: true} to signal the target and its descendants, read before any signal and deepest first (tests pass on Windows and Linux). Long-lived starts now have a stop: ssh_tunnel answers the ssh pid, which proc_kill stops (checked in ae-disposable against a real sshd: traffic through the tunnel, the tunnel outliving the ae that opened it, a second ae stopping it, a port in use refused E_BAD_ARG, an unknown host E_TOOL_FAILED with ssh's message); tmux_kill ends a session (slot 556); detached containers already had docker_stop and podman_stop. Found while testing tmux_kill: tmux resolves `-t work` to a session named `workshop` when none is called `work`, so tmux_kill, tmux_attach and tmux_send could reach another session; they now pass an exact `=name` target (tests/tmux_session_contracts.rs, against a private tmux server). proc_spawn takes {timeout_ms: N}: a watcher stops the process and its tree at the deadline (Unix: its own process group, SIGTERM then SIGKILL after 2 s; Windows: taskkill /T), checking start time as well as pid and never reaping, so proc_wait still sees the exit (tests/proc_spawn_deadline_contracts.rs, 5 on Linux and Windows). It watches the process itself: descendants left after it exits on its own are not stopped. Making it callable needed the dispatch check to accept one optional parameter after a variadic one; it had refused the trailing Record as a non-String argument. On Windows the bounded runners (run_tree and the tool runner) now put the child in a job object, so a descendant still holding a pipe after the child exits is killed, as the process group already was on Unix, and a deadline reaches descendants whose parent has exited (win_job_tests). The job does not kill on close, so a descendant that closed its pipes survives, as on Unix; one started before the child joined the job escapes. Open by design: the POSIX runner's foreground program and devtool_attach wait for the person;. The Agent API, MCP and AI model API servers now stop on Ctrl-C or SIGTERM: no new connections, open requests finish, and any still open after 10s are dropped (src/serve.rs, 4 tests). Checked on Linux with a real `ae agent serve` and SIGTERM: a 2 s eval in flight answered and the process exited 1.4 s after the signal; a `sleep(3000)` eval was dropped and the process exited at 11.0 s. That run found that dropping the runtime waited on the still-running eval thread (the process was still up at 15 s), fixed by serve::run_then_release.
- [ ] Verify each operation's positive result against real effects in owned fixtures or disposable environments; malformed-call refusal alone does not verify implementation.
- [ ] Re-run the final supported-platform matrix and remaining release gates.

The previous source scan found **123 bi_* functions containing explicit
unsupported branches**, down from 125 before the two source-extraction
implementations. This is a candidate count, not a count of missing callable
implementations: it includes helpers, platform branches, deliberate algorithm
policy constraints and rollback refusals. Every candidate requires source review.
Signature coverage and implementation completeness are separate measures.

## 2. Finish all builtin contracts — the critical path

- [x] Add declaration uniqueness, alias inheritance and repeatable coverage inventory.
- [x] Cover the E1/E2/E7 working set with declared contracts.
- [ ] Finish the remaining implementations, grouped by module and actual behavior. Refresh the inventory after every tranche. Every dispatched name has carried a declaration since 4290804 (2026-10-20), so the "586" this line used to give is a stale declaration count. What remains is the unsupported branches listed under section 1 and the per-builtin lists below.
- [x] Migrate sudo_check: strict input rejection, native Unix effective UID, checked Windows subprocess status and boolean parsing, and coded failures; Windows checks passed, native Unix validation remains a platform gate.
- [ ] Read each body before authoring its declaration; record direct-call, pipeline, optional-argument and alias semantics without generating contracts from probes.
- [ ] Reject surplus arguments, wrong types, conflicting input and invalid options before effects or expensive work begin.
- [ ] Add executable examples for each declaration, witnessing optional parameters both absent and present. Use owned fixtures for mutations and explicit machine-dependent checks where necessary.
  - 2026-10-27, measured: all 1,166 declarations carry an example, and `an_optional_parameter_is_demonstrated_both_ways` asks for two examples where there is an optional parameter but not that they differ. A read-only count found 9 declarations whose examples never pass an optional value; the three pure ones (lines, finetune_start, a2ui_progress) now have one. The other six would run kubectl, uv, poetry or a tool in the test, so they stay as they are. A static check that each optional parameter is shown both omitted and supplied was tried and dropped: a direct call passes the subject as its first argument, and the declarations differ on whether that is a parameter (`round` declares only `digits`; `lines` declares its subject as `text`), so it flagged 93 including `round` and `crypto_hash`, which do show both forms. A sound check needs the dispatch checker to report which parameters each example bound. In the same run, sys_boot_time's example failed when powershell did not answer in 30 s on a loaded host; every other example passed.
- [ ] Check each return shape and refusal code on all applicable platforms; require unsupported behavior to remain explicit.
  - 2026-10-27: a read-only survey listed the Windows builtins that start powershell, cmd, net or wmic for a simple fact. Twenty-six now ask the OS directly: fs_df, fs_mounts and platform_disks (GetLogicalDrives, GetDiskFreeSpaceExW and GetVolumeInformationW on a worker bounded by the probe deadline; sizes equal to Get-PSDrive's and WMI's byte for byte, fs_mounts now names the real file system, and Get-PSDrive's `Temp:` alias is no longer listed as a disk), proc_priority, proc_cpu_usage and proc_threads (OpenProcess, GetPriorityClass, GetProcessTimes and a Toolhelp32 thread walk; equal to Get-Process), net_ip (GetAdaptersAddresses; the same 12 rows as Get-NetIPAddress), platform_os_version, platform_kernel, platform_build and platform_snapshot (RtlGetVersion; a test compares the strings with `cmd /c ver` so the fingerprint stays the same), platform_memory, platform_memory_total, proc_mem_usage, proc_exe and proc_children (sysinfo; total memory equal to WMI's, free within 6 MB, exe and children equal to Get-Process and Win32_Process), sys_locale (GetUserDefaultLocaleName), sys_timezone (the registry's TimeZoneKeyName; its doc said IANA, which Windows does not use), sys_uptime, sys_boot_time (GetTickCount64), sudo_check, platform_has_admin, platform_capabilities (the token's elevation flag), sys_cpu_freq, platform_cpu_freq (registry `~MHz`) and sys_kernel (RtlGetVersion plus the registry's UBR). On this host they gave the same answers as before, except `~MHz` 4392 against MaxClockSpeed 4400 and sys_kernel's new format. An elevated `true` was not observed, because this session is not elevated. Left spawning: hw_battery (GetSystemPowerStatus would drop its `name` field) and the WMI, CIM and struct-array readers the survey classed as not cheap (proc_list, proc_cmdline, sys_load_avg, users and groups, services, scheduled tasks, startup items, PnP devices, net_interfaces and the routing and connection tables, machine id, GPUs). platform_network_interfaces had called `wmic`, which this Windows 11 host does not have, and without it answered one record holding `ipconfig`'s raw text; it now lists GetAdaptersAddresses's table, as Get-NetIPAddress does (12 adapters here, with name, state, mac and addresses). sysinfo's interface list was tried first and dropped: it left out loopback, the Mullvad tunnel carrying the host's traffic and every disconnected Wi-Fi adapter.
- [ ] Complete containment, audit identity, effect classification, budget and transaction checks for effectful contracts. Refuse transaction use where rollback cannot restore the operation.
- [ ] Reach full coverage for both implementations and callable names, using the final inventory totals rather than assuming the current denominator is fixed.
- [ ] Remove interim undeclared-builtin assertions only after the inventory has no uncovered implementations or aliases.

**Gate:** zero uncovered implementations and callable names; declaration examples
and malformed-call regressions pass without broad skip lists masking failures.

## 3. Close correctness and security gaps

- [x] Fix legacy password hash/verification consistency across platforms, including fresh salts, supported formats and FIPS behavior; test success, wrong passwords and malformed hashes.
- [x] Implement cron_list output validation: complete string-valued Windows task records, explicit query failures, Unix environment exclusions, known special schedules and repeated whitespace. Native Unix execution remains a platform gate.
- [ ] Review remaining unchecked subprocess exits and false/zero/empty-result failure paths; lower the ratchet counts only with source review and meaningful tests. 2026-10-24: unchecked exits are done. No builtin runs a tool without reading its exit status, and tests/exit_status_ratchet.rs now requires that list to be empty (it was 38). The fixes: the platform probes and screen_list (2a466fc, e94e8e5); package queries, which also tell a no-match (E_NOT_FOUND) from a failed tool (c2c1a46); and platform_snapshot and platform_system_libs (b10e0d3). The scanner had also counted 24 builtins that check the status in a helper they hand the Output to: the 20 GUI builtins, cron_list, sudo_check, at_list and crypto_key_generate. It now follows those helpers (b03e85a); removing at_list's check fails it. 2026-10-25: the Bool(status.success()) ceiling was stale at 64; two sites were left, and it is now 1 (proc_exists fixed, 09eaf47; crypto_cert_verify's exit is its answer). A survey of builtins.rs found about 70 false/zero/empty-result failure paths. The first set is fixed: the sys_* and platform_* probes, the directory walkers, apply's rollback report, project_version/name/dependencies and alert_list's disk reading. The second set is fixed too: the fs_stat and project_readme families, net counters and ports, tmux_list, detect_hypervisor, the CUDA, SDK, GPU and battery probes, project_root, env, ssh_config, tar_entries, run exit codes and the username lookups (07268ef, bfce331, bd35aca). svc_list on Windows and Linux, the network record fields and the Windows socket state are fixed too (7d951ab, 7a1c2ef); clippy is clean, but their tests have not run yet. Left on purpose: web_json_path keeps its documented null for a missing key. declared_signatures passed 30/30 on bd35aca under the 4 GiB cap; the Linux quiet_ tests pass as an unprivileged user in ae-disposable. macOS is not compiled or run.
- [ ] Verify that unsupported, absent-tool, failed-tool, invalid-input and policy refusals remain distinct, coded outcomes. 2026-10-25: a survey found about 106 uncoded anyhow!/bail! errors in builtins.rs, which reach callers as E_UNKNOWN. Nineteen caller-input ones are now E_BAD_ARG (04be046), and tests/uncoded_error_ratchet.rs pins the remaining 89 (lock, protocol and environment faults) as a ceiling. project_root without git now says git is missing. 2026-10-26: ls directory and metadata errors are fs_error, save_json writes through the atomic writer, and an unregistered provider in ai_set_default is E_NOT_FOUND; the ceiling is 87 (4d5307e). A journal directory tx::begin cannot make is fs_error, which apply and try_repair pass through; the ceiling is 86 (8776ca6). What is left is lock poisoning (about 75 sites), the reserved slots, internal serialization, and two a2ui protocol faults. 2026-10-26: the 78 lock-poisoning sites now go through safety::lock_poisoned (E_BAD_STATE, naming the state and saying to restart); the ceiling is 8 (f16590a cherry-picked). The lock faults in tx.rs (5) and auth.rs (3), outside the ratchet, go through the same helper. 2026-10-26, decided by the maintainer's delegation: the operating system's refusals are the new E_OS_DENIED (exit 77, not retryable) — fs_error's PermissionDenied, EPERM from kill, setpriority and the process readers, and Windows access-denied for suspend/resume and process environments — and E_POLICY_DENY means only the shell's own policy (6c77d38). An absent tool whose PATH search hit an unreadable directory (execvp's EACCES) is E_TOOL_MISSING, not "exists but could not be run" (found by running as an unprivileged user with root's PATH). Checked: 37 test binaries on Windows under the 4 GiB cap (784 library tests, declared_signatures 30/30, the process, text-file, signature, governance and code-taxonomy suites); in ae-disposable as an unprivileged user, clippy clean and text_file_contracts, proc_tranche_contracts, os_branch_contracts, four_more_knowable_conditions, signature_migration and the transaction suites pass. Exit 69 is shared by E_TOOL_FAILED, E_BUDGET_EXCEEDED and E_NO_UI.
- [x] Complete native Unix validation for ownership, modes, execute bits, symlink targets, dangling links and audit aliases. Validate Windows privilege and read-only behavior separately. 2026-10-26: tests/unix_fs_metadata.rs (7b6730b) pins fs_stat/fs_lstat, ls, file_copy, file_write, fs_chmod, rm, read_text and fs_chown on modes, execute bits, ownership, symlinks and dangling links; 16 pass in ae-disposable as an unprivileged user. Writing them found that ls failed a whole directory with E_NOT_FOUND on one dangling link (fixed, 8e72dd6), and the probes found that touch refused a directory and a rollback could not restore a directory's times (fixed, ed08551; tests/tx_times.rs). Audit aliases (ab7879e): a symlink in the workspace whose target was the audit log got writes through, because the guard compared the path as written; file_write, file_append, file_replace, file_patch, file_insert and save_json changed the log and fs_chmod its mode. is_audit_artifact now also compares the resolved path and, on Unix, device and inode; tests/audit_alias_contracts.rs pins 23 writers and failed before the fix. Windows (512698a): tests/windows_readonly_contracts.rs pins the read-only attribute (writes, appends, patches, copies and touch refused as E_OS_DENIED with nothing left behind; rm removes; fs_chmod maps owner-write only) and the missing symlink privilege, which is now E_OS_DENIED instead of E_IO. Checked: the alias test, tx_times and unix_fs_metadata in ae-disposable as an unprivileged user with clippy clean, and the broad quiet suite; on Windows under the 4 GiB cap, 785 library tests, the read-only, audit, guard, text-file, transaction-metadata and signature suites, and declared_signatures 30/30. This host lacks the symlink privilege, so the privileged branch of the Windows symlink test did not run, and hard-link identity is not compared on Windows. macOS is not run, as accepted.
- [ ] Verify bounded reads, allocations, edit batches, output sizes and time limits for the remaining contracts. 2026-10-25: whole-file reads of caller-named paths go through read_text_capped (max_file_size_mb, 100 MiB; E_BUDGET_EXCEEDED), and check_file_size_limit is coded (710ee90, a06ab1e). range is limited to 10,000,000 elements and cannot overflow (04be046). The directory walkers and the POSIX-subset find and grep -r stop at 1,000,000 entries (c448732). Binary whole-file reads and read_text_lossy go through read_bytes_capped (fad74f4). AI provider, plugin, workflow and model-metadata response bodies go through src/http_body.rs at 32 MiB (4d1349b). On Windows, 28 test binaries pass under the 4 GiB cap (756 library tests, declared_signatures 30/30); in ae-disposable, clippy is clean and the quiet_, capped_, walk_cap_, range_cap_ and http_body tests pass as an unprivileged user. The glob now stops gathering past the cap (glob_paths_capped), docs_search reads each file capped (bfcf8c0), and the best-effort body reads no longer turn a refused body into an empty answer (4c41e95). 2026-10-26: a survey of every direct .output()/.status()/.wait() in builtins.rs and posix.rs found the one-shot tools all bounded; the rest are interactive by design (dialogs, editors, proc_spawn, the foreground program of ae -b). file_patch now refuses more than 10,000 patches, as file_edit does (a1104e8). The HTTP clients built with no timeout (an OpenAI-compatible completion, the marketplace registry, the TUI's registry announcement) connect within 10 s and finish within 120 s for completions and downloads, 30 s otherwise (810a187). Checked on Windows under the 4 GiB cap: 779 library tests, declared_signatures 30/30, tx_coverage, write_evidence, signature_migration, structured_codes_contracts and uncoded_error_ratchet; in ae-disposable on the release branch, clippy is clean and the filtered unit tests (coded_errors2_ included), tx_coverage, transactions, tx_times, apply_preflight, awk_contracts, write_evidence and both ratchets pass as an unprivileged user. The macOS and Linux clipboard writers feed their helper from a thread and stop it after 120 s without reading its output (feed_stdin_and_wait, 060e3c0; tested with stand-in programs only; 782 library tests pass on Windows under the cap, and in ae-disposable the clip_deadline_ tests and the transaction suites pass as an unprivileged user, with clippy clean after a Linux-only needless mut was removed). Marketplace replies stop at 32 MiB and a package download at the file limit (d9bbe16). apply refuses a plan of more than 10,000 ops before parsing it, the batch limit of file_edit and file_patch (6c77d38). Item 131's surveyed bounds are all in place.
- [x] Give every remaining subprocess runner a deadline, a process-tree kill and an output cap. Done: container_cli, run_tool, pkg_admin exec, the taskkill calls and manifest probes (b85e803); every `.output()` in builtins.rs (8c4738c, 188374b, b7822db), held by a ratchet in tests/probe_deadline_contracts.rs. web_curl runs through run_tree with a 45 s kill limit behind curl's own 30 s --max-time. Left unbounded on purpose: the seven gui dialog builtins and read_terminal_password, which wait for a person.
- [x] Replace the lossy UTF-8 conversion of legacy CBC decrypt plaintext (`from_utf8_lossy`) with a coded refusal. Being fixed by another agent; the coordinator updates this item. Done in b85e803: non-UTF-8 plaintext is a coded E_DECRYPT_FAILED refusal.
- [x] Decide the wget_download classification. It stays Network (transactions must refuse it: traffic cannot be rolled back). It is self-guarded and calls guard_network on the URL and guard_local_write on the output path before curl runs. Its one write marker was deleting the partial download after a failure; that now goes through remove_partial, the helper the other tool runners use for a path checked absent before the run, so tests/write_evidence.rs passes with its allowlist still at four.
- [ ] Verify atomic publication, owned temporary-file cleanup, preflight refusal without mutation, and transaction rollback for supported filesystem changes. 2026-10-25: a survey found writers that a rollback missed. file_patch/replace/insert, file_move (both ends), save_json, session_export, platform_db_export, touch, git_ignore, gui_screenshot_window, zip_create/add and gzip now snapshot; zip_extract and tar_extract refuse inside a transaction; tests/tx_coverage_ratchet.rs requires a snapshot or a refusal of every writer, with a pinned allowlist of 9 (the shell's own stores, write_file_atomically itself), and tests/tx_coverage.rs rolls each back (ee4f4c8). Running those on Linux found that a backup failed whenever the copy's permissions could not be set (WSL /mnt/c as a non-owner), and rollback then restored nothing and reported success; backups copy content first, and an incomplete rollback is E_IO with the journal kept (21372fa). Checked: 33 test binaries pass on Windows under the 4 GiB cap (762 library tests, declared_signatures 30/30); in ae-disposable, clippy is clean, and tx_coverage, transactions, tx_coverage_ratchet, write_evidence, awk_contracts and the new unit-test modules (Unix mode and symlink cases included) pass as an unprivileged user. write_file_atomically keeps the target's permissions, syncs before the rename, and writes through a symlink; gzip creates its output create_new; monitor_store_save goes through the atomic writer (aff4505). Rollback now restores modified and accessed times, so a touch is undone (77a54a0; tests/tx_times.rs). zip_extract and tar_extract list every entry first and refuse the whole archive with E_POLICY_DENY if a name is absolute or climbs out with `..`, or a link target does; bzip2, xz and zstd tars have their names checked through `tar -tf`, link targets left to tar (4c37c5c, c47fe38). 2026-10-27: their link targets are checked too; the stream goes through its decompressor into the in-process reader under the tool deadline, with `tar -tf` names only where the decompressor is absent (91b7e2d; tested with a real xz in ae-disposable, including a corrupt stream; the tests also pass on Windows, where xz may not have been on the runner's PATH, so they may have skipped there). apply checks every op before tx::begin, refuses a copy or move onto its own source, resolves relative paths once, and writes atomically (dd0c252; tests/apply_preflight.rs). Checked on Windows under the 4 GiB cap: 775 library tests, tx_coverage, tx_times, apply_preflight, transactions, tx_coverage_ratchet, write_evidence, structured_codes_contracts, crypto_archive_tranche_contracts, signature_migration, uncoded_error_ratchet and declared_signatures 30/30; clippy all-targets clean. session_export and platform_db_export replace an existing file the way file_write does, and were already jailed, journalled and snapshotted; they now write through write_file_atomically, their docs say so, and tx_coverage rolls an export back (58683a5). In ae-disposable, on the release branch before that commit: clippy clean; the archive_prescan_ and apply_preflight_ unit tests, tx_times, apply_preflight, tx_coverage, transactions, tx_coverage_ratchet, uncoded_error_ratchet, awk_contracts and write_evidence pass as an unprivileged user.
- [ ] Recheck agent policy enforcement, effect provenance, aliases, audit protection and network restrictions after the catalogue is fully migrated. 2026-10-26: the guard protected only the audit log. The RBAC config (`AETHER_RBAC_CONFIG`, else `<workspace>/.ae/rbac.toml`), read by every process at startup, was writable by a jailed builtin whenever the audit log lived outside `.ae`, so an agent could write itself a role table for its next call; the file named by `AETHER_AUDIT_KEY_FILE` likewise. Both, and the default config's `.ae` directory, are now refused like the log, aliases included (10f0a20; tests/governance_file_contracts.rs failed 4/4 before the fix, passes on Linux and Windows). The policy itself is an environment variable, and the journal defaults to the user cache directory, outside the jail. A hard link to a protected file is now caught on Windows too, by volume serial and file index (21d7d8f; tests/audit_hardlink_windows.rs). Still to recheck: reads of the key file (reads are allowed anywhere in agent mode by design, so its secrecy rests on file permissions), and effect provenance and network restrictions once the catalogue is final. 2026-10-27, reads of the key file, from a read-only survey: no single place sees builtin reads. guard_dispatch returns before any path check for ReadLocal, about 20 to 25 builtin sites and the POSIX runner read caller paths with std::fs directly, and an approved Exec can run cat on the key anyway. A refusal in the builtins would therefore be partial while reading as a guarantee, so none was added. The key's secrecy rests on file permissions; README now says so where it shows AETHER_AUDIT_KEY_FILE, and says to make the key unreadable by the account the agent runs as. Effect provenance and network restrictions remain to recheck once the catalogue is final.

**Gate:** no open correctness or security defect can turn an unsuccessful operation
into apparent success, bypass its effect gate, or make a documented mutation unsafe.

## 4. Validate the complete release tree

- [x] Run cargo fmt --all -- --check and git diff --check. 2026-10-23: both clean at 4e66585, and again on the release commit beee1f2.
- [ ] Run cargo test --workspace --no-fail-fast on the complete final tree; preserve logs and account for every ignored test. 2026-10-27, Linux (ae-disposable, release-13 at bfcc728 plus docs), every test binary as the unprivileged tester under a 4 GiB scope, with tester's own Rust toolchain, ripgrep and hyperfine on a Linux-only PATH: 265 of 266 test binaries pass. The one failure is declared_signatures' project_root example, because git in Linux cannot follow this worktree's Windows-path gitdir (an environment limit; it passes on Windows). Seven clipboard, GUI and input-simulation suites were not run, because WSLg can reach the Windows desktop. An earlier run without the toolchain had 15 failures, all missing tools or that gitdir, plus one gate-variable test that failed once and passed in four reruns. 2026-10-23, every binary under a memory cap (4 GiB Job object on Windows, 8 GiB systemd scope on Linux):
  - Windows: 260 binaries, 3,572 passed, 0 failed, 5 ignored: the `child_sleeper` helper process, `disposable_smoke` (disposable distro only), and examples 03 (network), 05 (an AI backend) and 22 (MCP servers present on the host).
  - Linux (ae-disposable): 260 binaries, 3,582 passed, 49 ignored. The first run failed exit_status_ratchet and option_injection_ratchet: both scanners predated the move to bounded_cmd_output/run_tree and found almost nothing (fixed in 4e66585). Of the 49 ignored, 45 are disposable-only and all 45 pass with --ignored in ae-disposable; the other four are the helper and the three examples above.
  - Rerun at b547789, after the fallback declaration tranches: 264 binaries on each.
    - Windows: 3,647 passed, 1 failed, 5 ignored. The failure was session_journal_contracts asserting that session_undo, session_redo and session_end are Pure, which 2d4da16 changed on purpose (fixed in cf51f69; 11/11 after).
    - Linux: 3,648 passed, 1 failed, 49 ignored. The failure was pipeline_scaling's wide-record check (69.6x against a 13.6x baseline), and it failed 4 of 5 reruns straight after. An hour later the same binary passed 3/3, and so did a fresh build of cf51f69, with the host under heavy unrelated load in the failing window. A load flake, not a regression: the calibration divides out CPU load, but its `sum` baseline does not allocate, so memory-bandwidth contention hits only the subject. Left as is; rerun it if it fails again.
  - Release branch, 2026-10-23: the 13.0.0 bump at bc90948, run from the worktree .claude/worktrees/release-13. 264 binaries on each platform.
    - Windows: 3,649 passed, 1 failed, 5 ignored.
    - Linux: 3,657 passed, 3 failed, 49 ignored.
    - The failure both shared: the_homebrew_formula_names_the_current_release demanded that the formula name the crate's own version, which no release commit can do before its tag exists. Fixed in 5c4d04e by allowing one release of lag. It passes on master and on the rebased release branch on both platforms, and fails with the formula at v11.0.2.
    - The other two Linux failures were caused by running from the worktree:
      - The worktree's `.git` file holds a Windows path, so Linux git found no repository and declared_signatures' git_branches example failed.
      - CARGO_TARGET_TMPDIR in the new target directory was not writable by the test user, so structured_codes_contracts could not write its fixture.
      - Run from the main checkout with that directory opened, both pass: 30/30 and 8/8.
    - Runner fault: on Windows, clipboard_agent_gate (2 tests, passed) ran outside its memory job after AssignProcessToJobObject failed with access denied. The runner now kills a binary it cannot cap and retries it once.
  - Not yet done on macOS.
- [ ] Temporarily clear inherited LC_ALL for the known locale-sensitive Windows check, restore it afterward, and record the environment adjustment. 2026-10-23: no adjustment was needed. LC_ALL and LANG were unset in the environment of the Windows full run at b547789 (Git Bash, and the pwsh it started), and that run had no locale-related failure. docs/release/13.0.0.md:1998 calls it "the known collation-test environment dependency". The code that reads the locale is `locale_collates` and `collate` in src/posix.rs: the POSIX runner sorts by byte unless LC_ALL, LC_COLLATE or LANG names a non-C locale that, on Linux, glibc can load (f1ad2c2), and then it follows glibc's en_US ordering. Its unit test now checks the ordering directly (`collate_in`), so it no longer sets LC_COLLATE or depends on the host's locales. If a release-commit run inherits LC_ALL, clear it for the run and say so here.
- [ ] Run cargo clippy --workspace --all-targets -- -D warnings. 2026-10-23: clean on Windows and Linux; rerun on the release commit.
- [ ] Build ae, aimodel and the LSP in supported release configurations. 2026-10-23, Windows, release profile with `--locked` on the release commit: all three built (16 min). `ae --version` and `aimodel --version` print 13.0.0; a typed pipeline, a record and the surplus-argument refusal (`journal_clear("a")`, E_BAD_ARG, exit 64) behave as documented. The Agent API answered `/health` (version 13.0.0), refused unauthenticated calls, and with `AETHER_API_TOKEN` ran `/api/v1/execute` and `/api/v1/eval`. Found: the first schema request takes 17-24 s (`ae agent schema` 17.5 s; 12.0.1 takes 21.8 s, so not a regression) because `CLIToolRegistry::new()` detects about 95 CLI tools one after another. Timed in a debug test binary, the cost is the PATH lookups (about 160 ms each across 108 PATH entries and 14 PATHEXT extensions), not the `--version` probes (26 installed tools, 10.5 s summed). 27063b4 and 4afed95 detect on eight threads, preserving results and order (checked by injecting a mis-assigned slot): the registry fell from 15.9 s to 5.7-7.8 s, and `ae agent schema` in the rebuilt Windows release binary from 17.5 s to 3.9-4.4 s (three runs). The ontology tests pass 7/7 on Windows and Linux. 1f2788a and 9e79e91 then list each PATH directory once and resolve from the listing as `which` 6.0.3 does (an empty PATH entry is the current directory; extension-less candidates and unreadable directories fall back to `which`): resolving all 95 tools took 207 ms instead of ~15 s with no tool resolving differently, and `ae agent schema` 3.4-3.8 s. The ontology tests pass 8/8 on Windows and Linux; clippy is clean on both. On the rebased release branch (11bead4), the two test binaries that build the schema also pass on both, each under a memory cap: agent_api 48/48 and documented_builtin_count 3/3. Full Windows rerun on the release branch at 3d7c209 (after both changes), every binary under the 4 GiB Job object: 264 binaries, 3,652 passed, 1 failed, 5 ignored. The failure, evolution_contracts' zero-generation case, was a pre-existing intermittent bug: a random genome value read back from the population's JSON one bit off, because serde_json's default float parser is not correctly rounded. 7bd49cd enables `float_roundtrip` with a test on that value (fails without, passes with); evolution_contracts passes 12/12. The Linux full suite has not been rerun since these changes. Linux (ae-disposable), release profile with `--locked`: all three built in 8 min with no warnings; `ae` and `aimodel` print 13.0.0, the pipeline and the E_BAD_ARG refusal (exit 64) match Windows, and `ae agent schema` takes 2.6 s.
- [ ] Validate the Rust 1.88 MSRV and the supported feature combinations, including optional local inference and WASM where advertised. 2026-10-22: the library check without default features and the wasm32 library check had failed since 4ba43bc (import/export added an ungated module_loader); fixed by gating it on `native`, and both pass again, clippy clean. The `ae` and `aimodel` binaries and the tests need `native`, as before. 2026-10-23 at f24cc7f, on Windows: `cargo +1.88 check --workspace --features native` (the CI MSRV job), `cargo +1.88 check --workspace --all-targets`, `cargo +1.88 check --lib --no-default-features` and the stable wasm32 library check all pass. Optional local inference and the other advertised feature sets are not yet checked. 2026-10-27, `cargo check --workspace --all-targets` in ae-disposable with each advertised set: `native,real-tokens` and `native,vendored-tls` pass with no warnings. `native,candle` and `native,onnx` did not compile, and had not since at least the lock's move to ort rc.11 (one type error in candle; ten in onnx, which used the rc.9 API and an undeclared ndarray). Both now compile and are clippy-clean (see the local-inference commit after 9e8b915; ort is pinned exactly). The onnx backend's generate had answered "[generated 1 token(s)]" from one forward pass over Unicode code points, and its embed had used code points as token ids with a zero-vector fallback; embed now uses the model's tokenizer and real pooling, and generate is E_UNIMPLEMENTED. The first run against real models was stopped by Claude Code for low host memory; the maintainer said to proceed, and it ran with the build at 2 jobs under an 8 GiB cap. A read-only review first found that every ai_local_* call built new backends, so a loaded model was gone by the next call, and that candle decoded each token one position late; both are fixed, with id 2 an end token only when the tokenizer names none and max_tokens over 4096 refused, not clamped. Then, with a release build in ae-disposable: all-MiniLM-L6-v2 (ONNX) embeds in 384 dimensions with cosine 0.70 between two cat sentences and 0.09 against a stock-market one (a sanity check, not a comparison with sentence-transformers' own output); SmolLM2-135M-Instruct Q8_0 (GGUF) continues "The capital of France is" with " Paris. Paris is the largest city in France and the second"; ONNX generate is E_UNIMPLEMENTED and max_tokens 5000 E_BAD_ARG. From the review, then fixed: an architecture other than llama, mistral, phi2 or phi3 is E_UNIMPLEMENTED instead of loading as Llama; the Mistral config needs its size keys in the file (it had filled each missing one with Mistral 7B's), so a Mistral GGUF without `mistral.vocab_size` is refused; safetensors files and GGUF embeddings are E_UNIMPLEMENTED (embed had averaged input-embedding rows); the tokenizer is read only from the model's own directory; ai_local_load keeps a backend's code (it reported everything as E_BAD_ARG); the dead filename heuristics are gone. Rechecked on the same two models with a release build, both features clippy-clean. 2026-10-28: chat templates, through minijinja (with minijinja-contrib's pycompat for the Python string methods HF templates call) behind `candle`: `ai_local_generate` takes an Array of {role, content} messages and renders the GGUF's `tokenizer.chat_template`, or the one in the `tokenizer_config.json` beside it, with bos/eos tokens and raise_exception; the rendered text is encoded without a second BOS, and generation also stops on the model's own EOS. Release build in ae-disposable on SmolLM2-135M-Instruct Q8_0: a system and a user message give "The capital of France is Paris." in 7 tokens (prompt 28), stopping at <|im_end|>; a bad role and an empty Array are E_BAD_ARG; the 6 new unit tests pass; clippy `--lib --bins --tests --features native,candle -D warnings` clean. CI now has an `inference-features` job (.github/workflows/ci.yml) that runs clippy and the `local_inference` tests for candle and for onnx; it has not run yet, so it stays open until it passes on the release commit with the MSRV checks.
- [ ] Run Linux, macOS and Windows CI, including the declared supported architectures and packaging targets. 2026-10-23: the maintainer accepted tagging without a macOS run beforehand. macOS stays listed as not run in the release notes, and CI runs on the tag push. 2026-10-28: CI run 40798506394 on the release commit 7e845b1 (ci/14.0.0-r3, dispatched) passed every job: Test on ubuntu-latest, windows-latest and macos-latest (image macos-26-arm64), the x86_64 Linux, x86_64 Windows, x86_64 and aarch64 Apple builds, Lint, MSRV, WASM, the candle and onnx feature jobs, the VS Code extension and the agentic probes; Security Audit 40798510332 passed too. The release notes now say macOS passes the suite there, not that it was not run. aarch64 Linux is built only by the release workflow, and nothing builds aarch64 Windows.
  - 2026-10-28, decided by the maintainer: macOS is no longer an accepted limitation. Push the release branch, then run the CI and an install smoke test on GitHub's macOS runners before tagging.
- [ ] Validate hardware, GUI, clipboard, service and other environment-dependent contracts on suitable hosts or disposable environments; document unavailable coverage. 2026-10-26, on the maintainer's Windows host (Ryzen 9 9900X, two RTX 3090 Ti, AMD iGPU, an APC UPS, Windows 11 26200): about 40 read-only hw_, sys_, platform_ and monitor_ builtins were run and their values compared with WMI and the registry. Wrong and fixed: hw_gpu's VRAM (32-bit AdapterRAM, now the driver's 64-bit size; platform_gpus's fallback no longer runs the absent wmic), hw_battery's status for a UPS on mains (now `ac`, with the device name), platform_gpu_memory counting one GPU of two, and a fabricated 0.0 load average in four monitor_ builtins (now null on Windows) (ddfe450, b27a05f). Rechecked on the host after the fix: 25757220864 bytes per 3090 Ti, `ac` and the UPS name, 49128 MB over 2 devices, null loads. The rest answered plausibly. Not covered here: hw_sensors is null on this host; GUI, clipboard and service contracts were not run, by the standing rule not to touch the desktop or clipboard.
- [ ] Run uncoded-failure, second-path, discarded-argument, silent-success and network-egress probes in isolated environments; verify their non-vacuity checks and final thresholds.
  - 2026-10-23, the CI agentic-probes job reproduced in ae-disposable, as the unprivileged tester under an 8 GiB cap, with AE_PROBE_ASSERT=1:
    - uncoded: 0 of 1,171.
    - uncoded-paths: 0 of 1,171.
    - discarded-args: 1 of 103 comparable (journal_clear), under the ratchet of 5.
    - network-egress: 0 of 1,171 connected with AETHER_MAX_NET=0.
  - network-egress first refused to report. Its non-vacuity control attributed curl's connect() to `?`, because strace 6 pads the pid column and the probe matched exactly one space. With that fixed, the control sees 1 connect() by curl.
  - silent-success is not a gate (see ci.yml) and was not run.
  - 2026-10-26, on master after the E_OS_DENIED and bounds work, same setup: uncoded 0 of 1,171; uncoded-paths 0 of 1,171; network-egress 0 of 1,171 (control sees curl's connect()). discarded-args first refused to report: 99 comparable, under its floor of 100. The cause was the probe, which ran every builtin in one directory, so `mkdir("a")` left a directory every later path builtin was handed. Emptying the jail before each builtin (and fixing touch on a directory, which that directory exposed) gives 100 comparable and 0 discarding. The ratchet stays at 5 until CI confirms it; 100 is the floor exactly, so a host with fewer comparable builtins will refuse to report.
  - 2026-10-27, on release-13 after the lifecycle, tmux, tunnel, spawn-deadline, job-object and workflow changes, same setup but a 4 GiB cap: uncoded 0 of 1,172; uncoded-paths 0 of 1,172; discarded-args 0 of 101 comparable (15 excluded as nondeterministic), under the ratchet of 5; network-egress 0 of 1,172 with AETHER_MAX_NET=0 (control sees curl's connect()). The ratchet stays at 5: two runs on one host are not CI. silent-success, which is not a gate, refused to report: 37 of 1,172 answered, "not a usable split".
- [ ] Confirm CI gates execute the intended tests and fail on representative injected regressions. 2026-10-27, a read-only survey of .github/workflows: the test job runs `cargo test --workspace --features native` on Linux, macOS and Windows with no skip, exclude or continue-on-error, so every non-ignored test binary runs; it now has `--no-fail-fast`, since without it cargo stopped at the first failing binary. Gaps left for a decision: the 64 `#[ignore]` disposable-only tests never run in CI; tests that skip when tmux, openssl, git, kubectl or sqlite3 is absent report green; no memory cap on the test run (only timeout-minutes: 30); the MSRV job only checks; release.yml builds tag binaries with no test run; branches without a PR are never tested. Injected regressions, 2026-10-27 on Windows, each applied alone, built, run under the 4 GiB cap and reverted: the option guard removed from `tar -tf` (option_injection_ratchet failed), a lock fault made uncoded (uncoded_error_ratchet), the server drain made unbounded (serve::tests::a_hung_request_does_not_outlast_the_drain), proc_spawn's exec gate removed (exec_gate_coverage: the program ran unapproved in agent mode), and governance-file protection disabled (governance_file_contracts, 3 tests). 5 of 5 were caught; CI runs all five suites.

**Gate:** all required jobs pass on the exact release commit. A historical pass,
a targeted suite or a started workflow does not satisfy this gate.

## 5. Recheck performance and agentic evidence

- [x] Record paired evaluator measurements and ownership/binding regression evidence.
- [x] Rebuild baseline and final optimized executables, identify them by commit/hash, and rerun the paired performance probe after compilation is complete.
  - 2026-10-27, `benches/agentic/results/perf-2026-10-27.md`: baseline 570bff8 and final d8ab42b, release builds from `git archive` in separate target dirs, sha256 recorded, four paired rounds in the follow-up's order in ae-disposable. All eight runs gave the same results.
- [x] Check module calls, map/filter/reduce, nested records and the recorded neutral or slower cases; investigate regressions before setting final acceptance thresholds.
  - Module calls were faster in every round (median 4.56x, warm module requests about 2x), and so was reduce (1.91x). Every other case straddles 1 across rounds, so no gain is claimed. None regressed: the follow-up's neutral cases have medians of 1.24, 1.27, 1.06 and 0.99. Round 4 slowed six unrelated cases at once, which is host noise; thresholds for later releases need a quieter host.
- [x] Measure CLI cold startup and Agent API latency/throughput separately if claiming improvements for those paths; keep in-process results labeled accurately.
  - 2026-10-27: no release text claims a startup or Agent API improvement, so neither was remeasured. The CHANGELOG's evaluator entry now says its figures are in-process, says no startup or API speedup is claimed, and qualifies the map/filter figure, which the Linux rerun did not reproduce consistently.
- [ ] Rerun the applicable agentic corpora with pinned tools and reproducible fixtures. Reconcile every claim against its command and result artifact.
  - 2026-10-27, E4 to E6 on release-13 in ae-disposable, as tester under a 4 GiB scope, against a private copy of the tree. E4 safety: ae agent+jail 6 of 6 dangerous operations contained and 7 of 7 probes as intended, the same as `e4-safety.json`; `ae agent serve` with no flags, a runner the artifact lacks, also 6 of 6 and 7 of 7; ae human 1 of 6 and bash 0 of 5, both as recorded. pwsh was not installed, so it was not scored. E5 environment: aethershell and agent mode 1 distinct output of 6 environments, bash 2 of 6 (the Tokyo timezone), as recorded. E6 first scored 29 of 32: `ls`, `ls src/*.rs` and `ls src | sort | head -3` sorted by the en_US.UTF-8 that LANG named, while bash's ls, finding that locale was never generated, sorted by bytes. Fixed in f1ad2c2 (collate by bytes when the named locale does not load); the rerun was 32 of 32, matching `e6-bashcompat.json`. The artifacts were not regenerated, and nushell, a column in E5, was not installed.
  - 2026-10-27, E2, E3 and E6 held-out on release-13 in ae-disposable from a fresh clone, the same way. E2 first scored 6 of 8 for every engine alike: its oracle wanted the branch `master|main` and a 12.x version, false on release/13.0.0. t4 and t6 now read the branch and Cargo.toml version of the checkout measured (`expectFor` in shellops.mjs). The rerun: 8 of 8 and byte-stable for aethershell, agent mode and bash; total tokens 1,289, 672 and 2,504, so agent mode 3.7x below bash (the addendum's 447 against 1,566, 3.5x, was a smaller tree; t1, the `ls` of src, is most of each). E3: aethershell 10 of 10 failed, machine code, repair hint and distinct exit (the artifact has 9 of 10 distinct exits), bash 10, 0, 1 and 5 of 10, as recorded. E6 held-out: 32 of 32 native, as recorded. pwsh and nushell were not installed and are not scored. E1 was not rerun: its corpus is 500 live cli/cli issues fetched with `gh api` and not committed, so a rerun answers different questions; its result stands as recorded on 2026-09-26.
- [ ] Finish E7 with an authorized model credential, or clearly mark live-model evidence as unavailable without treating it as measured.
  - 2026-10-28: the user asked for every open item to be finished before release. A fresh fetch (500 records, 206 PRs, 68 open) replays 10/10 on all five arms with Git Bash first on PATH. With the WSL launcher first, the jq arm's known-good q9 failed (9/10), because that `bash` drops `|` and `$`; e7.mjs now refuses that bash and skips the arm by name. The live run (`--provider anthropic --model claude-opus-5-5`, headline tier) stopped at the first call: the API key here is not scoped to a workspace and ANTHROPIC_WORKSPACE_ID is unset. Waiting on the workspace id; no result was written.
- [ ] Finalize docs/TYPED_SHELL_RESPONSE.md and choose its publication destination.
  - 2026-10-28, decided by the maintainer: the repository only (docs/, linked from the README, which it already is, and the release notes). Finalize it after E7 has a result, since its E7 section depends on one.

**Gate:** every performance and correctness claim is traceable to final evidence;
no speedup is inferred from a refactor alone.

## 6. Prepare versions, documentation and packages

- [ ] Update root and LSP manifests, lockfiles, SDK/extension/package metadata and API/OpenAPI versions to the intended release versions together.
  - Inventory, 2026-10-23. Sites that state the shell version and move to 13.0.0 in the release commit:
    - Cargo.toml:7, crates/aethershell-lsp/Cargo.toml:3, and that crate's dependency on the root crate (:42).
    - Cargo.lock and web/Cargo.lock (regenerate both).
    - .well-known/openapi.yaml: info.version (:4) and the /health example (:410).
    - AGENTS.md:11, llms.txt:5, and llms-full.txt:4 and :505.
    - Formula/aethershell.rb:8-9: the tag URL and its sha256, only after the tag exists.
  - Every other version answer comes from CARGO_PKG_VERSION: the CLI, aimodel, /health, MCP serverInfo and ae_version in WASM.
  - ROADMAP.md:3, :77, :385 and :387 describe the published release; change them at publication.
  - The Python SDK (1.5.1), the VS Code extension (1.6.1), the browser extension (0.2.0) and web/ (1.2.0) version independently. The VS Code lockfile said 1.6.0 against package.json's 1.6.1; fixed.
- [ ] Finalize CHANGELOG.md and release/13.0.0.md with breaking-call examples, migration guidance, supported platforms and accepted limitations. 2026-10-23: drafted at the top of docs/release/13.0.0.md from a cited fact sheet of the journal and CHANGELOG. Review it before release, and add the CHANGELOG `[13.0.0]` heading in the release commit.
- [ ] Reconcile README, AGENTS.md, llms.txt, llms-full.txt, ROADMAP.md and generated discovery metadata with the final inventory and feature register. 2026-10-23: counts measured from `ae agent schema` and tests/documented_builtin_count.rs (1,171 builtins, 1,437 names the dispatcher accepts, 108 modules; the schema also listed `ask` as an alias of `ai`, which was never callable, so it is removed and every advertised name is now checked against the dispatcher), effect_coverage (905 of 1,318 dispatch names classified), crates.io and GitHub releases (12.0.2); stale versions (1.5.0, 1.4.0, 1.2.0, 0.3.1), MSRV 1.75, module counts 106/38 and test counts corrected. The 13.0.0 version strings wait for the release commit. 2026-10-28, re-measured on master 0c4b7a0: documented_builtin_count still gives 1,172 listed, 1,438 names, 1,172 implementations, and every advertised count and name reproduces; effect_coverage now gives 911 of 1,319 dispatch names classified (408 fall through to Pure), so README and ROADMAP moved from 905/1,318/413. The release branch carries 14.0.0 in AGENTS.md and the manifests; its remaining 13.0.0 strings are test fixtures and dated history.
- [ ] Update installation, upgrade and troubleshooting instructions; verify every advertised channel's actual package name and expected version. 2026-10-27, registry check (read-only GETs): crates.io `aethershell` and `aethershell-lsp` are at 12.0.2 with repository nervosys/AetherShell, older versions yanked; PyPI `aethershell` is 1.5.0 against 1.5.1 in integrations/python; Open VSX has admercs.aethershell 1.6.1 (nervosys.aethershell is a 404), and the book told users to install nervosys.aethershell, now fixed; GitHub Releases' latest is v12.0.2 with Windows, macOS and Linux assets; npm has no `aethershell`, and winget, choco and Docker appear only in planning docs. Not checked: the VS Code Marketplace listing, the PyPI extras and the Homebrew sha256.
  - Channels checked against the registries, 2026-10-23:
    - Match: crates.io aethershell and aethershell-lsp at 12.0.2. The GitHub release v12.0.2 has nine assets named as release.yml builds them. The VS Code Marketplace and Open VSX both have admercs.aethershell 1.6.1. The formula's sha256 matches the v12.0.2 tag archive.
    - **Broken:** `brew install nervosys/tap/aethershell` (README.md:75, website/index.html:631). github.com/nervosys/homebrew-tap answers 404 (it may be private), and the formula exists only in this repository. Decided 2026-10-23: the docs (README, website, ROADMAP and the formula's own header) now tap this repository by URL, `brew tap nervosys/aethershell https://github.com/nervosys/AetherShell`, then `brew install nervosys/aethershell/aethershell`. Not run here, since there is no Mac; check it on the first macOS smoke test.
    - **Stale:** PyPI has only aethershell 1.5.0, a manual upload; the repository's SDK is 1.5.1. The SDK README now says so.
    - Not channels: no npm package, Docker image, winget or scoop manifest exists, and none is advertised. release.yml:395 runs `npm publish` all the same.
- [ ] Run scripts/test_release_version.py and scripts/test-release-preflight.ps1 under PowerShell 5.1 and 7. 2026-10-23, with git removed from PATH so only the shim could answer: the preflight passes 8/8 under Windows PowerShell 5.1.26100 and PowerShell 7.6.8, and test_release_version.py passes 3/3 under Python 3.12. It needs 3.11+ for tomllib: the host's default python, 3.10, fails to import it, and CI pins 3.12 (ubuntu-latest's python3 is 3.12). The preflight now deletes its temp directory when it passes; earlier runs left 16 under temp/. Rerun on the release commit.
- [ ] Inspect package file lists, license/source inclusion, artifact names and platform coverage; build and smoke-test the packages intended for publication. 2026-10-23, on the release commit beee1f2: `cargo package --list` gives 569 files for aethershell (src, tests and fixtures, examples, benches, lib/*.ae, LICENSE, README, AGENTS.md, llms*.txt, .well-known; no keys or tokens) and 14 for aethershell-lsp (src, LICENSE, README). `cargo publish --dry-run -p aethershell -p aethershell-lsp` (cargo 1.99.1) packaged both, 2.4 MiB and 60.6 KiB compressed, and built each from its packaged form, the LSP against the locally packaged 13.0.0 root, so root-then-LSP publication should resolve. Nothing was uploaded. The binary archives are built by release.yml and are checked after the tag.
- [ ] Check credentials only for channels included in this release, including crates.io and any chosen SDK or extension registries; never record secrets in the plan or logs. 2026-10-28, presence only, on this host: a crates.io token (credentials file and CARGO_REGISTRY_TOKEN), VSCE_PAT and OVSX_PAT are set; gh is logged in with repo and workflow scopes. No PyPI credential exists (no .pypirc, TWINE_* or token), and the repository has no Actions secrets or variables, so PYPI_TRUSTED_PUBLISHING is unset and every registry job in release.yml will be skipped: crates.io, VS Code/Open VSX and PyPI are published by hand. PyPI waits on a token from the maintainer.
- [ ] Prepare a concrete release commit, release notes, artifact checklist and recovery procedure for approval. 2026-10-23: the release procedure, expected assets and recovery steps are in docs/release/13.0.0.md, after the release notes. The version-bump commit is prepared on the local branch release/13.0.0.

**Gate:** metadata agrees, preflight passes, and the release candidate is reviewable
without tagging or publishing it first.

## 7. Authorize, publish and verify

- [ ] Obtain explicit approval for the release push/tag, public release and each registry publication included in the release.
- [ ] Publish the approved release commit and tag; verify that the tag identifies the validated commit.
- [ ] Monitor every build and upload to completion. 2026-10-28: the workflow no longer creates a public release before the binary jobs finish. publish-release creates it once, after every build has passed, with all nine assets and the notes, and the registry jobs wait for it. This is untested until the tag run.
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

### Scope decisions, 2026-10-22

- **wget_download** stays Network; its partial-file cleanup goes through remove_partial (see section 3).
- **Clipboard reads in agent mode** are E_POLICY_DENY, nothing read, unless AETHER_SECRETS=allow or AETHER_POLICY=permissive: clipboard_get, clipboard_get_image and clipboard_history. A clipboard often holds a password, which shape redaction cannot recognise. clipboard_types reads no contents and is not gated. Tested in tests/clipboard_agent_gate.rs.
- **A corrupt bzip2, xz or zstd file** answers `false` from archive_test, as a corrupt zip, tar or gz does; it is not E_TOOL_FAILED. The builtin asks whether the archive is intact, and `false` is that answer; tool failures and refusals stay coded errors. No change was needed.
- **No new crates for 13.0.0.** sxd-xpath would add four unmaintained packages and parses XML only, while web pages are HTML, so web_xpath stays E_UNIMPLEMENTED. arboard and xcap would only add Windows clipboard-image and window-capture backends, which cannot be verified on the available Windows host without touching a real desktop. cbc and an X.509 parser are not needed: legacy decryption and certificate inspection run through openssl on every OS.

## Source-function implementation audit candidates

Unchecked means the unsupported branches still require classification and review,
not that the entire function is nonfunctional. Generated from source headers and
explicit refusal calls, without executing builtins or generating their contracts.

- [x] bi_rl_dqn_step — working implementation added; final platform/release validation remains open.
- [x] bi_proc_kill
- [x] bi_fs_chmod
- [x] bi_fs_chown
- [x] bi_fs_symlink
- [x] bi_fs_watch
- [x] bi_fs_unwatch
- [x] bi_fs_mount — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_fs_unmount — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_net_listen — placeholder discovered beyond explicit-refusal scan; native one-shot TCP accept implemented.
- [x] bi_net_connect — native TCP implementation; final platform validation remains open.
- [x] bi_net_send — native TCP implementation; final platform validation remains open.
- [x] bi_net_recv — native TCP implementation; final platform validation remains open.
- [x] bi_net_close — native TCP implementation; final platform validation remains open.
- [x] bi_sys_cpu_freq
- [x] bi_whoami
- [x] bi_svc_status
- [x] bi_svc_create — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_svc_delete — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_cron_list
- [x] bi_cron_add — working implementation added; final platform/release validation remains open.
- [x] bi_cron_remove — native lifecycle backend implemented; disposable-platform verification remains open.
- [x] bi_cron_enable — native lifecycle backend implemented; disposable-platform verification remains open.
- [x] bi_cron_disable — native lifecycle backend implemented; disposable-platform verification remains open.
- [x] bi_at_schedule — working implementation added; final platform/release validation remains open.
- [x] bi_at_list — working implementation added; final platform/release validation remains open.
- [x] bi_at_remove — working implementation added; final platform/release validation remains open.
- [x] bi_startup_list
- [x] bi_archive_test
- [x] bi_user_add — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_user_del — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_user_mod — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_user_passwd — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_user_lock — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_user_unlock — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_group_add — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_group_del — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_group_mod — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_perm_check
- [x] bi_acl_get — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_acl_set — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_sudo_check
- [x] bi_sudo_exec — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_capabilities — Linux-only kernel concept; explicit coded refusal elsewhere.
- [x] bi_pkg_list
- [x] bi_pkg_install — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_remove — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_update — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_upgrade — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_clean — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_add_source — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_hold — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_unhold — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_autoremove — Linux backend verified as root in the disposable distro; Windows/macOS refuse explicitly
- [x] bi_pkg_history
- [ ] bi_gui_screenshot
- [ ] bi_gui_screenshot_window — Linux X11 implemented (xwininfo + ImageMagick import); Windows/macOS open
- [x] bi_gui_find_image
- [x] bi_web_websocket — one bounded exchange through tungstenite (already built for axum): ws and wss, 8 MiB, 1000 messages, 120 s
- [ ] bi_web_xpath
- [ ] bi_clipboard_types — Linux implemented (X11 via xclip/xsel, Wayland via wl-paste); Windows implemented; macOS implemented (osascript `clipboard info`), not yet run on a Mac.
- [x] bi_input_timeout
- [x] bi_input_autocomplete
- [x] bi_crypto_decrypt — authenticated AE1 everywhere; the opt-in legacy CBC path runs wherever `openssl` is on PATH, Windows included (35aefa5)
- [x] bi_crypto_sign
- [x] bi_crypto_key_generate — native Ed25519; RSA/EC through `openssl` on every OS, E_TOOL_MISSING without it (35aefa5)
- [x] bi_crypto_password_verify
- [x] bi_crypto_cert_info — `openssl x509` on every OS, E_TOOL_MISSING without it (35aefa5)
- [ ] bi_clipboard_get_image — Linux implemented (X11 xclip, Wayland wl-clipboard); Windows (PowerShell) and macOS (osascript) implemented, tested only on the disposable runners (tests/clipboard_image_admin.rs)
- [ ] bi_clipboard_set_image — Linux implemented (X11 xclip, Wayland wl-clipboard); Windows (PowerShell) and macOS (osascript) implemented, except WEBP on macOS; tested only on the disposable runners (tests/clipboard_image_admin.rs)
- [x] bi_input_read_char
- [x] bi_input_spinner
- [x] bi_input_progress
- [x] bi_input_table
- [x] bi_input_form
- [x] bi_crypto_cert_verify — `openssl verify` on every OS (35aefa5). Accepted limitation: a missing trust store reads as false because openssl's exit status does not distinguish it.
- [x] bi_crypto_verify_signature
- [x] bi_code_references
- [x] bi_code_definition
- [x] bi_code_callers
- [x] bi_code_callees
- [x] bi_code_hover
- [x] bi_code_signature
- [x] bi_code_completions
- [x] bi_project_scripts
- [x] bi_search_duplicates
- [x] bi_search_semantic
- [x] bi_search_similar
- [x] bi_test_coverage
- [x] bi_test_failing
- [x] bi_test_passing
- [x] bi_test_skipped
- [x] bi_test_watch
- [x] bi_test_snapshot
- [x] bi_test_generate
- [x] bi_diag_suppress
- [x] bi_refactor_rename
- [x] bi_refactor_extract_function
- [x] bi_refactor_extract_variable
- [x] bi_refactor_extract_constant
- [x] bi_refactor_inline
- [x] bi_refactor_move_to_file
- [x] bi_refactor_change_signature
- [x] bi_refactor_add_parameter
- [x] bi_refactor_remove_parameter
- [x] bi_session_end
- [x] bi_session_history
- [x] bi_session_undo
- [x] bi_session_redo
- [x] bi_docs_api
- [ ] bi_platform_libcpp — Windows reads msvcp140.dll's version and Linux codes unreadable libraries (40389de); macOS still answers libc++ without a version, untested here.
- [x] bi_blkid
- [x] bi_lsof
- [x] bi_tcpdump_capture
- [x] bi_ethtool_info
- [x] bi_perf_stat
- [x] bi_perf_record
- [x] bi_awk_process
- [x] bi_nc_listen
- [x] bi_iperf3_server
- [x] bi_lazygit_open
- [x] bi_entr_watch
- [x] bi_watchexec_run
- [x] bi_nodemon_run

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
- [x] Choose where the response is published. 2026-10-28: the repository only (see the TYPED_SHELL_RESPONSE item).

#### Phase 1 — Finish the signature migration

- [x] 1a. Declare the 156 real builtins that take no arguments, in tranches, read-only first. Builtins that change state need a disposable container, because gathering evidence means running them. Progress: 187 declarations (was 116). The fourth tranche added 71 read-only parameterless builtins, each type measured identically on Linux and Windows. Measuring them turned up four defects, all fixed: `whoami()` listed every account; `platform_cpu_freq` and `platform_libcpp` changed type by OS; and the narrowness harness leaked AETHER_MAX_NET=0 into later tests. Still undeclared among the parameterless ones: state-changing builtins (container needed), hardware and GUI builtins, and `docs_*`/`diag_config`, whose answer depends on the working directory. 2026-09-27: 14 more declared (docs_*, diag_config and config_reload in the machine-dependent tier; clear, governor_reset, session_start, telemetry_*, ui_clear strict). Reading them found `fs_watch` answering "watch_id_stub", `platform_memory_free` assuming 4 KB pages (a quarter of the truth on Apple Silicon), `sys_users` answering [] on failure and `config_reload` uncoded; all fixed. Still undeclared: state-changing (container needed), hw_battery/hw_gpu/hw_sensors (Phase 0 hardware check), GUI and clipboard. 2026-10-22: done; every dispatch index has a declaration (1,104 of 1,104).
- [x] 1b. Declare the ~700 builtins that take parameters, by reading each body; the E1/E2/E7 working set first. Never generate declarations from probe data. Progress: the working set is done. Every builtin the E1/E2 corpora or the E7 prompts use is declared (`each`, `echo`, `git_branch` were the last). Declaring them found that `each` swallowed its action's errors and that `echo` dropped its pipe input. Later tranches, 2026-09-26, took declarations from 190 to 283, each body read and each example measured. On the runner, discarded args fell from 231 to 136 of 343. Reading the bodies found defects worse than discarded arguments, all fixed with tests: `md5()` returned SHA-256, and an unknown hash algorithm silently became SHA-256; `proc_kill("name")` signalled pid 0 (the shell's own group) and `proc_kill(-1)` everything; `db_sqlite_count` always returned 0; `hex_decode` panicked; `role_create` dropped malformed permissions; `touch` did not touch; 13 `monitor_*` builtins ran programs while classified Pure, two of them running `sh -c`, which the effect ratchet missed because it matched names instead of dispatch rows; and six stubs returned advice as their answer. What remains is mostly environment-dependent (package managers, objdump/readelf, systemd, GUI, interactive input). Their examples cannot give the same answer on all three CI systems. The designed escape is the example test's `NEEDS_THE_WORLD` skip list, which keeps the call-shape check but not the result check; using it at scale is a judgement call. Resolved ("Proceed", 2026-09-26) with a stricter tier than the skip list: `MACHINE_DEPENDENT` in tests/declared_signatures.rs runs every such example and requires the declared return type or a code meaning absent-here (E_TOOL_MISSING, E_TOOL_FAILED, E_UNIMPLEMENTED, E_NOT_FOUND, E_NO_UI, E_IO); E_BAD_ARG or an uncoded failure still fails. Each entry names what it depends on. 2026-10-22: done; no dispatch index is left without a declaration.
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

### Scheduler lifecycle implementation checkpoint — 2026-10-02

`cron_remove(target)`, `cron_enable(target)` and `cron_disable(target)` now return
Bool true only after successful native operations (or an already achieved Unix
state). Windows targets are full task names, passed as separate schtasks argv;
wildcards are refused. Unix targets are the exact active crontab entry, even when
currently disabled through `# aethershell-disabled: `. Duplicate or absent matches
refuse instead of guessing. Unrelated lines are preserved. Operations are gated
as Destructive, Exec and Process respectively, and refuse active transactions.
The process serializes its own crontab changes; external writers can still race.
No host jobs were mutated during verification.

Backend syntax was checked against [Microsoft's schtasks reference](https://learn.microsoft.com/en-us/windows/win32/taskschd/schtasks)
and the [native crontab manual](https://man7.org/linux/man-pages/man1/crontab.1.html).
The Windows contract/safety sweep passed 700 unique tests across eight suites
(548 library, 31 declarations, 74 text/file, 18 effect ratchet, 14 guard, 2 effect
coverage, 10 error conditions and 3 module aliases). Actual mutation witnesses on
disposable platforms and all remaining unsupported operations are still required.

Final scheduler target-validation regression passed after the sweep. Workspace
all-target Clippy passed with warnings denied, and the locked offline Rust 1.88
workspace check passed. Evidence: `temp/ae-perf/cron-lifecycle-validation.log`,
`cron-lifecycle-final.log`, `cron-lifecycle-clippy.log`, and
`cron-lifecycle-msrv.log`. Versions and publication state remain unchanged.

### Scheduler creation and DQN checkpoint — 2026-10-02

Scheduler creation now has working adapters: Unix cron appends exact entries and
at submits stdin to `at -t`; Windows registers unique tasks through fixed
PowerShell code and JSON stdin. Windows recurring schedules are @hourly (starting
at the next whole hour), @daily (midnight) and @weekly (Sunday midnight). Windows
jobs execute cmd.exe under the current limited interactive user, so that user
must be logged in. One-time timestamps are future local `YYYY-MM-DDTHH:MM`; invalid,
ambiguous or nonexistent times fail before approval. Returned targets/IDs feed
native lifecycle/removal operations. Unix at-list queries the native queue;
Windows lists root-folder AetherShell-at tasks with typed records. Failed tools
and malformed registration confirmations never become successful results. If a
successful registration returns an unreadable ID, the error explicitly warns
that a job may exist and directs inspection before retrying. Host jobs were not
mutated during tests; real disposable-platform mutation witnesses remain open.

DQN now optimizes batch-mean half squared TD loss with SGD backpropagation,
differentiating the selected action and holding target-network predictions fixed.
Terminal transitions use their reward directly. Replay samples with replacement;
the trained policy synchronizes at the configured frequency. Arithmetic failure
preserves the entire agent, including weights, replay, counters, epsilon and
targets. MultiAgentRL.step propagates failures atomically across agents; both Rust
step APIs now return Result. The shell refuses training above twenty million
estimated dense operations while retaining prior size/encoding/replay limits.
Gradient tests use independent central differences through ReLU, sigmoid, tanh,
linear, leaky ReLU, swish and softmax. API tests verify actual parameter updates,
reduced terminal error, target timing, direct/pipeline equivalence and budget/error
refusals without changed input state. The Windows checkpoint passed 721 unique tests across nine suites: 561 library,
31 declaration/example, eight DQN, 74 text/file, 18 effect ratchet, 14 guard, two
effect coverage, ten error conditions and three module alias tests. The final
scheduler input-limit and script-stub tests passed again after tightening Windows
commands to 4096 UTF-16 code units and timestamps to exactly 16 characters.
Workspace all-target Clippy passed; final library Clippy and Rust 1.88 workspace
checks passed. The locked offline wasm32 web library check passed with the
existing unused Env-method warning. CLI binaries are native targets. Evidence:
`temp/ae-perf/scheduler-dqn-validation-final.log`, `scheduler-dqn-text-final.log`,
`scheduler-creation-final.log`, `scheduler-dqn-clippy.log`,
`scheduler-dqn-clippy-final.log`, `scheduler-dqn-msrv-final.log` and
`scheduler-dqn-wasm-lib.log`. Failed initial test/build attempts are not counted
as successful checks; repeated tests are counted only once.

Windows creation also rejects commands longer than 4096 UTF-16 code units
before approval; this leaves room below the [documented cmd.exe limit](https://learn.microsoft.com/en-us/troubleshoot/windows-client/shell-experience/command-line-string-limitation).

Backend references: [Microsoft scheduled triggers](https://learn.microsoft.com/en-us/powershell/module/scheduledtasks/new-scheduledtasktrigger),
[POSIX at](https://manpages.ubuntu.com/manpages/focal/man1/at.1posix.html), and the
[DQN paper](https://arxiv.org/abs/1312.5602). These references support the adapter and
learning design; local tests establish this implementation's behavior.

### Source/project implementation checkpoint, 2026-10-02

Implemented seven bounded, pure AetherShell source-navigation operations and
ReadLocal manifest-script discovery. Navigation is syntactic and single-source;
manifest discovery reads literal npm/Deno/Python/Cargo declarations and never
executes them. Five new contract tests and the final 84-test text/file suite passed, including
keyword field indexing and a scheduling-aware socket deadline witness. Across
nine suites, 731 unique tests passed. Final lint, Rust 1.88, WASM, formatting
and diff checks passed. Inventory confirms 598/1171 implementations and 723/1437 callable names. See docs/release/13.0.0.md and temp/ae-perf/source-project-*.log.
The full unsupported-builtin goal and 13.0.0 release gates remain open.

### Code-search/API implementation checkpoint, 2026-10-02

Implemented exact normalized Rust free-function clone grouping, explicit
syntax-shingle similarity ranking, cosine ranking of supplied embeddings,
and top-level public Rust API/documentation extraction. Contracts state their
language, algorithm, corpus bounds and lack of implicit source/model execution.
69 tests passed across six relevant suites, including five new regression
tests. All-target lint, Rust 1.88 and WASM compatibility passed. Formatting/diff checks passed; inventory confirms 602/1171 implementations
and 727/1437 callable names. See
 docs/release/13.0.0.md and temp/ae-perf/code-search-*.log. The remaining
unsupported-builtin implementations and 13.0.0 release gates remain open.

### Testing implementation checkpoint, 2026-10-02

Implemented typed snapshot comparison with bounded structured differences and
Rust unit-test generation from explicit parsed expressions. Both are Pure;
snapshot persistence and test execution use separate existing operations.
Six new regressions passed, including actual compiled generated tests that
both pass correct oracles and fail an incorrect oracle, plus language-level
examples. The final batch passed 62 tests across six relevant suites. All-target
Clippy, Rust 1.88 and WASM checks passed. Signature inventory confirms
604/1171 implementations and 729/1437 callable names. Formatting/diff checks and final example-test lint passed. See docs/release/13.0.0.md and temp/ae-perf/testing-*.log.


## Native Ed25519 implementation checkpoint, 2026-10-02

Crypto_sign and crypto_verify_signature now implement native Ed25519 rather than
parameterless refusals. Signing requires supplied PKCS#8 v2 PEM and returns a
Record with algorithm, public_key and signature; verification accepts canonical
Base64 public-key/signature data and returns Bool. UTF-8 String and binary byte
Array messages are bounded to 1 MiB. Crypto_generate_key adds native Ed25519,
with operating-system randomness and no key-file/process backend. Its existing
conservative WriteLocal effect is retained; signing and verification stay Pure.
AETHER_FIPS rejects the unvalidated backend. Non-Unix RSA/EC generation remains
an implementation blocker. Certificate and other crypto gaps remain open.

The 92 affected tests passed, including RFC 8032 known-answer data, tampering,
malformed and inconsistent keys, executable examples, argument/size bounds and
FIPS refusals. Native library, Rust 1.88 workspace, WASM web library and all-target
Clippy passed. Ring 0.17.14 was already locked through TLS; the root dependency
edge is now explicit and native-only. Signature coverage is unchanged because
all three signatures already existed. See docs/release/13.0.0.md and
temp/ae-perf/signatures-*.log. No version bump or publication was performed.

Final batch validation also passed 48 signature-discovery/effect tests (140
passed in five suites total) and the separately locked WASM browser wrapper.
Three unchanged broad example sweeps were filtered; the new Ed25519 examples
were executed directly. Formatting, diff and Rust 1.88 all-target workspace checks passed. The web lockfile
remains unchanged because it does not enable the optional native dependency.


## Test report implementation checkpoint, 2026-10-02

Test_coverage now summarizes supplied LCOV 1.x source records into deterministic
line/function/branch metrics and sorted per-file summaries. It validates optional
summary counts, unions covered locations across repeated source records and
rejects conflicting supplied checksums/function locations. Test_passing,
test_failing and test_skipped parse supplied completed Rust libtest JSON reports,
validate event sequencing and exact suite counts, and preserve typed result data.
Filtered-out identities are not inferred. Incomplete/fail-fast, discovery,
benchmark, Cargo compiler JSON and malformed reports reject. These are Pure report
operations: test execution, instrumentation and file loading remain explicit.
Test_watch and other runner/implementation gaps remain open.

Seven new regressions cover weighted coverage and repeated-record union, malformed
and contradictory data, multiple suites, escaped Unicode/diagnostics, bounded
work and call shapes, executable declaration examples/module aliases, and an
actual owned Rust harness with passing/failing/ignored/filtered tests. Only the
owned child harness enables libtest's unstable JSON reporter. All 13 testing
contracts passed, including all canonical and module examples. Ten existing refusal/error-contract
tests and 51 discovery/effect/alias tests also passed: 74 tests in six suites.
Three unchanged broad signature-example sweeps were filtered; all four new
examples were evaluated directly. Native library, Rust 1.88 workspace and WASM
web library checks passed; WASM retains its existing unused Env-method warning.
Inventory confirms 608/1171 implementations and 733/1437 callable names, leaving
563 implementations and 704 names undeclared. Logs: temp/ae-perf/reports-*.log.
No dependency, version or publication changes are included.

Final all-target Clippy with warnings denied, formatting and diff checks passed
(reports-clippy-final.log). The final-tree workspace and release gates remain open.


## Rust function signature editing checkpoint, 2026-10-02

Refactor_change_signature, refactor_add_parameter and refactor_remove_parameter
now perform bounded syntax transformations on exactly one supplied Rust free
function. They return token-formatted source without files, compiler execution,
call-site rewriting, type resolution or macro/cfg expansion. Function bodies,
visibility and documentation/attributes are retained as tokens; ordinary comments
and formatting are discarded. At most 64 plain identifier parameters are allowed;
receivers, variadics and destructuring/ref/subpatterns reject. Raw identifiers
normalize for duplicate binding checks. Dropping bindings uses a conservative,
scope-insensitive visitor: identifier mentions (including fields/shadowing),
macros, nonliteral-doc attributes, nested items and verbatim syntax reject.
Callers still update call sites/body types and compile the output. All three
operations retain their Pure effect classifications.

Six new regression tests passed, with actual compilation/execution of transformed
functions under denied warnings, including append/front/middle insertion,
reordering and unused/raw parameter removal. Other witnesses cover body/literal/
documentation preservation, qualifiers/generics, conservative refusals,
argument/syntax/work bounds and all new examples through module aliases.
Rust 1.88 workspace and WASM web library checks passed; the existing non-native
unused Env-method warning remains. Only the already-locked syn native dependency's
visit feature was enabled; no lockfile/package versions changed. Live inventory
is 611/1171 implementations and 736/1437 callable names, leaving 560 implementations
and 701 names undeclared (temp/ae-perf/refactor-inventory.log). Other refactoring,
implementation and release gates remain open. No version bump or publication.

Final targeted validation passed 57 tests across five suites: six new refactoring
contracts, 28 declaration checks, 18 effect ratchets, two effect coverage checks
and three module alias checks (temp/ae-perf/refactor-validation.log). Three broad
existing example sweeps were excluded; every new example, including omitted and
supplied insertion positions, ran through canonical and module spellings.
All-target workspace Clippy with warnings denied passed (refactor-clippy-final.log),
as did formatting and diff checks. Final-tree workspace and release gates remain open.

## Diagnostic report filtering checkpoint, 2026-10-02

Diag_suppress(report, codes) now filters supplied rustc/Cargo JSON-lines text by
exact warning code. It removes only top-level warning diagnostics; errors, notes,
null-code summaries and other Cargo events retain their original bytes, including
CRLF and missing final newline. Child diagnostics stay with their parent. Cargo
short-format text is unsupported because it does not supply warning codes.
Each diagnostic requires String level/message and null/object code. Unknown
levels, malformed JSON/objects and duplicate keys at any depth reject the report.
Other Cargo events with String reason pass through. There are no compiler runs,
source/configuration changes or implicit cached reports inside the builtin; it
retains its Pure effect. Limits are 1 MiB, 25000 lines, 64 KiB per line and 128
unique codes of at most 128 bytes, with serde's JSON recursion limit. Selectors
are ASCII identifier segments separated by double colon. Empty reports are valid;
blank JSON lines are not. This is display-data filtering, not compiler policy.

Final targeted validation passed 56 tests across five suites: five diagnostic
contracts, 28 declaration checks, 18 effect ratchets, two effect coverage checks
and three alias checks (temp/ae-perf/diag-validation.log). An exclusively owned
fixture compiles Rust with --error-format=json and verifies exact removal of its
unused_variables warning while unmatched data remains. Both canonical and module
examples ran; the three broad existing example sweeps were excluded. All-target
workspace Clippy with warnings denied, Rust 1.88 workspace, WASM web library,
formatting and diff checks passed (diag-clippy.log, diag-msrv.log, diag-wasm.log).
The existing non-native unused Env-method warning remains. Final-tree workspace,
cross-platform and release gates are still open; no version bump/publication.
Live signature inventory is 612/1171 implementations and 737/1437 callable names,
leaving 559 implementations and 700 names undeclared (diag-inventory.log).

## Rust tail-expression extraction checkpoint, 2026-10-02

Refactor_extract_variable(source, name) and refactor_extract_constant(source,
name, type) now transform exactly one supplied Rust free function's top-level
final expression without semicolon. Variable extraction inserts let NAME = EXPR;
NAME; constant extraction inserts const NAME: TYPE = EXPR; NAME. Preceding
statements and function declaration/body-expression tokens remain. Ordinary
comments/layout are discarded and documentation comments become attributes.
Identifier collisions are checked conservatively over the function and explicit
constant type, including field names, shadowed bindings and normalized raw
identifiers. Macros, nonliteral-doc attributes, nested items, verbatim syntax,
receivers and variadics reject. This does not select arbitrary subexpressions or
resolve symbols/types/borrowing, expand macros/cfg, edit files or execute code.
Variable binding can change temporary lifetimes; constant validity and types are
checked by the caller's compiler. This is bounded syntax editing, not a guarantee
of semantic equivalence or const-evaluability. Source <=64 KiB/256 raw ASCII
punctuation; name <=256 bytes/32 punctuation; type <=16 KiB/128 punctuation;
output <=1 MiB. Both are direct-only and retain their Pure effects.

Targeted validation passed 63 tests across six suites: six new expression
extraction contracts, six existing signature refactoring contracts, 28 declaration
checks, 18 effect ratchets, two effect coverage checks and three module alias
checks (temp/ae-perf/extract-validation.log). Owned Rust fixtures compile and run
variable/constant transformations with warnings denied; a runtime-dependent
constant deliberately fails caller compilation with E0435. Tests cover evaluation
order and single evaluation, borrowing/raw identifiers, declarations/docs,
collisions/opaque syntax, invalid arguments/tails/work limits and both new
canonical/module examples. Three broad existing example sweeps were excluded.
Rust 1.88 workspace check passed (extract-msrv.log).
All-target workspace Clippy with warnings denied, WASM web library, formatting
and diff checks passed (extract-clippy.log, extract-wasm.log); the existing
non-native unused Env-method warning remains. Live inventory is 614/1171
implementations and 739/1437 callable names, leaving 557 implementations and
698 names undeclared (extract-inventory.log). No dependencies/package versions
changed. Other refactorings, unsupported implementations, final-tree workspace,
cross-platform and release gates remain open. No version bump/publication.

## Rust expression inlining checkpoint, 2026-10-17

Refactor_inline(source, name) (alias refactor.inline) was an unimplemented
refusal and now transforms exactly one supplied Rust free function. It inlines
one top-level let NAME = EXPR; with a plain identifier pattern into its single
use and removes the binding. No mut, ref, subpattern, type annotation,
attributes or let-else are accepted. The identifier must occur exactly twice in
the parsed function, conservatively counting field names, struct shorthand and
shadowing; normalized raw identifiers are compared. The use must be in the
immediately following top-level statement and be evaluated first, once,
unconditionally and consumed by value: operands of arithmetic/bitwise operators,
call/method arguments, tuple/array/struct elements, index expressions, casts,
unary -/!, ?, return/break values, if conditions and the left side of &&/||.
Method receivers, borrows, comparisons, derefs, field/index bases, match/if-let
scrutinees, closures, loops, blocks, conditional branches, and non-moving places
(NAME;, let _ = NAME;, destructuring) reject. Literals and place reads may
precede the use only when EXPR has no calls, assignments, blocks, mutable borrows
or derefs; operator trait impls are assumed not to mutate unrelated locals.
Operator operands are parenthesized unless EXPR is atomic. This does not resolve
symbols, types or borrows, expand macros or cfg, edit files or execute code.
Source <=64 KiB/256 punctuation; name <=256 bytes/32 punctuation; output <=1 MiB.
The operation is Pure and direct-only.

Tests in tests/expression_refactor_contracts.rs now total nine, including three
new ones. Owned fixtures compile with warnings denied and run: extract_variable
followed by inline restores the original tokens; (a+b)*2 precedence is preserved;
an effectful value evaluated first moves; and an effect-free value follows the
inert path i32::max. A witness shows that a naive textual inline of
let y = { x += 1; 5 }; x + y compiles and returns 8 instead of 9; the builtin
refuses it. Targeted Windows validation passed 112 tests across nine suites:
expression_refactor 9, signature_refactor 6, declared_signatures 31,
effect_ratchet 18, effect_coverage 2, effect_snapshot 1, effect_alias_agreement 3,
module_aliases 3 and signature_migration 39. Formatting and diff checks passed.
Live inventory is 615/1171 implementations and 740/1437 callable names, leaving
556 implementations and 697 names undeclared.

Clippy is not claimed for this tranche. On the local rustc 1.99 toolchain,
all-target Clippy with warnings denied failed on 31 pre-existing toolchain lints
in untouched files (29 clippy::double_must_use and 2 fetch_update deprecations);
these are fixed in a separate commit. No dependencies or package versions changed.
Remaining refactorings are refactor_rename, refactor_extract_function and
refactor_move_to_file. Final-tree workspace, cross-platform and release gates
remain open. No version bump/publication.

## Rust rename, function extraction and file move checkpoint, 2026-10-17

Refactor_rename(source, old, new), refactor_extract_function(source, name, start,
end) and refactor_move_to_file(source, item, destination) were unimplemented
refusals and are now implemented. All three are Pure and direct-only, operate
only on supplied source text, read and write no files and execute no code.

Refactor_rename renames every identifier token in one whole Rust source file
whose normalized text equals old (raw prefixes ignored) to new, using a syn
visitor and token-formatted output. It is syntactic and whole-file: different
items that share the name are all renamed. It refuses when new already occurs
as an identifier, when old is absent or equals new, when either is self, Self,
super, crate or _, and when old occurs in macro invocation tokens (new too),
non-doc attributes or verbatim syntax. Other files, external crates, trait
methods implemented for std or dependency traits, string contents and consumers
of pub API are not updated.

Refactor_extract_function moves the top-level statements [start, end) of one
Rust function into a new private function NAME and leaves one call statement,
or the tail call with the original return type when the range ends at a tail
expression. Captured outer names are passed by value in first-use order with
their declared types, which must be plain by-value parameters or top-level lets
with type annotations before start. It refuses receivers, generics, macros,
nested items, NAME already in the function, any identifier bound more than once
(no shadowing), untyped captures, return, ?, .await and break or continue outside
a loop in the range, assignments to captured variables, &mut of a captured
variable, method calls on a captured receiver, and a name bound in the range
that is used after end. It performs no type inference, borrow analysis or symbol
resolution. A by-value capture can move a non-Copy value, so a later use fails
to compile; a captured value that the range only borrowed and that is not used
later is dropped when the new function returns, possibly earlier than before
(for example a lock guard), with no compile error.

Refactor_move_to_file moves one top-level fn, struct, enum, union, const, static,
type alias, trait or inline mod from SOURCE to the end of DESTINATION and returns
Record {source, destination}. Both texts are token-reformatted: ordinary
comments are discarded and doc comments become doc attributes. Attributes are
limited to doc, derive, allow, warn, deny, inline, must_use, cfg and repr;
macro_rules and other attributes are refused. It refuses a SOURCE that does not
define the name exactly once at top level, an impl block in SOURCE whose self
type is that name (it would be orphaned), an out-of-line mod, a DESTINATION that
already defines or imports the name, and a shebang. It adds no use statements,
no pub and no path rewriting, so references in SOURCE stop resolving until the
caller adds a use or visibility change. The sources are not compiled.

Tests: 146 passed across 13 suites: rename 8, extract_function 6, move 10,
expression_refactor 9, signature_refactor 6, four_more_knowable_conditions 10,
declared_signatures 31, effect_ratchet 18, effect_coverage 2, effect_snapshot 1,
effect_alias_agreement 3, module_aliases 3 and signature_migration 39. The
four_more_knowable_conditions suite no longer lists rename or extract_function
as unimplemented stubs. Owned fixtures compile and run renamed and moved code and
extracted functions. All-target workspace Clippy with warnings denied and
formatting passed. Rust 1.88 all-target workspace check passed.

Live inventory is 618/1171 implementations and 743/1437 callable names, leaving
553 implementations and 694 names undeclared. The syn dependency now enables its
visit-mut feature in Cargo.toml; no new dependency and no package version changed.
Test_watch and the session_end/history/undo/redo operations were implemented in
the session operations and test_watch checkpoint below. Final-tree workspace,
cross-platform and release gates remain open. No version bump/publication.

## Session operations and test_watch checkpoint, 2026-10-17

Session_undo(n?), session_redo(n?), session_history() and session_end() were
unimplemented refusals and are now implemented over the existing reversible-session
journal in src/journal.rs. Session_undo(n?) rewinds like undo(n) and keeps each
reversed step on a persisted redo stack. Session_redo(n?) re-applies undone steps,
journaling each one first so session_undo() can reverse it again. Session_history()
lists the done and undone steps. Session_end() keeps all changes and forgets the
journal and the redo stack; no files are touched. They cover local file contents
only. Journalling is on in agent mode or with AETHER_JOURNAL=on. Any new recorded
write discards the redo stack, including a write that is recorded and then refused.
Each file is bounded to 8 MiB, and 64 MiB in total is shared with the redo stack.
The effects remain Pure, as for undo and journal.

Test_watch(command, paths, options?) runs an argv command once and again on polled
file changes. Change detection compares length and modified time, so an edit that
preserves both can be missed. It stops on max_runs, on idle, or at the total
deadline, and it kills the whole process tree on timeout or deadline (the process
group on Unix, taskkill /T on Windows). It was reclassified from Pure to Exec, and
tests/effect_snapshot.txt was updated. The bare-form test_watch example waits out
the 30 second default idle period, which lengthens declared_signatures.

Safety incident: the undo journal is keyed by working directory. tests/reversible_sessions.rs
cleared the default, repository-keyed journal on every run. On 2026-10-17 a
validation run deleted a nine-entry journal recorded in this checkout on 2026-10-01.
No files were rewound, because clear only forgets, and git status was clean.
Fixed: reversible_sessions sets AETHER_SESSION_DIR per session (235fbbc);
declared_signatures evaluates every example in an isolated session directory
(55d12b1), which matters because session_undo() and session_end() examples now exist;
session_journal_contracts was isolated from the start. A sentinel placed at the
checkout's journal path survived reversible_sessions, session_journal_contracts,
declared_signatures and four_more_knowable_conditions.

Validation on Windows passed 177 tests across 15 suites: declared_signatures 31,
effect_alias_agreement 3, effect_coverage 2, effect_ratchet 18, effect_snapshot 1,
expression_refactor 9, extract_function_refactor 6, four_more_knowable_conditions 10,
module_aliases 3, move_refactor 10, rename_refactor 8, reversible_sessions 10,
session_journal_contracts 11, signature_migration 39 and test_watch_contracts 16,
plus three journal unit tests. All-target workspace Clippy with warnings denied
passed, formatting passed, and the Rust 1.88 all-target workspace check passed.
On Linux (WSL Debian), test_watch_contracts 16, session_journal_contracts 11,
reversible_sessions 10 and four_more_knowable_conditions 10 passed, including the
Unix grandchild-kill test, and no stray child processes remained. macOS was not run.
Review added the process-tree kill. The work was done by parallel subagents in
worktrees and reviewed.

Live inventory is 623/1171 implementations and 748/1437 callable names, leaving
548 implementations and 689 names undeclared. No dependencies or package versions
changed. The source navigation, refactoring, search, tests, sessions and
documentation register item is checked: no unchecked bi_refactor_*, bi_test_*,
bi_session_*, bi_docs_*, bi_diag_*, bi_nav_* or bi_search_* candidate remains.
Final-tree workspace, cross-platform and release gates remain open. No version
bump/publication.

## Filesystem watch, SHA-crypt, listener and platform-branch checkpoint, 2026-10-17

fs_watch(path, options?) and fs_unwatch(handle) are implemented over the existing
notify dependency. Each watch has an explicit "watch-N" handle in a process-global
registry of at most 64 watches. Each handle has a bounded event buffer (max_events),
and later events are counted in dropped. Events are read by unwatching. Watches are
process-local, so one `ae -c` invocation cannot keep a watch across calls. Their
effects are corrected from Pure to ReadLocal.

crypto_password_verify verifies SHA-crypt $5$ and $6$ hashes per Drepper's
specification. It does not generate them. The vectors were fetched from the
specification text, because two remembered strings were wrong, and were
cross-checked against openssl passwd -6. Rounds above 1,000,000 are refused with
E_BUDGET_EXCEEDED, and the comparison is constant-time. Under AETHER_FIPS the call
is refused with E_POLICY_DENY, because only PBKDF2 is accepted there; review
reversed the agent's choice to allow SHA-crypt under that policy.

nc_listen(port, options?) is a one-shot loopback listener. The host option is
accepted only where AETHER_NET_ALLOW permits it. timeout_ms bounds the whole call,
and max_bytes with the truncated flag bounds the data. The bind address and the
accepted peer are both checked by guard_network. nc_listen is in SELF_GUARDED, so
guard_enforcement holds its body to a guard_network call. Its declared examples
are refusals only, so no example binds a port.

perm_check(path, "execute") on Windows is true only for a regular file whose
extension is listed in PATHEXT (default .COM;.EXE;.BAT;.CMD). ACLs are not
consulted.

A platform-branch sweep of ten builtins is covered by tests/os_branch_contracts.rs.
Silent failures became coded errors: sys_cpu_freq no longer returns Null when its
source fails; svc_status on Linux no longer answers status "" for an empty
ActiveState; and crypto_cert_verify no longer answers false for an existing file
that cannot be read. On Windows, svc_status reports a missing service as E_NOT_FOUND,
as declared; before this it was a tool failure.

Validation on Windows passed 286 tests across 18 suites: declared_signatures 31,
text_file_contracts 88, signature_migration 39, effect_ratchet 18,
test_watch_contracts 16, guard_enforcement 14, os_branch_contracts 12,
session_journal_contracts 11, reversible_sessions 10, four_more_knowable_conditions 10,
fs_watch_contracts 8, sha_crypt_contracts 8, nc_perm_contracts 8, crypto_fail_closed 4,
effect_alias_agreement 3, module_aliases 3, effect_coverage 2 and effect_snapshot 1.
All-target Clippy with warnings denied and the Rust 1.88 all-target check passed.
On Linux (WSL Debian), 193 tests passed across 11 suites: text_file_contracts 92,
test_watch_contracts 16, guard_enforcement 14, os_branch_contracts 13,
session_journal_contracts 11, reversible_sessions 10, four_more_knowable_conditions 10,
fs_watch_contracts 8, sha_crypt_contracts 8, nc_perm_contracts 7 and
crypto_fail_closed 4, and all-target Clippy passed, compiling the Unix-only branches.
macOS was not run; its sys_cpu_freq and svc_status branches rely on stderr text that
has not been verified on a Mac.

Live inventory is 624/1171 implementations and 749/1437 callable names, leaving 547
implementations and 688 names undeclared. The work was done by four parallel
subagents in worktrees and a classification agent, then reviewed and merged. No
dependencies or package versions changed. No version bump or publication.

## Linux system-administration backends checkpoint, 2026-10-19

Linux backends now implement user_add, user_del, user_mod, user_passwd, user_lock,
user_unlock, group_add, group_del, group_mod, acl_get, acl_set, fs_mount, fs_unmount,
svc_create, svc_delete, sudo_exec, and the apt-only package operations pkg_install,
pkg_remove, pkg_update, pkg_upgrade, pkg_clean, pkg_add_source, pkg_hold, pkg_unhold and
pkg_autoremove. Each runs as argv without a shell and refuses inside transactions.
Twenty-five builtins gained Linux backends (8a604bb, whose message says twenty-four). The twenty-four that change the system were classified Privileged in 97875f3 before any body was
implemented; acl_get is ReadLocal because it only reads. Windows and macOS refuse all of
them with E_UNIMPLEMENTED, and nothing is changed there.

Safety rules: passwords reach chpasswd on stdin only and are blanked from errors.
Accounts and groups below 1000, and the running user, are never deleted, modified, locked
or given a password. Users are never added to a group below gid 1000. ACL entries use a
strict syntax, with getfacl and setfacl run after --. Loop and bind mounts are used only
when asked, and /, /proc, /sys, /dev, /run, /boot, /usr, /etc, /var and /home are never
unmounted. Services live under ae-*, carry a marker line and are written atomically; a
failed reload or start removes the half-made unit, and svc_delete removes only marked
units. sudo_exec runs sudo -n and reports a password requirement as a refusal. Essential
and required packages are never removed, sources must be https and live under ae-*, and
apt runs in its own process group so a timeout kills dpkg too.

Verification ran for real as root in a separate WSL distro, ae-disposable, built from the
official Debian 13 (trixie) image. Its SHA-256 was verified against the OCI manifest layer
digest, and it runs systemd from a clean snapshot at E:\wsl\images\ae-disposable-base.tar.
scripts/disposable_linux_tests.sh builds the named tests as the developer and runs only
those test binaries as root there. System-changing tests are #[ignore]d and refuse unless
AETHER_DISPOSABLE=1, the /etc/ae-disposable marker and root are all present. 19 tests passed
(accounts 3, ACL/mount 5, services 4, apt 6, smoke 1), each checking its effect
independently with getent, passwd -S, getfacl, /proc/self/mounts, systemctl and dpkg.

Verification found that apt-get update exits 0 when a source cannot be fetched, so
pkg_add_source kept a broken source. --error-on=any now makes that an error, for pkg_update
as well.

Windows passed 16 suites: declared_signatures 31, text_file_contracts 87, signature_migration
39, effect_ratchet 18, guard_enforcement 14, four_more_knowable_conditions 10, packages 6,
accounts 5, acl_mount 5, privilege_escalation 5, services 3, effect_alias_agreement 3,
module_aliases 3, effect_coverage 2, effect_snapshot 1 and disposable_smoke 1. Clippy and the
Rust 1.88 check passed. Linux (WSL Debian) passed the same 16 suites, with text_file_contracts
91, and Clippy. env_java was added to the machine-dependent list because the Debian test
distro lacks java (12fb46e). The disposable runner now reports failed builds instead of
exiting silently.

Environment notes: WSL repeatedly failed with "Catastrophic failure", and builds were
OOM-killed while another project's Kani/cbmc runs used about 15 GB of the 45 GB WSL VM. The
user restarted WSL with wsl --shutdown. The shared E:\cargo-target\debug was cleared with the
user's authorization to free space.

Four parallel subagents worked in worktrees; their output was reviewed, merged, verified and
fixed centrally.

Live inventory is 635/1171 implementations and 762/1437 callable names, leaving 536
implementations and 675 names undeclared. No dependencies or package versions changed. No
version bump or publication.
