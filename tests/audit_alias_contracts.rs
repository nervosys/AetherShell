#![cfg(unix)]
//! Every file-writing builtin must refuse to touch the audit log through an
//! audit alias: a symbolic link inside the workspace whose target is the log.
//!
//! Each case builds a fresh fixture, calls one builtin with the alias as its
//! target path, and then checks two things: the call fails with
//! `E_OUTSIDE_WORKSPACE`, and the audit log is unchanged. The log can still
//! gain lines, because every refused call is itself recorded there as a
//! `deny_*` entry; so the check is that the old bytes are a prefix of the new
//! ones and every appended line is either a denial or an allow for some other
//! path (a two-path builtin guards both). A change to existing bytes, an
//! allowed alias, or a changed mode fails the case.
//!
//! Most of these wrote through the alias before 14.0.0: the guard's audit
//! check compared the path as written, so a link whose name was not the log's
//! passed, and the builtin then followed it. `safety::is_audit_artifact` now
//! also compares the resolved path and, on Unix, the file's identity. The
//! "Wrote through" comments record which cases were open.
use iron_shell::{builtins::call, env::Env, value::Value};
use std::{os::unix::fs::MetadataExt, os::unix::fs::PermissionsExt, path::PathBuf, sync::Mutex};

static LOCK: Mutex<()> = Mutex::new(());

struct Fixture {
    root: PathBuf,
    vars: Vec<(&'static str, Option<std::ffi::OsString>)>,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-audit-alias-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let keys = [
            "NO_COLOR",
            "IRONSHELL_MODE",
            "IRONSHELL_AGENT",
            "IRONSHELL_WORKSPACE",
            "IRONSHELL_POLICY",
            "IRONSHELL_AUDIT_LOG",
            "IRONSHELL_AUDIT_KEY",
            "IRONSHELL_AUDIT_KEY_FILE",
            "IRONSHELL_AUDIT_SINK",
            "IRONSHELL_MAX_FILES",
            "IRONSHELL_FIPS",
            "IRONSHELL_NET_ALLOW",
            "IRONSHELL_MAX_NET",
            "IRONSHELL_MAX_OPS",
            "IRONSHELL_MAX_PROCS",
        ];
        let fixture = Self {
            root,
            vars: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        };
        for key in keys {
            std::env::remove_var(key);
        }
        std::env::set_var("IRONSHELL_AUDIT_LOG", fixture.root.join("audit.log"));
        std::env::set_var("NO_COLOR", "1");
        std::env::set_var("IRONSHELL_AUDIT_KEY", "11".repeat(32));
        fixture
    }
    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        while iron_shell::tx::rollback().is_ok() {}
        for (key, value) in &self.vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        // Only recursively remove the exact absolute directory we exclusively created.
        if self
            .root
            .canonicalize()
            .is_ok_and(|resolved| resolved == self.root)
        {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn edit_record(fields: Vec<(&str, Value)>) -> Value {
    Value::Record(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

/// Each entry is a label. The builtin is the label's first word, so a label
/// such as "file_move source" names `file_move` with the alias as its source.
const CASES: &[&str] = &[
    // Wrote through before 14.0.0: write_file_atomically resolves the link and renames a
    // temporary file over the audit log; the guard only checked the alias string.
    "file_write",
    // Wrote through before 14.0.0: opens the alias with append and writes; the guard checks
    // the alias string, which is inside the workspace.
    "file_append",
    // Wrote through before 14.0.0: delegates to file_patch, which writes via write_file_atomically.
    "file_replace",
    // Wrote through before 14.0.0: write_file_atomically on the resolved link.
    "file_patch",
    // Wrote through before 14.0.0: write_file_atomically on the resolved link.
    "file_insert",
    // Wrote through before 14.0.0: write_file_atomically on the resolved link.
    "file_edit",
    // Wrote through before 14.0.0: save_json goes through write_file_atomically.
    "save_json",
    // Wrote through before 14.0.0 (metadata): opens the alias and sets its mtime, which is the log's mtime.
    "touch",
    // Removes the link itself, not the log. Still expected to be refused.
    "rm",
    // Renames the link itself, not the log. Still expected to be refused.
    "file_move source",
    // Renames over the link, not the log. Still expected to be refused.
    "file_move destination",
    // Destination is the alias; the copy is not verified to be guarded on the resolved path.
    "file_copy destination",
    // Wrote through before 14.0.0 (mode): set_permissions follows the link; only guard_local_write checks the alias string.
    "fs_chmod",
    // Canonicalises the path first, so the audit-artifact check fires.
    "fs_chown",
    // Reads the log and writes a sibling .bak; the write does not go through the alias.
    "file_backup",
    // The alias exists, so the archive target is not new.
    "zip_create",
    // Suspected write-through before 14.0.0: zip -u would rewrite the log as the archive.
    "zip_add",
    // The alias exists, so the archive target is not new.
    "tar_create",
    // Wrote through before 14.0.0: sed in file mode calls write_file_atomically on the file.
    "sed",
    // Wrote through before 14.0.0: file_delete_lines calls write_file_atomically on the file.
    "file_delete_lines",
    // Hard-links the log under a new name; the source must not be a symlink.
    "fs_link source",
    // Creates a link to the log; the existing fs_symlink test covers this.
    "fs_symlink target",
];

fn args_for(label: &str, fixture: &Fixture, alias: &str) -> Vec<Value> {
    let src = fixture.path("src");
    let audit = fixture.path("audit.log");
    match label {
        "file_write" => vec![s(alias), s("new")],
        "file_append" => vec![s(alias), s("more")],
        "file_replace" => vec![s(alias), s("known"), s("changed")],
        "file_patch" => vec![
            s(alias),
            edit_record(vec![("find", s("known")), ("replace", s("changed"))]),
        ],
        "file_insert" => vec![s(alias), Value::Int(1), s("inserted")],
        "file_edit" => vec![
            s(alias),
            edit_record(vec![
                ("op", s("replace")),
                ("line", Value::Int(1)),
                ("content", s("x")),
            ]),
        ],
        "save_json" => vec![s(alias), edit_record(vec![("a", Value::Int(1))])],
        "touch" => vec![s(alias)],
        "rm" => vec![s(alias)],
        "file_move source" => vec![s(alias), s(&fixture.path("moved"))],
        "file_move destination" => vec![s(&src), s(alias)],
        "file_copy destination" => vec![s(&src), s(alias)],
        "fs_chmod" => vec![s(alias), Value::Int(0o600)],
        "fs_chown" => {
            let uid = std::fs::metadata(&audit).unwrap().uid();
            vec![s(alias), Value::Int(uid as i64)]
        }
        "file_backup" => vec![s(alias)],
        "zip_create" => vec![s(alias), s(&src)],
        "zip_add" => vec![s(alias), s(&src)],
        "tar_create" => vec![s(alias), s(&src)],
        "sed" => vec![
            s("known"),
            s("changed"),
            s(alias),
            edit_record(vec![("file", Value::Bool(true))]),
        ],
        "file_delete_lines" => vec![s(alias), Value::Int(1)],
        "fs_link source" => vec![s(alias), s(&fixture.path("hard"))],
        "fs_symlink target" => vec![s(alias), s(&fixture.path("link"))],
        other => panic!("no arguments defined for case {other}"),
    }
}

/// Checks that `after` is `before` plus only denial lines.
fn audit_intact(before: &[u8], after: &[u8]) -> Result<(), String> {
    if !after.starts_with(before) {
        return Err("existing audit bytes changed".to_string());
    }
    let appended = String::from_utf8_lossy(&after[before.len()..]).into_owned();
    for line in appended.lines().filter(|line| !line.trim().is_empty()) {
        // A builtin that takes two paths guards each one, so an "allow" line for
        // the other, ordinary path is expected. Only an allowed alias is a gap.
        let names_alias = serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .and_then(|entry| {
                entry["resource"].as_str().map(|r| {
                    r.split(", ")
                        .any(|path| path.trim_end_matches('/').ends_with("/audit-alias"))
                })
            })
            .unwrap_or(true);
        if !line.contains("deny") && names_alias {
            return Err(format!("appended a non-denial line: {line}"));
        }
    }
    Ok(())
}

#[test]
fn every_file_writer_refuses_audit_aliases_and_leaves_the_log_alone() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut problems: Vec<String> = Vec::new();
    for label in CASES {
        let label: &str = label;
        let builtin = label.split_whitespace().next().unwrap();
        let fixture = Fixture::new();
        let mut env = Env::default();
        std::env::set_var("IRONSHELL_WORKSPACE", &fixture.root);
        let audit = fixture.path("audit.log");
        std::fs::write(&audit, "known audit entry\n").unwrap();
        std::fs::set_permissions(&audit, std::fs::Permissions::from_mode(0o640)).unwrap();
        let alias = fixture.path("audit-alias");
        std::os::unix::fs::symlink(&audit, &alias).unwrap();
        std::fs::write(fixture.path("src"), "source").unwrap();

        let before = std::fs::read(&audit).unwrap();
        let mode_before = std::fs::metadata(&audit).unwrap().permissions().mode() & 0o7777;
        let args = args_for(label, &fixture, &alias);

        match call(builtin, args, &mut env) {
            Ok(_) => problems.push(format!(
                "{label}: SUCCEEDED with an audit alias as its target"
            )),
            Err(e) => {
                let text = e.to_string();
                // file_copy refuses any symlink destination before it reaches
                // the guard, which is a refusal of the alias all the same.
                let own_refusal = label == "file_copy destination"
                    && text.contains("E_BAD_ARG")
                    && text.contains("symlink");
                if !text.contains("E_OUTSIDE_WORKSPACE") && !own_refusal {
                    let short: String = text.chars().take(200).collect();
                    problems.push(format!(
                        "{label}: expected E_OUTSIDE_WORKSPACE, got: {short}"
                    ));
                }
            }
        }

        let after = std::fs::read(&audit).unwrap();
        if let Err(why) = audit_intact(&before, &after) {
            problems.push(format!("{label}: audit log damaged: {why}"));
        }
        let mode_after = std::fs::metadata(&audit).unwrap().permissions().mode() & 0o7777;
        if mode_after != mode_before {
            problems.push(format!(
                "{label}: audit log mode changed from {mode_before:o} to {mode_after:o}"
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "audit alias protection gaps:\n{}",
        problems.join("\n")
    );
}
