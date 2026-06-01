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

/// Best-effort classifier for a builtin name, used by the ontology and audit
/// when an explicit [`Effect`] was not supplied at the call site. Conservative:
/// known-dangerous names are classified precisely; unknown names default to
/// [`Effect::Pure`] (callers that need gating pass the effect explicitly).
pub fn effect_of(name: &str) -> Effect {
    match name {
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
        | "platform_db_delete" => Effect::Destructive,
        "proc_kill" | "kill" | "signal" => Effect::Process,
        "sh" | "exec" | "system" => Effect::Exec,
        n if n.starts_with("http") || n.starts_with("net_") || n.starts_with("nc_") => {
            Effect::Network
        }
        "file_write" | "file_append" | "file_copy" | "mkdir" | "touch" => Effect::WriteLocal,
        n if n.starts_with("file_") || n.starts_with("proc_") || n.starts_with("sys_") => {
            Effect::ReadLocal
        }
        _ => Effect::Pure,
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
}

impl ErrorCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorCode::PolicyDeny => "E_POLICY_DENY",
            ErrorCode::NeedsApproval => "E_NEEDS_APPROVAL",
            ErrorCode::OutsideWorkspace => "E_OUTSIDE_WORKSPACE",
            ErrorCode::BadArg => "E_BAD_ARG",
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
}

impl SafetyError {
    /// The structured JSON form an agent reads programmatically.
    pub fn to_json(&self) -> Json {
        let mut err = json!({
            "code": self.code.as_str(),
            "message": self.message,
            "builtin": self.builtin,
            "hint": self.hint,
            "retryable": self.code != ErrorCode::PolicyDeny,
        });
        if let Some(a) = &self.approval {
            err["approval"] = serde_json::to_value(a).unwrap_or(Json::Null);
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
fn jail_enforced(mode: Mode) -> bool {
    mode == Mode::Agent || std::env::var("AETHER_WORKSPACE").is_ok()
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

/// Gate an effecting call. Returns `Ok(())` if the call may proceed (and records
/// an audit entry), or a [`SafetyError`] with a stable code, an actionable hint,
/// and — for approvable actions — a bound approval token.
pub fn guard(ctx: GuardCtx) -> Result<(), SafetyError> {
    let mode = current_mode();
    let resource = ctx.targets.join(", ");

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
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // Env mutation in these tests is process-global; serialize them.
    lazy_static! {
        static ref ENV_LOCK: StdMutex<()> = StdMutex::new(());
    }

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
        ] {
            std::env::remove_var(k);
        }
        if let Ok(mut g) = GRANTED.lock() {
            g.clear();
        }
        set_principal(None);
        clear_rbac_manager();
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
}
