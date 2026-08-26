//! A body that writes must carry a label the jail keys on.
//!
//! `tests/effect_ratchet.rs` compares evidence against the label, and it is the
//! reason 306 process-spawning builtins got classified. But it only fires when
//! the label is `Effect::Pure` — "acts while claiming to do nothing". A builtin
//! that claims to do *something*, just not the thing it does, is invisible to
//! it, and four were:
//!
//! ```text
//! file_edit      ReadLocal   fs::write + fs::rename over the caller's path
//! file_insert    ReadLocal   fs::write + fs::rename over the caller's path
//! file_patch     ReadLocal   fs::write + fs::rename over the caller's path
//! session_export ReadLocal   fs::write to a caller-named path
//! ```
//!
//! Each reads a file, changes it, and writes it back. `ReadLocal` is a fair
//! description of the first step and of nothing after it.
//!
//! The consequence is not cosmetic, because the workspace jail keys on exactly
//! this label: `safety::guard` contains the path check only when
//! `effect.is_filesystem()`, which is `WriteLocal | Destructive`. So in agent
//! mode this held, for all four and for their alias spellings `edit_file`,
//! `insert_lines`, `patch_file`, `text_edit`:
//!
//! ```text
//! file.write "C:\outside\workspace\x" "…"   → refused, OutsideWorkspace
//! file.patch "C:\outside\workspace\x" […]   → allowed, and rewrote the file
//! ```
//!
//! This file holds both halves: the behaviour, and the general rule that would
//! have caught it without anyone looking — **if a body writes to the filesystem,
//! its effect must be one the jail keys on**. That rule is stated against
//! `is_filesystem()` rather than against a severity ordering on purpose: the jail
//! is the control, so the control's own predicate is the thing to assert.

use aethershell::builtins;
use aethershell::env::Env;
use aethershell::safety::{effect_of, Effect};
use aethershell::value::Value;
use std::sync::Mutex;

static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

// ── The rule ────────────────────────────────────────────────────────────────

/// Syntax that writes to the filesystem. Deliberately narrower than
/// `effect_ratchet`'s list: only markers whose presence means *this* body writes,
/// so the rule below can be an equality rather than a guess.
const WRITE_EVIDENCE: &[(&str, &str)] = &[
    ("fs::write(", "writes a file"),
    ("fs::rename(", "renames over a path"),
    ("fs::remove_file(", "deletes a file"),
    ("fs::remove_dir", "deletes a directory"),
    ("fs::copy(", "copies onto a path"),
    ("File::create(", "creates a file"),
    (".truncate(true)", "truncates a file"),
];

/// Builtins whose body writes but whose label is deliberately something else,
/// each with the reason. This list may only shrink.
const ALLOWED: &[(&str, &str)] = &[
    (
        "apply",
        "Exec — runs a whole plan and gates it on a plan-derived approval token",
    ),
    (
        "input_editor",
        "Exec — spawns the user's editor; the temp file is an implementation detail",
    ),
    (
        "db_sqlite_export_csv",
        "unregistered (§5 item 3); tests/sql_injection.rs fires if it is dispatched",
    ),
    (
        "nodemon_run",
        "Exec — writes a generated config, then runs the watcher it configures",
    ),
];

fn source() -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/builtins.rs");
    strip_comments(&std::fs::read_to_string(path).expect("src/builtins.rs is readable"))
}

/// Blank out comments and char literals, preserving length and newlines.
///
/// Not optional, and worth recording why. The first version of this file skipped
/// only string literals, and reported nine offenders. Seven of them —
/// `sys_info`, `agent`, `project_name`, `project_version`, `platform_os_version`,
/// `platform_machine_id`, `vm_info` — contain no write at all. `'"'` and `'}'`
/// appear throughout `builtins.rs`, and copying a char literal through means the
/// `"` reads as opening a string and the `}` as closing the function: the
/// extracted bodies came out at 250KB, 490KB and 1.6MB, whole spans of the file,
/// and whichever builtins happened to sit inside a span were reported as
/// writers.
///
/// The tempting fix was to add all seven to the allowlist. That would have
/// recorded a scanner bug as a set of deliberate exceptions, and left the rule
/// checking less than it claimed. A broken scanner produces confident nonsense
/// in both directions — the two real findings in the same run, `file_backup` and
/// `file_move`, were sitting in the same list.
fn strip_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let (mut i, mut in_str, mut esc) = (0usize, false, false);
    while i < bytes.len() {
        let c = bytes[i] as char;
        let next = bytes.get(i + 1).map(|b| *b as char);
        if in_str {
            out.push(c);
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if c == '/' && next == Some('/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                out.push(' ');
                i += 1;
            }
            continue;
        }
        if c == '/' && next == Some('*') {
            let mut depth = 1;
            out.push_str("  ");
            i += 2;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                    i += 1;
                }
            }
            continue;
        }
        if c == '"' {
            in_str = true;
            out.push(c);
            i += 1;
            continue;
        }
        if c == '\'' {
            // A char literal, blanked entirely rather than copied. This is the
            // bug that actually mattered: `'"'` and `'}'` appear throughout this
            // file, and a scanner that copies them through then reads the `"` as
            // opening a string, or the `}` as closing the function. Bodies came
            // out at 250KB, 490KB and 1.6MB — whole spans of the file — and the
            // builtins that happened to sit inside those spans were reported as
            // writers. Blanking is length-preserving so offsets stay valid.
            //
            // A lifetime (`&'a str`) has no closing quote, so it is left alone.
            let close = (1..=4).find(|k| bytes.get(i + k) == Some(&b'\''));
            if let Some(k) = close {
                for _ in 0..=k {
                    out.push(' ');
                }
                i += k + 1;
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// `fn bi_<name>` bodies by brace matching, skipping string literals so a `}` in
/// a format string does not close the body early.
fn builtin_bodies(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut search = 0usize;
    while let Some(rel) = src[search..].find("\nfn bi_") {
        let start = search + rel + 1;
        search = start + 6;
        let rest = &src[start + 3..];
        let Some(name_end) = rest.find(|c: char| !(c.is_alphanumeric() || c == '_')) else {
            continue;
        };
        let Some(name) = rest[..name_end].strip_prefix("bi_") else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let Some(rel_brace) = src[start..].find('{') else {
            continue;
        };
        let brace = start + rel_brace;
        let (mut depth, mut i, mut in_str, mut esc) = (0i32, brace, false, false);
        while i < bytes.len() {
            let c = bytes[i] as char;
            if in_str {
                if esc {
                    esc = false;
                } else if c == '\\' {
                    esc = true;
                } else if c == '"' {
                    in_str = false;
                }
            } else if c == '"' {
                in_str = true;
            } else if c == '{' {
                depth += 1;
            } else if c == '}' {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            i += 1;
        }
        out.push((name.to_string(), src[brace..i.min(bytes.len())].to_string()));
    }
    out
}

#[test]
fn a_body_that_writes_carries_a_label_the_jail_keys_on() {
    let src = source();
    let bodies = builtin_bodies(&src);
    assert!(
        bodies.len() > 900,
        "only {} builtin bodies parsed; the scanner has drifted and this test is \
         checking almost nothing",
        bodies.len()
    );

    let mut offenders = Vec::new();
    let mut writers = 0usize;
    for (name, body) in &bodies {
        let Some((marker, why)) = WRITE_EVIDENCE
            .iter()
            .find(|(m, _)| body.contains(m))
            .copied()
        else {
            continue;
        };
        writers += 1;
        if ALLOWED.iter().any(|(n, _)| n == name) {
            continue;
        }
        let effect = effect_of(name);
        if effect.is_filesystem() {
            continue;
        }
        offenders.push(format!(
            "  {name}: {why} (`{marker}`) but effect_of = {effect:?}"
        ));
    }

    assert!(
        writers >= 15,
        "only {writers} bodies matched a write marker; the evidence list has drifted"
    );
    assert!(
        offenders.is_empty(),
        "{} builtin(s) write to the filesystem while carrying an effect the jail \
         does not key on.\n\n\
         `safety::guard` applies the workspace check only when \
         `effect.is_filesystem()` — `WriteLocal` or `Destructive`. A writing body \
         labelled `ReadLocal`, `Network` or `Pure` is therefore uncontained in \
         agent mode, which is how `file_edit`, `file_insert`, `file_patch` and \
         `session_export` modified files anywhere on disk while `file_write` to \
         the same path was refused.\n\n\
         Classify it in `safety::effect_of` and give it a guard, or add it to \
         ALLOWED in this file with the reason.\n\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

#[test]
fn the_scanner_would_catch_the_shape_it_is_for() {
    // A check on the checker, both directions.
    let bodies = builtin_bodies(&source());
    let by_name = |n: &str| {
        bodies
            .iter()
            .find(|(name, _)| name == n)
            .map(|(_, b)| b.clone())
    };
    let patch = by_name("file_patch").expect("file_patch must be found by the scanner");
    assert!(
        WRITE_EVIDENCE.iter().any(|(m, _)| patch.contains(m)),
        "file_patch's body must still register as a writer, or the rule above is \
         asserting nothing about the case that motivated it"
    );
    let pure_reader = by_name("file_read").or_else(|| by_name("cat"));
    if let Some(body) = pure_reader {
        assert!(
            !WRITE_EVIDENCE.iter().any(|(m, _)| body.contains(m)),
            "a plain reader must not register as a writer"
        );
    }
    assert!(Effect::WriteLocal.is_filesystem());
    assert!(Effect::Destructive.is_filesystem());
    assert!(!Effect::ReadLocal.is_filesystem());
    assert!(!Effect::Network.is_filesystem());
}

#[test]
fn the_allowlist_has_a_reason_for_every_entry() {
    for (name, reason) in ALLOWED {
        assert!(
            !reason.trim().is_empty(),
            "{name} is allowed without a reason; the reason is the point"
        );
    }
    assert!(
        ALLOWED.len() <= 4,
        "the write-evidence allowlist has grown to {}; it may only shrink",
        ALLOWED.len()
    );
}

// ── The behaviour ───────────────────────────────────────────────────────────

struct Jail {
    workspace: std::path::PathBuf,
    outside: std::path::PathBuf,
}

impl Jail {
    fn new(tag: &str) -> Self {
        let base = std::env::temp_dir();
        let workspace = base.join(format!("ae_wev_ws_{tag}_{}", std::process::id()));
        std::fs::create_dir_all(&workspace).expect("workspace");
        let outside = base.join(format!("ae_wev_out_{tag}_{}.txt", std::process::id()));
        std::fs::write(&outside, "original\n").expect("seed the victim file");
        std::env::set_var("AETHER_MODE", "agent");
        std::env::set_var("AETHER_WORKSPACE", &workspace);
        Self { workspace, outside }
    }
    fn target(&self) -> String {
        self.outside.to_string_lossy().into_owned()
    }
    fn unchanged(&self) -> bool {
        std::fs::read_to_string(&self.outside).is_ok_and(|s| s == "original\n")
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
fn an_in_place_editor_is_refused_where_a_write_is_refused() {
    let _g = lock();
    let jail = Jail::new("editors");
    let target = jail.target();

    // The reference answer from the builtin that was always labelled correctly.
    let write = call(
        "file_write",
        vec![Value::Str(target.clone()), Value::Str("x".into())],
    );
    assert!(
        refused_by_the_jail(&write),
        "precondition: file_write outside the workspace must be refused, else this \
         test proves nothing. Got: {write:?}"
    );

    // `file_patch` takes a list of {find, replace} records.
    let mut patch = std::collections::BTreeMap::new();
    patch.insert("find".to_string(), Value::Str("original".into()));
    patch.insert("replace".to_string(), Value::Str("pwned".into()));
    let r = call(
        "file_patch",
        vec![
            Value::Str(target.clone()),
            Value::Array(vec![Value::Record(patch)]),
        ],
    );
    assert!(
        refused_by_the_jail(&r),
        "file_patch rewrote a file that file_write is refused for. Got: {r:?}"
    );
    assert!(
        jail.unchanged(),
        "the file outside the workspace was modified"
    );
}

#[test]
fn the_alias_spellings_are_refused_too() {
    // The aliases share the implementation, so they share the guard — but the
    // effect label is what the jail reads, and `effect_of` resolves aliases by
    // inheritance. Both halves have to line up, so both are checked.
    let _g = lock();
    let jail = Jail::new("aliases");
    for name in ["patch_file", "edit_file", "text_edit", "insert_lines"] {
        assert!(
            effect_of(name).is_filesystem(),
            "{name} must carry a label the jail keys on, got {:?}",
            effect_of(name)
        );
    }
    let mut patch = std::collections::BTreeMap::new();
    patch.insert("find".to_string(), Value::Str("original".into()));
    patch.insert("replace".to_string(), Value::Str("pwned".into()));
    let r = call(
        "patch_file",
        vec![
            Value::Str(jail.target()),
            Value::Array(vec![Value::Record(patch)]),
        ],
    );
    assert!(
        refused_by_the_jail(&r),
        "patch_file must be jailed too: {r:?}"
    );
    assert!(jail.unchanged());
}

#[test]
fn session_export_does_not_write_outside_the_workspace() {
    let _g = lock();
    let jail = Jail::new("export");
    let r = call("session_export", vec![Value::Str(jail.target())]);
    assert!(
        refused_by_the_jail(&r),
        "session_export wrote a patch to an arbitrary path: {r:?}"
    );
    assert!(jail.unchanged());
}

#[test]
fn editing_inside_the_workspace_still_works() {
    // The check that keeps the fix from being a removal.
    let _g = lock();
    let jail = Jail::new("inside");
    let inside = jail.workspace.join("doc.txt");
    std::fs::write(&inside, "original\n").expect("seed");

    let mut patch = std::collections::BTreeMap::new();
    patch.insert("find".to_string(), Value::Str("original".into()));
    patch.insert("replace".to_string(), Value::Str("edited".into()));
    let r = call(
        "file_patch",
        vec![
            Value::Str(inside.to_string_lossy().into_owned()),
            Value::Array(vec![Value::Record(patch)]),
        ],
    );
    assert!(
        !refused_by_the_jail(&r),
        "a path inside the workspace must still be editable: {r:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&inside).unwrap_or_default(),
        "edited\n",
        "the edit must actually have been applied"
    );
}
