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
//! ## Where the jail lives
//!
//! A central check over every existing path a `WriteLocal` call names was
//! written, and reverted: it refused copying a file *into* the workspace from
//! outside, because the source exists outside, and from the dispatcher that is
//! indistinguishable from overwriting something outside. (The comment in
//! `guard_dispatch` keeps the reasoning.) So the jail is at call sites, which
//! know which argument is the destination, and a `WriteLocal` builtin whose body
//! does not guard its destination is not jailed at all. The tests below hold the
//! known ones to it, and the count of names relying on their bodies is asserted
//! so it is a measured number rather than an assumption.

use iron_shell::builtins::{self, BUILTIN_LOOKUP, FALLBACK_BUILTINS};
use iron_shell::env::Env;
use iron_shell::safety::{effect_of, Effect, SELF_GUARDED};
use iron_shell::value::Value;
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
        std::env::set_var("IRONSHELL_MODE", "agent");
        std::env::set_var("IRONSHELL_WORKSPACE", &workspace);
        Self { workspace, victim }
    }
    fn victim_intact(&self) -> bool {
        std::fs::read_to_string(&self.victim).is_ok_and(|s| s == "original\n")
    }
}

impl Drop for Jail {
    fn drop(&mut self) {
        std::env::remove_var("IRONSHELL_WORKSPACE");
        std::env::remove_var("IRONSHELL_MODE");
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
fn csv_import_refuses_a_new_destination_outside_workspace() {
    let _g = lock();
    let jail = Jail::new("csv_import");
    let source = jail.workspace.join("rows.csv");
    std::fs::write(&source, "name,age\nAda,42\n").unwrap();
    let destination = jail.victim.with_extension("new.sqlite");
    assert!(!destination.exists());
    let result = call(
        "db_csv_to_sqlite",
        vec![
            Value::Str(destination.to_string_lossy().into()),
            Value::Str(source.to_string_lossy().into()),
        ],
    );
    assert!(refused_by_the_jail(&result), "{result:?}");
    assert!(!destination.exists());
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
    // it" stays a claim someone can check rather than an assumption.
    //
    // There is no central jail for WriteLocal: one was tried in `guard_dispatch`
    // and reverted (see the comment there), so every write is jailed at its call
    // site or not at all. The name list overstates the gap, because many of
    // these guard in their bodies without being listed in SELF_GUARDED (file_copy,
    // file_mkdir, the gzip pair), and many write no caller-named path (in-memory
    // registries, the fixed config and platform stores). A 2026-10-29 read of
    // all 102 found eight that wrote a caller's path unjailed: the sqlite
    // writers, db_sqlite_open, db_sqlite_backup, bzip2_compress, code_format and
    // chgrp on Unix. They are jailed now, and
    // `the_database_compression_and_format_writers_are_jailed` holds them to it.
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
        // 130 after the Pure review: what it added writes process memory or a
        // private file, and handle_drop refuses an id that is not its own.
        // 142 after the open-set review (2026-10-31): the 12 names added are
        // a2a_send, a2a_receive, a2a_broadcast, a2a_register, a2a_unregister,
        // nanda_propose, nanda_vote, nanda_commit, nanda_abort, job_cancel,
        // finetune_cancel and its alias cancel_finetune. Each writes only a
        // process-static table (A2A_REGISTRY, A2A_MESSAGES, NANDA_PROPOSALS,
        // DISTRIBUTED_JOBS, FINETUNE_JOBS) and takes no caller path.
        unguarded.len() <= 142,
        "{} of {} WriteLocal builtins are not in SELF_GUARDED, up from the \
         measured 142. Nothing in `guard_dispatch` jails them, so a new one that \
         writes a caller's path must guard it in its body. Adding another is \
         allowed; letting the number grow silently is not.",
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

#[test]
fn file_move_finds_a_relative_source_in_the_workspace() {
    // The existence probe looked at the bare name, which the OS resolves
    // against the working directory, while the move itself resolves it against
    // the workspace. With the two apart, a file the move would have moved was
    // reported missing.
    let _g = lock();
    let jail = Jail::new("move_probe");
    let name = format!("ae_wlj_move_src_{}.txt", std::process::id());
    std::fs::write(jail.workspace.join(&name), "x").unwrap();
    assert!(
        !std::path::Path::new(&name).exists(),
        "the cwd must not hold it"
    );
    let result = call(
        "file_move",
        vec![
            Value::Str(name.clone()),
            Value::Str(format!("{name}.moved")),
        ],
    );
    let code = result
        .as_ref()
        .err()
        .and_then(|e| e.downcast_ref::<iron_shell::safety::SafetyError>())
        .map(|s| s.code);
    assert_ne!(
        code,
        Some(iron_shell::safety::ErrorCode::NotFound),
        "{result:?}"
    );
}

#[test]
fn the_database_compression_and_format_writers_are_jailed() {
    // WriteLocal builtins the dispatcher does not jail, which wrote a file
    // outside the workspace that file_write is refused for. The jail refuses
    // before any tool runs, so this holds whether or not sqlite3, bzip2 or a
    // formatter is installed.
    let _g = lock();
    let jail = Jail::new("writers");
    let victim = jail.victim.to_string_lossy().into_owned();
    let s = |v: &str| Value::Str(v.to_string());
    let fresh = |ext: &str| jail.victim.with_extension(ext);
    let cases: Vec<(&str, Vec<Value>, Option<std::path::PathBuf>)> = vec![
        (
            "db_sqlite_insert",
            vec![
                s(&victim),
                s("t"),
                Value::Record([("a".to_string(), Value::Int(1))].into_iter().collect()),
            ],
            None,
        ),
        (
            "db_sqlite_create_table",
            vec![s(&victim), s("t"), s("a integer")],
            None,
        ),
        ("db_sqlite_vacuum", vec![s(&victim)], None),
        ("db_kv_set", vec![s(&victim), s("k"), s("v")], None),
        (
            "db_sqlite_open",
            vec![s(&fresh("open.db").to_string_lossy())],
            Some(fresh("open.db")),
        ),
        (
            "db_sqlite_backup",
            vec![s(&victim), s(&fresh("bak.db").to_string_lossy())],
            Some(fresh("bak.db")),
        ),
        ("bzip2_compress", vec![s(&victim)], Some(fresh("txt.bz2"))),
        ("code_format", vec![s(&victim)], None),
    ];
    let mut escaped = Vec::new();
    for (name, args, created) in cases {
        let r = call(name, args);
        let made = created.filter(|p| p.exists());
        if !refused_by_the_jail(&r) || !jail.victim_intact() || made.is_some() {
            escaped.push(format!("{name}: {r:?} (created: {made:?})"));
        }
    }
    assert!(escaped.is_empty(), "not jailed:\n{}", escaped.join("\n"));
}
