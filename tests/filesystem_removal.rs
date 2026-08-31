//! `rm`, `rmdir` and `touch` were implemented, guarded, and unreachable.
//!
//! `bi_rm` has existed with a full `GuardCtx` — Destructive, jailed, with a
//! transaction snapshot — and was never entered into `BUILTIN_LOOKUP`. So
//! `rm("x")` answered `E_UNKNOWN_BUILTIN` while `effect_of("rm")` answered
//! `Destructive`: the safety layer was guarding a name the dispatcher did not
//! have, and the shell could not delete a file at all. Nor could it via
//! `file_delete`, `file.delete` or `fs_remove` — none of which exist.
//!
//! The design doc recorded half of this as "needs a product decision". Whether
//! a *shell* should be able to remove a file is not one.

use aethershell::env::Env;
use aethershell::value::Value;

fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env).map_err(|e| e.to_string())
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// A temp path unique to this process and call.
///
/// These names used to be fixed — `%TEMP%/ae_rm_reachable.txt` — so a second
/// run, a leftover from a killed run, or a virus scanner still holding the
/// handle made the test fail with "Access is denied" or "cannot find the file"
/// and look like a bug in `rm`. Observed exactly that in a full-workspace run
/// while the isolated run passed, which is the worst shape a test can have:
/// the failure says nothing about the code under test.
fn tmp(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let unique = format!(
        "ae_rm_{}_{}_{}_{tag}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
    );
    let p = std::env::temp_dir().join(unique);
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_dir_all(&p);
    p
}

#[test]
fn rm_is_reachable_by_name() {
    // The whole bug in one assertion: this returned E_UNKNOWN_BUILTIN.
    let p = tmp("reachable.txt");
    std::fs::write(&p, "x").unwrap();
    let r = call("rm", vec![s(&p.to_string_lossy())]);
    assert!(r.is_ok(), "rm should be callable by name, got {r:?}");
    assert!(!p.exists(), "rm should have removed the file");
}

#[test]
fn touch_is_reachable_by_name() {
    let p = tmp("touch.txt");
    let r = call("touch", vec![s(&p.to_string_lossy())]);
    assert!(r.is_ok(), "touch should be callable by name, got {r:?}");
    assert!(p.exists(), "touch should have created the file");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn rmdir_is_reachable_by_name() {
    let d = tmp("dir");
    std::fs::create_dir_all(&d).unwrap();
    let r = call("rmdir", vec![s(&d.to_string_lossy())]);
    assert!(r.is_ok(), "rmdir should be callable by name, got {r:?}");
    assert!(!d.exists(), "rmdir should have removed the directory");
}

#[test]
fn registering_them_did_not_weaken_their_effect_classification() {
    // `rm` was classified Destructive while unreachable. Registration must not
    // have quietly turned it into something the policy engine waves through —
    // Destructive is what makes it need approval in agent mode.
    use aethershell::safety::{effect_is_declared, effect_of, Effect};
    assert_eq!(effect_of("rm"), Effect::Destructive);
    assert_eq!(effect_of("rmdir"), Effect::Destructive);
    assert!(effect_is_declared("rm"));
    assert!(effect_is_declared("rmdir"));
    // touch creates a file; non-destructive, but not pure either.
    assert_eq!(effect_of("touch"), Effect::WriteLocal);
}

#[test]
fn the_deletion_builtins_are_not_silently_missing_from_the_catalog() {
    // An agent discovers what it can do from the builtin table. A destructive
    // operation that exists in the safety model but not the catalog is worse
    // than one that is absent from both: policy claims to govern something no
    // caller can reach, which reads as coverage.
    use aethershell::builtins::BUILTIN_LOOKUP;
    for name in ["rm", "rmdir", "touch"] {
        assert!(
            BUILTIN_LOOKUP.contains_key(name),
            "{name} is classified by `effect_of` but absent from BUILTIN_LOOKUP"
        );
    }
}
