//! Safety core for AetherShell's agentic-first design.
//!
//! Implements the capability → policy → approval → audit model from
//! `docs/AGENTIC_FIRST_DESIGN.md` (§5.3, §7). Every effecting builtin can route
//! through [`guard`], which:
//!
//! 1. classifies the call by [`Effect`] (the effect taxonomy),
//! 2. enforces the workspace jail for filesystem-effecting classes,
//! 3. consults the active [`Policy`] for the current [`Mode`] to get a
//!    [`Decision`] (allow / deny / approve),
//! 4. resolves `approve` against supplied/granted approval tokens, and
//! 5. appends a tamper-evident entry to the hash-chained audit log.
//!
//! ## Default behaviour
//!
//! Defaults preserve the human REPL exactly (default-allow, no audit file) while
//! the agent surface is default-deny for the dangerous effect classes. Mode is
//! selected by `AETHER_MODE=agent` (or `AETHER_AGENT=1`); everything else is
//! human mode. This means existing scripts and tests are unaffected unless they
//! opt into agent mode.
//!
//! | Effect        | Human  | Agent    |
//! |---------------|--------|----------|
//! | Pure          | allow  | allow    |
//! | ReadLocal     | allow  | allow    |
//! | WriteLocal    | allow  | allow*   | (* jailed to workspace)
//! | Network       | allow  | allow    |
//! | Process       | allow  | approve  |
//! | Destructive   | allow  | approve* | (* jailed to workspace)
//! | Exec          | allow  | approve  |
//! | Privileged    | allow  | deny     |
//!
//! `AETHER_POLICY=permissive` makes agent mode behave like human mode (allow
//! all) for trusted automation; `AETHER_POLICY=strict` is the default.

use lazy_static::lazy_static;
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Mutex;

const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";

// ════════════════════════════════════════════════════════════════════════
// Effect taxonomy (§5.3)
// ════════════════════════════════════════════════════════════════════════

/// The effect class of a builtin — the single property the safety model reasons
/// about. Travels with the builtin so every surface (REPL, API, MCP, agent) is
/// gated identically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    /// No observable effect (math, string ops, pure transforms).
    Pure,
    /// Reads local state (filesystem reads, process listing, env).
    ReadLocal,
    /// Creates or modifies local state non-destructively (file write, mkdir).
    WriteLocal,
    /// Irreversibly removes or overwrites local state (rm, truncate, db delete).
    Destructive,
    /// Affects other processes (kill, signal).
    Process,
    /// Performs network I/O.
    Network,
    /// Executes an arbitrary external command (shell passthrough).
    Exec,
    /// Requires elevated privileges or affects system-wide state.
    Privileged,
}

impl Effect {
    /// Whether re-running an operation of this class is safe after an ambiguous
    /// failure — a timeout, a dropped connection, a killed process — where the agent
    /// cannot tell whether the first attempt took effect.
    ///
    /// This is a different question from [`ErrorCode::retryable`], which describes
    /// the *error*: a network timeout is a retryable error, but re-issuing the POST
    /// behind it may charge a card twice. An agent needs both, and conflating them
    /// is how duplicate side effects happen.
    ///
    /// Deliberately conservative — only `Pure` and `ReadLocal` are safe by class,
    /// and everything else must opt in per builtin via [`idempotent`]. The asymmetry
    /// is intentional: a false "unsafe" costs one stalled retry, a false "safe"
    /// costs a duplicated effect that cannot be taken back.
    pub fn retry_safe_by_class(&self) -> bool {
        matches!(self, Effect::Pure | Effect::ReadLocal)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Effect::Pure => "pure",
            Effect::ReadLocal => "read_local",
            Effect::WriteLocal => "write_local",
            Effect::Destructive => "destructive",
            Effect::Process => "process",
            Effect::Network => "network",
            Effect::Exec => "exec",
            Effect::Privileged => "privileged",
        }
    }

    /// Whether this effect touches the local filesystem in a way that must be
    /// confined to the workspace jail.
    pub fn is_filesystem(&self) -> bool {
        matches!(self, Effect::WriteLocal | Effect::Destructive)
    }
}

/// Whether FIPS-strict mode is active (`AETHER_FIPS` ∈ {`1`,`on`,`true`}). In FIPS
/// mode, non-FIPS-approved cryptographic algorithms (MD5, SHA-1) are refused, so any
/// security-relevant operation uses only FIPS-approved algorithms (SHA-2 family).
/// Note: this enforces *approved-algorithm-only* at the application layer; it does
/// not by itself make the underlying crypto a FIPS-140-*validated* module (see
/// `docs/security/CRYPTO_AND_FIPS.md`).
pub fn fips_enabled() -> bool {
    std::env::var("AETHER_FIPS")
        .map(|v| {
            let v = v.trim();
            v.eq_ignore_ascii_case("1")
                || v.eq_ignore_ascii_case("on")
                || v.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

/// Reject a non-FIPS-approved hash algorithm when FIPS mode is active. Returns an
/// `E_FIPS_DISALLOWED` error for `md5`/`sha1`; approved algorithms (and all calls
/// when FIPS mode is off) pass through. Pure aside from reading the FIPS flag.
pub fn require_fips_hash(algo: &str) -> anyhow::Result<()> {
    if fips_enabled() && is_weak_hash(algo) {
        return Err(anyhow::anyhow!(
            "E_FIPS_DISALLOWED: hash algorithm '{}' is not FIPS-approved (AETHER_FIPS active); \
             use sha256/sha384/sha512",
            algo
        ));
    }
    Ok(())
}

/// Whether `algo` names a non-FIPS-approved (legacy/broken) hash: MD5 or SHA-1.
pub fn is_weak_hash(algo: &str) -> bool {
    matches!(
        algo.trim().to_ascii_lowercase().as_str(),
        "md5" | "sha1" | "sha-1"
    )
}

/// Best-effort classifier for a builtin name, used by the ontology and audit
/// when an explicit [`Effect`] was not supplied at the call site. Conservative:
/// known-dangerous names are classified precisely; unknown names default to
/// [`Effect::Pure`] (callers that need gating pass the effect explicitly).
/// Whether re-running `name` after an ambiguous failure is safe — i.e. whether the
/// operation is idempotent, not whether the error was transient.
///
/// Most builtins inherit the answer from their effect class (see
/// [`Effect::retry_safe_by_class`]). The list below is the opt-in for
/// side-effecting builtins that are nonetheless idempotent because they express a
/// *desired end state* rather than an increment: writing a whole file, creating a
/// directory that may exist, deleting something already gone. Each is safe because
/// running it twice leaves the same state as running it once.
///
/// Anything absent is reported unsafe. That is the honest default for a table this
/// large — an unlisted builtin means "nobody has established that this is
/// idempotent", which is exactly what an agent should assume.
pub fn idempotent(name: &str) -> bool {
    // End-state writes: the content is fully specified, so a repeat is a no-op.
    const END_STATE: &[&str] = &[
        "write_file",
        "write_json",
        "text_write",
        "save_json",
        "fs_write",
        "mkdir",
        "fs_mkdir",
        "create_dir",
        "chmod",
        "fs_chmod",
        "chown",
        "fs_chown",
        "symlink",
        "fs_symlink",
    ];
    // Removals: the second attempt finds nothing to remove and converges.
    const REMOVALS: &[&str] = &[
        "rm",
        "remove_file",
        "fs_remove",
        "delete_file",
        "rmdir",
        "remove_dir",
        "kubectl_delete",
        "svc_delete",
        "role_delete",
        "delete_role",
    ];
    if END_STATE.contains(&name) || REMOVALS.contains(&name) {
        return true;
    }
    // Reads over the network are safe to repeat; writes over it are the canonical
    // non-idempotent case, so `Network` as a class does not qualify.
    if name.starts_with("http_get") || name.starts_with("web_get") {
        return true;
    }
    effect_of(name).retry_safe_by_class()
}

/// The effect a builtin declares, falling back to [`Effect::Pure`].
///
/// **A `Pure` from this function is not a finding.** It is either a
/// classification or the fall-through, and the two are indistinguishable here
/// by design — every caller that only needs to gate an action wants the
/// conservative answer without caring which. Callers that report the effect to
/// an *agent* should ask [`effect_is_declared`] as well, so a builtin nobody
/// has classified is not advertised with the same confidence as one that was
/// read and found pure.
pub fn effect_of(name: &str) -> Effect {
    classified_effect(name)
        .or_else(|| inherited_effect(name))
        .unwrap_or(Effect::Pure)
}

/// How strictly an effect is treated, for picking between siblings.
///
/// This orders *enforcement*, not danger: the four classes
/// [`centrally_enforced`] gates rank above the three it lets through, so
/// inheriting always picks the answer that keeps a gate rather than the one that
/// removes it. Within each band the order is the enum's own.
fn severity(effect: Effect) -> u8 {
    match effect {
        Effect::Pure => 0,
        Effect::ReadLocal => 1,
        Effect::WriteLocal => 2,
        Effect::Network => 3,
        Effect::Process => 4,
        Effect::Destructive => 5,
        Effect::Exec => 6,
        Effect::Privileged => 7,
    }
}

lazy_static! {
    /// The effect a name inherits from the other spellings of its own
    /// implementation.
    ///
    /// Dispatch is by implementation; classification is by name. `lldb_run` and
    /// `lldb` are one dispatch index, `vault_convert` and `vault-convert` are one
    /// match arm — and only one spelling of each was ever classified. The other
    /// read as `Pure`, `centrally_enforced(Pure)` is false, so
    /// [`guard_dispatch`] returned `Ok` before any policy ran and, because the
    /// audit line covers only `WriteLocal`/`Network`, left no trace either.
    ///
    /// Measured, not argued: 104 alias groups disagreed with themselves, 26 of
    /// them produced *different guard decisions* for the same implementation, and
    /// `lldb` ran an external debugger to completion in agent mode one call after
    /// `lldb_run` was refused for requiring approval.
    ///
    /// Built from `builtins::alias_groups`, so it cannot drift from the
    /// dispatcher the way a hand-written alias list would. A name classified in
    /// its own right is untouched — this only fills the fall-through, and it
    /// fills it with the strictest classification any sibling carries, because
    /// the failure that matters is a gate that quietly is not there.
    static ref INHERITED_EFFECT: std::collections::HashMap<&'static str, Effect> = {
        let mut map = std::collections::HashMap::new();
        for group in crate::builtins::alias_groups() {
            let strictest = group
                .iter()
                .filter_map(|n| classified_effect(n))
                .max_by_key(|e| severity(*e));
            let Some(strictest) = strictest else { continue };
            for name in group {
                if classified_effect(name).is_none() {
                    map.insert(name, strictest);
                }
            }
        }
        map
    };
}

fn inherited_effect(name: &str) -> Option<Effect> {
    INHERITED_EFFECT.get(name).copied()
}

/// Whether [`effect_of`]'s answer came from a rule rather than the fall-through.
///
/// A rule the name inherited from another spelling of its own implementation
/// counts: `lldb` is not classified in its own right, but `lldb_run` is, and
/// they are one dispatch index — the label `lldb` now carries was reasoned
/// about, just under the other name.
///
/// This is a claim about what has been *looked at*, which is weaker than a
/// claim that the rest do not act: that one belongs to the body-evidence
/// ratchet (`tests/effect_ratchet.rs`), which reports zero builtins acting while
/// classified `Pure` across both halves of the dispatcher. Agents deciding
/// whether to trust a label deserve to know which of the two they are reading.
pub fn effect_is_declared(name: &str) -> bool {
    classified_effect(name).is_some() || inherited_effect(name).is_some()
}

fn classified_effect(name: &str) -> Option<Effect> {
    match name {
        // The privilege boundary itself. `decide(Privileged, Agent)` is `Deny`,
        // and until these names arrived the class governed nothing: every other
        // privilege-shaped builtin in the catalog (`sudo_exec`, `user_add`,
        // `acl_set`, `fs_unmount`) is a stub that performs no effect, so `Pure`
        // is honest for them -- measured, not assumed.
        //
        // These are not stubs. `rbac_grant` writes into the permission store
        // `guard` consults, and `rbac_principal` decides which entry of that
        // store applies -- and an authorized principal *skips approval
        // entirely*. Classified `Pure`, they let an agent grant itself
        // `effect:*`, become that principal, and walk past the gate it had just
        // been refused by. `tests/privilege_escalation.rs` runs exactly that
        // sequence.
        //
        // Human mode still allows both: `Privileged` is `Allow` there, which is
        // how an operator administers RBAC in the first place.
        // `rbac_login`/`rbac_register` join them: authenticating is how
        // authority is acquired, so an agent able to do it unassisted has
        // none. `rbac_logout` is deliberately *not* here -- giving up
        // authority is the one privilege operation that cannot escalate --
        // and `rbac_session`/`rbac_can` only read.
        "rbac_grant" | "rbac_principal" | "rbac_login" | "rbac_register" => {
            Some(Effect::Privileged)
        }
        "rbac_logout" => Some(Effect::WriteLocal),
        "rbac_session" | "rbac_can" => Some(Effect::ReadLocal),
        "rm"
        | "rmdir"
        | "file_delete"
        | "file_delete_lines"
        | "db_kv_delete"
        | "db_sqlite_delete"
        | "docker_rm"
        | "docker_compose_down"
        | "truncate"
        | "k8s_delete"
        | "platform_db_delete"
        // Found by `tests/effect_coverage.rs`: these name a destructive action
        // and fell through to `Pure`, so `x-effect` advertised them to agents as
        // side-effect-free and `guard()` would have allowed them outright.
        | "cloud_instance_destroy"
        | "db_sqlite_drop_table"
        | "delete_lines"
        | "delete_role"
        | "kubectl_delete"
        | "role_delete"
        | "svc_delete"
        | "terraform_destroy"
        | "helm_uninstall"
        | "marketplace_uninstall" => Some(Effect::Destructive),
        // `sudo_check` shells out to determine admin status — it spawns a process,
        // so it is not pure even though it only reports.
        // Restarting a service is process-lifecycle control, not a data write:
        // it interrupts whatever that service was doing.
        "proc_kill" | "kill" | "signal" | "pkill" | "sudo_check"
        | "svc_restart" | "k8s_rollout_restart" => Some(Effect::Process),
        // Every builtin whose argument *is* a command to run. These are the
        // same capability as `sh` under different names; classifying them as
        // `Pure` told `agent_api`'s discovery — and any other consumer of this
        // function — that `timeout("rm -rf /")` was a side-effect-free call.
        // Keep in step with the `guard_exec` call sites in `builtins.rs`.
        "sh" | "exec" | "system" | "timeout" | "timeout_cmd" | "xargs" | "xargs_exec"
        | "proc_spawn" | "strace" | "strace_cmd" | "ltrace" | "ltrace_cmd"
        | "perf_stat" | "perf_record" | "tmux_new" | "tmux_send"
        // Also found by `tests/effect_coverage.rs`. Each of these runs a program
        // (locally, in a container, or on another host) with caller-supplied
        // input; `ssh_exec`/`sudo_exec`/`remote_exec` classifying as `Pure` was
        // the same defect as `timeout("rm -rf /")` classifying as `Pure`.
        | "docker_exec" | "podman_exec" | "k8s_exec" | "kubectl_exec"
        | "ssh_exec"
        | "rlm_spawn" | "spawn_agent"
        | "tool_exec" | "tool_execute"
        // These run the sqlite binary against a caller-supplied statement, which
        // includes DDL. Note this changes what is *advertised* and what `guard()`
        // would decide — neither is currently guarded, so nothing is newly gated.
        | "db_sqlite_exec" | "sqlite_exec"
        // Package installers. Each shells out to a package manager, which fetches
        // remote code and runs its install scripts — the supply-chain surface
        // (CWE-494). Classifying these `Pure` told every consumer of `effect_of`
        // that `npm_install("anything")` was side-effect-free.
        | "npm_install" | "yarn_install" | "pnpm_install" | "bun_install"
        | "pipx_install" | "poetry_install" | "pkg_install" | "asdf_install"
        | "helm_install" | "marketplace_install" | "pre_commit_install" => Some(Effect::Exec),
        // Egress by name. `scp_*` moves bytes to/from another host; the
        // `marketplace_publish` case sends a package somewhere it cannot be
        // recalled from.
        "scp_upload" | "scp_download" | "wget_download" | "marketplace_publish" => Some(Effect::Network),

        // The NERVOSYS stack. `ai_gateway` probes IronGate over HTTP; it reads
        // and reports, so Network rather than Exec.
        "ai_gateway" => Some(Effect::Network),
        // The vault builtins spawn the `iv` binary. `vault_models` and
        // `vault_conversions` only read, but they still hand arguments to a
        // process, which is what Exec is about — the ratchet reads
        // `Command::new` in `model_plane::vault_run` and would refuse anything
        // weaker. `vault_convert` additionally writes a converted model.
        "vault_models" | "vault_conversions" => Some(Effect::Exec),
        "vault_convert" | "ai_convert_model" => Some(Effect::Exec),

        // ════════════════════════════════════════════════════════════
        // Process-spawning builtins, classified from their argv (§12).
        //
        // These 306 were found by `tests/effect_ratchet.rs`, which reads function
        // bodies rather than names — the name-based reasoning is what produced the
        // original misclassifications. Each one below constructs an OS process while
        // `effect_of` returned `Pure`, i.e. every consumer of this function was told
        // it was side-effect-free.
        //
        // The tier comes from the argv the body actually builds, in this order:
        // deletes irrecoverably → Destructive; contacts another host → Network;
        // controls a process or window lifecycle → Process; writes a file or rewrites
        // source → WriteLocal; only reads *and* executes no caller- or project-supplied
        // code → ReadLocal; anything else → Exec. The default is the unsafe-side one:
        // an unclear argv is `Exec`, never `ReadLocal`.
        //
        // Three were not merely unclassified but actively dangerous while advertised
        // as pure: `git_clean -d` deletes untracked files, `session_rollback` is
        // `git reset --hard`, and `dd_copy` can overwrite a block device. Note also
        // `db_sqlite_query`, which passes caller SQL to the sqlite3 binary — `query`
        // is a name, not a constraint — and `diag_fix`/`refactor_remove_unused`, which
        // run `cargo fix --allow-dirty` and rewrite sources with the net switched off.
        //
        // Linters divide on whether they execute project-supplied code: `eslint`
        // loads and runs `eslint.config.js`, so it is `Exec`; `shellcheck`,
        // `hadolint` and `yamllint` only parse, so they are `ReadLocal`.
        // ---- Destructive (4) ----
        | "dd_copy"
        | "git_clean"
        | "git_reset"
        // `fs::rename` removes the source and can overwrite the destination;
        // it was classified `ReadLocal`.
        | "file_move" | "file_rename"
        | "session_rollback" => Some(Effect::Destructive),

        // ---- Network (24) ----
        | "gh_issue"
        | "gh_pr"
        | "gh_repo_cmd"
        | "git_fetch"
        | "git_pull"
        | "git_push"
        | "glab_issue"
        | "glab_mr"
        | "host_lookup"
        | "iperf3_client"
        | "mtr_trace"
        | "nmap_quick"
        | "nmap_scan"
        | "pkg_changelog"
        | "rsync_sync"
        | "rustup_update"
        | "skopeo_copy"
        | "skopeo_inspect"
        | "socat_relay"
        | "ssh_copy_id"
        | "ssh_tunnel"
        | "trivy_image"
        | "trivy_scan"
        | "uv_pip" => Some(Effect::Network),

        // ---- Process (13) ----
        | "gui_close_window"
        | "gui_focus_window"
        | "gui_maximize_window"
        | "gui_minimize_window"
        | "gui_move_window"
        | "gui_resize_window"
        | "screen_attach"
        | "screen_new"
        | "svc_disable"
        | "svc_enable"
        | "svc_start"
        | "svc_stop"
        | "tmux_attach" => Some(Effect::Process),

        // ---- WriteLocal (52) ----
        | "age_decrypt"
        | "age_encrypt"
        | "age_keygen"
        | "black_format"
        | "bzip2_compress"
        | "bzip2_decompress"
        | "chgrp"
        | "clipboard_set"
        | "cmake_configure"
        | "code_format"
        | "config_init"
        | "crypto_decrypt"
        | "crypto_encrypt"
        | "db_sqlite_backup"
        | "diag_fix"
        | "direnv_allow"
        | "docs_generate"
        | "fs_tempdir"
        | "fs_tempfile"
        | "gui_screenshot"
        | "gui_screenshot_window"
        | "gzip_compress"
        | "gzip_decompress"
        | "mise_use"
        | "mypy_check"
        | "platform_db_export"
        | "platform_db_import"
        | "platform_db_init"
        | "platform_db_store"
        | "prettier_format"
        | "refactor_remove_unused"
        | "refactor_rename_file"
        | "ruff_check"
        | "ruff_format"
        | "sd_replace"
        | "sed_replace"
        | "session_checkpoint"
        | "session_restore"
        | "shfmt_format"
        | "ssh_keygen"
        | "tar_create"
        | "tar_extract"
        | "tee_output"
        | "uv_venv"
        | "xz_compress"
        | "xz_decompress"
        | "zip_add"
        | "zip_create"
        | "zip_extract"
        | "zoxide_add"
        | "zstd_compress"
        | "zstd_decompress" => Some(Effect::WriteLocal),

        // ---- Exec (73) ----
        | "act_run"
        | "apply"
        | "buildah_build"
        | "bun_run"
        | "cargo_build_cmd"
        | "cargo_run"
        | "cargo_test_cmd"
        | "cmake_build"
        | "db_sqlite_query"
        | "deno_run"
        | "deno_task"
        | "diag_check"
        | "diag_errors"
        | "diag_lint"
        | "diag_warnings"
        | "docker_run"
        | "eslint_check"
        | "fzf_select"
        | "gdb_bt"
        | "gdb_run"
        | "git_add"
        | "git_checkout"
        | "git_cherry_pick"
        | "git_commit"
        | "git_merge"
        | "git_rebase"
        | "git_stash"
        | "git_stash_pop"
        | "go_build"
        | "go_run"
        | "go_test"
        | "gui_dialog_file_open"
        | "gui_dialog_folder"
        | "gui_dialog_input"
        | "gui_dialog_message"
        | "gui_key_combo"
        | "gui_key_press"
        | "gui_mouse_click"
        | "gui_mouse_drag"
        | "gui_mouse_move"
        | "gui_mouse_scroll"
        | "gui_notify"
        | "gui_type_text"
        | "hyperfine_bench"
        | "input_editor"
        | "input_read_password"
        | "jq_filter"
        | "just_run"
        | "lldb_run"
        | "lnav_open"
        | "logrotate_run"
        | "mage_run"
        | "make_run"
        | "multitail_open"
        | "ninja_build"
        | "node_run"
        | "npm_run"
        | "pnpm_run"
        | "podman_run"
        | "poetry_run"
        | "pre_commit_run"
        | "pytest_run"
        | "task_run"
        | "test_bench"
        | "test_list"
        | "test_run"
        | "test_run_file"
        | "test_run_function"
        | "uv_run"
        | "valgrind_memcheck"
        | "valgrind_run"
        | "yarn_run"
        | "yq_query" => Some(Effect::Exec),

        // ---- ReadLocal (140) ----
        | "archive_test"
        | "asdf_list"
        | "at_list"
        | "bat_view"
        | "blkid"
        | "buildah_images"
        | "capabilities"
        | "clipboard_get"
        | "clipboard_types"
        | "clipboard_history"
        | "cron_list"
        | "crypto_base64_decode"
        | "crypto_base64_encode"
        | "crypto_cert_info"
        | "crypto_cert_verify"
        | "crypto_hash"
        | "crypto_hash_file"
        | "crypto_hmac"
        | "crypto_jwt_decode"
        | "crypto_password_hash"
        | "crypto_random_bytes"
        | "crypto_random_string"
        | "crypto_uuid"
        | "db_csv_query"
        | "db_csv_to_json"
        | "db_json_query"
        | "db_json_to_csv"
        | "db_sqlite_dump"
        | "delta_diff"
        | "diag_explain"
        | "direnv_status"
        | "docs_search"
        | "env_docker"
        | "env_dotnet"
        | "env_go"
        | "env_java"
        | "env_node"
        | "env_python"
        | "env_ruby"
        | "env_rust"
        | "eza_list"
        | "fd_find"
        | "fs_df"
        | "git_blame"
        | "git_branch"
        | "git_branches"
        | "git_diff"
        | "git_diff_staged"
        | "git_log"
        | "git_remote"
        | "git_rev_parse"
        | "git_root"
        | "git_show"
        | "git_stash_list"
        | "git_status"
        | "git_tags"
        | "group_members"
        | "gui_color_picker"
        | "gui_get_active_window"
        | "gui_list_windows"
        | "gui_mouse_position"
        | "gui_ocr"
        | "gui_screen_size"
        | "hadolint_check"
        | "hw_audio"
        | "hw_battery"
        | "hw_gpu"
        | "hw_pci"
        | "hw_sensors"
        | "hw_usb"
        | "journalctl"
        | "just_list"
        | "lsof"
        | "mise_list"
        | "netstat_info"
        | "nm_symbols"
        | "objdump_disasm"
        | "objdump_headers"
        | "pgrep"
        | "pipx_list"
        | "pkg_deps"
        | "pkg_files"
        | "pkg_info"
        | "pkg_list"
        | "pkg_owner"
        | "pkg_rdeps"
        | "pkg_search"
        | "pkg_verify"
        | "platform_build"
        | "platform_capabilities"
        | "platform_cpu"
        | "platform_cpu_freq"
        | "platform_cuda_version"
        | "platform_disk_usage"
        | "platform_disks"
        | "platform_gpu_memory"
        | "platform_gpus"
        | "platform_has_admin"
        | "platform_hostname"
        | "platform_kernel"
        | "platform_lib_version"
        | "platform_libc"
        | "platform_libcpp"
        | "platform_libs"
        | "platform_memory"
        | "platform_memory_free"
        | "platform_memory_total"
        | "platform_network_interfaces"
        | "platform_os_version"
        | "platform_sdk_version"
        | "platform_snapshot"
        | "platform_ssl_version"
        | "platform_system_libs"
        | "project_loc"
        | "project_root"
        | "project_version"
        | "readelf_headers"
        | "readelf_sections"
        | "rg_search"
        | "rga_search"
        | "rustup_show"
        | "screen_list"
        | "search_by_size"
        | "search_by_type"
        | "search_code"
        | "search_files"
        | "search_fixmes"
        | "search_modified"
        | "search_recent"
        | "search_symbols"
        | "search_todos"
        | "session_diff_since"
        | "shellcheck_check"
        | "startup_list"
        | "strings_extract"
        | "svc_list"
        | "svc_logs"
        | "svc_status"
        | "tar_list"
        | "task_list"
        | "tmux_list"
        | "tokei_count"
        | "yamllint_check"
        | "zip_list"
        | "zoxide_query" => Some(Effect::ReadLocal),

        // ════════════════════════════════════════════════════════════
        // Builtins that act through a *helper* (§12).
        //
        // The 306 above were found by reading each builtin's own body. That
        // missed everything that hands its side effect to a helper: a builtin
        // calling `cloud_run_cmd`, or `kubectl_text`, or another builtin, is
        // exactly as dangerous as one spawning the process itself — only the
        // lint could not tell, which made the difference a blind spot rather
        // than a fact. Following calls two levels deep found 116 more.
        //
        // Each is classified from what its helper actually runs, and a builtin
        // that delegates to another builtin inherits that builtin's effect.
        // Two groups are deliberately *not* inherited: the `db_sqlite_*`
        // wrappers build their own statements, and the only reason
        // `db_sqlite_exec`/`_query` are `Exec` is that a caller can supply
        // arbitrary SQL — which these do not permit. They are the write/read
        // they appear to be.
        //
        // `syntax_*` are `WriteLocal` for an honest but unobvious reason: their
        // store is created lazily, so a lookup really does create a directory
        // the first time. `WriteLocal` is `Allow` in agent mode, so saying so
        // costs no friction and beats pretending the write does not happen.
        // ---- delegated: ReadLocal (41) ----
        | "clipboard_has_text"
        | "crypto_checksum"
        | "crypto_password_verify"
        | "db_kv_get"
        | "db_kv_keys"
        | "db_sqlite_count"
        | "db_sqlite_schema"
        | "db_sqlite_tables"
        | "db_sqlite_to_json"
        | "docker_images"
        | "docker_networks"
        | "docker_ps"
        | "docker_stats"
        | "docker_volumes"
        | "gpg_list_keys"
        | "group_list"
        | "hw_cpu"
        | "hw_disk"
        | "hw_memory"
        | "hw_network"
        | "openssl_cert_info"
        | "platform_build_systems"
        | "platform_cloud_clis"
        | "platform_compilers"
        | "platform_containers"
        | "platform_databases"
        | "platform_fingerprint"
        | "platform_hardware_summary"
        | "platform_iac_tools"
        | "platform_linters"
        | "platform_pkg_lang"
        | "platform_runtimes"
        | "platform_tool_version"
        | "platform_tool_versions"
        | "platform_vcs"
        | "search_grep"
        | "search_regex"
        | "session_changes"
        | "session_checkpoints"
        | "svc_info"
        | "user_list" => Some(Effect::ReadLocal),

        // ---- delegated: WriteLocal (21) ----
        | "clipboard_clear"
        | "crypto_generate_key"
        | "db_csv_to_sqlite"
        | "db_json_to_sqlite"
        | "db_kv_set"
        | "db_kv_store"
        | "db_sqlite_create_table"
        | "db_sqlite_import"
        | "db_sqlite_insert"
        | "db_sqlite_open"
        | "db_sqlite_to_csv"
        | "db_sqlite_update"
        | "db_sqlite_vacuum"
        | "gpg_decrypt"
        | "gpg_encrypt"
        | "openssl_genrsa"
        | "syntax_add"
        | "syntax_categories"
        | "syntax_get"
        | "syntax_list"
        | "syntax_search" => Some(Effect::WriteLocal),

        // ---- delegated: Network (23) ----
        | "helm_list"
        | "helm_repos"
        | "helm_search"
        | "helm_status"
        | "helm_upgrade"
        | "k8s_apply"
        | "k8s_cluster_info"
        | "k8s_configmaps"
        | "k8s_context"
        | "k8s_contexts"
        | "k8s_describe"
        | "k8s_events"
        | "k8s_get"
        | "k8s_ingresses"
        | "k8s_logs"
        | "k8s_namespaces"
        | "k8s_nodes"
        | "k8s_pods"
        | "k8s_rollout_status"
        | "k8s_scale"
        | "k8s_secrets"
        | "k8s_top_nodes"
        | "k8s_top_pods" => Some(Effect::Network),

        // ---- delegated: Exec (31) ----
        | "ansible_galaxy"
        | "ansible_inventory"
        | "ansible_playbook"
        | "ansible_vault"
        | "diag_all"
        | "docker_build"
        | "docker_compose_ps"
        | "docker_compose_up"
        | "docker_cp"
        | "docker_inspect"
        | "docker_logs"
        | "docker_pull"
        | "docker_push"
        | "docker_stop"
        | "docker_tag"
        | "docker_top"
        | "gui_mouse_double_click"
        | "podman_build"
        | "podman_images"
        | "podman_logs"
        | "podman_ps"
        | "podman_pull"
        | "podman_rm"
        | "podman_stop"
        | "terraform_apply"
        | "terraform_init"
        | "terraform_output"
        | "terraform_plan"
        | "terraform_state"
        | "terraform_validate"
        | "terraform_workspace" => Some(Effect::Exec),
        // ════════════════════════════════════════════════════════════
        // Found only after the lint stopped reading a single file (§12).
        //
        // The ratchet read `builtins.rs` alone, so an effect reached through
        // `security`, `os_tools` or any other module was invisible —
        // `Command::new` appears in six other modules, `fs::write` in ten.
        // Reading the whole crate, with precise markers, surfaced these six.
        //
        // `platform_has_network` binds a loopback socket and immediately drops
        // it to test whether networking works; nothing leaves the machine, so
        // it is a read rather than egress. `platform_machine_id` shells out to
        // `ioreg`/equivalent purely to read an identifier.
        "fs_link" | "fs_symlink" | "git_ignore" | "perm_set" => Some(Effect::WriteLocal),
        "platform_has_network" | "platform_machine_id" => Some(Effect::ReadLocal),
        // The `web_*` fetch family already routes through `guard_network` at each
        // call site, so it was *gated* as Network at runtime while `effect_of`
        // reported `Pure` — meaning the agent-facing ontology advertised
        // `web_post` as side-effect-free. Make the label agree with the control.
        n if n.starts_with("http")
            || n.starts_with("web_")
            || n.starts_with("net_")
            || n.starts_with("nc_") =>
        {
            Some(Effect::Network)
        }
        // `kubectl get` against a cluster endpoint: a read, but a *remote* one
        // that ships credentials off the machine, so it is metered as egress.
        "k8s_deployments" | "k8s_services" => Some(Effect::Network),
        // Listing mounts is a read; mounting one is not. Order matters here.
        "fs_mounts" => Some(Effect::ReadLocal),
        // Permission and ownership changes modify filesystem metadata.
        // `file_mkdir` is the same dispatch index as `mkdir`; without it the
        // `file_*` prefix rule below claimed the directory-creating spelling only
        // read. `tests/effect_alias_agreement.rs` is what noticed.
        "file_write" | "file_append" | "file_copy" | "mkdir" | "mkdirp" | "file_mkdir" | "touch"
        // `file_edit`, `file_insert` and `file_patch` read a file, change it and
        // write it back — the `file_*` prefix rule below saw only the read. The
        // jail keys on `WriteLocal`, so all three modified files anywhere on disk
        // in agent mode while `file_write` to the same path was refused.
        // `session_export` is the same shape one family over: labelled a read,
        // writes the diff to a caller-named path.
        | "file_edit" | "file_insert" | "file_patch" | "session_export"
        // `file_backup` copies onto `<path><suffix>`; the label said read.
        | "file_backup"
        | "write_file" | "write_json" | "text_write" | "save_json"
        | "gui_dialog_file_save" | "fs_mount"
        | "chmod" | "fs_chmod" | "fs_chown"
        // Changes what every later relative path means; `pwd()` answers
        // differently afterwards, so it is emphatically not Pure.
        | "cd" => Some(Effect::WriteLocal),
        // Found by body evidence, not by name: each of these opens a file,
        // lists a directory, stats a path or reads the environment, and every
        // one of them was falling through to `Pure`.
        //
        // This changes no policy -- `decide()` returns Allow for `Pure` and
        // `ReadLocal` alike -- it changes a *claim*. `Pure` says the call is
        // referentially transparent, so an agent may cache it, reorder it, or
        // skip a repeat. `ls` and `cat` are the plain counterexamples: their
        // answers change the moment anything touches the directory.
        "archive_info" | "audit_log" | "cat" | "code_comments" |
        "code_exports" | "code_imports" | "code_parse" | "code_symbols" |
        "code_todos" | "diag_config" | "docs_changelog" | "docs_examples" |
        "env" | "env_container" | "env_path" | "env_shell" |
        "env_var" | "env_vars" | "env_venv" | "fs_du" |
        "fs_glob" | "fs_lstat" | "fs_readlink" | "fs_realpath" |
        "fs_stat" | "fs_tree" | "fs_walk" | "grep" |
        "head" | "ls" | "make_targets" | "pager" |
        "perm_check" | "perm_get" | "pkg_history" | "pkg_sources" |
        "platform_db_list" | "platform_db_load" | "platform_has_gui" | "platform_shell_type" |
        "project_dependencies" | "project_dev_dependencies" | "project_gitignore" | "project_languages" |
        "project_license" | "project_name" | "project_readme" | "project_size" |
        "plan_diff" | "project_structure" | "project_test_files" | "read_text"
        | "refactor_organize_imports" |
        "ssh_config" | "tail" | "wc" => Some(Effect::ReadLocal),
        n if n.starts_with("file_") || n.starts_with("proc_") || n.starts_with("sys_") => {
            Some(Effect::ReadLocal)
        }
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════
// Mode & policy (§7.1)
// ════════════════════════════════════════════════════════════════════════

/// Execution surface, which selects policy defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Human at a REPL / running scripts — default-allow.
    Human,
    /// LLM agent over the API / agent syntax — default-deny for dangerous ops.
    Agent,
}

fn truthy_env(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("yes") | Some("on")
    )
}

#[cfg(test)]
lazy_static! {
    /// Serializes any test that mutates the process-global environment this
    /// module reads (`AETHER_MODE`, `AETHER_AGENT`, `AETHER_WORKSPACE`, …).
    ///
    /// Crate-visible on purpose. `security::validate_safe_path` consults
    /// [`current_mode`], so `security`'s tests are affected by an
    /// `AETHER_MODE=agent` set by a concurrently-running `safety` test — a
    /// `.` path that is fine in human mode gets jailed in agent mode. Every
    /// test on either side of that coupling must take *this* lock, not a
    /// module-private one.
    pub(crate) static ref ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
}

/// The active execution mode, derived from the environment.
pub fn current_mode() -> Mode {
    if std::env::var("AETHER_MODE").ok().as_deref() == Some("agent") || truthy_env("AETHER_AGENT") {
        Mode::Agent
    } else {
        Mode::Human
    }
}

/// Whether the policy has been globally relaxed to permissive (agent mode
/// behaves like human mode). `AETHER_POLICY=permissive`.
fn policy_permissive() -> bool {
    std::env::var("AETHER_POLICY").ok().as_deref() == Some("permissive")
}

/// The decision the policy engine reaches for a given effect under a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Run the operation.
    Allow,
    /// Refuse outright (no approval path).
    Deny,
    /// Refuse unless a matching approval token is present.
    Approve,
}

/// The default policy table (§7.1). `permissive` short-circuits to allow-all.
pub fn decide(effect: Effect, mode: Mode) -> Decision {
    if policy_permissive() {
        return Decision::Allow;
    }
    match mode {
        Mode::Human => Decision::Allow,
        Mode::Agent => match effect {
            Effect::Pure | Effect::ReadLocal | Effect::WriteLocal | Effect::Network => {
                Decision::Allow
            }
            Effect::Process | Effect::Destructive | Effect::Exec => Decision::Approve,
            Effect::Privileged => Decision::Deny,
        },
    }
}

// ════════════════════════════════════════════════════════════════════════
// Structured errors (§5.2, §7.3)
// ════════════════════════════════════════════════════════════════════════

/// Stable, machine-branchable error/refusal codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    PolicyDeny,
    NeedsApproval,
    OutsideWorkspace,
    /// A builtin was called with a missing or wrong-typed argument.
    BadArg,
    /// A resource governor (op count, files, processes, or wall-clock) was hit.
    BudgetExceeded,
    /// A builtin was called by a name that does not exist. Carries
    /// `did_you_mean` candidates, so this is the one *retryable* lookup failure.
    UnknownBuiltin,
    /// A record field (which is how module functions like `file.read` resolve)
    /// does not exist. Like `UnknownBuiltin`, carries `did_you_mean`.
    UnknownField,
    /// A failure that reached the boundary without a specific code. The message
    /// is whatever the builtin produced; treat it as opaque and **not**
    /// retryable — an agent that cannot identify the fault should stop rather
    /// than re-run the same call. Every uncoded failure lands here rather than
    /// escaping as bare prose, so *every* failure is branchable on `.code`.
    Unknown,
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::PolicyDeny => "E_POLICY_DENY",
            ErrorCode::NeedsApproval => "E_NEEDS_APPROVAL",
            ErrorCode::OutsideWorkspace => "E_OUTSIDE_WORKSPACE",
            ErrorCode::BadArg => "E_BAD_ARG",
            ErrorCode::BudgetExceeded => "E_BUDGET_EXCEEDED",
            ErrorCode::UnknownBuiltin => "E_UNKNOWN_BUILTIN",
            ErrorCode::UnknownField => "E_UNKNOWN_FIELD",
            ErrorCode::Unknown => "E_UNKNOWN",
        }
    }

    /// Whether re-running the call *with a correction* can plausibly succeed.
    ///
    /// This is the field a self-healing loop branches on, so it is deliberately
    /// conservative: a refusal (`PolicyDeny`), an exhausted envelope
    /// (`BudgetExceeded`), and an unidentified fault (`Unknown`) all report
    /// `false`, because retrying them burns budget without changing the outcome.
    pub fn retryable(&self) -> bool {
        match self {
            ErrorCode::BadArg | ErrorCode::UnknownBuiltin | ErrorCode::UnknownField => true,
            ErrorCode::NeedsApproval | ErrorCode::OutsideWorkspace => true,
            ErrorCode::PolicyDeny | ErrorCode::BudgetExceeded | ErrorCode::Unknown => false,
        }
    }
}

/// Build a structured argument error (`E_BAD_ARG`). Threads through `anyhow`/`?`
/// like any error, and — because it is a [`SafetyError`] — is caught by the
/// evaluator's try/catch as a structured `{error: {code, message, hint, …}}`
/// record, so an agent can branch on `e.error.code` and read the expected
/// signature instead of parsing prose. `got` is the offending value's type name
/// (e.g. `value.type_name()`), or `"nothing"` when an argument is missing.
pub fn bad_arg(builtin: &str, expected: &str, got: &str) -> anyhow::Error {
    anyhow::Error::new(SafetyError {
        code: ErrorCode::BadArg,
        message: format!("{}: expected {}, got {}", builtin, expected, got),
        builtin: builtin.to_string(),
        hint: format!("pass an argument matching: {}", expected),
        approval: None,
        did_you_mean: Vec::new(),
        expected: expected.to_string(),
        got: got.to_string(),
    })
}

/// Build a structured unknown-builtin error (`E_UNKNOWN_BUILTIN`) carrying the
/// nearest real names. `candidates` must already have been filtered against the
/// live builtin table — this constructor does not invent names.
pub fn unknown_builtin(name: &str, candidates: Vec<String>) -> anyhow::Error {
    let hint = if candidates.is_empty() {
        "no builtin has a similar name; call ontology_manifest() to list the \
         available categories"
            .to_string()
    } else {
        format!("did you mean: {}", candidates.join(", "))
    };
    anyhow::Error::new(SafetyError {
        code: ErrorCode::UnknownBuiltin,
        message: format!("unknown builtin: {}", name),
        builtin: name.to_string(),
        hint,
        approval: None,
        did_you_mean: candidates,
        expected: String::new(),
        got: String::new(),
    })
}

/// Build a structured unknown-field error (`E_UNKNOWN_FIELD`) carrying the
/// nearest real field names.
///
/// Module functions are record fields, so this is the code an agent sees for
/// `file.raed(…)` — the shape most agent typos actually take, since a model
/// writes dotted module paths far more often than bare builtin names.
/// `candidates` must come from the record's own keys.
pub fn unknown_field(field: &str, candidates: Vec<String>) -> anyhow::Error {
    let hint = if candidates.is_empty() {
        format!("no field named '{}'; check the record's keys", field)
    } else {
        format!("did you mean: {}", candidates.join(", "))
    };
    anyhow::Error::new(SafetyError {
        code: ErrorCode::UnknownField,
        message: format!("field '{}' not found in record", field),
        builtin: String::new(),
        hint,
        approval: None,
        did_you_mean: candidates,
        expected: String::new(),
        got: String::new(),
    })
}

/// Give an otherwise-uncoded failure a stable code (`E_UNKNOWN`).
///
/// This is the boundary net that makes "every failure is branchable" true
/// rather than aspirational: an error that is *already* a [`SafetyError`] is
/// returned untouched (its specific code is strictly better information), and
/// anything else is wrapped with its original message preserved verbatim.
pub fn ensure_structured(builtin: &str, e: anyhow::Error) -> anyhow::Error {
    if let Some(se) = e.downcast_ref::<SafetyError>() {
        // `arg_err` — by far the most-used structured helper — takes only a
        // message, so its `builtin` field is empty. The boundary is the one
        // place that knows the name being called, so fill it in here rather
        // than editing hundreds of call sites. Without this, `diagnose` cannot
        // look up the signature for the majority of E_BAD_ARG failures.
        if se.builtin.is_empty() && !builtin.is_empty() {
            let mut filled = se.clone();
            filled.builtin = builtin.to_string();
            return anyhow::Error::new(filled);
        }
        return e;
    }
    let message = e.to_string();
    anyhow::Error::new(SafetyError {
        code: ErrorCode::Unknown,
        message,
        builtin: builtin.to_string(),
        hint: format!(
            "{} failed without a specific error code; inspect the message rather \
             than retrying the same call",
            builtin
        ),
        approval: None,
        did_you_mean: Vec::new(),
        expected: String::new(),
        got: String::new(),
    })
}

/// Build a structured argument error (`E_BAD_ARG`) from a free-form arity/usage
/// message that already names the builtin and what it needs (e.g.
/// `"map requires a lambda"`). The missing/extra-argument counterpart to
/// [`bad_arg`] for the many call sites whose message doesn't split cleanly into
/// `expected`/`got`. Like [`bad_arg`] it is a [`SafetyError`], so try/catch binds
/// it as a structured `{error:{code:"E_BAD_ARG", message, hint, retryable}}`
/// record and the human REPL renders it as legible prose.
pub fn arg_err(message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(SafetyError {
        code: ErrorCode::BadArg,
        message: message.into(),
        builtin: String::new(),
        hint: "check this builtin's required argument count and types".to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: String::new(),
        got: String::new(),
    })
}

/// A describable action an agent may be asked to approve (§7.2). The `token`
/// is cryptographically bound to the action's content, so it cannot be replayed
/// to approve a different action.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ApprovalDescriptor {
    pub what: String,
    pub builtin: String,
    pub targets: Vec<String>,
    pub blast_radius: Json,
    pub reversible: bool,
    pub token: String,
}

impl ApprovalDescriptor {
    fn new(
        what: &str,
        builtin: &str,
        targets: Vec<String>,
        blast_radius: Json,
        reversible: bool,
    ) -> Self {
        // token = apv_<first 16 hex of sha256 over the action content>
        let content = json!({
            "what": what,
            "builtin": builtin,
            "targets": targets,
            "blast_radius": blast_radius,
            "reversible": reversible,
        });
        let mut hasher = Sha256::new();
        hasher.update(content.to_string().as_bytes());
        let digest = hasher.finalize();
        let token = format!("apv_{}", &hex(&digest)[..16]);
        Self {
            what: what.to_string(),
            builtin: builtin.to_string(),
            targets,
            blast_radius,
            reversible,
            token,
        }
    }
}

/// A safety refusal/failure carrying a stable code and an actionable hint.
/// Implements [`std::error::Error`] so it threads through `anyhow`/`?`.
#[derive(Debug, Clone)]
pub struct SafetyError {
    pub code: ErrorCode,
    pub message: String,
    pub builtin: String,
    pub hint: String,
    pub approval: Option<ApprovalDescriptor>,
    /// Nearest existing builtin names for a misspelled call. Emitted **only**
    /// when non-empty, and only ever containing names that really exist — an
    /// empty list is the honest answer when nothing is close, which costs an
    /// agent fewer tokens than a confident wrong guess costs it retries.
    pub did_you_mean: Vec<String>,
    /// What the call wanted, as a bare type/shape name, and what it received.
    /// The same facts `message` states in prose, but machine-readable: an agent
    /// repairing a `E_BAD_ARG` should not have to parse English to learn that an
    /// `Int` was wanted where a `Str` arrived. Both empty when not applicable.
    pub expected: String,
    pub got: String,
}

impl SafetyError {
    /// The structured JSON form an agent reads programmatically.
    pub fn to_json(&self) -> Json {
        let mut err = json!({
            "code": self.code.as_str(),
            "message": self.message,
            "builtin": self.builtin,
            "hint": self.hint,
            "retryable": self.code.retryable(),
        });
        if let Some(a) = &self.approval {
            err["approval"] = serde_json::to_value(a).unwrap_or(Json::Null);
        }
        if !self.did_you_mean.is_empty() {
            err["did_you_mean"] = serde_json::to_value(&self.did_you_mean).unwrap_or(Json::Null);
        }
        // Emitted only as a pair, and only when populated — a lone `expected` tells
        // an agent nothing it cannot already read in `hint`, and costs tokens.
        if !self.expected.is_empty() && !self.got.is_empty() {
            err["expected"] = Json::String(self.expected.clone());
            err["got"] = Json::String(self.got.clone());
        }
        json!({ "error": err })
    }
}

impl std::fmt::Display for SafetyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deterministic single-line rendering; the JSON form is authoritative.
        write!(f, "{}", self.to_json())
    }
}

impl std::error::Error for SafetyError {}

// ════════════════════════════════════════════════════════════════════════
// Workspace jail (§7.4)
// ════════════════════════════════════════════════════════════════════════

/// The workspace root. `AETHER_WORKSPACE` if set, else the current directory.
pub fn workspace_root() -> PathBuf {
    if let Ok(w) = std::env::var("AETHER_WORKSPACE") {
        PathBuf::from(w)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

/// Whether the jail is enforced. Enforced in agent mode, or whenever the
/// workspace root is set explicitly (humans opting into reproducible jails).
/// Whether `path` names the active audit log or the directory holding it.
///
/// Compared lexically after normalising separators, because the target may not
/// exist yet — a truncating write to a path that is about to *become* the log is
/// the same attack as editing the one that is there.
pub fn is_audit_artifact(path: &str) -> bool {
    let norm = |s: &str| {
        s.replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    let p = norm(path);
    if p.is_empty() {
        return false;
    }
    match audit_path() {
        Some(log) => {
            let l = norm(&log.to_string_lossy());
            if p == l {
                return true;
            }
            // The containing directory too, so `rm -r .ae` is caught — but
            // **only when that directory is ours**. The first version
            // protected the parent unconditionally, which meant an
            // `AETHER_AUDIT_LOG` pointing at `/tmp/x.log` made the whole of
            // `/tmp` unwritable. A guard that swallows a shared directory
            // because a file was placed in it is a denial of service, not a
            // control.
            match log.parent() {
                Some(dir) if dir.file_name().is_some_and(|n| n == ".ae") => {
                    let d = norm(&dir.to_string_lossy());
                    !d.is_empty() && (p == d || p.starts_with(&format!("{d}/")))
                }
                _ => false,
            }
        }
        None => false,
    }
}

fn jail_enforced(mode: Mode) -> bool {
    mode == Mode::Agent || std::env::var("AETHER_WORKSPACE").is_ok()
}

/// Resolve a user-supplied path the way an effecting builtin should operate on it.
///
/// Absolute paths are returned unchanged. In a **jailed** context (agent mode, or
/// an explicit `AETHER_WORKSPACE`) a relative path is resolved against the
/// workspace root, so writes/deletes land inside the jail and agree with both the
/// `within_workspace` check and the transaction journal — closing the gap where a
/// relative path was resolved against the process CWD (escaping the workspace when
/// CWD ≠ workspace). In plain human mode the path is left as-is, so the OS resolves
/// it against the current directory like any normal shell.
pub fn resolve_path_str(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() || !jail_enforced(current_mode()) {
        path.to_string()
    } else {
        workspace_root().join(p).to_string_lossy().to_string()
    }
}

/// Test whether a (possibly non-existent) path is contained in the workspace
/// root. Both sides are resolved to the same canonical form so the comparison
/// is correct across platforms (Windows verbatim `\\?\` prefixes, POSIX symlink
/// targets) and cannot be escaped by `..` traversal.
pub fn within_workspace(path: &str) -> bool {
    let root = match workspace_root().canonicalize() {
        Ok(r) => r,
        Err(_) => return false,
    };
    resolve_for_jail(path).starts_with(&root)
}

/// Lexically resolve a path to an absolute, `..`/`.`-free form against the
/// workspace root (no filesystem access; defends against traversal even for
/// paths that don't exist on disk).
fn lexical_abs(path: &str) -> PathBuf {
    use std::path::Component;
    let p = std::path::Path::new(path);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        workspace_root().join(p)
    };
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Resolve a path for jail comparison: lexically normalize it, then canonicalize
/// its deepest existing ancestor (resolving symlinks) and re-append the clean,
/// `..`-free remainder. This yields a path in the same canonical namespace as a
/// canonicalized workspace root, even when the leaf does not yet exist.
fn resolve_for_jail(path: &str) -> PathBuf {
    let clean = lexical_abs(path);
    for anc in clean.ancestors() {
        if let Ok(canon) = anc.canonicalize() {
            return match clean.strip_prefix(anc) {
                Ok(rest) => canon.join(rest),
                Err(_) => canon,
            };
        }
    }
    clean
}

// ════════════════════════════════════════════════════════════════════════
// Approval registry (§7.2)
// ════════════════════════════════════════════════════════════════════════

lazy_static! {
    static ref GRANTED: Mutex<HashSet<String>> = Mutex::new(HashSet::new());
}

/// Grant an approval token in-process (e.g. after an interactive confirmation
/// or an A2UI prompt). The token must equal the descriptor's bound token.
pub fn grant_approval(token: &str) {
    if let Ok(mut g) = GRANTED.lock() {
        g.insert(token.to_string());
    }
}

/// Revoke a previously granted token.
pub fn revoke_approval(token: &str) {
    if let Ok(mut g) = GRANTED.lock() {
        g.remove(token);
    }
}

/// Whether `token` has been approved (via `AETHER_APPROVE_ALL`, the
/// `AETHER_APPROVE` list, or an in-process `grant_approval`). Public so batch
/// executors (plan/apply) can gate a whole plan on a single approval token.
pub fn is_approved(token: &str) -> bool {
    is_token_approved(token)
}

fn is_token_approved(token: &str) -> bool {
    if truthy_env("AETHER_APPROVE_ALL") {
        return true;
    }
    // AETHER_APPROVE may carry a comma-separated list of approved tokens.
    if let Ok(list) = std::env::var("AETHER_APPROVE") {
        if list.split(',').map(str::trim).any(|t| t == token) {
            return true;
        }
    }
    GRANTED.lock().map(|g| g.contains(token)).unwrap_or(false)
}

// ════════════════════════════════════════════════════════════════════════
// RBAC principal authorization (§7.1) — an authenticated principal with the
// right permission bypasses the per-action approval requirement.
// ════════════════════════════════════════════════════════════════════════

lazy_static! {
    /// The RBAC manager backing principal authorization (None = RBAC disabled).
    static ref RBAC: Mutex<Option<std::sync::Arc<crate::auth::RbacManager>>> = Mutex::new(None);
    /// The current acting principal's user id (None = anonymous).
    static ref PRINCIPAL: Mutex<Option<String>> = Mutex::new(None);
}

/// Install the RBAC manager used to authorize principals against effect classes.
pub fn set_rbac_manager(mgr: std::sync::Arc<crate::auth::RbacManager>) {
    if let Ok(mut g) = RBAC.lock() {
        *g = Some(mgr);
    }
}

/// Disable RBAC authorization (revert to plain policy + approval).
pub fn clear_rbac_manager() {
    if let Ok(mut g) = RBAC.lock() {
        *g = None;
    }
}

/// Set the current acting principal (user id) for authorization decisions.
pub fn set_principal(user_id: Option<String>) {
    if let Ok(mut g) = PRINCIPAL.lock() {
        *g = user_id;
    }
}

/// The current acting principal's user id, if any.
pub fn current_principal() -> Option<String> {
    PRINCIPAL.lock().ok().and_then(|g| g.clone())
}

/// Load and install an RBAC manager from config at startup, if configured, and
/// set the acting principal. Sources, in order:
///
/// - `AETHER_PRINCIPAL=<id>` sets the acting principal (independent of a config).
/// - `AETHER_RBAC_CONFIG=<path>` (or `<workspace>/.ae/rbac.toml` if it exists)
///   is parsed as a TOML [`crate::auth::RbacConfig`], installed as the manager,
///   and its `principal` is used if `AETHER_PRINCIPAL` didn't already set one.
///
/// No-op when nothing is configured, so default runs are unaffected. Parse/read
/// failures warn (security_audit) and leave RBAC disabled rather than aborting.
pub fn init_rbac_from_env() {
    if let Ok(p) = std::env::var("AETHER_PRINCIPAL") {
        if !p.is_empty() {
            set_principal(Some(p));
        }
    }
    let path = match std::env::var("AETHER_RBAC_CONFIG") {
        Ok(p) => PathBuf::from(p),
        Err(_) => {
            let default = workspace_root().join(".ae").join("rbac.toml");
            if !default.exists() {
                return;
            }
            default
        }
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(target: "security_audit", "rbac config read {}: {}", path.display(), e);
            return;
        }
    };
    match crate::auth::RbacManager::from_config_str(&text) {
        Ok((mgr, principal)) => {
            set_rbac_manager(std::sync::Arc::new(mgr));
            if current_principal().is_none() {
                if let Some(p) = principal {
                    set_principal(Some(p));
                }
            }
        }
        Err(e) => {
            tracing::warn!(target: "security_audit", "rbac config parse {}: {}", path.display(), e);
        }
    }
}

/// Whether the current principal is RBAC-authorized for this effect. A grant
/// (`effect:<class>`, `effect:*`, `*:*`, or `builtin:<name>`) returns
/// `Some(true)`; the absence of a manager/principal or grant returns `None`
/// (defer to the default policy — RBAC here is additive, never a hard deny).
fn rbac_authorized(effect: Effect, builtin: &str) -> Option<bool> {
    let mgr = RBAC.lock().ok()?.clone()?;
    let user = current_principal()?;
    let by_effect = format!("effect:{}", effect.as_str());
    let by_builtin = format!("builtin:{}", builtin);
    if mgr.check_permission(&user, &by_effect) || mgr.check_permission(&user, &by_builtin) {
        Some(true)
    } else {
        None
    }
}

// ════════════════════════════════════════════════════════════════════════
// Hash-chained audit log (§7.5)
// ════════════════════════════════════════════════════════════════════════

struct AuditState {
    seq: u64,
    last_hash: String,
    /// The log path the in-memory chain (`seq`/`last_hash`) belongs to. When the
    /// active path differs, the chain is reset and the new file's tail reloaded,
    /// so switching logs never continues a stale chain into a fresh file.
    path: Option<PathBuf>,
}

lazy_static! {
    static ref AUDIT: Mutex<AuditState> = Mutex::new(AuditState {
        seq: 0,
        last_hash: GENESIS_HASH.to_string(),
        path: None,
    });
}

/// The audit log path, or `None` if auditing is disabled for this run.
/// `AETHER_AUDIT_LOG` overrides; in agent mode it defaults to
/// `<workspace>/.ae/audit.log`. Human mode without an explicit path = no audit
/// (so the default REPL has no side effects).
pub fn audit_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("AETHER_AUDIT_LOG") {
        return Some(PathBuf::from(p));
    }
    if current_mode() == Mode::Agent {
        return Some(workspace_root().join(".ae").join("audit.log"));
    }
    None
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

fn sha256_hex(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    hex(&h.finalize())
}

/// Recover `seq`/`last_hash` from the tail of an existing log so the chain
/// continues across process restarts. Assumes the caller has already reset
/// `seq`/`last_hash` to genesis for a fresh file.
fn load_tail(path: &PathBuf, state: &mut AuditState) {
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Some(last) = content.lines().rev().find(|l| !l.trim().is_empty()) {
            if let Ok(obj) = serde_json::from_str::<Json>(last) {
                if let Some(seq) = obj.get("seq").and_then(|v| v.as_u64()) {
                    state.seq = seq;
                }
                if let Some(h) = obj.get("entry_hash").and_then(|v| v.as_str()) {
                    state.last_hash = h.to_string();
                }
            }
        }
    }
}

/// Append a tamper-evident audit entry. Best-effort: a write failure logs a
/// warning but never blocks the guarded operation, unless
/// `AETHER_AUDIT_REQUIRED=1`, in which case the error is returned.
pub fn audit(
    builtin: &str,
    effect: Effect,
    decision: &str,
    resource: &str,
    detail: Json,
) -> Result<(), String> {
    let path = match audit_path() {
        Some(p) => p,
        None => return Ok(()),
    };

    let mut state = match AUDIT.lock() {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };
    // If the active log path changed (multi-workspace use, tests), reset the
    // chain and reload the new file's tail rather than continuing a stale chain.
    if state.path.as_deref() != Some(path.as_path()) {
        state.seq = 0;
        state.last_hash = GENESIS_HASH.to_string();
        state.path = Some(path.clone());
        load_tail(&path, &mut state);
    }

    let seq = state.seq + 1;
    let ts = chrono::Utc::now().to_rfc3339();
    let principal = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .ok();

    // Scrub secret shapes out of the resource string and detail metadata before
    // they are hashed and persisted — the audit log is a durable artifact, so a
    // leaked credential there would outlive the run (§7.6).
    let (resource, detail) = if redaction_enabled() {
        let (r, _) = redact_str(resource);
        let mut d = detail;
        redact_json(&mut d);
        (r, d)
    } else {
        (resource.to_string(), detail)
    };

    // Canonical core (everything but entry_hash); prev_hash chains entries.
    let core = json!({
        "seq": seq,
        "ts": ts,
        "principal": principal,
        "builtin": builtin,
        "effect": effect.as_str(),
        "decision": decision,
        "resource": resource,
        "detail": detail,
        "prev_hash": state.last_hash,
    });
    let entry_hash = sha256_hex(&core.to_string());

    let mut full = core;
    full["entry_hash"] = json!(entry_hash);
    let line = full.to_string();

    let write_result = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        writeln!(f, "{}", line)
    })();

    match write_result {
        Ok(()) => {
            state.seq = seq;
            state.last_hash = entry_hash;
            Ok(())
        }
        Err(e) => {
            if truthy_env("AETHER_AUDIT_REQUIRED") {
                Err(format!("audit write failed: {}", e))
            } else {
                tracing::warn!(target: "security_audit", "audit write failed: {}", e);
                Ok(())
            }
        }
    }
}

/// Read the most recent `n` audit entries (parsed JSON objects) from the active
/// log, oldest-to-newest. Returns empty if auditing is disabled or unreadable —
/// a read-only review of what was allowed / denied / approved.
pub fn read_audit_tail(n: usize) -> Vec<Json> {
    let path = match audit_path() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..]
        .iter()
        .filter_map(|l| serde_json::from_str::<Json>(l).ok())
        .collect()
}

/// Verify a hash-chained audit log. Returns the number of valid entries, or an
/// error describing the first inconsistency (broken hash, bad chain link, or
/// non-monotonic sequence).
pub fn verify_audit(path: &PathBuf) -> Result<u64, String> {
    let content = std::fs::read_to_string(path).map_err(|e| format!("read: {}", e))?;
    let mut prev = GENESIS_HASH.to_string();
    let mut expected_seq = 1u64;
    let mut count = 0u64;

    for (lineno, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut obj: Json =
            serde_json::from_str(line).map_err(|e| format!("line {}: parse: {}", lineno + 1, e))?;

        let stored = obj
            .get("entry_hash")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("line {}: missing entry_hash", lineno + 1))?
            .to_string();

        // Recompute over the core (object minus entry_hash). Removing the key
        // yields the same sorted object that was hashed at append time.
        if let Json::Object(m) = &mut obj {
            m.remove("entry_hash");
        }
        let recomputed = sha256_hex(&obj.to_string());
        if recomputed != stored {
            return Err(format!(
                "line {}: entry_hash mismatch (tampered)",
                lineno + 1
            ));
        }

        let prev_hash = obj.get("prev_hash").and_then(|v| v.as_str()).unwrap_or("");
        if prev_hash != prev {
            return Err(format!("line {}: broken chain link", lineno + 1));
        }

        let seq = obj.get("seq").and_then(|v| v.as_u64()).unwrap_or(0);
        if seq != expected_seq {
            return Err(format!(
                "line {}: non-monotonic seq (expected {}, got {})",
                lineno + 1,
                expected_seq,
                seq
            ));
        }

        prev = stored;
        expected_seq += 1;
        count += 1;
    }
    Ok(count)
}

// ════════════════════════════════════════════════════════════════════════
// Resource governors (§7.6)
// ════════════════════════════════════════════════════════════════════════
//
// A per-run blast-radius envelope: an agent may perform at most a bounded number
// of effecting operations and run for a bounded wall-clock time. Enforced at the
// `guard()` chokepoint (so it covers every effecting builtin uniformly) and only
// in agent mode. All limits are opt-in via env — unset = unlimited — so existing
// runs are unaffected until a limit is configured. A breach returns the structured
// `E_BUDGET_EXCEEDED` so an agent stops rather than looping. Counters tally
// *attempts* at the guard boundary (an op that is then denied by jail/policy still
// counts — the envelope bounds what the agent may try, the strictly-safe reading).
//
// | Env var             | Bounds                                              |
// |---------------------|-----------------------------------------------------|
// | `AETHER_MAX_OPS`    | total guarded operations                            |
// | `AETHER_MAX_FILES`  | filesystem ops (WriteLocal + Destructive)           |
// | `AETHER_MAX_PROCS`  | process/exec ops (Process + Exec)                   |
// | `AETHER_MAX_NET`    | network ops (Network) — egress request count        |
// | `AETHER_TIMEOUT_MS` | wall-clock ms since the first guarded op (or reset) |

#[derive(Default)]
struct GovernorState {
    start: Option<std::time::Instant>,
    total: u64,
    files: u64,
    procs: u64,
    net: u64,
}

lazy_static! {
    static ref GOVERNOR: Mutex<GovernorState> = Mutex::new(GovernorState::default());
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
}

fn budget_error(builtin: &str, message: String, hint: &str) -> SafetyError {
    SafetyError {
        code: ErrorCode::BudgetExceeded,
        message: format!("{}: {}", builtin, message),
        builtin: builtin.to_string(),
        hint: hint.to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: String::new(),
        got: String::new(),
    }
}

/// Account for one guarded operation against the resource governors and return
/// `E_BUDGET_EXCEEDED` if a configured limit is exceeded. No-op outside agent
/// mode or when no limit is set. Counts the attempt (increments before checking)
/// so the envelope bounds total attempts, not just successes.
fn governor_admit(effect: Effect, builtin: &str) -> Result<(), SafetyError> {
    if current_mode() != Mode::Agent {
        return Ok(());
    }
    let mut g = match GOVERNOR.lock() {
        Ok(g) => g,
        Err(_) => return Ok(()),
    };

    // Wall-clock: start the envelope lazily on the first guarded op.
    let now = std::time::Instant::now();
    let start = *g.start.get_or_insert(now);
    if let Some(limit_ms) = env_u64("AETHER_TIMEOUT_MS") {
        let elapsed = now.duration_since(start).as_millis() as u64;
        if elapsed > limit_ms {
            return Err(budget_error(
                builtin,
                format!("wall-clock budget exhausted ({}ms > {}ms)", elapsed, limit_ms),
                "the run exceeded AETHER_TIMEOUT_MS; reset with governor_reset(), start a new session, or raise the limit",
            ));
        }
    }

    g.total += 1;
    if let Some(max) = env_u64("AETHER_MAX_OPS") {
        if g.total > max {
            return Err(budget_error(
                builtin,
                format!("operation budget exhausted ({} > {})", g.total, max),
                "raise AETHER_MAX_OPS or call governor_reset() to start a fresh envelope",
            ));
        }
    }
    match effect {
        Effect::WriteLocal | Effect::Destructive => {
            g.files += 1;
            if let Some(max) = env_u64("AETHER_MAX_FILES") {
                if g.files > max {
                    return Err(budget_error(
                        builtin,
                        format!("file-operation budget exhausted ({} > {})", g.files, max),
                        "raise AETHER_MAX_FILES or call governor_reset()",
                    ));
                }
            }
        }
        Effect::Process | Effect::Exec => {
            g.procs += 1;
            if let Some(max) = env_u64("AETHER_MAX_PROCS") {
                if g.procs > max {
                    return Err(budget_error(
                        builtin,
                        format!("process budget exhausted ({} > {})", g.procs, max),
                        "raise AETHER_MAX_PROCS or call governor_reset()",
                    ));
                }
            }
        }
        Effect::Network => {
            g.net += 1;
            if let Some(max) = env_u64("AETHER_MAX_NET") {
                if g.net > max {
                    return Err(budget_error(
                        builtin,
                        format!("network-egress budget exhausted ({} > {})", g.net, max),
                        "raise AETHER_MAX_NET or call governor_reset()",
                    ));
                }
            }
        }
        _ => {}
    }
    Ok(())
}

/// Reset the resource-governor counters and wall-clock start (e.g. at a session
/// boundary or after a deliberate raise of the limits).
pub fn governor_reset() {
    if let Ok(mut g) = GOVERNOR.lock() {
        *g = GovernorState::default();
    }
}

/// Snapshot of the governors for introspection: current counts, the configured
/// limits (null = unlimited), and elapsed wall-clock ms. Lets an agent watch its
/// envelope burn down before it hits a wall.
pub fn governor_snapshot() -> Json {
    let g = match GOVERNOR.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    let elapsed_ms = g
        .start
        .map(|s| std::time::Instant::now().duration_since(s).as_millis() as u64)
        .unwrap_or(0);
    let lim = |n: &str| env_u64(n).map(|v| json!(v)).unwrap_or(Json::Null);
    json!({
        "active": current_mode() == Mode::Agent,
        "elapsed_ms": elapsed_ms,
        "used": { "ops": g.total, "files": g.files, "procs": g.procs, "net": g.net },
        "limits": {
            "max_ops": lim("AETHER_MAX_OPS"),
            "max_files": lim("AETHER_MAX_FILES"),
            "max_procs": lim("AETHER_MAX_PROCS"),
            "max_net": lim("AETHER_MAX_NET"),
            "timeout_ms": lim("AETHER_TIMEOUT_MS"),
        }
    })
}

// ════════════════════════════════════════════════════════════════════════
// The guard (§7) — the one entry point effecting builtins call.
// ════════════════════════════════════════════════════════════════════════

/// Context describing a single effecting call.
pub struct GuardCtx<'a> {
    /// Builtin name, e.g. `"rm"`.
    pub builtin: &'a str,
    /// Effect class of this call.
    pub effect: Effect,
    /// Verb for the approval descriptor, e.g. `"delete"`, `"kill"`, `"exec"`.
    pub what: &'a str,
    /// Concrete targets (paths, pids, command) for jail + descriptor + audit.
    pub targets: Vec<String>,
    /// Estimated blast radius, e.g. `{"files": 412, "bytes": 1200000000}`.
    pub blast_radius: Json,
    /// Whether the action can be undone.
    pub reversible: bool,
    /// Whether `targets` are local filesystem paths subject to the workspace
    /// jail. True for `rm`/file ops; false for non-path targets like database
    /// rows, container names, or shell commands.
    pub fs_paths: bool,
}

impl<'a> GuardCtx<'a> {
    /// Convenience constructor for the common single-target case. `fs_paths`
    /// defaults to whether the effect is a filesystem effect.
    pub fn new(builtin: &'a str, effect: Effect, what: &'a str, target: impl Into<String>) -> Self {
        Self {
            builtin,
            effect,
            what,
            targets: vec![target.into()],
            blast_radius: Json::Null,
            reversible: false,
            fs_paths: effect.is_filesystem(),
        }
    }
}

/// Gate a builtin that runs a caller-supplied program or shell command.
///
/// Until 2026-08-04 `sh` was the only builtin that gated on [`Effect::Exec`],
/// which made the exec control a denylist of exactly one *name* rather than of
/// the *capability*. `timeout`, `xargs`, `proc.spawn`, `nohup`, `strace`,
/// `ltrace` and the `perf` builtins all hand a caller-controlled string to a
/// shell, so in agent mode — with `sh` disabled outright, the intended hardened
/// configuration — an agent could still run any command it liked, with no
/// approval prompt and no `exec`-classified audit entry.
///
/// Every builtin whose argument *is* a command must route through here, so that
/// adding another such builtin cannot silently reopen the hole.
pub fn guard_exec(builtin: &str, command: impl Into<String>) -> Result<(), SafetyError> {
    let command = command.into();
    guard(GuardCtx {
        builtin,
        effect: Effect::Exec,
        what: "exec",
        targets: vec![command.clone()],
        blast_radius: serde_json::json!({ "command": command }),
        reversible: false,
        // A command string is not a path, so it must not be jailed as one.
        fs_paths: false,
    })
}

/// Reject caller-supplied *positional* arguments that a tool would parse as
/// options (CWE-88).
///
/// Fixing the program name is not enough when the program can be told to run a
/// command by a flag. `tar -cvf out.tar <files>` with a "file" named
/// `--use-compress-program=sh -c '…'` executes it; Info-ZIP's `-TT` sets the
/// command used to test an archive. Both turn a file list into arbitrary
/// execution while the builtin still looks like a pure archiving call.
///
/// `--` would stop option parsing in `tar` and `zip`, and is passed as well
/// where those are invoked, but support is not universal and getting it wrong is
/// silent. Refusing the argument outright is the check that does not depend on
/// the tool's parser. A path that genuinely starts with `-` is reachable as
/// `./-name`, which the hint says.
pub fn reject_option_like(builtin: &str, values: &[String]) -> anyhow::Result<()> {
    for v in values {
        if v.starts_with('-') {
            return Err(anyhow::Error::new(SafetyError {
                code: ErrorCode::BadArg,
                message: format!(
                    "{}: refusing an option-like path argument: {:?}",
                    builtin, v
                ),
                builtin: builtin.to_string(),
                hint: "this position is a file path, and a leading '-' would be parsed as \
                       an option by the underlying tool — several of which can be made to \
                       run a command that way; pass './-name' if the file really is named \
                       that"
                    .to_string(),
                approval: None,
                did_you_mean: Vec::new(),
                expected: String::new(),
                got: String::new(),
            }));
        }
    }
    Ok(())
}

/// Reject a URL that a desktop launcher would treat as something other than a
/// web address.
///
/// This is the *second* layer under `web_open_url`, and it is worth being clear
/// about which layer does what, because the obvious defence here does not work.
///
/// The first layer is structural: the Windows branch used to run
/// `cmd /C start <url>`, and `cmd` splits its command line on `&`. A URL of
/// `http://example.com&echo.>marker.txt` therefore ran `echo` — demonstrated,
/// not theorised. The tempting fix is to refuse `&`, and it is the wrong one:
/// `&` is the query-string separator, so it is *legal data* in the very values
/// this builtin exists to accept. A blocklist that refuses it breaks
/// `?a=1&b=2`; one that allows it leaves the hole open. The only fix that can be
/// both correct and complete is to stop handing the value to a shell at all,
/// which is what the call site now does.
///
/// What is left for this function is the part a shell-free launcher does not
/// solve:
///
/// * **Scheme.** `ShellExecute` and `xdg-open` dispatch on the scheme, so
///   `ms-msdt:`, `search-ms:` and friends reach registered handlers that are not
///   browsers — the Follina shape. An allowlist is right here because the set of
///   things a *web*-opening builtin should reach is small and closed.
/// * **Control characters.** A newline is a command separator in every shell,
///   so it must not survive into any future call site that has one.
/// * **A leading `-`.** The macOS and Linux branches pass the value positionally
///   to `open` and `xdg-open`; `open -a <app>` launches an arbitrary
///   application. This is the same defect [`reject_option_like`] exists for, and
///   the scheme check would catch it too — it is stated separately so the error
///   message says which problem it is.
pub fn reject_unsafe_url(builtin: &str, url: &str) -> anyhow::Result<()> {
    /// Schemes a URL-opening builtin may hand to the desktop's handler.
    ///
    /// This list may only shrink. Every addition widens the set of registered
    /// protocol handlers an agent can reach with a caller-controlled argument.
    const ALLOWED_SCHEMES: &[&str] = &["http", "https", "ftp", "ftps", "mailto", "file"];

    let err = |message: String, hint: &str| {
        anyhow::Error::new(SafetyError {
            code: ErrorCode::BadArg,
            message,
            builtin: builtin.to_string(),
            hint: hint.to_string(),
            approval: None,
            did_you_mean: Vec::new(),
            expected: format!(
                "a URL with one of these schemes: {}",
                ALLOWED_SCHEMES.join(", ")
            ),
            got: url.to_string(),
        })
    };

    if let Some(c) = url.chars().find(|c| c.is_ascii_control()) {
        return Err(err(
            format!(
                "{builtin}: refusing a URL containing a control character ({:#04x})",
                c as u32
            ),
            "a newline or carriage return separates commands in every shell; a URL \
             cannot legitimately contain one",
        ));
    }
    if url.starts_with('-') {
        return Err(err(
            format!("{builtin}: refusing an option-like URL: {url:?}"),
            "this position is passed positionally to `open`/`xdg-open`, and a leading \
             '-' is parsed as an option — `open -a <app>` launches an arbitrary \
             application",
        ));
    }
    let scheme = match url.split_once(':') {
        Some((s, _)) if !s.is_empty() => s.to_ascii_lowercase(),
        _ => {
            return Err(err(
                format!("{builtin}: refusing a URL with no scheme: {url:?}"),
                "give an absolute URL such as 'https://example.com'; a bare path is \
                 dispatched by the desktop handler rather than opened as a web page",
            ))
        }
    };
    if !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return Err(err(
            format!("{builtin}: refusing the URL scheme {scheme:?}"),
            "the desktop handler dispatches on the scheme, so a non-web scheme reaches \
             whatever program is registered for it rather than a browser",
        ));
    }
    Ok(())
}

/// Quote a value for interpolation into a **single-quoted** PowerShell string
/// literal (CWE-78).
///
/// Many Windows builtins build a command with `format!("Start-Service '{}'",
/// name)`. In a single-quoted PowerShell literal the only metacharacter is `'`
/// itself, so a value containing one closes the string and everything after it
/// is executed. Verified, not assumed: a service name of
/// `x'; New-Item -ItemType File -Path '…' -Force; '` created the file.
///
/// PowerShell escapes a quote inside a single-quoted string by doubling it, so
/// that is the whole transformation. The returned string **includes** the
/// surrounding quotes — callers interpolate it with `{}`, not `'{}'`, which
/// makes a missed call site visible as a syntax error rather than silently
/// unquoted.
///
/// This is for single-quoted context only. A double-quoted PowerShell string
/// also expands `$` and backtick, and must not use this.
pub fn ps_quote(value: &str) -> PsLiteral {
    PsLiteral(format!("'{}'", value.replace('\'', "''")))
}

/// A PowerShell string literal, quoted and escaped, that only [`ps_quote`] can
/// build.
///
/// The point is that the *type* records that quoting happened. A `String` in a
/// command builder proves nothing — findings 10a, 10c and 10d were all raw
/// strings reaching a `format!` that looked fine on review, three times. A value
/// of this type cannot be constructed except by going through the escaper,
/// because the field is private to this module.
///
/// It renders through `Display`, so `format!("Start-Service {}", ps_quote(&n))`
/// works unchanged. It deliberately does **not** implement `Deref<Target=str>`
/// or `From<String>`: either would let an unescaped value be substituted for an
/// escaped one, which is the whole thing this prevents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsLiteral(String);

impl std::fmt::Display for PsLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Build a PowerShell command string, accepting only already-escaped values.
///
/// `format!` is the hole the newtypes alone cannot close: it accepts anything
/// implementing `Display`, so `format!("Start-Service '{}'", name)` with a bare
/// `String` still compiles. That is the exact shape of findings 10a, 10c and
/// 10d, and it got past manual review three times.
///
/// This macro binds every argument to `&PsLiteral` before formatting, so a
/// `String` is a *compile* error rather than an injection:
///
/// ```
/// use aethershell::{ps_script, safety::ps_quote};
/// let name = "my service";
/// assert_eq!(
///     ps_script!("Start-Service {}", ps_quote(name)),
///     "Start-Service 'my service'"
/// );
/// ```
///
/// Note the template uses `{}`, not `'{}'` — the literal carries its own
/// quotes, so adding more would nest them.
#[macro_export]
macro_rules! ps_script {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {{
        // Type-check every argument. A `String` or a borrowed `&str` fails
        // here, naming the argument, before it can reach a shell.
        $($crate::safety::ps_arg(&$arg);)*
        format!($fmt $(, $arg)*)
    }};
}

/// Argument types [`ps_script!`] will accept.
///
/// Deliberately **not** implemented for `String` or for a non-`'static` `&str`:
/// those are how caller data arrives, and letting them through is the defect
/// this whole mechanism exists to prevent. To pass one, quote it with
/// [`ps_quote`] first.
///
/// The three things that *are* safe:
///
/// - [`PsLiteral`] — escaped by construction.
/// - Integers — no PowerShell metacharacter has a numeric representation.
/// - `&'static str` — a compile-time literal, so it cannot be caller data. This
///   covers the common `match algo { "sha256" => "SHA256", … }` shape, where the
///   value is one of a fixed set chosen in-tree. (`String::leak` could forge a
///   `&'static str`, but that is a deliberate act, not an accident.)
pub trait PsArg {}

impl PsArg for PsLiteral {}
impl PsArg for &'static str {}
macro_rules! impl_ps_arg_for_numbers {
    ($($t:ty),*) => { $(impl PsArg for $t {})* };
}
impl_ps_arg_for_numbers!(i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);

/// Type-check a [`ps_script!`] argument. Called by the macro; not useful alone.
#[doc(hidden)]
pub fn ps_arg<T: PsArg + ?Sized>(_: &T) {}

/// Validate a value that must be interpolated **unquoted** — a size or a port.
///
/// Some PowerShell parameters cannot take a quoted string: `-MemoryStartupBytes
/// '4GB'` is not the same as `-MemoryStartupBytes 4GB`, because `4GB` is a
/// numeric literal. So [`ps_quote`] is not available for them, and they were
/// interpolated bare — which the source lint cannot flag either, because it
/// looks for *quoted* placeholders.
///
/// Found by [`ps_script!`]'s type check, which is the one layer that sees an
/// unquoted interpolation: `vm.create(name, memory, disk)` and
/// `firewall.allow(port)` both put caller strings straight into a command.
/// (Both of those builtins were deleted in the 2026-08-26 dead-code pass — they
/// were never registered. The helper stays because the shape recurs, and the
/// history is kept so the next reader knows what it was written against.)
///
/// The check is a whitelist — digits, at most one decimal point, and an
/// optional size suffix — so nothing that could carry a metacharacter survives.
/// The returned [`PsLiteral`] carries no quotes; the type here means "checked
/// safe to interpolate", which for this shape is validation rather than quoting.
pub fn ps_bare_number(builtin: &str, value: &str) -> anyhow::Result<PsLiteral> {
    let v = value.trim();
    let (digits, suffix) = match v.find(|c: char| c.is_ascii_alphabetic()) {
        Some(i) => (&v[..i], &v[i..]),
        None => (v, ""),
    };

    let digits_ok = !digits.is_empty()
        && digits.chars().all(|c| c.is_ascii_digit() || c == '.')
        && digits.matches('.').count() <= 1;
    let suffix_ok = matches!(
        suffix.to_ascii_uppercase().as_str(),
        "" | "KB" | "MB" | "GB" | "TB" | "PB"
    );

    if digits_ok && suffix_ok {
        return Ok(PsLiteral(v.to_string()));
    }
    Err(anyhow::Error::new(SafetyError {
        code: ErrorCode::BadArg,
        message: format!("{}: expected a number or size, got {:?}", builtin, value),
        builtin: builtin.to_string(),
        hint: "this value is interpolated into a command unquoted, so it is \
               restricted to digits with an optional KB/MB/GB/TB/PB suffix"
            .to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: String::new(),
        got: String::new(),
    }))
}

/// Join already-escaped literals into one, for the `-Path a,b,c` shape.
///
/// Exists so that building a list does not require dropping to `String` and
/// thereby losing the type that says "this was escaped".
pub fn ps_join(values: impl IntoIterator<Item = PsLiteral>, sep: &str) -> PsLiteral {
    PsLiteral(
        values
            .into_iter()
            .map(|v| v.0)
            .collect::<Vec<_>>()
            .join(sep),
    )
}

/// Type-check an [`applescript!`] argument. Called by the macro.
#[doc(hidden)]
pub fn applescript_arg<T: AppleScriptArg + ?Sized>(_: &T) {}

/// Argument types [`applescript!`] accepts. See [`PsArg`].
pub trait AppleScriptArg {}
impl AppleScriptArg for AppleScriptLiteral {}
impl AppleScriptArg for &'static str {}

/// The AppleScript counterpart to [`ps_script!`].
#[macro_export]
macro_rules! applescript {
    ($fmt:literal $(, $arg:expr)* $(,)?) => {{
        $($crate::safety::applescript_arg(&$arg);)*
        format!($fmt $(, $arg)*)
    }};
}

/// An AppleScript string literal, quoted and escaped, that only
/// [`applescript_quote`] can build. See [`PsLiteral`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppleScriptLiteral(String);

impl std::fmt::Display for AppleScriptLiteral {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Quote a value for interpolation into an AppleScript string literal
/// (CWE-78, macOS).
///
/// The `osascript` counterpart to [`ps_quote`]. AppleScript string literals are
/// double-quoted and escape with a backslash, so an unescaped `"` closes the
/// literal — after which `" & (do shell script "…") & "` runs a command.
///
/// Backslash is escaped first, or escaping the quote would itself be undone.
/// As with `ps_quote`, the surrounding quotes are included so a missed call site
/// is a syntax error rather than a silently unquoted value.
pub fn applescript_quote(value: &str) -> AppleScriptLiteral {
    AppleScriptLiteral(format!(
        "\"{}\"",
        value.replace('\\', "\\\\").replace('"', "\\\"")
    ))
}

/// Reject a sqlite3 CLI *dot-command* where SQL is expected (CWE-77).
///
/// `sqlite3 <db> "<sql>"` accepts the CLI's own dot-commands in the SQL
/// position, and two of them run programs: `.system` and `.shell`. Verified —
/// `sqlite3 db ".system cmd /c echo … > file"` created the file. That turns
/// `db.sqlite_query` from "run a query" into "run anything", with no
/// `Effect::Exec` gate in front of it.
///
/// Dot-commands are a feature of the `sqlite3` shell, not of SQL, so refusing
/// them costs a caller nothing that SQL can express. The check is on the first
/// non-whitespace character: SQL statements never begin with `.`.
/// Render a value as a SQL string literal, safe to interpolate.
///
/// The sqlite builtins shell out to the `sqlite3` CLI, so there is no bound
/// parameter to use -- the SQL is a string by the time it leaves this process.
/// That made every `format!("... WHERE key = '{}'", key)` an injection point,
/// and not theoretically:
///
/// ```text
/// db_kv_get(db, "x' OR '1'='1")            -> returned another key's value
/// db_kv_delete(db, "z'; DELETE FROM kv; --") -> emptied the table (2 rows -> 0)
/// ```
///
/// SQLite escapes a quote inside a literal by doubling it, so `'` becomes `''`
/// and the value can no longer terminate its own literal. An interior NUL is
/// refused rather than escaped: the CLI takes its SQL as a C string, so a NUL
/// truncates the statement, and silently storing a shortened key would be its
/// own bug.
pub fn sql_literal(builtin: &str, value: &str) -> anyhow::Result<String> {
    if value.contains('\0') {
        return Err(anyhow::Error::new(SafetyError {
            code: ErrorCode::BadArg,
            message: format!("{builtin}: NUL byte in a SQL value"),
            builtin: builtin.to_string(),
            hint: "sqlite3 reads its SQL as a C string, so a NUL would silently                    truncate the statement; remove it"
                .to_string(),
            approval: None,
            did_you_mean: Vec::new(),
            expected: String::new(),
            got: String::new(),
        }));
    }
    Ok(format!("'{}'", value.replace('\'', "''")))
}

/// Validate a table or column name for interpolation.
///
/// An identifier cannot be passed as a string literal -- `SELECT * FROM 'kv'`
/// is not the same statement -- so the only safe move is to refuse anything
/// that is not plainly an identifier. Deliberately strict: letters, digits and
/// underscore, not starting with a digit. A caller that needs more can quote it
/// themselves and own the consequences.
pub fn sql_identifier(builtin: &str, name: &str) -> anyhow::Result<String> {
    let ok = !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if !ok {
        return Err(anyhow::Error::new(SafetyError {
            code: ErrorCode::BadArg,
            message: format!("{builtin}: {name:?} is not a valid table or column name"),
            builtin: builtin.to_string(),
            hint: "identifiers are interpolated into SQL and cannot be quoted as                    values; use letters, digits and underscore only"
                .to_string(),
            approval: None,
            did_you_mean: Vec::new(),
            expected: "an identifier".to_string(),
            got: name.to_string(),
        }));
    }
    Ok(name.to_string())
}

/// Validate a column *type* clause for `CREATE TABLE` — the last unvalidated
/// interpolation in the SQL family.
///
/// [`sql_identifier`] covers the name half of `"<name> <type>"`. The type half
/// was left as written, with a note saying that constraining it "means inventing
/// a grammar for SQL type expressions, which is a deliberate decision rather
/// than a drive-by one". This is that decision, taken rather than carried
/// forward again.
///
/// The grammar is a token allowlist, not a parser, because the value being
/// described is small and closed: a type name, an optional size, and a handful
/// of column constraints. Each whitespace-separated token must be one of
///
/// * a type or constraint keyword from the lists below, optionally carrying a
///   `(N)` or `(N,M)` size — `VARCHAR(255)`, `DECIMAL(10,2)`;
/// * a numeric literal, for `DEFAULT 0`;
/// * a single-quoted literal containing no quote of its own, for `DEFAULT ''`.
///
/// Anything else is refused. That is deliberately narrower than SQLite accepts —
/// SQLite's type affinity rules will take almost any word — and it refuses
/// `CHECK(…)` and `REFERENCES t(c)` along with the injections, because both
/// carry a parenthesised expression this cannot judge. A caller who genuinely
/// needs those has the raw column-definition branch, which is SQL by contract
/// and says so.
///
/// **Why now, for a builtin nothing can reach.** `db_sqlite_create_table` is in
/// the 168 unregistered implementations (§5 item 3), so this fixes nothing
/// exploitable today. It removes a *precondition*: registering it was previously
/// blocked on a security decision nobody had made, which is the kind of debt
/// that gets paid by whoever is registering builtins in a hurry.
pub fn sql_column_type(builtin: &str, spec: &str) -> anyhow::Result<String> {
    /// Type names accepted before the optional size. Covers the SQLite affinity
    /// families and the common spellings borrowed from other dialects.
    const TYPES: &[&str] = &[
        "INT",
        "INTEGER",
        "TINYINT",
        "SMALLINT",
        "MEDIUMINT",
        "BIGINT",
        "UNSIGNED",
        "BIG",
        "INT2",
        "INT8",
        "CHARACTER",
        "VARCHAR",
        "VARYING",
        "NCHAR",
        "NATIVE",
        "NVARCHAR",
        "TEXT",
        "CLOB",
        "BLOB",
        "REAL",
        "DOUBLE",
        "PRECISION",
        "FLOAT",
        "NUMERIC",
        "DECIMAL",
        "BOOLEAN",
        "DATE",
        "DATETIME",
        "TIMESTAMP",
    ];
    /// Column constraints, and the bare words that may follow `DEFAULT` or
    /// `COLLATE`. This list may only shrink.
    const KEYWORDS: &[&str] = &[
        "NOT",
        "NULL",
        "PRIMARY",
        "KEY",
        "AUTOINCREMENT",
        "UNIQUE",
        "DEFAULT",
        "COLLATE",
        "NOCASE",
        "BINARY",
        "RTRIM",
        "ASC",
        "DESC",
        "CURRENT_TIMESTAMP",
        "CURRENT_DATE",
        "CURRENT_TIME",
        "TRUE",
        "FALSE",
    ];

    let reject = |token: &str| {
        anyhow::Error::new(SafetyError {
            code: ErrorCode::BadArg,
            message: format!("{builtin}: {token:?} is not a valid column type or constraint"),
            builtin: builtin.to_string(),
            hint: "this is interpolated into a CREATE TABLE statement, which sqlite3 \
                   executes as written — use a type name with an optional size and \
                   constraints such as 'NOT NULL' or 'PRIMARY KEY'; for a CHECK or \
                   REFERENCES clause, pass the whole column definition as a string \
                   instead of a record"
                .to_string(),
            approval: None,
            did_you_mean: Vec::new(),
            expected: "a column type such as 'TEXT', 'VARCHAR(255) NOT NULL'".to_string(),
            got: spec.to_string(),
        })
    };

    /// A token stripped of a trailing `(N)` or `(N,M)`, or `None` if the
    /// parentheses are anything other than that.
    fn without_size(token: &str) -> Option<&str> {
        let Some(open) = token.find('(') else {
            return Some(token);
        };
        let rest = &token[open + 1..];
        let inner = rest.strip_suffix(')')?;
        let digits_ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit());
        let sized = match inner.split_once(',') {
            Some((a, b)) => digits_ok(a.trim()) && digits_ok(b.trim()),
            None => digits_ok(inner.trim()),
        };
        sized.then(|| &token[..open])
    }

    fn is_number(token: &str) -> bool {
        let t = token.strip_prefix('-').unwrap_or(token);
        match t.split_once('.') {
            Some((a, b)) => {
                !a.is_empty()
                    && !b.is_empty()
                    && a.chars().all(|c| c.is_ascii_digit())
                    && b.chars().all(|c| c.is_ascii_digit())
            }
            None => !t.is_empty() && t.chars().all(|c| c.is_ascii_digit()),
        }
    }

    /// `'literal'` with no embedded quote, so it cannot end early and start
    /// something else.
    fn is_plain_string_literal(token: &str) -> bool {
        token.len() >= 2
            && token.starts_with('\'')
            && token.ends_with('\'')
            && !token[1..token.len() - 1].contains('\'')
    }

    let spec = spec.trim();
    if spec.is_empty() {
        return Err(reject(""));
    }

    // Whitespace inside a size is not a token boundary. `DECIMAL(10, 2)` is
    // ordinary SQL and the first version of this split it into `DECIMAL(10,` and
    // `2)`, refusing both — the check has to accept what people actually write
    // or it is a ban wearing a validator's coat. Collapsing spaces *inside*
    // parentheses is safe because the parenthesised part is separately required
    // to be digits and at most one comma.
    let mut flattened = String::with_capacity(spec.len());
    let mut depth = 0i32;
    for c in spec.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth > 0 && c.is_whitespace() {
            continue;
        }
        flattened.push(c);
    }

    for token in flattened.split_whitespace() {
        if is_number(token) || is_plain_string_literal(token) {
            continue;
        }
        let Some(word) = without_size(token) else {
            return Err(reject(token));
        };
        let upper = word.to_ascii_uppercase();
        if TYPES.contains(&upper.as_str()) || KEYWORDS.contains(&upper.as_str()) {
            continue;
        }
        return Err(reject(token));
    }
    Ok(spec.to_string())
}

pub fn reject_sqlite_dot_command(builtin: &str, sql: &str) -> anyhow::Result<()> {
    if sql.trim_start().starts_with('.') {
        return Err(anyhow::Error::new(SafetyError {
            code: ErrorCode::BadArg,
            message: format!(
                "{}: refusing a sqlite3 dot-command where SQL is expected",
                builtin
            ),
            builtin: builtin.to_string(),
            hint: "`.system` and `.shell` run programs, so dot-commands are not \
                   accepted here; pass a SQL statement"
                .to_string(),
            approval: None,
            did_you_mean: Vec::new(),
            expected: String::new(),
            got: String::new(),
        }));
    }
    Ok(())
}

/// Gate an effecting call. Returns `Ok(())` if the call may proceed (and records
/// an audit entry), or a [`SafetyError`] with a stable code, an actionable hint,
/// and — for approvable actions — a bound approval token.
pub fn guard(ctx: GuardCtx) -> Result<(), SafetyError> {
    let mode = current_mode();
    let resource = ctx.targets.join(", ");

    // 0. Resource governors (§7.6): bound the per-run blast radius before any
    //    other check, so a runaway loop is stopped even if every op is allowed.
    if let Err(e) = governor_admit(ctx.effect, ctx.builtin) {
        let _ = audit(
            ctx.builtin,
            ctx.effect,
            "deny_budget",
            &resource,
            json!({ "governor": governor_snapshot() }),
        );
        return Err(e);
    }

    // 1. Workspace jail for filesystem path targets.
    if jail_enforced(mode) && ctx.effect.is_filesystem() && ctx.fs_paths {
        for t in &ctx.targets {
            if !within_workspace(t) {
                let _ = audit(
                    ctx.builtin,
                    ctx.effect,
                    "deny_outside_workspace",
                    &resource,
                    json!({ "target": t }),
                );
                return Err(SafetyError {
                    code: ErrorCode::OutsideWorkspace,
                    message: format!("{}: '{}' is outside the workspace root", ctx.builtin, t),
                    builtin: ctx.builtin.to_string(),
                    hint: format!(
                        "operate on paths under {} or set AETHER_WORKSPACE",
                        workspace_root().display()
                    ),
                    approval: None,
                    did_you_mean: Vec::new(),
                    expected: String::new(),
                    got: String::new(),
                });
            }
            // The audited party must not own the evidence (AS-2026-02).
            //
            // `audit_path()` defaults to `<workspace>/.ae/audit.log`, which is
            // inside the jail — the one region a jailed builtin may write. So an
            // ordinary `file.write` to it was allowed, and since the hash chain
            // is unkeyed, the log could be rewritten end-to-end with a fresh,
            // internally consistent chain that `audit_verify()` accepts.
            //
            // This is defence in depth, not a complete fix, and the limit is
            // worth stating: it stops a *jailed filesystem* builtin. An approved
            // `Exec` call can still reach the file, and the chain is still
            // unkeyed. Keying it needs key management, which is a design
            // decision rather than a patch. What this closes is the cheap path.
            if is_audit_artifact(t) {
                let _ = audit(
                    ctx.builtin,
                    ctx.effect,
                    "deny_audit_tamper",
                    &resource,
                    json!({ "target": t }),
                );
                return Err(SafetyError {
                    code: ErrorCode::OutsideWorkspace,
                    message: format!(
                        "{}: '{}' is the audit log or its directory and cannot be written by a guarded builtin",
                        ctx.builtin, t
                    ),
                    builtin: ctx.builtin.to_string(),
                    hint: "the audit trail is evidence about this session, so the session \
                           may not edit it; point AETHER_AUDIT_LOG elsewhere if you need a \
                           different location"
                        .to_string(),
                    approval: None,
                    did_you_mean: Vec::new(),
                    expected: String::new(),
                    got: String::new(),
                });
            }
        }
    }

    // 2. RBAC: an authorized principal bypasses the approval requirement.
    //    (The workspace jail above is intentionally NOT bypassed — defense in
    //    depth: authorization grants capabilities, not an escape from the jail.)
    if let Some(true) = rbac_authorized(ctx.effect, ctx.builtin) {
        let _ = audit(
            ctx.builtin,
            ctx.effect,
            "rbac_allow",
            &resource,
            json!({ "principal": current_principal() }),
        );
        return Ok(());
    }

    // 3. Policy decision.
    match decide(ctx.effect, mode) {
        Decision::Allow => {
            let _ = audit(ctx.builtin, ctx.effect, "allow", &resource, json!({}));
            Ok(())
        }
        Decision::Deny => {
            let _ = audit(ctx.builtin, ctx.effect, "deny", &resource, json!({}));
            Err(SafetyError {
                code: ErrorCode::PolicyDeny,
                message: format!(
                    "{}: {} operations are denied by policy in agent mode",
                    ctx.builtin,
                    ctx.effect.as_str()
                ),
                builtin: ctx.builtin.to_string(),
                hint: "this effect class has no approval path; perform it outside agent mode"
                    .to_string(),
                approval: None,
                did_you_mean: Vec::new(),
                expected: String::new(),
                got: String::new(),
            })
        }
        Decision::Approve => {
            let descriptor = ApprovalDescriptor::new(
                ctx.what,
                ctx.builtin,
                ctx.targets.clone(),
                ctx.blast_radius.clone(),
                ctx.reversible,
            );
            if is_token_approved(&descriptor.token) {
                let _ = audit(
                    ctx.builtin,
                    ctx.effect,
                    "approved",
                    &resource,
                    json!({ "token": descriptor.token }),
                );
                Ok(())
            } else {
                let _ = audit(
                    ctx.builtin,
                    ctx.effect,
                    "needs_approval",
                    &resource,
                    json!({ "token": descriptor.token }),
                );
                let token = descriptor.token.clone();
                Err(SafetyError {
                    code: ErrorCode::NeedsApproval,
                    message: format!("{}: requires approval ({})", ctx.builtin, ctx.what),
                    builtin: ctx.builtin.to_string(),
                    hint: format!(
                        "re-run with AETHER_APPROVE={} (or call approve(\"{}\"))",
                        token, token
                    ),
                    approval: Some(descriptor),
                    did_you_mean: Vec::new(),
                    expected: String::new(),
                    got: String::new(),
                })
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
// Central enforcement
// ════════════════════════════════════════════════════════════════════════
//
// `effect_of` is an *advertisement*; `guard` is the *control*. Until 7.1.0 only
// 51 of ~1,300 builtins reached the control, so 6.0.0's classification of 306
// process-spawning builtins improved what the ontology told an agent without
// changing what the shell would let one do. An agent that reads `x-effect` and
// respects it was protected; an agent that simply called the tool was not.
//
// `guard_dispatch` closes that at the dispatcher, which is the one place every
// builtin already passes through.

/// Builtins that enforce policy themselves. They are skipped centrally so a
/// single action is not admitted twice — double-guarding would charge the
/// resource governor twice and write two audit entries for one call.
///
/// "Enforces itself" means calling a `guard_*` helper **or** consulting the
/// approval system directly. That second form is not a technicality: `apply`
/// gates a whole plan on one plan-derived token, checks the jail per operation,
/// and snapshots into a transaction. Guarding it centrally as a generic `Exec`
/// demanded a *second*, unrelated token and pre-empted the `needs_approval`
/// response that carries the plan token — turning a working approval flow into
/// a dead end. Found by `tests/transactions.rs`, which is why the detector reads
/// for approval checks and not merely for the word `guard`.
///
/// Hand-written, and verified against the source by
/// `tests/guard_enforcement.rs`, so it cannot drift as call sites are added or
/// removed.
pub const SELF_GUARDED: &[&str] = &[
    "apply",
    "cd",
    "cloud_instance_destroy",
    "db_kv_delete",
    "db_sqlite_delete",
    "db_sqlite_drop_table",
    "docker_compose_down",
    "docker_exec",
    "docker_rm",
    "file_append",
    "file_backup",
    "file_copy",
    "file_delete_lines",
    "file_edit",
    "file_insert",
    "file_mkdir",
    "file_move",
    "file_patch",
    "file_write",
    "http_get",
    "k8s_delete",
    "k8s_exec",
    "ltrace_cmd",
    "perf_record",
    "perf_stat",
    "platform_db_delete",
    "podman_exec",
    "proc_kill",
    "proc_spawn",
    "rbac_principal",
    "rlm_spawn",
    "rm",
    "rmdir",
    "session_export",
    "sh",
    "ssh_exec",
    "strace_cmd",
    "tar_extract",
    "terraform_destroy",
    "timeout_cmd",
    "tmux_new",
    "tmux_send",
    "tool_exec",
    "web_check_url",
    "web_cookies",
    "web_download",
    "web_fetch",
    "web_form_submit",
    "web_graphql",
    "web_headers",
    "web_json_get",
    "web_json_post",
    "web_open_url",
    "web_post",
    "web_rest_api",
    "web_scrape",
    "web_upload_file",
    "wget_download",
    "xargs_exec",
    "zip_extract",
];

/// Whether an effect is one the policy table can refuse.
///
/// Only these are enforced centrally. In agent mode `WriteLocal` and `Network`
/// already decide to `Allow`, so guarding them here would change no decision
/// while doubling their audit and governor accounting — cost without safety.
/// They stay with the hand-written call sites, which know their real targets.
fn centrally_enforced(effect: Effect) -> bool {
    matches!(
        effect,
        Effect::Process | Effect::Destructive | Effect::Exec | Effect::Privileged
    )
}

/// Enforce policy for a builtin that does not guard itself. Called from the
/// dispatcher immediately before a builtin runs.
///
/// The targets are the call's string arguments, which is the best a central
/// point can do: it cannot know which of them are paths.
///
/// The jail is applied to the subset it *can* be sure about — arguments that
/// name a path which already exists on disk. A string that resolves to a real
/// file or directory is a path by observation rather than by guesswork, and a
/// mutating call naming one outside the workspace is exactly what the jail is
/// for. Anything else (a subcommand, a container name, a SQL fragment, a path
/// that does not yet exist) is left to the hand-written call sites, which know
/// their arguments' meaning. That asymmetry is deliberate: a missed jail check
/// is a gap the call sites still cover, while a false one rejects a legitimate
/// call and has no workaround.
pub fn existing_paths(args: &[String]) -> Vec<String> {
    args.iter()
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 4096
                && !s.contains('\n')
                && std::path::Path::new(s).exists()
        })
        .cloned()
        .collect()
}
pub fn guard_dispatch(builtin: &str, args: &[crate::value::Value]) -> Result<(), SafetyError> {
    let effect = effect_of(builtin);
    let args_as_strings: Vec<String> = args
        .iter()
        .filter_map(|a| match a {
            crate::value::Value::Str(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    if SELF_GUARDED.contains(&builtin) {
        return Ok(());
    }
    if !centrally_enforced(effect) {
        // Containment is not the policy decision, and conflating the two left the
        // jail reaching 8 of the 119 `WriteLocal` builtins.
        //
        // The original reasoning here was that `WriteLocal` and `Network` decide
        // `Allow`, so there is no decision to make and nothing to do but audit.
        // That is true about the *policy* and beside the point about the *jail*,
        // which lives inside `guard` and is therefore skipped by this early
        // return. §5.3 promises `WriteLocal` is "jailed to workspace"; in
        // practice only the eight builtins that guard themselves were, and
        // `copy_file` would overwrite a file outside the workspace that
        // `file_write` was refused for — demonstrated, then fixed here.
        //
        // Scope, stated precisely because the restriction is load-bearing: this
        // judges only arguments naming a path that *already exists*, exactly as
        // the centrally-enforced branch below does. A string that resolves to a
        // real file is a path by observation; one that does not may be a
        // container name, a subcommand or a SQL fragment, and refusing those
        // would break legitimate calls with no workaround. So creating a *new*
        // file outside the workspace is still the call site's job — what this
        // catches is overwriting something already there, which is the half that
        // can damage a system rather than litter it.
        // A central jail for `WriteLocal` was tried here and reverted, and the
        // reason is worth keeping so it is not tried again.
        //
        // The gap is real: `guard` holds the jail, this early return skips it, and
        // §5.3 promises `WriteLocal` is "jailed to workspace" — so `copy_file`
        // would overwrite a file outside the workspace that `file_write` was
        // refused for. Demonstrated in `tests/writelocal_jail.rs`.
        //
        // The obvious fix — run the jail over `existing_paths(&args)` here, as the
        // centrally-enforced branch does — was implemented, and then measured:
        // it refuses `copy_file <outside-source> <inside-destination>`, which is
        // copying a file *into* the workspace. That is a legitimate call. Reading
        // from outside is allowed by policy (`ReadLocal` is unjailed) and the
        // write lands inside the jail, so refusing it is a false positive with no
        // workaround — precisely what the comment below warns about.
        //
        // The two cases are indistinguishable from here: both are "a `WriteLocal`
        // call naming an existing path outside the workspace". Only the call site
        // knows which of its arguments is the destination. So the jail stays at
        // call sites, and `tests/writelocal_jail.rs` measures how many builtins
        // have one rather than assuming they all do.
        //
        // "Allowed" and "unobserved" are different things, and until this these
        // left no trace at all. Record them so an agent session can be
        // reconstructed afterwards, which is most of what an audit log is for.
        //
        // Agent surface only: a human REPL should not pay a log write per file
        // write, and its actions were never gated to begin with.
        if matches!(effect, Effect::WriteLocal | Effect::Network) && current_mode() == Mode::Agent {
            let _ = audit(
                builtin,
                effect,
                "allow_unguarded",
                &args_as_strings.join(", "),
                json!({ "central": true }),
            );
        }
        return Ok(());
    }
    // Jail only what is demonstrably a path. `guard` applies the workspace
    // check when `fs_paths` is set, so the targets handed to it in that case
    // must all be real paths — otherwise a container name or a subcommand would
    // be judged "outside the workspace" and the call refused for no reason.
    let real_paths = existing_paths(&args_as_strings);
    let jailable = effect.is_filesystem() && !real_paths.is_empty();
    let targets = if jailable {
        real_paths
    } else {
        args_as_strings
    };
    // The approval token is a hash of the descriptor, so whatever distinguishes
    // two calls must appear in it. Passing only the *string* arguments meant
    // `git_clean(true)` (dry run) and `git_clean(false)` (deletes untracked
    // files) produced an identical token: approving the harmless preview
    // silently authorised the destructive call, which is the exact opposite of
    // what content-binding is for. Every argument goes in, typed.
    let argv = serde_json::to_value(args).unwrap_or(Json::Null);
    guard(GuardCtx {
        builtin,
        effect,
        what: effect.as_str(),
        targets,
        blast_radius: json!({ "args": argv }),
        // Honest rather than optimistic: a journalled file write can be rewound,
        // but this path covers process/exec/destructive classes whose effects
        // reach outside the filesystem, and claiming reversibility would make an
        // approval prompt read as safer than it is.
        reversible: false,
        fs_paths: jailable,
    })
}

// ════════════════════════════════════════════════════════════════════════
// Secret hygiene (§7.6)
// ════════════════════════════════════════════════════════════════════════
//
// Two complementary defenses, both deterministic:
//
// 1. **Shape redaction** — `redact_str` scrubs known secret *shapes* (API-key
//    prefixes, JWTs, AWS access-key ids, PEM private-key blocks, URL credentials,
//    and `key=secret` assignment forms) from any text. Applied to agent output
//    (`builtins::render_agent`) and to every audit entry, so a secret that flows
//    through a result or a guarded call's metadata never lands in the agent's
//    context window or the persistent, hash-chained audit log.
// 2. **Name gating** — `env_secret_gated` reports whether reading an env var by a
//    secret-denoting name (`*_KEY`, `*TOKEN*`, `*SECRET*`, …) should be replaced
//    with an opaque `[REDACTED:NAME]` handle *before* the value ever enters the
//    program's value space. Active only in agent mode and only when the value
//    isn't explicitly permitted (`AETHER_SECRETS=allow`).
//
// All of this is opt-out via `AETHER_REDACT=off` for trusted automation; human
// mode keeps full fidelity (the gate is agent-only; render redaction runs on the
// agent render path only).

/// The marker substituted for a redacted secret. ASCII + bracketed to match the
/// house style (`[PATH]`, `[SECURITY WARNING]`) and to tokenize cheaply.
pub const REDACTION_MARKER: &str = "[REDACTED]";

lazy_static! {
    /// Self-contained secret tokens — the whole match *is* the secret and is
    /// replaced wholesale. Ordered alternation; each branch is anchored on a
    /// distinctive prefix/structure so it can't fire on ordinary prose.
    static ref SECRET_TOKEN_RE: regex::Regex = regex::Regex::new(concat!(
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
        r"|eyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}", // JWT
        r"|AKIA[0-9A-Z]{16}",                                                 // AWS access key id
        r"|sk-(?:ant-)?[A-Za-z0-9_-]{16,}",                                   // OpenAI / Anthropic
        r"|gh[pousr]_[A-Za-z0-9]{20,}",                                       // GitHub PAT/OAuth
        r"|xox[baprs]-[A-Za-z0-9-]{10,}",                                     // Slack
        r"|AIza[0-9A-Za-z_-]{20,}",                                           // Google API key
        r"|[rs]k_(?:live|test)_[A-Za-z0-9]{16,}",                             // Stripe
    )).unwrap();

    /// URL credentials: `scheme://user:password@host` — redact only the password,
    /// keeping the scheme/user/host so the result stays diagnostic.
    static ref URL_CRED_RE: regex::Regex =
        regex::Regex::new(r"([a-zA-Z][a-zA-Z0-9+.\-]*://[^/\s:@]+:)([^/\s:@]+)(@)").unwrap();

    /// `key = secret` / `key: secret` assignment forms (case-insensitive key,
    /// value ≥6 chars to skip trivial placeholders). Group 1 (key + separator)
    /// is kept; group 2 (the value) is redacted.
    static ref SECRET_ASSIGN_RE: regex::Regex = regex::Regex::new(
        r#"(?i)((?:password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key|client[_-]?secret|auth[_-]?token)["']?\s*[:=]\s*)["']?([^\s"',}]{6,})"#
    ).unwrap();
}

/// Whether secret redaction is active. On by default; `AETHER_REDACT=off`
/// (or `0`/`false`/`no`) disables it for trusted automation.
pub fn redaction_enabled() -> bool {
    !matches!(
        std::env::var("AETHER_REDACT").ok().as_deref(),
        Some("off") | Some("0") | Some("false") | Some("no")
    )
}

/// Whether the caller has explicitly permitted reading secret-named env vars in
/// the clear (`AETHER_SECRETS=allow`). Defaults to denied in agent mode.
pub fn secrets_permitted() -> bool {
    matches!(
        std::env::var("AETHER_SECRETS").ok().as_deref(),
        Some("allow") | Some("1") | Some("true")
    )
}

/// Whether an env-var name denotes a secret. Conservative substring match
/// (uppercased) on strong indicators only — `KEY` alone is excluded to avoid
/// `KEYBOARD`/`MONKEY` false positives; `_KEY` requires the underscore.
pub fn is_secret_name(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    const NEEDLES: &[&str] = &[
        "SECRET",
        "TOKEN",
        "PASSWORD",
        "PASSWD",
        "PASSPHRASE",
        "_KEY",
        "APIKEY",
        "API_KEY",
        "ACCESS_KEY",
        "PRIVATE_KEY",
        "CREDENTIAL",
        "CLIENT_SECRET",
        "AUTH_TOKEN",
        "SESSION_KEY",
    ];
    NEEDLES.iter().any(|needle| n.contains(needle))
}

/// Whether reading the env var `name` should be replaced with an opaque handle:
/// agent mode, redaction enabled, the name denotes a secret, and the operator
/// has not explicitly permitted clear reads. Human mode is never gated (a person
/// at a REPL reading their own env is legitimate and wants the value).
pub fn env_secret_gated(name: &str) -> bool {
    current_mode() == Mode::Agent
        && redaction_enabled()
        && !secrets_permitted()
        && is_secret_name(name)
}

/// Redact known secret shapes from a string. Returns the scrubbed string and
/// whether anything changed. Deterministic; each pass only ever *replaces* a
/// secret with [`REDACTION_MARKER`], never inflates non-secret text.
pub fn redact_str(s: &str) -> (String, bool) {
    let mut cur = s.to_string();
    let mut changed = false;
    if SECRET_TOKEN_RE.is_match(&cur) {
        cur = SECRET_TOKEN_RE
            .replace_all(&cur, REDACTION_MARKER)
            .into_owned();
        changed = true;
    }
    if URL_CRED_RE.is_match(&cur) {
        cur = URL_CRED_RE
            .replace_all(&cur, concat!("${1}", "[REDACTED]", "${3}"))
            .into_owned();
        changed = true;
    }
    if SECRET_ASSIGN_RE.is_match(&cur) {
        cur = SECRET_ASSIGN_RE
            .replace_all(&cur, concat!("${1}", "[REDACTED]"))
            .into_owned();
        changed = true;
    }
    (cur, changed)
}

/// Recursively redact secret shapes from a JSON value in place: every string
/// leaf is shape-scrubbed, and any object member whose *key* denotes a secret
/// is replaced with the marker even if its value isn't shape-matched. Used to
/// keep secrets out of the persistent audit log.
/// Whether a string is an approval or plan **capability handle** rather than a
/// credential.
///
/// `apv_…`/`apl_…` are content hashes the caller is required to echo back. They
/// are not secret — anyone who knows the action can recompute one — and hiding
/// them breaks the flows that depend on them: `plan()` returned an unusable
/// `token` field, and an audit entry saying `needs_approval` with the token
/// blanked cannot be correlated with the grant that followed it.
///
/// Shared by both redaction layers (`redact_json` here, `redact_field_map` in
/// `builtins`) so the two cannot drift apart.
pub fn is_capability_token(s: &str) -> bool {
    s.starts_with("apv_") || s.starts_with("apl_")
}

pub fn redact_json(v: &mut Json) {
    match v {
        Json::String(s) => {
            let (r, changed) = redact_str(s);
            if changed {
                *s = r;
            }
        }
        Json::Array(items) => {
            for it in items.iter_mut() {
                redact_json(it);
            }
        }
        Json::Object(map) => {
            for (k, val) in map.iter_mut() {
                // Only a *string* under a secret-sounding name can be a secret.
                // `TOKEN` matches `full_tokens`/`page_tokens`, which are counts,
                // and blanking a number destroys information for no gain.
                let hide =
                    is_secret_name(k) && matches!(val, Json::String(s) if !is_capability_token(s));
                if hide {
                    *val = Json::String(REDACTION_MARKER.to_string());
                } else {
                    redact_json(val);
                }
            }
        }
        _ => {}
    }
}

// ============================================================================
// Evaluation deadline
// ============================================================================
//
// Closes the residual left by finding 6a. The Agent API's request deadline
// frees the connection and the async worker, but it cannot stop the
// evaluation: dropping a `spawn_blocking` handle does not cancel the closure,
// so a wedged evaluation keeps a blocking-pool thread until it returns on its
// own. Stopping it needs cooperation from the interpreter, which is here.
//
// The language has no loop constructs — unbounded work arrives as recursion or
// as very large data — so a check at the top of `eval_expr` covers it.
//
// **What this bounds and what it does not.** It interrupts *evaluation*:
// runaway recursion and large computations. It cannot interrupt a builtin that
// is already blocking inside a syscall — `sleep 3600`, a subprocess wait, a
// network read — because those never return to the interpreter to be asked.
// That is a real remaining gap and it is the honest statement of it.

thread_local! {
    /// When evaluation must stop, if a limit is in force.
    static DEADLINE: std::cell::Cell<Option<std::time::Instant>> =
        const { std::cell::Cell::new(None) };
    /// Steps since the clock was last read.
    ///
    /// Sampling rather than reading `Instant::now()` per AST node is a
    /// precaution, not a measured optimisation — no before/after benchmark was
    /// run, and this comment previously implied one had been. A clock read is
    /// on the order of tens of nanoseconds and `eval_expr` is the interpreter's
    /// hot path, so the cost seemed worth avoiding for a deadline that does not
    /// need per-node resolution. If it ever matters, measure before tuning
    /// `DEADLINE_CHECK_INTERVAL`.
    static STEPS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// How many `eval_expr` entries pass between clock reads.
const DEADLINE_CHECK_INTERVAL: u32 = 1024;

/// Clears the deadline when it goes out of scope.
///
/// A bare setter would leave the deadline set on a pooled thread, so the *next*
/// piece of work on that thread would inherit an already-expired limit and fail
/// immediately. Tying it to a scope is what makes it safe to use from a thread
/// pool at all.
pub struct DeadlineGuard {
    previous: Option<std::time::Instant>,
}

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        DEADLINE.with(|d| d.set(self.previous));
        STEPS.with(|s| s.set(0));
    }
}

/// Bound evaluation on this thread to `limit`, until the returned guard drops.
///
/// Nested calls restore the outer deadline rather than clearing it.
pub fn enter_deadline(limit: std::time::Duration) -> DeadlineGuard {
    let previous = DEADLINE.with(|d| d.get());
    DEADLINE.with(|d| d.set(Some(std::time::Instant::now() + limit)));
    STEPS.with(|s| s.set(0));
    DeadlineGuard { previous }
}

/// Fail if this thread's evaluation deadline has passed.
///
/// Called from `eval_expr`. Cheap by design: with no deadline set — the REPL,
/// scripts, every test — this is one thread-local read and a `None` check.
#[inline]
pub fn check_deadline() -> anyhow::Result<()> {
    let n = STEPS.with(|s| {
        let n = s.get().wrapping_add(1);
        s.set(n);
        n
    });
    if !n.is_multiple_of(DEADLINE_CHECK_INTERVAL) {
        return Ok(());
    }
    DEADLINE.with(|d| match d.get() {
        Some(at) if std::time::Instant::now() >= at => Err(anyhow::anyhow!(
            "evaluation exceeded its time limit and was cancelled"
        )),
        _ => Ok(()),
    })
}

// ============================================================================
// Recursion depth
// ============================================================================
//
// Finding 13: `let f = fn(x) => f(x)` overflowed the stack and *aborted the
// process*. The evaluation deadline above cannot catch that — a stack overflow
// is not a `Result`, so nothing unwinds.
//
// Two halves, and the order matters. A depth limit on its own is not a fix:
// measured on Windows debug builds the stack died at ~35 frames, so a limit low
// enough to fire (~25) would reject ordinary recursive programs, while a usable
// limit (~1000) would never fire before the stack did. So the stack is enlarged
// first (`EVAL_STACK_SIZE`, applied at the process entry point and to the
// runtime's worker threads), which is what makes `MAX_CALL_DEPTH` meaningful.

/// Stack for threads that evaluate AetherShell code.
///
/// This is *reserved* address space, not committed memory — pages are backed
/// only as they are touched — so a large value costs essentially nothing on a
/// 64-bit host. Sized so that `MAX_CALL_DEPTH` is reached with a wide margin
/// even in a debug build, where frames were measured at roughly 30 KB.
pub const EVAL_STACK_SIZE: usize = 256 * 1024 * 1024;

/// Maximum nested lambda calls before evaluation is refused.
///
/// Chosen against the measured worst case rather than a round number:
/// 256 MB / ~30 KB per debug frame ≈ 8,500 frames available, so firing at 2,000
/// leaves roughly a 4× margin. Release frames are smaller, so the margin there
/// is larger still. Deep enough that realistic recursive programs are
/// unaffected; shallow enough to fire long before the stack does.
pub const MAX_CALL_DEPTH: u32 = 2_000;

thread_local! {
    static CALL_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Decrements the call depth however the call exits, including on `?`.
pub struct CallDepthGuard;

impl Drop for CallDepthGuard {
    fn drop(&mut self) {
        CALL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    }
}

/// Enter one level of lambda nesting, or fail if that would go too deep.
///
/// Hold the returned guard for the duration of the call — the depth is released
/// when it drops, so an error propagating out of a nested call still unwinds
/// the count correctly.
pub fn enter_call() -> anyhow::Result<CallDepthGuard> {
    let depth = CALL_DEPTH.with(|d| {
        let n = d.get() + 1;
        d.set(n);
        n
    });
    if depth > MAX_CALL_DEPTH {
        // Release immediately; the caller gets an error, not a guard.
        CALL_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        return Err(anyhow::anyhow!(
            "recursion too deep (limit {MAX_CALL_DEPTH}) — a function is most \
             likely calling itself without a base case"
        ));
    }
    Ok(CallDepthGuard)
}

/// Current nesting depth. Test/diagnostic use.
pub fn current_call_depth() -> u32 {
    CALL_DEPTH.with(|d| d.get())
}

/// Run `f` on a thread with a stack large enough for deep evaluation.
///
/// Used at the process entry point so the REPL, scripts and `-c` all evaluate
/// with room for `MAX_CALL_DEPTH`. Panics are propagated so behaviour matches
/// running `f` directly — swallowing them here would turn a crash into a silent
/// wrong answer.
pub fn with_eval_stack<T, F>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // No inline fallback: the closure is moved into `spawn`, so it cannot be
    // reused if that fails. Spawning the first thread failing means the process
    // is out of memory or handles, where a clear abort beats limping on with a
    // small stack and overflowing later somewhere less obvious.
    let handle = std::thread::Builder::new()
        .name("aether-eval".into())
        .stack_size(EVAL_STACK_SIZE)
        .spawn(f)
        .expect("failed to spawn the evaluation thread");

    match handle.join() {
        Ok(v) => v,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
mod depth_tests {
    use super::*;

    #[test]
    fn depth_is_released_when_the_guard_drops() {
        let before = current_call_depth();
        {
            let _g = enter_call().expect("first level must be allowed");
            assert_eq!(current_call_depth(), before + 1);
        }
        assert_eq!(
            current_call_depth(),
            before,
            "depth must unwind, or a long-lived thread would drift to the limit"
        );
    }

    #[test]
    fn exceeding_the_limit_is_an_error_not_a_crash() {
        let mut guards = Vec::new();
        let mut refused = false;
        for _ in 0..(MAX_CALL_DEPTH + 10) {
            match enter_call() {
                Ok(g) => guards.push(g),
                Err(e) => {
                    assert!(e.to_string().contains("recursion too deep"), "got {e}");
                    refused = true;
                    break;
                }
            }
        }
        assert!(refused, "the limit must be enforced");
        drop(guards);
        assert_eq!(current_call_depth(), 0, "depth must return to zero");
    }

    /// A refused call must not consume a level, or repeated failures would
    /// ratchet the counter down and reject progressively shallower calls.
    #[test]
    fn a_refused_call_does_not_leak_depth() {
        let mut guards = Vec::new();
        while let Ok(g) = enter_call() {
            guards.push(g);
        }
        let at_limit = current_call_depth();
        for _ in 0..5 {
            assert!(enter_call().is_err());
        }
        assert_eq!(
            current_call_depth(),
            at_limit,
            "a rejected call must leave the depth unchanged"
        );
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn no_deadline_means_no_interruption() {
        for _ in 0..(DEADLINE_CHECK_INTERVAL * 4) {
            check_deadline().expect("must not fail when no deadline is set");
        }
    }

    #[test]
    fn an_expired_deadline_is_reported() {
        let _g = enter_deadline(Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(20));
        // Only sampled every DEADLINE_CHECK_INTERVAL steps, so drive it there.
        let mut hit = false;
        for _ in 0..(DEADLINE_CHECK_INTERVAL * 2) {
            if check_deadline().is_err() {
                hit = true;
                break;
            }
        }
        assert!(hit, "an expired deadline must eventually stop evaluation");
    }

    #[test]
    fn a_live_deadline_does_not_fire() {
        let _g = enter_deadline(Duration::from_secs(60));
        for _ in 0..(DEADLINE_CHECK_INTERVAL * 4) {
            check_deadline().expect("a deadline far in the future must not fire");
        }
    }

    /// The guard must restore, not clear — otherwise the next task on a pooled
    /// thread inherits an expired deadline and fails for no reason.
    #[test]
    fn the_guard_restores_state_so_pooled_threads_are_not_poisoned() {
        {
            let _g = enter_deadline(Duration::from_millis(1));
            std::thread::sleep(Duration::from_millis(20));
        }
        for _ in 0..(DEADLINE_CHECK_INTERVAL * 4) {
            check_deadline().expect("deadline must not outlive its guard");
        }
    }

    #[test]
    fn nesting_restores_the_outer_deadline() {
        let _outer = enter_deadline(Duration::from_secs(60));
        {
            let _inner = enter_deadline(Duration::from_millis(1));
            std::thread::sleep(Duration::from_millis(20));
        }
        for _ in 0..(DEADLINE_CHECK_INTERVAL * 4) {
            check_deadline().expect("the outer deadline is still live and must not fire");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env mutation in these tests is process-global; serialize them via the
    // crate-wide lock (defined at module scope, since tests in other modules
    // read `AETHER_MODE` too and must take the same lock).
    use super::ENV_LOCK;

    fn clear_env() {
        for k in [
            "AETHER_MODE",
            "AETHER_AGENT",
            "AETHER_POLICY",
            "AETHER_APPROVE",
            "AETHER_APPROVE_ALL",
            "AETHER_WORKSPACE",
            "AETHER_AUDIT_LOG",
            "AETHER_AUDIT_REQUIRED",
            "AETHER_MAX_OPS",
            "AETHER_MAX_FILES",
            "AETHER_MAX_PROCS",
            "AETHER_MAX_NET",
            "AETHER_TIMEOUT_MS",
        ] {
            std::env::remove_var(k);
        }
        if let Ok(mut g) = GRANTED.lock() {
            g.clear();
        }
        set_principal(None);
        clear_rbac_manager();
        governor_reset();
    }

    /// Point auditing at a unique throwaway file and reset the in-memory chain,
    /// so tests that exercise agent mode don't share or inherit chain state.
    fn isolate_audit(tag: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("ae_audit_{}_{}.log", tag, std::process::id()));
        let _ = std::fs::remove_file(&p);
        std::env::set_var("AETHER_AUDIT_LOG", p.to_string_lossy().to_string());
        // The audit layer resets its chain automatically when the log path
        // changes, so no manual state reset is needed here.
        p
    }

    #[test]
    fn human_mode_allows_everything() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(decide(Effect::Destructive, Mode::Human), Decision::Allow);
        assert_eq!(decide(Effect::Exec, Mode::Human), Decision::Allow);
        assert_eq!(decide(Effect::Privileged, Mode::Human), Decision::Allow);
    }

    #[test]
    fn fips_mode_rejects_non_approved_hashes() {
        // Pure classifier (no env).
        assert!(is_weak_hash("md5") && is_weak_hash("MD5") && is_weak_hash("sha-1"));
        assert!(!is_weak_hash("sha256") && !is_weak_hash("sha512"));

        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        // FIPS off (default) → all algorithms pass through.
        assert!(require_fips_hash("md5").is_ok());
        // FIPS on → md5/sha1 refused, SHA-2 family allowed.
        std::env::set_var("AETHER_FIPS", "1");
        assert!(require_fips_hash("md5").is_err());
        assert!(require_fips_hash("sha1").is_err());
        assert!(require_fips_hash("sha256").is_ok());
        std::env::remove_var("AETHER_FIPS");
    }

    #[test]
    fn agent_mode_gates_dangerous() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        assert_eq!(decide(Effect::ReadLocal, Mode::Agent), Decision::Allow);
        assert_eq!(decide(Effect::Destructive, Mode::Agent), Decision::Approve);
        assert_eq!(decide(Effect::Exec, Mode::Agent), Decision::Approve);
        assert_eq!(decide(Effect::Privileged, Mode::Agent), Decision::Deny);
    }

    #[test]
    fn permissive_policy_allows_all_in_agent() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("AETHER_POLICY", "permissive");
        assert_eq!(decide(Effect::Destructive, Mode::Agent), Decision::Allow);
        assert_eq!(decide(Effect::Privileged, Mode::Agent), Decision::Allow);
        clear_env();
    }

    #[test]
    fn approval_token_is_bound_to_action() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        let a = ApprovalDescriptor::new(
            "delete",
            "rm",
            vec!["/x/a".into()],
            json!({"files":1}),
            false,
        );
        let b = ApprovalDescriptor::new(
            "delete",
            "rm",
            vec!["/x/b".into()],
            json!({"files":1}),
            false,
        );
        assert_ne!(
            a.token, b.token,
            "different targets must yield different tokens"
        );
        assert!(a.token.starts_with("apv_"));
    }

    #[test]
    fn guard_blocks_then_allows_with_token() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        let log = isolate_audit("approve");
        // Use a workspace so the jail does not interfere with this approval test.
        let tmp = std::env::temp_dir();
        std::env::set_var("AETHER_WORKSPACE", &tmp);
        let target = tmp
            .join("ae_safety_test_file")
            .to_string_lossy()
            .to_string();

        let mk = || GuardCtx {
            builtin: "rm",
            effect: Effect::Destructive,
            what: "delete",
            targets: vec![target.clone()],
            blast_radius: json!({"files": 1}),
            reversible: false,
            fs_paths: true,
        };

        let err = guard(mk()).unwrap_err();
        assert_eq!(err.code, ErrorCode::NeedsApproval);
        let token = err.approval.unwrap().token;

        std::env::set_var("AETHER_APPROVE", &token);
        assert!(
            guard(mk()).is_ok(),
            "matching token should permit the action"
        );
        let _ = std::fs::remove_file(&log);
        clear_env();
    }

    #[test]
    fn jail_blocks_paths_outside_workspace() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        let log = isolate_audit("jail");
        let tmp = std::env::temp_dir();
        std::env::set_var("AETHER_WORKSPACE", &tmp);

        let outside = if cfg!(windows) {
            "C:/Windows/System32/x"
        } else {
            "/etc/x"
        };
        let ctx = GuardCtx::new("rm", Effect::Destructive, "delete", outside);
        let err = guard(ctx).unwrap_err();
        assert_eq!(err.code, ErrorCode::OutsideWorkspace);
        let _ = std::fs::remove_file(&log);
        clear_env();
    }

    #[test]
    fn rbac_authorized_principal_bypasses_approval() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        let tmp = std::env::temp_dir();
        std::env::set_var("AETHER_WORKSPACE", &tmp);
        let _log = isolate_audit("rbac");

        // Build an RBAC manager: a role granting effect:destructive, a user holding it.
        let mgr = std::sync::Arc::new(crate::auth::RbacManager::new());
        mgr.add_role(crate::auth::Role::new("destroyer").with_permission("effect:destructive"));
        let user = crate::auth::User::new("alice");
        let uid = user.id.clone();
        mgr.add_user(user).unwrap();
        mgr.assign_role(&uid, "destroyer").unwrap();
        set_rbac_manager(mgr);

        let target = tmp.join("rbac_target").to_string_lossy().to_string();
        let mk = || GuardCtx {
            builtin: "rm",
            effect: Effect::Destructive,
            what: "delete",
            targets: vec![target.clone()],
            blast_radius: json!({}),
            reversible: false,
            fs_paths: true,
        };

        // Anonymous principal → normal approval gating applies.
        set_principal(None);
        assert_eq!(guard(mk()).unwrap_err().code, ErrorCode::NeedsApproval);

        // Authorized principal → allowed without approval.
        set_principal(Some(uid.clone()));
        assert!(
            guard(mk()).is_ok(),
            "principal with effect:destructive should bypass approval"
        );

        // An unrelated principal (no such permission) is still gated.
        let other = crate::auth::User::new("bob");
        let other_id = other.id.clone();
        // bob isn't in the manager → check_permission false → defer to policy.
        set_principal(Some(other_id));
        assert_eq!(guard(mk()).unwrap_err().code, ErrorCode::NeedsApproval);

        clear_env();
    }

    #[test]
    fn audit_chain_verifies_and_detects_tampering() {
        let _l = ENV_LOCK.lock().unwrap();
        clear_env();
        let mut log = std::env::temp_dir();
        log.push(format!("ae_audit_test_{}.log", std::process::id()));
        let _ = std::fs::remove_file(&log);
        std::env::set_var("AETHER_AUDIT_LOG", log.to_string_lossy().to_string());
        // The chain resets automatically for this fresh (removed) log path.

        audit("rm", Effect::Destructive, "allow", "/x/a", json!({})).unwrap();
        audit("sh", Effect::Exec, "approved", "echo hi", json!({})).unwrap();
        audit("kill", Effect::Process, "allow", "1234", json!({})).unwrap();

        let n = verify_audit(&log).expect("clean chain verifies");
        assert_eq!(n, 3);

        // Tamper: flip a byte in the middle line's resource.
        let content = std::fs::read_to_string(&log).unwrap();
        let mut lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
        lines[1] = lines[1].replace("echo hi", "rm -rf /");
        std::fs::write(&log, lines.join("\n")).unwrap();
        assert!(verify_audit(&log).is_err(), "tampering must be detected");

        let _ = std::fs::remove_file(&log);
        clear_env();
    }

    // ──────────────────────────────────────────────────────────────────
    // Secret hygiene (§7.6)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn redact_str_scrubs_known_secret_shapes() {
        // Provider token prefixes.
        let (r, c) = redact_str("authorization: Bearer sk-abcdefghijklmnopqrstuvwxyz12");
        assert!(c && r.contains("[REDACTED]") && !r.contains("abcdefghij"));
        let (r, _) = redact_str("token ghp_0123456789abcdefABCDEFghijklmnop12");
        assert!(r.contains("[REDACTED]") && !r.contains("ghp_0123"));
        // AWS access key id.
        let (r, _) = redact_str("AKIAIOSFODNN7EXAMPLE in the config");
        assert!(r.contains("[REDACTED]") && !r.contains("AKIAIOSF"));
        // JWT.
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N";
        let (r, _) = redact_str(jwt);
        assert!(r.contains("[REDACTED]") && !r.contains("eyJzdWIi"));
        // PEM private-key block.
        let pem =
            "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE KEY-----";
        let (r, _) = redact_str(pem);
        assert!(r.contains("[REDACTED]") && !r.contains("MIIEpAIB"));
    }

    #[test]
    fn redact_str_scrubs_url_credentials_and_assignments() {
        let (r, _) = redact_str("postgres://admin:hunter2pass@db.internal:5432/app");
        assert!(
            r.contains("admin:[REDACTED]@") && !r.contains("hunter2pass"),
            "password redacted, scheme/user/host kept: {r}"
        );
        let (r, _) = redact_str("password = swordfish123");
        assert!(r.contains("[REDACTED]") && !r.contains("swordfish123"));
        let (r, _) = redact_str("api_key: \"abcdef123456\"");
        assert!(r.contains("[REDACTED]") && !r.contains("abcdef123456"));
    }

    #[test]
    fn redact_str_leaves_ordinary_text_untouched() {
        let plain = "the quick brown fox reads file.txt at 12:00 and exits 0";
        let (r, c) = redact_str(plain);
        assert!(!c, "no secret shape — must report unchanged");
        assert_eq!(r, plain, "ordinary prose must pass through byte-for-byte");
    }

    #[test]
    fn is_secret_name_matches_indicators_not_false_positives() {
        for yes in [
            "OPENAI_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
            "GITHUB_TOKEN",
            "DB_PASSWORD",
            "ANTHROPIC_KEY",
            "client_secret",
        ] {
            assert!(is_secret_name(yes), "{yes} should be a secret name");
        }
        for no in ["PATH", "KEYBOARD", "MONKEY", "HOME", "USER", "LANG"] {
            assert!(!is_secret_name(no), "{no} must not be flagged");
        }
    }

    #[test]
    fn redact_json_scrubs_values_and_secret_named_keys() {
        let mut v = json!({
            "note": "use sk-abcdefghijklmnopqrstuvwx99 to auth",
            "API_KEY": "literally-anything",
            "nested": { "PASSWORD": "p", "ok": "plain text" },
            "list": ["ghp_0123456789abcdefABCDEFghijklmnop12", "fine"],
        });
        redact_json(&mut v);
        assert_eq!(v["API_KEY"], json!("[REDACTED]"));
        assert_eq!(v["nested"]["PASSWORD"], json!("[REDACTED]"));
        assert_eq!(v["nested"]["ok"], json!("plain text"));
        assert!(v["note"].as_str().unwrap().contains("[REDACTED]"));
        assert!(!v["note"].as_str().unwrap().contains("sk-abcdef"));
        assert_eq!(v["list"][1], json!("fine"));
        assert!(v["list"][0].as_str().unwrap().contains("[REDACTED]"));
    }

    #[test]
    fn env_secret_gated_is_agent_only_and_policy_aware() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        std::env::remove_var("AETHER_REDACT");
        std::env::remove_var("AETHER_SECRETS");

        // Human mode: never gated (legibility — the person reads their own env).
        assert!(!env_secret_gated("OPENAI_API_KEY"));

        // Agent mode: secret names gated, ordinary names not.
        std::env::set_var("AETHER_MODE", "agent");
        assert!(env_secret_gated("OPENAI_API_KEY"));
        assert!(!env_secret_gated("HOME"));

        // Explicit permission re-opens clear reads.
        std::env::set_var("AETHER_SECRETS", "allow");
        assert!(!env_secret_gated("OPENAI_API_KEY"));
        std::env::remove_var("AETHER_SECRETS");

        // Global opt-out disables redaction entirely.
        std::env::set_var("AETHER_REDACT", "off");
        assert!(!env_secret_gated("OPENAI_API_KEY"));

        clear_env();
        std::env::remove_var("AETHER_REDACT");
    }

    // ──────────────────────────────────────────────────────────────────
    // Resource governors (§7.6)
    // ──────────────────────────────────────────────────────────────────

    #[test]
    fn governor_is_inert_in_human_mode_and_when_unset() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();

        // Human mode: even a tiny limit is ignored.
        std::env::set_var("AETHER_MAX_FILES", "1");
        for _ in 0..5 {
            assert!(governor_admit(Effect::Destructive, "rm").is_ok());
        }

        // Agent mode but no limit set: unlimited.
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        for _ in 0..50 {
            assert!(governor_admit(Effect::Destructive, "rm").is_ok());
        }
        clear_env();
    }

    #[test]
    fn governor_breaches_file_and_op_budgets_independently() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_MAX_FILES", "2");

        assert!(governor_admit(Effect::WriteLocal, "file_write").is_ok());
        assert!(governor_admit(Effect::Destructive, "rm").is_ok());
        let err = governor_admit(Effect::WriteLocal, "file_write").unwrap_err();
        assert_eq!(err.code, ErrorCode::BudgetExceeded);
        assert!(err.to_json()["error"]["retryable"] == json!(false));

        // A non-file effect class is not charged to the file budget.
        assert!(governor_admit(Effect::Process, "proc_kill").is_ok());

        // Snapshot reflects the tally.
        let snap = governor_snapshot();
        assert_eq!(snap["used"]["files"], json!(3)); // attempts counted, incl. the breached one
        assert_eq!(snap["limits"]["max_files"], json!(2));

        clear_env();
    }

    #[test]
    fn governor_enforces_network_egress_budget() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_MAX_NET", "2");

        assert!(governor_admit(Effect::Network, "http_get").is_ok());
        assert!(governor_admit(Effect::Network, "web_fetch").is_ok());
        let err = governor_admit(Effect::Network, "http_get").unwrap_err();
        assert_eq!(err.code, ErrorCode::BudgetExceeded);
        assert!(err.message.contains("network-egress"));

        // A non-network effect is not charged to the egress budget.
        assert!(governor_admit(Effect::WriteLocal, "file_write").is_ok());

        let snap = governor_snapshot();
        assert_eq!(snap["used"]["net"], json!(3));
        assert_eq!(snap["limits"]["max_net"], json!(2));

        clear_env();
    }

    #[test]
    fn governor_enforces_total_op_budget_across_effects() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_MAX_OPS", "3");

        assert!(governor_admit(Effect::WriteLocal, "file_write").is_ok());
        assert!(governor_admit(Effect::Process, "proc_kill").is_ok());
        assert!(governor_admit(Effect::Exec, "sh").is_ok());
        let err = governor_admit(Effect::ReadLocal, "cat").unwrap_err();
        assert_eq!(err.code, ErrorCode::BudgetExceeded);

        clear_env();
    }

    #[test]
    fn governor_enforces_wall_clock_timeout() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_TIMEOUT_MS", "1");

        // First call starts the clock (elapsed 0 ≤ 1 → admitted).
        assert!(governor_admit(Effect::WriteLocal, "file_write").is_ok());
        std::thread::sleep(std::time::Duration::from_millis(15));
        // Now elapsed (~15ms) exceeds the 1ms budget.
        let err = governor_admit(Effect::WriteLocal, "file_write").unwrap_err();
        assert_eq!(err.code, ErrorCode::BudgetExceeded);
        assert!(err.message.contains("wall-clock"));

        clear_env();
    }

    #[test]
    fn governor_reset_starts_a_fresh_envelope() {
        let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        clear_env();
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_MAX_OPS", "1");

        assert!(governor_admit(Effect::WriteLocal, "file_write").is_ok());
        assert!(governor_admit(Effect::WriteLocal, "file_write").is_err());
        governor_reset();
        // After reset the budget is available again.
        assert!(governor_admit(Effect::WriteLocal, "file_write").is_ok());

        clear_env();
    }
}
