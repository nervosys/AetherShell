//! A download has two effects, and the effect taxonomy carries one.
//!
//! `web_download`, `wget_download` and `web_upload_file` are classified
//! `Network` by the `web_*`/`wget_*` prefix rule, they call `guard_network` on
//! the URL, and they are listed in `safety::SELF_GUARDED`. Every place a reader
//! checks says "gated". None of it touches the *file*.
//!
//! `guard_network` passes `Effect::Network` and `fs_paths: false`, and the jail
//! in `safety::guard` fires only when the effect `is_filesystem()` *and*
//! `fs_paths` is set. So the download path went through no containment check at
//! all, and in agent mode this held:
//!
//! ```text
//! file.write   "C:\outside\workspace\x" "…"   → refused, OutsideWorkspace
//! web.download "http://…"  "C:\outside\workspace\x"  → allowed
//! ```
//!
//! Same jail, same path, opposite answers, because one builtin's label said
//! `WriteLocal` and the other's said `Network`. The workspace jail is the
//! containment story for agent mode, so a builtin that writes outside it is not
//! a lesser version of `file_write` — it is the way around it.
//!
//! What this file pins is the *agreement*: the two builtins must give the same
//! answer about the same path. Asserting the agreement rather than the refusal
//! is deliberate — if the jail's policy is later loosened, this test should
//! follow it rather than fossilise today's answer.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::value::Value;
use std::sync::Mutex;

/// `AETHER_MODE` and `AETHER_WORKSPACE` are process-global.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct Jail {
    workspace: std::path::PathBuf,
    outside: std::path::PathBuf,
}

impl Jail {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        let workspace = base.join(format!("ae_netjail_ws_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&workspace).expect("workspace");
        let outside = base.join(format!("ae_netjail_out_{tag}_{}.bin", std::process::id()));
        let _ = std::fs::remove_file(&outside);
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_WORKSPACE", &workspace);
        Self { workspace, outside }
    }
}

impl Drop for Jail {
    fn drop(&mut self) {
        std::env::remove_var("AETHER_WORKSPACE");
        std::env::remove_var("AETHER_MODE");
        let _ = std::fs::remove_file(&self.outside);
        let _ = std::fs::remove_dir_all(&self.workspace);
    }
}

/// Does this call fail because of the workspace jail, as opposed to because
/// `curl` is missing, the network is down, or the URL is unreachable?
///
/// The distinction matters: a test that accepted *any* error would pass on a
/// machine with no `curl` while the hole was wide open — the blind-green failure
/// this repo keeps naming. Only a containment refusal counts.
fn refused_by_the_jail(r: &anyhow::Result<Value>) -> bool {
    match r {
        Ok(_) => false,
        Err(e) => {
            let text = format!("{e:#}").to_ascii_lowercase();
            text.contains("workspace") || text.contains("outside")
        }
    }
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

#[test]
fn a_download_and_a_write_agree_about_a_path_outside_the_workspace() {
    let _g = lock();
    let jail = Jail::new("agree");
    let target = jail.outside.to_string_lossy().into_owned();

    // The reference answer, from the builtin whose label already says
    // `WriteLocal`. If this stops refusing, the jail itself has changed and the
    // assertion below should change with it rather than being patched.
    let write = call(
        "file_write",
        vec![Value::Str(target.clone()), Value::Str("x".into())],
    );
    assert!(
        refused_by_the_jail(&write),
        "precondition: file_write outside the workspace must be refused in agent \
         mode, otherwise this test proves nothing. Got: {write:?}"
    );

    // A URL that never leaves the machine, so the result depends on the guard
    // rather than on whether this machine has a network.
    let download = call(
        "web_download",
        vec![
            Value::Str("https://example.invalid/payload".into()),
            Value::Str(target.clone()),
        ],
    );
    assert!(
        refused_by_the_jail(&download),
        "web_download wrote to a path that file_write is refused for. The URL was \
         gated by `guard_network` and the file was not — `Effect::Network` is not \
         `is_filesystem()`, so `safety::guard` never ran the jail. Got: {download:?}"
    );
    assert!(
        !jail.outside.exists(),
        "the file outside the workspace must not have been created"
    );
}

#[test]
fn the_wget_spelling_is_gated_the_same_way() {
    // Two builtins, two spellings, one hole — `wget_download` had the identical
    // shape and would have been missed by a fix aimed only at `web_download`.
    let _g = lock();
    let jail = Jail::new("wget");
    let target = jail.outside.to_string_lossy().into_owned();

    let r = call(
        "wget_download",
        vec![
            Value::Str("https://example.invalid/payload".into()),
            Value::Str(target),
        ],
    );
    assert!(
        refused_by_the_jail(&r),
        "wget_download must be jailed like web_download. Got: {r:?}"
    );
    assert!(!jail.outside.exists());
}

#[test]
fn a_path_inside_the_workspace_is_not_refused_by_the_guard() {
    // The check that keeps the fix from being a removal. The call may still fail
    // — `example.invalid` does not resolve, and the machine may have no `curl` —
    // but it must not fail *because of containment*.
    let _g = lock();
    let jail = Jail::new("inside");
    let inside = jail.workspace.join("ok.bin").to_string_lossy().into_owned();

    let r = call(
        "web_download",
        vec![
            Value::Str("https://example.invalid/payload".into()),
            Value::Str(inside),
        ],
    );
    assert!(
        !refused_by_the_jail(&r),
        "a path inside the workspace must pass the guard. Got: {r:?}"
    );
}

#[test]
fn a_relative_path_lands_in_the_workspace_not_the_working_directory() {
    // `file_write` resolves a relative path against the workspace root so the
    // write lands in the jail rather than beside the process, and the download
    // path now gets the same treatment.
    //
    // Stated honestly: this is a no-regression check, not a red-green one. It
    // passes with the guard removed as well, because the fetch fails before curl
    // writes anything. What it holds is that resolving the path did not start
    // refusing ordinary relative names.
    let _g = lock();
    let _jail = Jail::new("relative");

    let r = call(
        "web_download",
        vec![
            Value::Str("https://example.invalid/payload".into()),
            Value::Str("relative-name.bin".into()),
        ],
    );
    assert!(
        !refused_by_the_jail(&r),
        "a bare filename resolves inside the workspace and must pass. Got: {r:?}"
    );
    assert!(
        !std::path::Path::new("relative-name.bin").exists(),
        "a relative download path must not resolve against the working directory"
    );
}

#[test]
fn an_upload_is_deliberately_not_jailed_and_this_is_the_note_saying_so() {
    // Not an oversight, and pinned so it does not become one. The jail in
    // `safety::guard` covers `WriteLocal` and `Destructive`; `ReadLocal` is
    // unjailed by design, and `file.read` of an outside path followed by
    // `web.post` is already an allowed pair — so jailing `web_upload_file`
    // would be a new policy invented at a call site rather than an
    // inconsistency being corrected.
    //
    // If reads are ever jailed, `is_filesystem()` is the one place to change and
    // this test is the marker that says `web_upload_file` should follow.
    use aethershell::safety::Effect;
    assert!(
        !Effect::ReadLocal.is_filesystem(),
        "reads are unjailed by design; if that changes, jail web_upload_file too"
    );
    assert!(Effect::WriteLocal.is_filesystem());
    assert!(Effect::Destructive.is_filesystem());
}

#[test]
fn an_option_like_url_cannot_reach_curl() {
    // The other half, checked once in `guard_network` rather than at each of the
    // fourteen call sites. `curl -K<file>` reads a config file that can set
    // `output` and `url`, so an option-like "URL" turns a fetch into a write.
    let _g = lock();
    let jail = Jail::new("optlike");
    let inside = jail.workspace.join("x.bin").to_string_lossy().into_owned();

    for url in ["-Kconfig.txt", "--config=config.txt", "-o/etc/passwd"] {
        let r = call(
            "web_download",
            vec![Value::Str(url.into()), Value::Str(inside.clone())],
        );
        let e = r.expect_err("an option-like URL must be refused");
        let text = format!("{e:#}").to_ascii_lowercase();
        assert!(
            text.contains("option-like"),
            "{url} must be refused as option-like, got: {text}"
        );
    }

    // And an ordinary URL still gets through the same check.
    let r = call(
        "web_download",
        vec![
            Value::Str("https://example.invalid/x".into()),
            Value::Str(inside),
        ],
    );
    let text = r.err().map(|e| format!("{e:#}")).unwrap_or_default();
    assert!(
        !text.to_ascii_lowercase().contains("option-like"),
        "a normal URL must not be caught by the option check: {text}"
    );
}
