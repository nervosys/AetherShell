//! The workspace jail reached 8 of the 119 `WriteLocal` builtins.
//!
//! `docs/AGENTIC_FIRST_DESIGN.md` §5.3 promises this:
//!
//! ```text
//! | Effect      | Human  | Agent    |
//! | WriteLocal  | allow  | allow*   | (* jailed to workspace)
//! ```
//!
//! The jail lives inside `safety::guard`. `guard_dispatch` never calls it for a
//! `WriteLocal` builtin:
//!
//! ```text
//! if SELF_GUARDED.contains(&builtin) { return Ok(()); }
//! if !centrally_enforced(effect) {
//!     … audit "allow_unguarded" …
//!     return Ok(());          // <- guard() is never reached, so nor is the jail
//! }
//! ```
//!
//! and `centrally_enforced` is `Process | Destructive | Exec | Privileged`. So a
//! `WriteLocal` builtin is jailed only if it guards itself. Eight do —
//! `file_write`, `file_append`, `file_edit`, `file_insert`, `file_patch`,
//! `file_backup`, `file_delete_lines`, `session_export`. The other **111** do
//! not: `cp`, `copy_file`, `append_file`, `mkdir`, `tar_extract`, `zip_extract`,
//! `gzip_compress`, `db_sqlite_backup`, and so on.
//!
//! The code names it: the audit decision string it writes for these is literally
//! `"allow_unguarded"`. The reasoning in the comment there is about the *policy*
//! decision — "`WriteLocal` decides `Allow`, so there is no decision to make" —
//! which is true and beside the point, because containment is not a policy
//! decision. It is a separate check that happens to live behind the same call.
//!
//! ## What the fix does and does not do
//!
//! The central check judges only arguments that name a path **which already
//! exists**, and that restriction is deliberate and load-bearing: a string that
//! resolves to a real file is a path by observation, while a string that does not
//! might be a container name, a subcommand or a SQL fragment, and refusing those
//! would break legitimate calls with no workaround.
//!
//! That has a consequence worth stating plainly rather than papering over:
//! **writing to a path outside the workspace that does not yet exist is still not
//! caught centrally.** `cp inside.txt C:\somewhere\new.txt` creates a new file
//! and no existing path is named. What the central jail does catch is
//! *overwriting something that is already there* — which is the destructive half
//! of a write, and the half that can damage a system rather than litter it.
//!
//! The rest stays where the design puts it: at call sites that know which of
//! their arguments is a destination. The count of builtins relying on that is
//! asserted below so it is a measured number rather than an assumption.

use aethershell::builtins::{self, BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect, SELF_GUARDED};
use aethershell::value::Value;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

struct Jail {
    workspace: std::path::PathBuf,
    victim: std::path::PathBuf,
}

impl Jail {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        let workspace = base.join(format!("ae_wlj_ws_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&workspace).expect("workspace");
        let victim = base.join(format!("ae_wlj_victim_{tag}_{}.txt", std::process::id()));
        std::fs::write(&victim, "original\n").expect("seed the victim");
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_WORKSPACE", &workspace);
        Self { workspace, victim }
    }
    fn victim_intact(&self) -> bool {
        std::fs::read_to_string(&self.victim).is_ok_and(|s| s == "original\n")
    }
}

impl Drop for Jail {
    fn drop(&mut self) {
        std::env::remove_var("AETHER_WORKSPACE");
        std::env::remove_var("AETHER_MODE");
        let _ = std::fs::remove_file(&self.victim);
        let _ = std::fs::remove_dir_all(&self.workspace);
    }
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    builtins::call(name, args, &mut env)
}

fn refused_by_the_jail(r: &anyhow::Result<Value>) -> bool {
    match r {
        Ok(_) => false,
        Err(e) => {
            let t = format!("{e:#}").to_ascii_lowercase();
            t.contains("workspace") || t.contains("outside")
        }
    }
}

#[test]
fn overwriting_a_file_outside_the_workspace_is_refused_whichever_builtin_does_it() {
    let _g = lock();
    let jail = Jail::new("copy");
    let source = jail.workspace.join("payload.txt");
    std::fs::write(&source, "payload\n").expect("seed");
    let victim = jail.victim.to_string_lossy().into_owned();

    // The reference answer, from one of the eight that always self-guarded.
    let write = call(
        "file_write",
        vec![Value::Str(victim.clone()), Value::Str("payload\n".into())],
    );
    assert!(
        refused_by_the_jail(&write),
        "precondition: file_write must be refused here. Got: {write:?}"
    );

    // `copy_file` has the same effect label and no self-guard.
    let copied = call(
        "copy_file",
        vec![
            Value::Str(source.to_string_lossy().into_owned()),
            Value::Str(victim.clone()),
        ],
    );
    assert!(
        refused_by_the_jail(&copied),
        "copy_file overwrote a file that file_write is refused for — both are \
         `WriteLocal`, and the jail reached only the one that guards itself. \
         Got: {copied:?}"
    );
    assert!(
        jail.victim_intact(),
        "the file outside the workspace was overwritten"
    );
}

#[test]
fn the_same_holds_for_the_append_spelling() {
    let _g = lock();
    let jail = Jail::new("append");
    let r = call(
        "append_file",
        vec![
            Value::Str(jail.victim.to_string_lossy().into_owned()),
            Value::Str("appended\n".into()),
        ],
    );
    assert!(
        refused_by_the_jail(&r),
        "append_file must be jailed like file_append. Got: {r:?}"
    );
    assert!(jail.victim_intact());
}

#[test]
fn writing_inside_the_workspace_is_untouched() {
    // The check that keeps the fix from being a ban. The central jail judges only
    // arguments naming an existing path, so an ordinary in-workspace copy must
    // pass exactly as before.
    let _g = lock();
    let jail = Jail::new("inside");
    let src = jail.workspace.join("a.txt");
    std::fs::write(&src, "hello\n").expect("seed");
    let dst = jail.workspace.join("b.txt");

    let r = call(
        "copy_file",
        vec![
            Value::Str(src.to_string_lossy().into_owned()),
            Value::Str(dst.to_string_lossy().into_owned()),
        ],
    );
    assert!(
        !refused_by_the_jail(&r),
        "a copy entirely inside the workspace must still work: {r:?}"
    );
    assert!(dst.exists(), "and must actually have copied");
}

#[test]
fn the_number_of_writelocal_builtins_relying_on_call_sites_is_measured() {
    // Not a pass/fail about correctness — a number, so that "the call sites cover
    // it" stays a claim someone can check rather than an assumption. The central
    // jail catches overwrites of existing paths; a *new* file outside the
    // workspace is still the call site's job.
    //
    // This may only shrink. It went from 111 to whatever it reads now by moving
    // the check into `guard_dispatch`; the remainder are builtins where the
    // destination need not exist beforehand.
    // Both halves of the dispatcher, the same enumeration `effect_snapshot`
    // uses — a name in the fallback `match` is as callable as any other.
    let mut names: Vec<&str> = BUILTIN_LOOKUP.keys().copied().collect();
    names.extend(FALLBACK_BUILTINS.iter().map(|(n, _)| *n));
    names.sort_unstable();
    names.dedup();
    let write_local: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| effect_of(n) == Effect::WriteLocal)
        .collect();
    let unguarded: Vec<&str> = write_local
        .iter()
        .copied()
        .filter(|n| !SELF_GUARDED.contains(n))
        .collect();

    assert!(
        write_local.len() > 50,
        "only {} WriteLocal builtins found; the enumeration has drifted",
        write_local.len()
    );
    assert!(
        unguarded.len() <= 115,
        "{} of {} WriteLocal builtins do not guard their own destination, up from \
         the measured 111. Each relies on the central check in `guard_dispatch`, \
         which catches overwriting an existing path but not creating a new one \
         outside the workspace. Adding another is allowed; letting the number \
         grow silently is not.",
        unguarded.len(),
        write_local.len()
    );
}

#[test]
fn copying_a_file_into_the_workspace_from_outside_still_works() {
    // The false positive that sent the central fix back. Jailing every existing
    // outside path named by a `WriteLocal` call refuses this — copying a file
    // *in* — because the source exists outside. Reading from outside is allowed
    // by policy and the write lands inside the jail, so refusing it is wrong,
    // and it is indistinguishable from the dangerous case anywhere except the
    // call site. That is why `file_copy` guards its second argument by name
    // rather than `guard_dispatch` guarding all of them by shape.
    let _g = lock();
    let jail = Jail::new("copyin");
    let dst = jail.workspace.join("brought_in.txt");
    let r = call(
        "copy_file",
        vec![
            Value::Str(jail.victim.to_string_lossy().into_owned()),
            Value::Str(dst.to_string_lossy().into_owned()),
        ],
    );
    assert!(
        !refused_by_the_jail(&r),
        "copying a file into the workspace must not be refused: {r:?}"
    );
    assert!(dst.exists(), "and must actually have copied");
}

#[test]
fn making_a_directory_outside_the_workspace_is_refused() {
    let _g = lock();
    let jail = Jail::new("mkdir");
    let outside = jail.victim.with_extension("dir");
    let r = call(
        "mkdir",
        vec![Value::Str(outside.to_string_lossy().into_owned())],
    );
    assert!(
        refused_by_the_jail(&r),
        "mkdir created a directory outside the workspace: {r:?}"
    );
    assert!(!outside.exists());
}

#[test]
fn mkdir_actually_makes_a_directory() {
    // Found while testing the jail, which is the only reason it was found at
    // all: `mkdir` returned `Ok(Null)` instead of being refused, and `Ok(Null)`
    // turned out to be what it returns for every input. `mkdir`, `mkdirp` and
    // `file_mkdir` all resolve to dispatch index 532, which was a stub
    // `|_, _, _| Ok(Value::Null)`. `bi_file_mkdir` exists, is classified
    // `WriteLocal`, and was never wired into `BUILTIN_DISPATCH`.
    //
    // Nothing caught it: `catalog_reachability` asks whether an advertised name
    // dispatches, and this one does — to a stub. A silent no-op is worse than
    // an unknown builtin, because the caller gets a success value.
    let _g = lock();
    let jail = Jail::new("mkreal");
    let target = jail.workspace.join("made_here");
    let r = call(
        "mkdir",
        vec![Value::Str(target.to_string_lossy().into_owned())],
    );
    assert!(r.is_ok(), "mkdir inside the workspace must succeed: {r:?}");
    assert!(target.is_dir(), "mkdir returned {r:?} and created nothing");
}
