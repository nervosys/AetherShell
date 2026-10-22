use aethershell::{
    builtins::{call, call_with_input},
    env::Env,
    value::Value,
};
use std::{path::PathBuf, sync::Mutex};
static LOCK: Mutex<()> = Mutex::new(());
struct Fixture {
    root: PathBuf,
    vars: Vec<(&'static str, Option<std::ffi::OsString>)>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("ae-text-contract-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let keys = [
            "NO_COLOR",
            "AETHER_MODE",
            "AETHER_AGENT",
            "AETHER_WORKSPACE",
            "AETHER_POLICY",
            "AETHER_AUDIT_LOG",
            "AETHER_AUDIT_KEY",
            "AETHER_AUDIT_KEY_FILE",
            "AETHER_AUDIT_SINK",
            "AETHER_MAX_FILES",
            "AETHER_FIPS",
            "AETHER_NET_ALLOW",
            "AETHER_MAX_NET",
            "AETHER_MAX_OPS",
            "AETHER_MAX_PROCS",
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
        std::env::set_var("AETHER_AUDIT_LOG", fixture.root.join("audit.log"));
        std::env::set_var("NO_COLOR", "1");
        std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
        fixture
    }
    fn path(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        while aethershell::tx::rollback().is_ok() {}
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
#[test]
fn chown_refuses_invalid_ids_and_conflicting_calls_without_changes() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "unchanged").unwrap();
    for owner in [
        Value::Int(-1),
        Value::Int(4_294_967_295),
        Value::Int(i64::MAX),
        Value::Float(1.0),
        s("1"),
        Value::Bool(true),
    ] {
        code(
            call("fs_chown", vec![s(&path), owner.clone()], &mut env),
            "E_BAD_ARG",
        );
        code(
            call("fs_chown", vec![s(&path), Value::Int(0), owner], &mut env),
            "E_BAD_ARG",
        );
    }
    for args in [
        vec![],
        vec![s(&path)],
        vec![Value::Int(1), Value::Int(0)],
        vec![s(&path), Value::Int(0), Value::Int(0), s("ignored")],
    ] {
        code(call("fs_chown", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "fs_chown",
            vec![s(&path), Value::Int(0)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}
#[test]
fn chown_guards_paths_and_codes_a_denied_transaction_change() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "unchanged").unwrap();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    // Inside a transaction a Unix chown is journaled (tests/tx_metadata_contracts.rs
    // rolls one back as root); giving a file to root without privilege is the
    // operating system's refusal, coded. Windows has no chown at all.
    #[cfg(unix)]
    let refused = if unsafe { libc::geteuid() } == 0 {
        None
    } else {
        Some("E_POLICY_DENY")
    };
    #[cfg(not(unix))]
    let refused = Some("E_UNIMPLEMENTED");
    aethershell::tx::begin().unwrap();
    if let Some(expected) = refused {
        code(
            call("fs_chown", vec![s("input"), Value::Int(0)], &mut env),
            expected,
        );
    }
    aethershell::tx::rollback().unwrap();
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::env::set_var("AETHER_WORKSPACE", jail);
    code(
        call("fs_chown", vec![s(&path), Value::Int(0)], &mut env),
        "E_OUTSIDE_WORKSPACE",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}
#[cfg(not(unix))]
#[test]
fn chown_reports_non_unix_support_explicitly_instead_of_false() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "unchanged").unwrap();
    for args in [
        vec![s(&path), Value::Int(0)],
        vec![
            s(&path),
            Value::Int(4_294_967_294),
            Value::Int(4_294_967_294),
        ],
        vec![s(&fixture.path("missing")), Value::Int(0)],
    ] {
        code(call("fs_chown", args, &mut env), "E_UNIMPLEMENTED");
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}
#[cfg(unix)]
#[test]
fn chown_native_calls_preserve_omitted_group_and_code_missing_files() {
    use std::os::unix::fs::MetadataExt;
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("-owned");
    std::fs::write(&path, "unchanged").unwrap();
    let before = std::fs::metadata(&path).unwrap();
    assert_eq!(
        call(
            "fs_chown",
            vec![s("-owned"), Value::Int(before.uid() as i64)],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    assert_eq!(std::fs::metadata(&path).unwrap().gid(), before.gid());
    assert_eq!(
        call(
            "fs_chown",
            vec![
                s("-owned"),
                Value::Int(before.uid() as i64),
                Value::Int(before.gid() as i64)
            ],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    let after = std::fs::metadata(&path).unwrap();
    assert_eq!((after.uid(), after.gid()), (before.uid(), before.gid()));
    code(
        call(
            "fs_chown",
            vec![s("missing"), Value::Int(before.uid() as i64)],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::os::unix::fs::symlink(&path, jail.join("alias")).unwrap();
    std::env::set_var("AETHER_WORKSPACE", jail);
    code(
        call(
            "fs_chown",
            vec![s("alias"), Value::Int(before.uid() as i64)],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let audit = fixture.root.join("audit.log");
    if !audit.exists() {
        std::fs::write(&audit, "owned audit fixture").unwrap();
    }
    std::os::unix::fs::symlink(&audit, fixture.root.join("audit-alias")).unwrap();
    code(
        call(
            "fs_chown",
            vec![s("audit-alias"), Value::Int(before.uid() as i64)],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
}
#[test]
fn structured_edits_support_aliases_pipes_and_descending_operation_order() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("input");
    let batch = Value::Array(vec![
        edit_record(vec![("op", s("delete")), ("line", Value::Int(1))]),
        edit_record(vec![
            ("operation", s("insert_at")),
            ("line", Value::Int(2)),
            ("text", s("B0")),
        ]),
        edit_record(vec![
            ("op", s("replace_range")),
            ("start_line", Value::Int(3)),
            ("content", s("C")),
        ]),
    ]);
    for name in ["file_edit", "edit_file", "text_edit"] {
        std::fs::write(&path, "a\r\nb\r\nc").unwrap();
        let result = diff_record(call(name, vec![s("input"), batch.clone()], &mut env).unwrap());
        assert_eq!(result["path"], s(&path));
        assert_eq!(result["success"], Value::Bool(true));
        assert_eq!(result["edits_applied"], Value::Int(3));
        assert_eq!(result["edits_failed"], Value::Int(0));
        assert_eq!(result["failed"], Value::Array(vec![]));
        assert_eq!(result["final_lines"], Value::Int(3));
        let Value::Array(applied) = &result["applied"] else {
            panic!("expected applied metadata");
        };
        assert_eq!(diff_record(applied[0].clone())["op"], s("replace_range"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "B0\nb\nC\n");
        let append = edit_record(vec![
            ("op", s("insert")),
            ("line", Value::Int(i64::MAX)),
            ("content", s("tail")),
        ]);
        let result =
            diff_record(call_with_input(name, vec![s("input")], Some(append), &mut env).unwrap());
        assert_eq!(result["final_lines"], Value::Int(4));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "B0\nb\nC\ntail\n");
    }
    std::fs::write(&path, "a\nb\nc\n").unwrap();
    call(
        "file_edit",
        vec![
            s("input"),
            edit_record(vec![
                ("op", s("replace")),
                ("line", Value::Int(2)),
                ("end_line", Value::Int(i64::MAX)),
                ("content", s("")),
            ]),
        ],
        &mut env,
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\n");
    std::fs::write(&path, "x\n").unwrap();
    let tied = Value::Array(vec![
        edit_record(vec![
            ("op", s("insert")),
            ("line", Value::Int(1)),
            ("content", s("A")),
        ]),
        edit_record(vec![
            ("op", s("insert")),
            ("line", Value::Int(1)),
            ("content", s("B")),
        ]),
    ]);
    call("file_edit", vec![s("input"), tied], &mut env).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "B\nA\nx\n");
    std::fs::write(&path, "\n").unwrap();
    call(
        "file_edit",
        vec![s("input"), Value::Array(vec![])],
        &mut env,
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "\n");
}
#[test]
fn structured_edits_refuse_entire_malformed_or_inapplicable_batches() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "original\n").unwrap();
    let valid = edit_record(vec![
        ("op", s("insert")),
        ("line", Value::Int(1)),
        ("content", s("new")),
    ]);
    for invalid in [
        Value::Int(1),
        edit_record(vec![]),
        edit_record(vec![("op", s("unknown")), ("line", Value::Int(1))]),
        edit_record(vec![("op", s("delete")), ("line", Value::Int(0))]),
        edit_record(vec![("op", s("delete")), ("line", Value::Float(1.0))]),
        edit_record(vec![
            ("op", s("delete")),
            ("line", Value::Int(2)),
            ("end_line", Value::Int(1)),
        ]),
        edit_record(vec![
            ("op", s("delete")),
            ("line", Value::Int(1)),
            ("end_line", s("2")),
        ]),
        edit_record(vec![("op", s("replace")), ("line", Value::Int(1))]),
        edit_record(vec![
            ("op", s("replace")),
            ("line", Value::Int(1)),
            ("content", Value::Int(2)),
        ]),
        edit_record(vec![
            ("op", s("insert")),
            ("line", Value::Int(1)),
            ("content", s("x")),
            ("end_line", Value::Int(2)),
        ]),
        edit_record(vec![
            ("op", s("delete")),
            ("operation", s("delete")),
            ("line", Value::Int(1)),
        ]),
        edit_record(vec![
            ("op", s("delete")),
            ("line", Value::Int(1)),
            ("start_line", Value::Int(1)),
        ]),
        edit_record(vec![
            ("op", s("replace")),
            ("line", Value::Int(1)),
            ("content", s("x")),
            ("text", s("x")),
        ]),
        edit_record(vec![
            ("op", s("delete")),
            ("line", Value::Int(1)),
            ("content", s("ignored")),
        ]),
    ] {
        code(
            call(
                "file_edit",
                vec![s(&path), Value::Array(vec![valid.clone(), invalid])],
                &mut env,
            ),
            "E_BAD_ARG",
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    }
    let delete = edit_record(vec![("op", s("delete")), ("line", Value::Int(1))]);
    code(
        call(
            "file_edit",
            vec![s(&path), Value::Array(vec![delete.clone(), delete])],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
    code(
        call_with_input(
            "file_edit",
            vec![s(&path), valid.clone()],
            Some(valid.clone()),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call("file_edit", vec![s(&path), valid, s("ignored")], &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original\n");
}
#[test]
fn structured_edits_bound_batches_work_lines_and_output_before_writing() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    let insert = edit_record(vec![
        ("op", s("insert")),
        ("line", Value::Int(1)),
        ("content", s("")),
    ]);
    std::fs::write(&path, "original").unwrap();
    code(
        call(
            "file_edit",
            vec![s(&path), Value::Array(vec![insert.clone(); 10_001])],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    let content = "a\n".repeat(1001);
    std::fs::write(&path, &content).unwrap();
    code(
        call(
            "file_edit",
            vec![s(&path), Value::Array(vec![insert; 10_000])],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    std::fs::write(&path, "\n".repeat(1_000_001)).unwrap();
    code(
        call("file_edit", vec![s(&path), Value::Array(vec![])], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    assert_eq!(std::fs::metadata(&path).unwrap().len(), 1_000_001);
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 - 2)
        .unwrap();
    let append = edit_record(vec![
        ("op", s("insert")),
        ("line", Value::Int(2)),
        ("content", s("xx")),
    ]);
    code(
        call("file_edit", vec![s(&path), append], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().len(),
        64 * 1024 * 1024 - 2
    );
}
#[test]
fn structured_edits_code_read_failures_guard_aliases_and_roll_back() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("input");
    let edit = edit_record(vec![
        ("op", s("replace")),
        ("line", Value::Int(1)),
        ("content", s("new")),
    ]);
    code(
        call("file_edit", vec![s("missing"), edit.clone()], &mut env),
        "E_NOT_FOUND",
    );
    std::fs::write(&path, [255]).unwrap();
    code(
        call("file_edit", vec![s("input"), edit.clone()], &mut env),
        "E_IO",
    );
    assert_eq!(std::fs::read(&path).unwrap(), [255]);
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call("file_edit", vec![s("input"), edit.clone()], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    std::fs::write(&path, "a\r\nb").unwrap();
    let unrelated = path.clone() + ".aether_tmp";
    std::fs::write(&unrelated, "keep").unwrap();
    aethershell::tx::begin().unwrap();
    call("file_edit", vec![s("input"), edit.clone()], &mut env).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new\nb\n");
    assert_eq!(std::fs::read_to_string(&unrelated).unwrap(), "keep");
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb");
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::env::set_var("AETHER_WORKSPACE", jail);
    for name in ["file_edit", "edit_file", "text_edit"] {
        code(
            call(name, vec![s(&path), edit.clone()], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb");
}
#[test]
fn chmod_validates_modes_and_preserves_platform_permission_semantics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("input");
    std::fs::write(&path, "unchanged").unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    for name in ["fs_chmod", "chmod"] {
        for (mode, expected) in [
            (Value::Int(0), 0),
            (Value::Int(0o400), 0o400),
            (Value::Int(0o600), 0o600),
            (s("0o0644"), 0o644),
            (s("0"), 0),
            (s("0644"), 0o644),
        ] {
            let result = call(name, vec![s("input"), mode], &mut env);
            let changed = std::fs::metadata(&path).map(|m| m.permissions());
            // Restore before assertions so Windows read-only files cannot leak.
            std::fs::set_permissions(&path, original.clone()).unwrap();
            assert_eq!(result.unwrap(), Value::Bool(true));
            let changed = changed.unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(changed.mode() & 0o7777, expected);
            }
            #[cfg(not(unix))]
            assert_eq!(changed.readonly(), expected & 0o200 == 0);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
        }
    }
}
#[test]
fn chmod_refuses_invalid_modes_and_calls_without_changing_permissions() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "unchanged").unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    for mode in [
        Value::Int(-1),
        Value::Int(0o10000),
        Value::Float(420.0),
        Value::Bool(true),
        s(""),
        s("0o"),
        s("0x644"),
        s(" 644"),
        s("-1"),
        s("888"),
        s("10000"),
        s("777777777777777777777777777"),
    ] {
        code(
            call("fs_chmod", vec![s(&path), mode], &mut env),
            "E_BAD_ARG",
        );
        assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
    }
    for args in [
        vec![],
        vec![s(&path)],
        vec![Value::Int(1), Value::Int(420)],
        vec![s(&path), Value::Int(420), s("ignored")],
    ] {
        code(call("chmod", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "chmod",
            vec![s(&path), Value::Int(420)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
}
#[test]
fn chmod_codes_missing_paths_guards_aliases_and_journals_transaction_changes() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    code(
        call("chmod", vec![s("missing"), Value::Int(420)], &mut env),
        "E_NOT_FOUND",
    );
    std::fs::write(&path, "unchanged").unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    // Inside a transaction a mode change is journaled, not refused: rollback restores
    // the pre-image (commit 1b55800 replaced the old E_UNIMPLEMENTED refusal).
    aethershell::tx::begin().unwrap();
    for name in ["fs_chmod", "chmod"] {
        assert_eq!(
            call(name, vec![s("input"), Value::Int(0)], &mut env).unwrap(),
            Value::Bool(true)
        );
    }
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::env::set_var("AETHER_WORKSPACE", jail);
    for name in ["fs_chmod", "chmod"] {
        code(
            call(name, vec![s(&path), Value::Int(0)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}
#[test]
fn hard_links_share_binary_content_and_roll_back_new_destinations() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let source = fixture.path("source");
    let dest = fixture.path("link");
    std::fs::write(&source, [0, 255, 128]).unwrap();
    aethershell::tx::begin().unwrap();
    assert_eq!(
        call("fs_link", vec![s("source"), s("link")], &mut env).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(std::fs::read(&dest).unwrap(), [0, 255, 128]);
    // A real hard link shares bytes; an independent copy would fail this.
    std::fs::write(&source, "changed").unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "changed");
    aethershell::tx::rollback().unwrap();
    assert!(!std::path::Path::new(&dest).exists());
    // Creating a link journals its destination, not arbitrary later source writes.
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "changed");
}
#[test]
fn hard_links_refuse_invalid_calls_and_existing_destinations_without_snapshotting() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let source = fixture.path("source");
    let dest = fixture.path("dest");
    std::fs::write(&source, "source").unwrap();
    for args in [
        vec![],
        vec![s(&source)],
        vec![s(&source), s(&dest), Value::Bool(true)],
        vec![Value::Int(1), s(&dest)],
        vec![s(&source), Value::Bool(false)],
    ] {
        code(call("fs_link", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "fs_link",
            vec![s(&source), s(&dest)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert!(!std::path::Path::new(&dest).exists());
    std::fs::write(&dest, "prior").unwrap();
    code(
        call("fs_link", vec![s(&source), s(&dest)], &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "prior");
    let directory = fixture.root.join("directory");
    std::fs::create_dir(&directory).unwrap();
    aethershell::tx::begin().unwrap();
    code(
        call(
            "fs_link",
            vec![s(&source), s(&directory.to_string_lossy())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    std::fs::write(directory.join("later"), "keep").unwrap();
    aethershell::tx::rollback().unwrap();
    assert_eq!(
        std::fs::read_to_string(directory.join("later")).unwrap(),
        "keep"
    );
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "source");
}
#[test]
fn hard_links_code_missing_paths_and_refuse_directory_sources() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.path("source");
    let dest = fixture.path("dest");
    code(
        call("fs_link", vec![s(&source), s(&dest)], &mut env),
        "E_NOT_FOUND",
    );
    std::fs::write(&source, "source").unwrap();
    code(
        call(
            "fs_link",
            vec![
                s(&source),
                s(&fixture.root.join("missing").join("child").to_string_lossy()),
            ],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    let directory = fixture.root.join("directory");
    std::fs::create_dir(&directory).unwrap();
    code(
        call(
            "fs_link",
            vec![s(&directory.to_string_lossy()), s(&dest)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert!(!std::path::Path::new(&dest).exists());
    assert_eq!(std::fs::read_to_string(source).unwrap(), "source");
}
#[test]
fn hard_links_guard_both_source_and_new_destination() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let inside = workspace.join("source");
    let outside = fixture.root.join("outside");
    std::fs::write(&inside, "inside").unwrap();
    std::fs::write(&outside, "outside").unwrap();
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    code(
        call(
            "fs_link",
            vec![s(&outside.to_string_lossy()), s("import")],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    code(
        call(
            "fs_link",
            vec![s("source"), s(&fixture.path("escape"))],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    assert!(!workspace.join("import").exists());
    assert!(!fixture.root.join("escape").exists());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "outside");
    assert_eq!(std::fs::read_to_string(inside).unwrap(), "inside");
}
#[cfg(unix)]
#[test]
fn hard_links_refuse_symlink_sources_and_destinations() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.root.join("source");
    let alias = fixture.root.join("alias");
    let dest = fixture.root.join("dest");
    std::fs::write(&source, "source").unwrap();
    std::os::unix::fs::symlink(&source, &alias).unwrap();
    code(
        call(
            "fs_link",
            vec![s(&alias.to_string_lossy()), s(&dest.to_string_lossy())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert!(!dest.exists());
    std::os::unix::fs::symlink(fixture.root.join("absent"), &dest).unwrap();
    code(
        call(
            "fs_link",
            vec![s(&source.to_string_lossy()), s(&dest.to_string_lossy())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert!(std::fs::symlink_metadata(dest)
        .unwrap()
        .file_type()
        .is_symlink());
}
#[test]
fn delete_lines_preserves_aliases_ranges_and_literal_selectors() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("input");
    for name in ["file_delete_lines", "delete_lines", "remove_lines"] {
        std::fs::write(&path, "a\r\nb\r\nc").unwrap();
        let result = diff_record(call(name, vec![s("input"), Value::Int(2)], &mut env).unwrap());
        assert_eq!(result["path"], s(&path));
        assert_eq!(result["lines_deleted"], Value::Int(1));
        assert_eq!(result["lines_remaining"], Value::Int(2));
        assert_eq!(result["success"], Value::Bool(true));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\nc\n");
        std::fs::write(&path, "a\nb\nc\n").unwrap();
        call(
            name,
            vec![s("input"), Value::Int(1), Value::Int(2)],
            &mut env,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "c\n");
    }
    for key in ["matching", "contains", "pattern", "exact"] {
        std::fs::write(&path, "x\nprefix x\nother\n").unwrap();
        let selector = Value::Record(std::collections::BTreeMap::from([(key.into(), s("x"))]));
        call("file_delete_lines", vec![s("input"), selector], &mut env).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            if key == "exact" {
                "prefix x\nother\n"
            } else {
                "other\n"
            }
        );
    }
    // Patterns are literal, including metacharacters and the empty pattern.
    std::fs::write(&path, "x\n.*\n").unwrap();
    call(
        "delete_lines",
        vec![
            s("input"),
            Value::Record(std::collections::BTreeMap::from([(
                "pattern".into(),
                s(".*"),
            )])),
        ],
        &mut env,
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "x\n");
    call(
        "delete_lines",
        vec![
            s("input"),
            Value::Record(std::collections::BTreeMap::from([(
                "contains".into(),
                s(""),
            )])),
        ],
        &mut env,
    )
    .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
}
#[test]
fn delete_lines_handles_empty_files_and_ranges_beyond_eof() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    for text in ["", "\n", "a", "a\nb\n"] {
        std::fs::write(&path, text).unwrap();
        let result = diff_record(
            call(
                "delete_lines",
                vec![s(&path), Value::Int(i64::MAX)],
                &mut env,
            )
            .unwrap(),
        );
        assert_eq!(result["lines_deleted"], Value::Int(0));
        assert_eq!(
            result["lines_remaining"],
            Value::Int(text.lines().count() as i64)
        );
        let expected = text
            .lines()
            .map(|line| format!("{line}\n"))
            .collect::<String>();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), expected);
        call(
            "delete_lines",
            vec![s(&path), Value::Int(1), Value::Int(i64::MAX)],
            &mut env,
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }
}
#[test]
fn delete_lines_refuses_malformed_selections_without_modifying_files() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "original").unwrap();
    for tail in [
        vec![],
        vec![Value::Int(0)],
        vec![Value::Int(-1)],
        vec![Value::Float(1.0)],
        vec![Value::Int(2), Value::Int(1)],
        vec![Value::Int(1), Value::Int(0)],
        vec![Value::Int(1), s("2")],
        vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        vec![Value::Record(Default::default())],
        vec![Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            s("x"),
        )]))],
        vec![Value::Record(std::collections::BTreeMap::from([(
            "exact".into(),
            Value::Int(1),
        )]))],
        vec![Value::Record(std::collections::BTreeMap::from([
            ("exact".into(), s("x")),
            ("contains".into(), s("x")),
        ]))],
        vec![
            Value::Record(std::collections::BTreeMap::from([("exact".into(), s("x"))])),
            Value::Int(1),
        ],
    ] {
        let mut args = vec![s(&path)];
        args.extend(tail);
        code(call("file_delete_lines", args, &mut env), "E_BAD_ARG");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }
    code(
        call_with_input(
            "remove_lines",
            vec![s(&path), Value::Int(1)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    // The legacy predictable temporary name is an unrelated file.
    let unrelated = path.clone() + ".aether_tmp";
    std::fs::write(&unrelated, "keep").unwrap();
    call("delete_lines", vec![s(&path), Value::Int(1)], &mut env).unwrap();
    assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "keep");
}
#[test]
fn delete_lines_codes_read_failures_guards_paths_and_supports_rollback() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    code(
        call("delete_lines", vec![s("missing"), Value::Int(1)], &mut env),
        "E_NOT_FOUND",
    );
    std::fs::write(&path, [255]).unwrap();
    code(
        call("delete_lines", vec![s("input"), Value::Int(1)], &mut env),
        "E_IO",
    );
    assert_eq!(std::fs::read(&path).unwrap(), [255]);
    let large = fixture.path("large");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call("delete_lines", vec![s("large"), Value::Int(1)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    std::fs::write(&path, "a\r\nb").unwrap();
    aethershell::tx::begin().unwrap();
    call("remove_lines", vec![s("input"), Value::Int(1)], &mut env).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "b\n");
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb");
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::env::set_var("AETHER_WORKSPACE", jail);
    for name in ["file_delete_lines", "delete_lines", "remove_lines"] {
        code(
            call(name, vec![s(&path), Value::Int(1)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb");
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    code(
        call("delete_lines", vec![s("input"), Value::Int(1)], &mut env),
        "E_NEEDS_APPROVAL",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "a\r\nb");
}
#[test]
fn file_copy_aliases_preserve_binary_bytes_and_resolve_workspace_paths() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let bytes = [0, 255, 128, 10];
    std::fs::write(fixture.path("input"), bytes).unwrap();
    for name in ["file_copy", "copy_file", "cp"] {
        let result = diff_record(call(name, vec![s("input"), s(name)], &mut env).unwrap());
        assert_eq!(result["source"], s(&fixture.path("input")));
        assert_eq!(result["dest"], s(&fixture.path(name)));
        assert_eq!(result["bytes"], Value::Int(4));
        assert_eq!(result["success"], Value::Bool(true));
        assert_eq!(std::fs::read(fixture.path(name)).unwrap(), bytes);
    }
}
#[test]
fn file_copy_preflights_conflicts_overlap_and_budget_without_writes() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.path("source");
    let dest = fixture.path("dest");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(std::path::Path::new(&source).join("first"), "new").unwrap();
    std::fs::write(std::path::Path::new(&dest).join("first"), "old").unwrap();
    std::fs::write(std::path::Path::new(&source).join("conflict"), "file").unwrap();
    std::fs::create_dir(std::path::Path::new(&dest).join("conflict")).unwrap();
    code(
        call("cp", vec![s(&source), s(&dest)], &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(
        std::fs::read_to_string(std::path::Path::new(&dest).join("first")).unwrap(),
        "old"
    );
    code(
        call("cp", vec![s(&source), s(&source)], &mut env),
        "E_BAD_ARG",
    );
    code(
        call(
            "cp",
            vec![
                s(&source),
                s(&std::path::Path::new(&source)
                    .join("nested")
                    .to_string_lossy()),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert!(!std::path::Path::new(&std::path::Path::new(&source).join("nested")).exists());
    let oversized = fixture.path("oversized");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    let output = fixture.path("output");
    code(
        call("cp", vec![s(&oversized), s(&output)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    assert!(!std::path::Path::new(&output).exists());
    let many = fixture.path("many");
    std::fs::create_dir(&many).unwrap();
    for index in 0..10_000 {
        std::fs::write(std::path::Path::new(&many).join(index.to_string()), []).unwrap();
    }
    code(
        call("cp", vec![s(&many), s(&output)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    assert!(!std::path::Path::new(&output).exists());
}
#[test]
fn file_copy_refuses_invalid_calls_and_reports_missing_paths() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.path("input");
    let dest = fixture.path("output");
    std::fs::write(&source, "source").unwrap();
    for args in [
        vec![],
        vec![s(&source)],
        vec![s(&source), s(&dest), Value::Bool(true)],
        vec![Value::Int(1), s(&dest)],
        vec![s(&source), Value::Bool(false)],
    ] {
        code(call("file_copy", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "file_copy",
            vec![s(&source), s(&dest)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call("cp", vec![s(&fixture.path("missing")), s(&dest)], &mut env),
        "E_NOT_FOUND",
    );
    code(
        call(
            "cp",
            vec![s(&source), s(&fixture.path("missing/output"))],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    assert!(!std::path::Path::new(&dest).exists());
    assert_eq!(std::fs::read_to_string(source).unwrap(), "source");
}
#[test]
fn file_copy_imports_outside_sources_and_rolls_back_directory_merges() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.path("source");
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::create_dir(std::path::Path::new(&source).join("nested")).unwrap();
    std::fs::write(
        std::path::Path::new(&source).join("nested").join("new"),
        "new",
    )
    .unwrap();
    std::fs::write(
        std::path::Path::new(&source).join("existing"),
        "replacement",
    )
    .unwrap();
    let dest = workspace.join("merged");
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(dest.join("existing"), "old").unwrap();
    std::fs::write(dest.join("unrelated"), "keep").unwrap();
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    aethershell::tx::begin().unwrap();
    let result = diff_record(call("copy_file", vec![s(&source), s("merged")], &mut env).unwrap());
    assert_eq!(result["bytes"], Value::Int(14));
    assert_eq!(
        std::fs::read_to_string(dest.join("existing")).unwrap(),
        "replacement"
    );
    std::fs::write(dest.join("after_copy"), "unrelated later write").unwrap();
    aethershell::tx::rollback().unwrap();
    assert_eq!(
        std::fs::read_to_string(dest.join("existing")).unwrap(),
        "old"
    );
    assert!(!dest.join("nested").exists());
    assert_eq!(
        std::fs::read_to_string(dest.join("after_copy")).unwrap(),
        "unrelated later write"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("unrelated")).unwrap(),
        "keep"
    );
    aethershell::tx::begin().unwrap();
    call("cp", vec![s(&source), s("new_tree")], &mut env).unwrap();
    aethershell::tx::rollback().unwrap();
    assert!(!workspace.join("new_tree").exists());
    code(
        call(
            "cp",
            vec![s(&source), s(&fixture.path("outside_copy"))],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    assert!(!fixture.root.join("outside_copy").exists());
}
#[cfg(unix)]
#[test]
fn file_copy_refuses_source_and_destination_symlinks_before_merging() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.root.join("source");
    let dest = fixture.root.join("dest");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&dest).unwrap();
    std::fs::write(source.join("ordinary"), "new").unwrap();
    std::fs::write(dest.join("ordinary"), "old").unwrap();
    std::os::unix::fs::symlink(source.join("ordinary"), source.join("link")).unwrap();
    code(
        call(
            "cp",
            vec![s(&source.to_string_lossy()), s(&dest.to_string_lossy())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("ordinary")).unwrap(),
        "old"
    );
    std::fs::remove_file(source.join("link")).unwrap();
    std::fs::write(source.join("link"), "new").unwrap();
    std::os::unix::fs::symlink(dest.join("ordinary"), dest.join("link")).unwrap();
    code(
        call(
            "cp",
            vec![s(&source.to_string_lossy()), s(&dest.to_string_lossy())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("ordinary")).unwrap(),
        "old"
    );
    std::fs::remove_file(dest.join("link")).unwrap();
    use std::os::unix::ffi::OsStringExt;
    std::fs::write(
        source.join(std::ffi::OsString::from_vec(vec![255])),
        "bad name",
    )
    .unwrap();
    code(
        call(
            "cp",
            vec![s(&source.to_string_lossy()), s(&dest.to_string_lossy())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("ordinary")).unwrap(),
        "old"
    );
}
#[test]
fn file_backup_preserves_binary_content_and_supports_both_call_forms() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let source = fixture.path("input");
    let bytes = [0, 0xff, 0x80, 10];
    std::fs::write(&source, bytes).unwrap();
    for name in ["file_backup", "backup_file"] {
        let record = diff_record(call(name, vec![s("input")], &mut env).unwrap());
        assert_eq!(record["path"], s(&source));
        assert_eq!(record["backup"], s(&(source.clone() + ".bak")));
        assert_eq!(record["bytes"], Value::Int(4));
        assert_eq!(record["success"], Value::Bool(true));
        assert_eq!(std::fs::read(source.clone() + ".bak").unwrap(), bytes);
        call_with_input(name, vec![s(".save")], Some(s("input")), &mut env).unwrap();
        assert_eq!(std::fs::read(source.clone() + ".save").unwrap(), bytes);
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::WriteLocal
        );
    }
}
#[test]
fn file_backup_validates_suffixes_without_creating_or_clobbering_files() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.path("input");
    std::fs::write(&source, "original").unwrap();
    for suffix in [
        "", ".", " ", ".save ", "/escape", "\\escape", ":stream", "\n",
    ] {
        code(
            call("file_backup", vec![s(&source), s(suffix)], &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call("file_backup", vec![s(&source), Value::Int(1)], &mut env),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "backup_file",
            vec![Value::Int(1)],
            Some(s(&source)),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "file_backup",
            vec![s(&source), s(".save")],
            Some(s(&source)),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input("file_backup", vec![], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
    code(call("file_backup", vec![], &mut env), "E_BAD_ARG");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "original");
    assert!(!std::path::Path::new(&(source + ".bak")).exists());
}
#[test]
fn file_backup_jails_paths_and_rolls_back_new_and_existing_destinations() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "secret").unwrap();
    std::fs::write(workspace.join("input"), "new").unwrap();
    std::fs::write(workspace.join("input.bak"), "prior").unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    for name in ["file_backup", "backup_file"] {
        code(
            call(name, vec![s(&outside)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        assert!(!std::path::Path::new(&(outside.clone() + ".bak")).exists());
    }
    aethershell::tx::begin().unwrap();
    call("file_backup", vec![s("input")], &mut env).unwrap();
    call_with_input("backup_file", vec![s(".save")], Some(s("input")), &mut env).unwrap();
    assert_eq!(
        std::fs::read_to_string(workspace.join("input.bak")).unwrap(),
        "new"
    );
    aethershell::tx::rollback().unwrap();
    assert_eq!(
        std::fs::read_to_string(workspace.join("input.bak")).unwrap(),
        "prior"
    );
    assert!(!workspace.join("input.save").exists());
    assert_eq!(
        std::fs::read_to_string(workspace.join("input")).unwrap(),
        "new"
    );
}
#[test]
fn file_backup_codes_read_and_copy_failures_and_bounds_source_size() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    code(
        call("file_backup", vec![s(&fixture.path("missing"))], &mut env),
        "E_NOT_FOUND",
    );
    std::fs::create_dir(fixture.root.join("directory")).unwrap();
    code(
        call("file_backup", vec![s(&fixture.path("directory"))], &mut env),
        "E_BAD_ARG",
    );
    let source = fixture.path("large");
    std::fs::File::create(&source)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call("file_backup", vec![s(&source)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    assert!(!std::path::Path::new(&(source + ".bak")).exists());
    let source = fixture.path("input");
    std::fs::write(&source, "original").unwrap();
    std::fs::create_dir(source.clone() + ".bak").unwrap();
    aethershell::tx::begin().unwrap();
    code(call("file_backup", vec![s(&source)], &mut env), "E_BAD_ARG");
    let later = std::path::PathBuf::from(source.clone() + ".bak").join("later");
    std::fs::write(&later, "unrelated").unwrap();
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&later).unwrap(), "unrelated");
    assert_eq!(std::fs::read_to_string(&source).unwrap(), "original");
    assert!(!std::fs::read_dir(&fixture.root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".aether_tmp_")));
}
#[test]
fn file_backup_preserves_source_permissions() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let source = fixture.path("readonly");
    let backup = source.clone() + ".bak";
    std::fs::write(&source, "bytes").unwrap();
    let original = std::fs::metadata(&source).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_readonly(true);
    std::fs::set_permissions(&source, readonly).unwrap();
    let result = call("file_backup", vec![s(&source)], &mut env);
    let backup_readonly = std::fs::metadata(&backup).map(|m| m.permissions().readonly());
    // Restore permissions before assertions so failed assertions also leave cleanup possible.
    std::fs::set_permissions(&source, original.clone()).unwrap();
    if std::path::Path::new(&backup).exists() {
        std::fs::set_permissions(&backup, original.clone()).unwrap();
    }
    result.unwrap();
    assert!(backup_readonly.unwrap());
    assert_eq!(std::fs::read(&backup).unwrap(), b"bytes");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let locked_path = source.clone() + ".locked";
        std::fs::write(&locked_path, "prior").unwrap();
        // Permit metadata/read access, while denying deletion/replacement.
        let locked = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&locked_path)
            .unwrap();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        std::fs::set_permissions(&source, readonly).unwrap();
        let result = call("backup_file", vec![s(&source), s(".locked")], &mut env);
        drop(locked);
        std::fs::set_permissions(&source, original).unwrap();
        let failure = result.unwrap_err().to_string();
        assert!(
            failure.contains("E_IO") || failure.contains("E_POLICY_DENY"),
            "{failure}"
        );
        assert_eq!(std::fs::read_to_string(&locked_path).unwrap(), "prior");
        assert!(!std::fs::read_dir(&fixture.root).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".aether_tmp_")));
    }
}
fn pager_file(file: bool) -> Value {
    Value::Record(std::collections::BTreeMap::from([(
        "file".into(),
        Value::Bool(file),
    )]))
}
#[test]
fn bat_reads_literal_plain_files_and_guards_both_aliases() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("-plain"), "a\r\né\r\n").unwrap();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "secret").unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    for name in ["bat", "bat_view"] {
        assert_eq!(
            call(name, vec![s("-plain")], &mut env).unwrap(),
            s("a\r\né\r\n")
        );
        code(
            call(name, vec![s(&outside)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        code(call(name, vec![Value::Int(1)], &mut env), "E_BAD_ARG");
        code(
            call(name, vec![s("-plain"), s("extra")], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input(name, vec![s("-plain")], Some(s("ignored")), &mut env),
            "E_BAD_ARG",
        );
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::ReadLocal
        );
    }
}
#[test]
fn pager_preserves_short_text_and_exact_page_boundaries() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["less", "pager"] {
        for count in [0, 1, 40, 41, 80, 81] {
            let text = (0..count)
                .map(|n| format!("line{n}\r\n"))
                .collect::<String>();
            let direct = call(name, vec![s(&text)], &mut env).unwrap();
            assert_eq!(
                call_with_input(name, vec![pager_file(false)], Some(s(&text)), &mut env).unwrap(),
                direct
            );
            if count <= 40 {
                assert_eq!(direct, s(&text));
            } else {
                let record = diff_record(direct);
                assert_eq!(
                    record["content"],
                    s(&(0..40)
                        .map(|n| format!("line{n}"))
                        .collect::<Vec<_>>()
                        .join("\n"))
                );
                assert_eq!(record["total_lines"], Value::Int(count));
                assert_eq!(record["page_size"], Value::Int(40));
                assert_eq!(record["pages"], Value::Int((count + 39) / 40));
                assert_eq!(record["hint"], s("Use head/tail for specific line ranges"));
            }
        }
    }
}
#[test]
fn pager_validates_options_and_guards_resolved_file_reads() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("input"), "a\r\nb\r\n").unwrap();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "secret").unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    for name in ["less", "pager"] {
        assert_eq!(
            call(name, vec![s("input"), pager_file(true)], &mut env).unwrap(),
            s("a\r\nb\r\n")
        );
        assert_eq!(
            call_with_input(name, vec![pager_file(true)], Some(s("input")), &mut env).unwrap(),
            s("a\r\nb\r\n")
        );
        code(
            call(name, vec![s(&outside), pager_file(true)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        code(
            call_with_input(name, vec![pager_file(true)], Some(s(&outside)), &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        for options in [
            Value::Bool(true),
            Value::Record(std::collections::BTreeMap::from([(
                "file".into(),
                s("true"),
            )])),
            Value::Record(std::collections::BTreeMap::from([(
                "unknown".into(),
                Value::Bool(true),
            )])),
        ] {
            code(
                call(name, vec![s("input"), options.clone()], &mut env),
                "E_BAD_ARG",
            );
            code(
                call_with_input(name, vec![options], Some(s("input")), &mut env),
                "E_BAD_ARG",
            );
        }
        code(
            call_with_input(
                name,
                vec![s("input"), pager_file(true)],
                Some(s("conflict")),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        code(
            call_with_input(name, vec![], Some(Value::Int(1)), &mut env),
            "E_BAD_ARG",
        );
        code(call(name, vec![], &mut env), "E_BAD_ARG");
    }
}
#[test]
fn bat_and_pager_code_read_errors_and_bound_file_sizes() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let bad = fixture.path("bad");
    std::fs::write(&bad, [0xff]).unwrap();
    let large = fixture.path("large");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    for name in ["bat", "bat_view", "less", "pager"] {
        for (path, expected) in [
            (fixture.path("missing"), "E_NOT_FOUND"),
            (bad.clone(), "E_IO"),
            (fixture.path("."), "E_BAD_ARG"),
            (large.clone(), "E_BUDGET_EXCEEDED"),
        ] {
            let mut args = vec![s(&path)];
            if name == "less" || name == "pager" {
                args.push(pager_file(true));
            }
            code(call(name, args, &mut env), expected);
        }
    }
}
fn diff_record(value: Value) -> std::collections::BTreeMap<String, Value> {
    let Value::Record(record) = value else {
        panic!("expected diff record")
    };
    record
}
#[test]
fn file_diff_preserves_positional_changes_counts_and_line_equality() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let a = fixture.path("a");
    let b = fixture.path("b");
    std::fs::write(&a, "a\r\nb\r\n").unwrap();
    std::fs::write(&b, "a\nc\nd").unwrap();
    let mut env = Env::default();
    for name in ["file_diff", "diff_files"] {
        let result = diff_record(call(name, vec![s(&a), s(&b)], &mut env).unwrap());
        assert_eq!(result["file1"], s(&a));
        assert_eq!(result["file2"], s(&b));
        assert_eq!(result["lines1"], Value::Int(2));
        assert_eq!(result["lines2"], Value::Int(3));
        assert_eq!(result["diff_count"], Value::Int(2));
        assert_eq!(result["identical"], Value::Bool(false));
        let Value::Array(rows) = &result["differences"] else {
            panic!("expected differences")
        };
        assert_eq!(
            diff_record(rows[0].clone()),
            std::collections::BTreeMap::from([
                ("line".into(), Value::Int(2)),
                ("type".into(), s("changed")),
                ("old".into(), s("b")),
                ("new".into(), s("c"))
            ])
        );
        assert_eq!(
            diff_record(rows[1].clone()),
            std::collections::BTreeMap::from([
                ("line".into(), Value::Int(3)),
                ("type".into(), s("added")),
                ("new".into(), s("d"))
            ])
        );
    }
    std::fs::write(&b, "a").unwrap();
    let result = diff_record(call("file_diff", vec![s(&a), s(&b)], &mut env).unwrap());
    let Value::Array(rows) = &result["differences"] else {
        panic!("expected differences")
    };
    assert_eq!(
        diff_record(rows[0].clone()),
        std::collections::BTreeMap::from([
            ("line".into(), Value::Int(2)),
            ("type".into(), s("deleted")),
            ("old".into(), s("b"))
        ])
    );
    std::fs::write(&b, "a\nb").unwrap();
    assert_eq!(
        diff_record(call("file_diff", vec![s(&a), s(&b)], &mut env).unwrap())["identical"],
        Value::Bool(true)
    );
    std::fs::write(&a, "").unwrap();
    std::fs::write(&b, "").unwrap();
    let result = diff_record(call("file_diff", vec![s(&a), s(&b)], &mut env).unwrap());
    assert_eq!(result["lines1"], Value::Int(0));
    assert_eq!(result["differences"], Value::Array(vec![]));
}
#[test]
fn file_diff_declares_and_guards_both_actual_read_targets() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("a"), "a").unwrap();
    std::fs::write(workspace.join("b"), "b").unwrap();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "secret").unwrap();
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    std::env::set_var("AETHER_MODE", "agent");
    let mut env = Env::default();
    for name in ["file_diff", "diff_files"] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::ReadLocal
        );
        assert!(aethershell::safety::effect_is_declared(name));
        code(
            call(name, vec![s("a"), s(&outside)], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        code(
            call(name, vec![s(&outside), s("b")], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        assert_eq!(
            diff_record(call(name, vec![s("a"), s("b")], &mut env).unwrap())["diff_count"],
            Value::Int(1)
        );
    }
}
#[test]
fn file_diff_bounds_reads_and_refuses_invalid_arguments() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let good = fixture.path("good");
    std::fs::write(&good, "a").unwrap();
    let bad = fixture.path("bad");
    std::fs::write(&bad, [0xff]).unwrap();
    code(call("file_diff", vec![s(&good), s(&bad)], &mut env), "E_IO");
    code(
        call(
            "file_diff",
            vec![s(&good), s(&fixture.path("missing"))],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    code(
        call("file_diff", vec![s(&good), s(&fixture.path("."))], &mut env),
        "E_BAD_ARG",
    );
    let large = fixture.path("large");
    std::fs::File::create(&large)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call("file_diff", vec![s(&large), s(&good)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call("file_diff", vec![s(&good), Value::Int(1)], &mut env),
        "E_BAD_ARG",
    );
    code(call("file_diff", vec![s(&good)], &mut env), "E_BAD_ARG");
    code(
        call("file_diff", vec![s(&good), s(&good), s(&good)], &mut env),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "file_diff",
            vec![s(&good), s(&good)],
            Some(s(&good)),
            &mut env,
        ),
        "E_BAD_ARG",
    );
}
#[test]
fn file_diff_bounds_difference_records_at_the_exact_limit() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let a = fixture.path("a");
    let b = fixture.path("b");
    std::fs::write(&a, "a\n".repeat(10000)).unwrap();
    std::fs::write(&b, "b\n".repeat(10000)).unwrap();
    let result = diff_record(call("diff_files", vec![s(&a), s(&b)], &mut env).unwrap());
    assert_eq!(result["diff_count"], Value::Int(10000));
    let Value::Array(rows) = &result["differences"] else {
        panic!("expected differences")
    };
    assert_eq!(diff_record(rows[9999].clone())["line"], Value::Int(10000));
    std::fs::write(&a, "a\n".repeat(10001)).unwrap();
    std::fs::write(&b, "b\n".repeat(10001)).unwrap();
    code(
        call("file_diff", vec![s(&a), s(&b)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn sd_uses_native_regex_and_preserves_capture_and_line_semantics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["sd", "sd_replace"] {
        assert_eq!(
            call(name, vec![s("[a-z]+"), s("x"), s("abc 123 def")], &mut env).unwrap(),
            s("x 123 x")
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("(?P<word>[a-z]+)-(\\d+)"), s("${word}:$2:$$")],
                Some(s("abc-12")),
                &mut env
            )
            .unwrap(),
            s("abc:12:$")
        );
        assert_eq!(
            call(name, vec![s("a"), s("x"), s("a\r\na")], &mut env).unwrap(),
            s("x\r\nx")
        );
        assert_eq!(
            call(name, vec![s(""), s("x"), s("é")], &mut env).unwrap(),
            s("xéx")
        );
        assert_eq!(
            call(name, vec![s("missing"), s("x"), s("original")], &mut env).unwrap(),
            s("original")
        );
    }
}
#[test]
fn sd_refuses_invalid_regex_types_and_conflicting_input() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["sd", "sd_replace"] {
        code(
            call(name, vec![s("["), s("x"), s("a")], &mut env),
            "E_BAD_ARG",
        );
        code(call(name, vec![s("a"), s("x")], &mut env), "E_BAD_ARG");
        code(
            call(name, vec![s("a"), s("x"), Value::Int(1)], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input(
                name,
                vec![s("a"), s("x"), s("explicit")],
                Some(s("piped")),
                &mut env,
            ),
            "E_BAD_ARG",
        );
        code(
            call_with_input(name, vec![s("a"), s("x")], Some(Value::Int(1)), &mut env),
            "E_BAD_ARG",
        );
        code(
            call(
                name,
                vec![s("a"), s("x"), s("a"), Value::Record(Default::default())],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
}
#[test]
fn sd_is_declared_pure_and_runs_under_strict_agent_policy() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    std::env::set_var("AETHER_MAX_FILES", "0");
    let mut env = Env::default();
    for name in ["sd", "sd_replace"] {
        assert_eq!(
            aethershell::safety::effect_of(name),
            aethershell::safety::Effect::Pure
        );
        assert!(aethershell::safety::effect_is_declared(name));
        assert_eq!(
            call(name, vec![s("a+"), s("x"), s("aaa")], &mut env).unwrap(),
            s("x")
        );
    }
}
fn sed_options(global: bool, file: bool) -> Value {
    Value::Record(std::collections::BTreeMap::from([
        ("global".into(), Value::Bool(global)),
        ("file".into(), Value::Bool(file)),
    ]))
}
#[test]
fn sed_preserves_capture_expansion_and_whole_string_match_semantics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["sed", "sed_replace"] {
        assert_eq!(
            call(name, vec![s("a"), s("x"), s("a\r\na")], &mut env).unwrap(),
            s("x\r\nx")
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("a"), s("x"), sed_options(false, false)],
                Some(s("a\na")),
                &mut env
            )
            .unwrap(),
            s("x\na")
        );
        assert_eq!(
            call(
                name,
                vec![s("(?P<word>[a-z]+)-(\\d+)"), s("${word}:$2:$$"), s("ab-12")],
                &mut env
            )
            .unwrap(),
            s("ab:12:$")
        );
        assert_eq!(
            call(name, vec![s("missing"), s("x"), s("original")], &mut env).unwrap(),
            s("original")
        );
        assert_eq!(
            call(name, vec![s(""), s("x"), s("é")], &mut env).unwrap(),
            s("xéx")
        );
    }
}
#[test]
fn sed_refuses_bad_options_regex_and_conflicts_without_writing() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let path = fixture.path("original");
    std::fs::write(&path, "original").unwrap();
    let mut env = Env::default();
    for options in [
        Value::Bool(true),
        Value::Record(std::collections::BTreeMap::from([(
            "file".into(),
            s("true"),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "global".into(),
            Value::Int(1),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            Value::Bool(true),
        )])),
    ] {
        code(
            call(
                "sed",
                vec![s("o"), s("x"), s(&path), options.clone()],
                &mut env,
            ),
            "E_BAD_ARG",
        );
        code(
            call_with_input(
                "sed_replace",
                vec![s("o"), s("x"), options],
                Some(s(&path)),
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    code(
        call(
            "sed",
            vec![s("["), s("x"), s(&path), sed_options(true, true)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "sed",
            vec![s("o"), s("x"), s(&path), sed_options(true, true)],
            Some(s("other")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(call("sed", vec![s("o"), s("x")], &mut env), "E_BAD_ARG");
    code(
        call_with_input("sed", vec![s("o"), s("x")], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}
#[test]
fn sed_file_mode_resolves_paths_and_rolls_back_atomic_edits() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    std::fs::write(fixture.root.join("input"), "aba").unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    let result = call(
        "sed",
        vec![s("a"), s("x"), s("input"), sed_options(false, true)],
        &mut env,
    )
    .unwrap();
    assert_eq!(
        result,
        Value::Record(std::collections::BTreeMap::from([
            ("file".into(), s("input")),
            ("status".into(), s("modified"))
        ]))
    );
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("input")).unwrap(),
        "xba"
    );
    call_with_input(
        "sed_replace",
        vec![s("a"), s("y"), sed_options(true, true)],
        Some(s("input")),
        &mut env,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("input")).unwrap(),
        "xby"
    );
    aethershell::tx::rollback().unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.root.join("input")).unwrap(),
        "aba"
    );
    assert!(!std::fs::read_dir(&fixture.root).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".aether_tmp_")));
}
#[test]
fn sed_file_mode_jails_aliases_and_bounds_reads() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let outside = fixture.path("outside");
    std::fs::write(&outside, "original").unwrap();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    let mut env = Env::default();
    for name in ["sed", "sed_replace"] {
        code(
            call(
                name,
                vec![s("o"), s("x"), s(&outside), sed_options(true, true)],
                &mut env,
            ),
            "E_OUTSIDE_WORKSPACE",
        );
    }
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "original");
    std::fs::write(workspace.join("bad"), [0xff]).unwrap();
    code(
        call(
            "sed",
            vec![s("x"), s("y"), s("bad"), sed_options(true, true)],
            &mut env,
        ),
        "E_IO",
    );
    code(
        call(
            "sed",
            vec![s("x"), s("y"), s("missing"), sed_options(true, true)],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    std::fs::File::create(workspace.join("large"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call(
            "sed",
            vec![s("x"), s("y"), s("large"), sed_options(true, true)],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn awk_preserves_fields_whole_lines_and_literal_filter_semantics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["awk", "awk_process"] {
        assert_eq!(
            call(
                name,
                vec![s("{print $2,$1,$2,$9}"), s("a b\r\nc d\r\n")],
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("b a b "), s("d c d ")])
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("/a.b/{print $0, $2}")],
                Some(s("a.b x\naxb y")),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("a.b x x"), s("axb y y")])
        );
        assert_eq!(
            call(name, vec![s("/keep/"), s("keep a\nskip b")], &mut env).unwrap(),
            Value::Array(vec![s("keep a")])
        );
        assert_eq!(
            call(name, vec![s("{print}"), s("  a  b  ")], &mut env).unwrap(),
            Value::Array(vec![s("  a  b  ")])
        );
        assert_eq!(
            call(name, vec![s("{print $2,$1}"), s("a\u{2003}b")], &mut env).unwrap(),
            Value::Array(vec![s(" a\u{2003}b")])
        );
    }
}
#[test]
fn awk_honours_optional_separators_in_direct_and_piped_forms() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    // An empty separator is outside the awk subset and is refused.
    code(
        call(
            "awk",
            vec![
                s("{print $2}"),
                s("ab"),
                Value::Record(std::collections::BTreeMap::from([("fs".into(), s(""))])),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    for separator in [",", "::"] {
        let input = if separator.is_empty() {
            "ab".to_string()
        } else {
            format!("a{separator}b")
        };
        let options = Value::Record(std::collections::BTreeMap::from([(
            "fs".into(),
            s(separator),
        )]));
        let expected = if separator.is_empty() { s("a") } else { s("b") };
        assert_eq!(
            call(
                "awk",
                vec![s("{print $2}"), s(&input), options.clone()],
                &mut env
            )
            .unwrap(),
            Value::Array(vec![expected.clone()])
        );
        assert_eq!(
            call_with_input(
                "awk_process",
                vec![s("{print $2}"), options],
                Some(s(&input)),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![expected])
        );
    }
}
#[test]
fn awk_refuses_unhandled_programs_invalid_options_and_conflicts() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    // Unsupported constructs and malformed programs are all coded bad arguments.
    for program in [
        "",
        "{exit}",
        "{print $1; exit}",
        "printf",
        "{getline}",
        "{x=system(\"ls\")}",
        "{print > \"f\"}",
        "{a[1]=2}",
        "function f(x){return x}",
    ] {
        code(call("awk", vec![s(program), s("a")], &mut env), "E_BAD_ARG");
    }
    for program in [
        "/missing",
        "{print $1",
        "print $1}",
        "{print $-1}",
        "{print $}",
        "{print $18446744073709551616}",
    ] {
        code(call("awk", vec![s(program), s("a")], &mut env), "E_BAD_ARG");
    }
    for options in [
        Value::Bool(true),
        Value::Record(std::collections::BTreeMap::from([(
            "fs".into(),
            Value::Int(1),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            s(","),
        )])),
    ] {
        code(
            call("awk", vec![s("print"), s("a"), options.clone()], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input("awk", vec![s("print"), options], Some(s("a")), &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call_with_input(
            "awk",
            vec![s("print"), s("explicit"), Value::Record(Default::default())],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(call("awk", vec![s("print")], &mut env), "E_BAD_ARG");
    code(
        call_with_input("awk", vec![s("print")], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn awk_bounds_field_reference_expansion() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    // A bare `$1,$1` is a range pattern in awk, which the subset refuses; a
    // field list is a print statement, bounded by the 64 KiB program limit
    // rather than by a count of field references.
    let fields = |n| std::iter::repeat_n("$1", n).collect::<Vec<_>>().join(",");
    let program = format!("{{print {}}}", fields(10000));
    let Value::Array(result) = call("awk", vec![s(&program), s("a")], &mut env).unwrap() else {
        panic!("expected rows")
    };
    assert_eq!(
        result[0],
        s(&std::iter::repeat_n("a", 10000)
            .collect::<Vec<_>>()
            .join(" "))
    );
    let oversized = format!("{{print {}}}", fields(30000));
    code(
        call("awk", vec![s(&oversized), s("a")], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn cut_preserves_field_order_duplicates_and_literal_delimiters() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in ["cut", "cut_columns"] {
        assert_eq!(
            call(
                name,
                vec![s(","), s("3,1,3,9"), s("a,b,c\r\nx,y\r\n")],
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("c,a,c"), s("x")])
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("::"), s(" 2-3 ")],
                Some(s("a::b::c\nx::y")),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("b::c"), s("y")])
        );
        assert_eq!(
            call_with_input(name, vec![s(","), Value::Int(2)], Some(s("a,b")), &mut env).unwrap(),
            Value::Array(vec![s("b")])
        );
        assert_eq!(
            call(name, vec![s(""), s("2-3"), s("ab")], &mut env).unwrap(),
            Value::Array(vec![s("ab")])
        );
        assert_eq!(
            call(name, vec![s(","), s("1"), s("")], &mut env).unwrap(),
            Value::Array(vec![])
        );
    }
}
#[test]
fn cut_refuses_malformed_conflicting_and_oversized_selections() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for spec in [
        "",
        "0",
        "-1",
        "3-1",
        "1-2-3",
        "1,",
        "1--2",
        "+1",
        "1.0",
        "18446744073709551616",
    ] {
        code(
            call("cut", vec![s(","), s(spec), s("a,b")], &mut env),
            "E_BAD_ARG",
        );
    }
    for spec in ["1-10001", "1-10000,1"] {
        code(
            call("cut", vec![s(","), s(spec), s("a")], &mut env),
            "E_BUDGET_EXCEEDED",
        );
    }
    let result = call("cut", vec![s(","), s("1-10000"), s("a")], &mut env).unwrap();
    assert_eq!(result, Value::Array(vec![s("a")]));
    for fields in [
        Value::Int(-1),
        Value::Int(0),
        Value::Float(1.0),
        Value::Bool(true),
    ] {
        code(
            call("cut", vec![s(","), fields, s("a")], &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call_with_input(
            "cut",
            vec![s(","), s("1"), s("explicit")],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input("cut", vec![s(","), s("1")], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
    code(call("cut", vec![s(","), s("1")], &mut env), "E_BAD_ARG");
}
fn options(append: bool) -> Value {
    Value::Record(std::collections::BTreeMap::from([(
        "append".into(),
        Value::Bool(append),
    )]))
}
fn code(result: anyhow::Result<Value>, expected: &str) {
    let e = result.unwrap_err();
    assert!(e.to_string().contains(expected), "{e}");
}
#[test]
fn tail_preserves_both_file_orders_and_pipeline_line_semantics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let path = fixture.path("input");
    std::fs::write(&path, "a\r\nb\r\nc\r\n").unwrap();
    let mut env = Env::default();
    assert_eq!(
        call("tail", vec![s(&path)], &mut env).unwrap(),
        s("a\nb\nc")
    );
    for args in [vec![s(&path), Value::Int(2)], vec![Value::Int(2), s(&path)]] {
        assert_eq!(call("tail", args, &mut env).unwrap(), s("b\nc"));
    }
    assert_eq!(
        call_with_input("tail", vec![Value::Int(0)], Some(s("a\nb")), &mut env).unwrap(),
        s("")
    );
    assert_eq!(
        call_with_input("tail", vec![Value::Int(1)], Some(s("a\n\nb\n")), &mut env).unwrap(),
        s("b")
    );
    assert_eq!(
        call_with_input("tail", vec![], Some(s("")), &mut env).unwrap(),
        s("")
    );
    for count in [Value::Int(-1), Value::Float(1.0), s("bad")] {
        code(
            call_with_input("tail", vec![count.clone()], Some(s("a")), &mut env),
            "E_BAD_ARG",
        );
        code(call("tail", vec![s(&path), count], &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("tail", vec![], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn tail_bounds_reads_and_codes_missing_or_invalid_utf8_files() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    code(
        call("tail", vec![s(&fixture.path("missing"))], &mut env),
        "E_NOT_FOUND",
    );
    let path = fixture.path("bad");
    std::fs::write(&path, [0xff]).unwrap();
    code(call("tail", vec![s(&path)], &mut env), "E_IO");
    let path = fixture.path("large");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(call("tail", vec![s(&path)], &mut env), "E_BUDGET_EXCEEDED");
}
#[test]
fn tee_overwrites_appends_and_formats_data_in_both_forms_and_aliases() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let path = fixture.path("output");
    let mut env = Env::default();
    let rendered = Value::Int(2).to_string();
    assert_eq!(
        call("tee", vec![s(&path), s("a")], &mut env).unwrap(),
        s("a")
    );
    assert_eq!(
        call(
            "tee_output",
            vec![s(&path), Value::Int(2), options(true)],
            &mut env
        )
        .unwrap(),
        s(&rendered)
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("a{rendered}")
    );
    assert_eq!(
        call_with_input("tee", vec![s(&path), options(true)], Some(s("b")), &mut env).unwrap(),
        s("b")
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        format!("a{rendered}b")
    );
    call_with_input("tee", vec![s(&path)], Some(s("new")), &mut env).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert!(!std::fs::read_dir(&fixture.root).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".aether_tmp_")));
}
#[test]
fn malformed_tee_options_are_refused_without_clobbering() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let path = fixture.path("output");
    let mut env = Env::default();
    std::fs::write(&path, "original").unwrap();
    for bad in [
        Value::Bool(true),
        Value::Record(std::collections::BTreeMap::from([(
            "append".into(),
            s("true"),
        )])),
        Value::Record(std::collections::BTreeMap::from([(
            "unknown".into(),
            Value::Bool(true),
        )])),
    ] {
        code(
            call("tee", vec![s(&path), s("bad"), bad.clone()], &mut env),
            "E_BAD_ARG",
        );
        code(
            call_with_input("tee", vec![s(&path), bad], Some(s("bad")), &mut env),
            "E_BAD_ARG",
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    }
    code(
        call_with_input(
            "tee",
            vec![s(&path), s("explicit"), options(false)],
            Some(s("piped")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
}
#[test]
fn tee_jails_new_destinations_and_resolves_relative_paths_to_workspace() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let workspace = fixture.root.join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &workspace);
    let mut env = Env::default();
    for name in ["tee", "tee_output"] {
        code(
            call(name, vec![s(&fixture.path("outside")), s("bad")], &mut env),
            "E_OUTSIDE_WORKSPACE",
        );
        assert!(!fixture.root.join("outside").exists());
    }
    call("tee", vec![s("inside"), s("good")], &mut env).unwrap();
    assert_eq!(
        std::fs::read_to_string(workspace.join("inside")).unwrap(),
        "good"
    );
}
#[test]
fn tee_snapshots_overwrites_appends_and_new_files_for_rollback() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("existing");
    let new = fixture.path("new");
    std::fs::write(&path, "original").unwrap();
    let mut env = Env::default();
    aethershell::tx::begin().unwrap();
    call("tee", vec![s(&path), s("replacement")], &mut env).unwrap();
    call(
        "tee",
        vec![s(&path), s("appended"), options(true)],
        &mut env,
    )
    .unwrap();
    call("tee", vec![s(&new), s("created")], &mut env).unwrap();
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
    assert!(!std::path::Path::new(&new).exists());
}

#[test]
fn head_preserves_both_file_orders_and_pipeline_line_semantics() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let path = fixture.path("input");
    std::fs::write(&path, "a\r\nb\r\nc\r\n").unwrap();
    let mut env = Env::default();
    assert_eq!(
        call("head", vec![s(&path)], &mut env).unwrap(),
        s("a\nb\nc")
    );
    for args in [vec![s(&path), Value::Int(2)], vec![Value::Int(2), s(&path)]] {
        assert_eq!(call("head", args, &mut env).unwrap(), s("a\nb"));
    }
    assert_eq!(
        call_with_input("head", vec![Value::Int(0)], Some(s("a\nb")), &mut env).unwrap(),
        s("")
    );
    assert_eq!(
        call_with_input("head", vec![Value::Int(1)], Some(s("a\n\nb\n")), &mut env).unwrap(),
        s("a")
    );
    assert_eq!(
        call_with_input("head", vec![], Some(s("")), &mut env).unwrap(),
        s("")
    );
    for count in [Value::Int(-1), Value::Float(1.0), s("bad")] {
        code(
            call_with_input("head", vec![count.clone()], Some(s("a")), &mut env),
            "E_BAD_ARG",
        );
        code(call("head", vec![s(&path), count], &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("head", vec![], Some(Value::Int(1)), &mut env),
        "E_BAD_ARG",
    );
}
#[test]
fn head_bounds_reads_and_codes_missing_or_invalid_utf8_files() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    code(
        call("head", vec![s(&fixture.path("missing"))], &mut env),
        "E_NOT_FOUND",
    );
    let path = fixture.path("bad");
    std::fs::write(&path, [0xff]).unwrap();
    code(call("head", vec![s(&path)], &mut env), "E_IO");
    let path = fixture.path("large");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(call("head", vec![s(&path)], &mut env), "E_BUDGET_EXCEEDED");
}

#[test]
fn symlink_refuses_invalid_calls_and_existing_destinations() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let destination = fixture.path("existing");
    std::fs::write(&destination, "unchanged").unwrap();
    for args in [
        vec![],
        vec![s("target")],
        vec![Value::Int(1), s(&destination)],
        vec![s("target"), Value::Bool(true)],
        vec![s("target"), s(&destination), s("ignored")],
        vec![s(""), s(&destination)],
        vec![s("target"), s("")],
        vec![s("target"), s(&destination)],
    ] {
        code(call("fs_symlink", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "fs_symlink",
            vec![s("target"), s(&destination)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "unchanged");
}
#[test]
fn symlink_guards_referred_targets_and_journals_transactions() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let destination = fixture.path("link");
    let outside = fixture.root.parent().unwrap().join("outside-target");
    code(
        call(
            "fs_symlink",
            vec![s(outside.to_str().unwrap()), s(&destination)],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    code(
        call(
            "fs_symlink",
            vec![s("../outside-target"), s(&destination)],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    code(
        call(
            "fs_symlink",
            vec![s(&fixture.path("audit.log")), s(&destination)],
            &mut env,
        ),
        "E_OUTSIDE_WORKSPACE",
    );
    // Inside a transaction the new link is journaled, not refused (commit 1b55800):
    // rollback removes it and leaves the target alone. The target exists so that
    // Windows, which needs a link kind, can resolve it.
    std::fs::write(fixture.path("target"), "kept").unwrap();
    aethershell::tx::begin().unwrap();
    let created = call("fs_symlink", vec![s("target"), s(&destination)], &mut env);
    match &created {
        Ok(value) => assert_eq!(value, &Value::Bool(true)),
        #[cfg(windows)]
        Err(error) => assert!(
            error.to_string().contains("E_POLICY_DENY")
                || error.to_string().contains("os error 1314"),
            "{error}"
        ),
        #[cfg(not(windows))]
        Err(error) => panic!("{error}"),
    }
    aethershell::tx::rollback().unwrap();
    assert!(std::fs::symlink_metadata(&destination).is_err());
    assert_eq!(
        std::fs::read_to_string(fixture.path("target")).unwrap(),
        "kept"
    );
}
#[test]
fn symlink_relative_target_uses_destination_parent() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::fs::create_dir(fixture.root.join("nested")).unwrap();
    let target = fixture.root.join("nested").join("target");
    let destination = fixture.root.join("nested").join("link");
    std::fs::write(&target, "owned").unwrap();
    let result = call(
        "fs_symlink",
        vec![s("target"), s(destination.to_str().unwrap())],
        &mut env,
    );
    #[cfg(windows)]
    if let Err(error) = &result {
        // Windows hosts without symbolic-link privilege must report the OS
        // refusal explicitly, never pretend that creation returned false.
        assert!(
            error.to_string().contains("E_POLICY_DENY")
                || error.to_string().contains("os error 1314"),
            "{error}"
        );
        assert!(std::fs::symlink_metadata(&destination).is_err());
        return;
    }
    assert!(matches!(result.unwrap(), Value::Bool(true)));
    assert_eq!(
        std::fs::read_link(&destination).unwrap(),
        std::path::PathBuf::from("target")
    );
    assert_eq!(std::fs::read_to_string(&destination).unwrap(), "owned");
    std::fs::remove_file(&destination).unwrap();
}

#[cfg(windows)]
#[test]
fn symlink_windows_refuses_ambiguous_and_missing_targets_before_creation() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let destination = fixture.path("link");
    for target in [r"C:target", r"\target"] {
        code(
            call("fs_symlink", vec![s(target), s(&destination)], &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call("fs_symlink", vec![s("missing"), s(&destination)], &mut env),
        "E_NOT_FOUND",
    );
    assert!(std::fs::symlink_metadata(&destination).is_err());
}

#[cfg(unix)]
#[test]
fn symlink_unix_preserves_dangling_targets_and_protects_audit_aliases() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let destination = fixture.path("link");
    assert!(matches!(
        call("fs_symlink", vec![s("missing"), s(&destination)], &mut env).unwrap(),
        Value::Bool(true)
    ));
    assert_eq!(
        std::fs::read_link(&destination).unwrap(),
        std::path::PathBuf::from("missing")
    );
    code(
        call("fs_symlink", vec![s("another"), s(&destination)], &mut env),
        "E_BAD_ARG",
    );
    std::fs::remove_file(&destination).unwrap();
    let audit = fixture.path("audit.log");
    std::fs::write(&audit, "owned").unwrap();
    let alias = fixture.path("alias");
    std::os::unix::fs::symlink(&audit, &alias).unwrap();
    code(
        call("fs_symlink", vec![s(&alias), s(&destination)], &mut env),
        "E_OUTSIDE_WORKSPACE",
    );
    assert!(std::fs::symlink_metadata(&destination).is_err());
    std::fs::remove_file(&alias).unwrap();
}

#[test]
fn permission_checks_validate_calls_and_code_missing_paths() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "unchanged").unwrap();
    for args in [
        vec![],
        vec![Value::Int(1)],
        vec![s(&path), Value::Int(1)],
        vec![s(&path), s("unknown")],
        vec![s(&path), s("")],
        vec![s(&path), s("read"), s("ignored")],
    ] {
        code(call("perm_check", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("perm_check", vec![s(&path)], Some(s("ignored")), &mut env),
        "E_BAD_ARG",
    );
    for mode in ["read", "write", "execute"] {
        code(
            call(
                "perm_check",
                vec![s(&fixture.path("missing")), s(mode)],
                &mut env,
            ),
            "E_NOT_FOUND",
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}

#[test]
fn permission_checks_report_metadata_and_resolve_workspace_paths() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("input");
    std::fs::write(&path, "owned").unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    // Restore host-specific metadata before assertions, even on a failed call.
    let set_readonly = call("perm_set", vec![s("input"), s("0400")], &mut env);
    let readonly = ["r", "read", "w", "write"]
        .map(|mode| call("perm_check", vec![s("input"), s(mode)], &mut env));
    let set_writable = call("perm_set", vec![s("input"), Value::Int(0o600)], &mut env);
    let writable = call("perm_check", vec![s("input"), s("write")], &mut env);
    let default_read = call("perm_check", vec![s("input")], &mut env);
    let execute = call("perm_check", vec![s("input"), s("execute")], &mut env);
    std::fs::set_permissions(&path, original).unwrap();
    assert!(matches!(set_readonly.unwrap(), Value::Bool(true)));
    assert!(matches!(set_writable.unwrap(), Value::Bool(true)));
    for (result, expected) in readonly.into_iter().zip([true, true, false, false]) {
        assert!(matches!(result.unwrap(), Value::Bool(value) if value == expected));
    }
    assert!(matches!(writable.unwrap(), Value::Bool(true)));
    assert!(matches!(default_read.unwrap(), Value::Bool(true)));
    #[cfg(unix)]
    assert!(matches!(execute.unwrap(), Value::Bool(false)));
    // "input" has no PATHEXT extension, so Windows reports it non-executable.
    #[cfg(windows)]
    assert!(matches!(execute.unwrap(), Value::Bool(false)));
    #[cfg(not(any(unix, windows)))]
    code(execute, "E_UNIMPLEMENTED");
}

#[test]
fn permission_set_validates_calls_and_never_discards_pipeline_input() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "owned").unwrap();
    let original = std::fs::metadata(&path).unwrap().permissions();
    for args in [
        vec![],
        vec![s(&path)],
        vec![Value::Int(1), Value::Int(0o600)],
        vec![s(&path), Value::Int(-1)],
        vec![s(&path), Value::Int(0o10000)],
        vec![s(&path), Value::Float(384.0)],
        vec![s(&path), s("0888")],
        vec![s(&path), s("0o")],
        vec![s(&path), s("0644"), s("ignored")],
    ] {
        code(call("perm_set", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "perm_set",
            vec![s(&path), Value::Int(0)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "owned");
}

#[test]
fn permission_set_guards_and_audits_its_own_name_and_journals_transactions() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let path = fixture.path("input");
    std::fs::write(&path, "owned").unwrap();
    code(
        call("perm_set", vec![s("missing"), Value::Int(0o600)], &mut env),
        "E_NOT_FOUND",
    );
    // Inside a transaction the mode change is journaled, not refused (commit 1b55800):
    // rollback restores the pre-image.
    let original = std::fs::metadata(&path).unwrap().permissions();
    aethershell::tx::begin().unwrap();
    assert_eq!(
        call("perm_set", vec![s("input"), Value::Int(0o400)], &mut env).unwrap(),
        Value::Bool(true)
    );
    aethershell::tx::rollback().unwrap();
    assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::env::set_var("AETHER_WORKSPACE", &jail);
    code(
        call("perm_set", vec![s(&path), Value::Int(0o600)], &mut env),
        "E_OUTSIDE_WORKSPACE",
    );
    let audit = std::fs::read_to_string(fixture.path("audit.log")).unwrap();
    assert!(audit.contains("perm_set"));
    assert!(!audit.contains("fs_chmod"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "owned");
}

#[cfg(unix)]
#[test]
fn permission_check_execute_uses_any_unix_execute_bit() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("input");
    std::fs::write(&path, "owned").unwrap();
    for mode in [0o100, 0o010, 0o001] {
        call("perm_set", vec![s(&path), Value::Int(mode)], &mut env).unwrap();
        for name in ["x", "execute"] {
            assert!(matches!(
                call("perm_check", vec![s(&path), s(name)], &mut env).unwrap(),
                Value::Bool(true)
            ));
        }
    }
    call("perm_set", vec![s(&path), Value::Int(0o600)], &mut env).unwrap();
}

#[test]
fn unsupported_acl_operations_refuse_without_mutation() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("owned");
    std::fs::write(&path, "unchanged").unwrap();
    // acl_get and acl_set are implemented on Linux (tests/acl_mount_contracts.rs).
    // Malformed calls are still refused before any ACL is read or changed.
    for name in ["acl_get", "acl_set"] {
        code(call(name, vec![], &mut env), "E_BAD_ARG");
        for args in [
            vec![Value::Int(1)],
            vec![s(&path), s("ignored"), Value::Int(1)],
        ] {
            code(call(name, args, &mut env), "E_BAD_ARG");
        }
        code(
            call_with_input(name, vec![], Some(s(&path)), &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call("acl_set", vec![s(&path), s("ignored")], &mut env),
        "E_BAD_ARG",
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}

#[test]
fn sudo_check_rejects_ignored_input_and_inspects_without_elevating() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    for args in [vec![Value::Int(1)], vec![s("ignored")], vec![Value::Null]] {
        code(call("sudo_check", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("sudo_check", vec![], Some(s("ignored")), &mut env),
        "E_BAD_ARG",
    );
    match call("sudo_check", vec![], &mut env) {
        Ok(Value::Bool(value)) => {
            #[cfg(unix)]
            assert_eq!(value, unsafe { libc::geteuid() } == 0);
            #[cfg(not(unix))]
            let _ = value;
        }
        Err(error) => assert!(
            error.to_string().contains("E_TOOL_MISSING")
                || error.to_string().contains("E_TOOL_FAILED")
                || error.to_string().contains("E_UNIMPLEMENTED"),
            "{error}"
        ),
        other => panic!("unexpected privilege inspection result: {other:?}"),
    }
    assert!(fixture.root.is_dir());
}

#[test]
fn unsupported_filesystem_operations_refuse_without_mutation() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let path = fixture.path("owned");
    std::fs::write(&path, "unchanged").unwrap();
    // fs_mount and fs_unmount are implemented on Linux (tests/acl_mount_contracts.rs).
    // Malformed calls are still refused before any mount or unmount is attempted.
    for (name, cases) in [
        (
            "fs_mount",
            vec![
                vec![],
                vec![s(&path)],
                vec![Value::Int(1)],
                vec![s("relative"), s(&path)],
            ],
        ),
        (
            "fs_unmount",
            vec![
                vec![],
                vec![Value::Int(1)],
                vec![s("relative")],
                vec![s(&path), s("ignored")],
            ],
        ),
    ] {
        for args in cases {
            code(call(name, args, &mut env), "E_BAD_ARG");
        }
        code(
            call_with_input(name, vec![], Some(s(&path)), &mut env),
            "E_BAD_ARG",
        );
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "unchanged");
}

#[test]
fn password_hashes_use_stored_salts_and_bounded_parameters() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let password = "-secret\n\0雪";
    let first = call("crypto_password_hash", vec![s(password)], &mut env).unwrap();
    let second = call("crypto_password_hash", vec![s(password)], &mut env).unwrap();
    assert_ne!(format!("{first:?}"), format!("{second:?}"));
    assert!(matches!(
        call(
            "crypto_password_verify",
            vec![s(password), first.clone()],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        call(
            "crypto_password_verify",
            vec![s("wrong"), first.clone()],
            &mut env
        )
        .unwrap(),
        Value::Bool(false)
    ));
    code(
        call("crypto_password_hash", vec![s("x"), s("extra")], &mut env),
        "E_BAD_ARG",
    );
    code(
        call(
            "crypto_password_verify",
            vec![s("x"), s("garbage")],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "crypto_password_hash",
            vec![s(&"x".repeat(65537))],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    let Value::Str(original) = &first else {
        panic!("hash must be String")
    };
    let excessive = original.replace("m=19456", "m=65537");
    assert_ne!(&excessive, original);
    code(
        call(
            "crypto_password_verify",
            vec![s(password), s(&excessive)],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call(
            "crypto_password_verify",
            vec![s("x"), s("$6$legacy")],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "crypto_password_hash",
            vec![s("x")],
            Some(s("pipe")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "crypto_password_verify",
            vec![s("x"), first.clone()],
            Some(s("pipe")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    use sha2::{Digest, Sha256};
    let legacy = hex::encode(Sha256::digest(password.as_bytes()));
    assert!(matches!(
        call(
            "crypto_password_verify",
            vec![s(password), s(&legacy)],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    ));
    assert!(matches!(
        call(
            "crypto_password_verify",
            vec![s("wrong"), s(&legacy)],
            &mut env
        )
        .unwrap(),
        Value::Bool(false)
    ));
    let excessive_pbkdf2 =
        "$pbkdf2-sha256$i=1200001,l=32$c2FsdHNhbHQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    code(
        call(
            "crypto_password_verify",
            vec![s(password), s(excessive_pbkdf2)],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    std::env::set_var("AETHER_FIPS", "1");
    code(
        call(
            "crypto_password_verify",
            vec![s(password), s(&legacy)],
            &mut env,
        ),
        "E_POLICY_DENY",
    );
    code(
        call("crypto_password_verify", vec![s(password), first], &mut env),
        "E_POLICY_DENY",
    );
    let hash = call("crypto_password_hash", vec![s(password)], &mut env).unwrap();
    let Value::Str(encoded) = &hash else {
        panic!("hash must be String")
    };
    assert!(encoded.starts_with("$pbkdf2-sha256$i=600000,"));
    assert!(matches!(
        call("crypto_password_verify", vec![s(password), hash], &mut env).unwrap(),
        Value::Bool(true)
    ));
}

#[test]
fn crypto_refusals_and_key_generation_reject_discarded_input() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::new();
    for name in ["crypto_sign", "crypto_verify_signature"] {
        code(call(name, vec![], &mut env), "E_BAD_ARG");
        code(call(name, vec![s("ignored")], &mut env), "E_BAD_ARG");
        code(
            call_with_input(name, vec![], Some(s("ignored")), &mut env),
            "E_BAD_ARG",
        );
    }
    for args in [
        vec![Value::Int(1)],
        vec![s("dsa")],
        vec![s("rsa"), s("2048")],
        vec![s("rsa"), Value::Int(2047)],
        vec![s("rsa"), Value::Int(16385)],
        vec![s("ec"), Value::Int(2048)],
        vec![s("ecdsa"), Value::Int(256)],
        vec![s("rsa"), Value::Int(2048), s("extra")],
    ] {
        code(call("crypto_generate_key", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input("crypto_generate_key", vec![], Some(s("ignored")), &mut env),
        "E_BAD_ARG",
    );
}

#[test]
fn certificate_inspection_validates_owned_files_before_platform_or_tool_checks() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::new();
    let path = fixture.path("invalid.pem");
    std::fs::write(&path, "not a certificate").unwrap();
    for args in [vec![], vec![Value::Int(1)], vec![s(&path), s("ignored")]] {
        code(call("crypto_cert_info", args, &mut env), "E_BAD_ARG");
    }
    code(
        call_with_input(
            "crypto_cert_info",
            vec![s(&path)],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call("crypto_cert_info", vec![s("-in")], &mut env),
        "E_BAD_ARG",
    );
    code(
        call(
            "crypto_cert_info",
            vec![s(&fixture.path("absent.pem"))],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    code(
        call(
            "crypto_cert_info",
            vec![s(fixture.root.to_str().unwrap())],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    let oversized = fixture.path("large.pem");
    std::fs::File::create(&oversized)
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();
    code(
        call("crypto_cert_info", vec![s(&oversized)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    let err = call("crypto_cert_info", vec![s(&path)], &mut env)
        .unwrap_err()
        .to_string();
    // openssl refuses the garbage (E_TOOL_FAILED), or is not on PATH.
    assert!(
        err.contains("E_TOOL_FAILED") || err.contains("E_TOOL_MISSING"),
        "{err}"
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a certificate");
}

#[cfg(unix)]
#[test]
fn generated_ec_key_is_checkable_by_openssl_in_an_owned_fixture() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::new();
    let key = match call("crypto_generate_key", vec![s("EC")], &mut env) {
        Ok(Value::Str(key)) => key,
        Err(err) if err.to_string().contains("E_TOOL_MISSING") => return,
        Err(err) => panic!("key generation failed: {err}"),
        Ok(_) => panic!("key generation must return String"),
    };
    let path = fixture.path("test-only-private-key.pem");
    std::fs::write(&path, key).unwrap();
    let output = std::process::Command::new("openssl")
        .args(["pkey", "-in", &path, "-check", "-noout"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "generated key failed OpenSSL validation"
    );
}

#[test]
fn scheduling_contracts_reject_ignored_input_without_scheduling_jobs() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::new();
    for name in ["cron_add", "at_schedule", "at_remove"] {
        code(call(name, vec![], &mut env), "E_BAD_ARG");
        code(call(name, vec![s("")], &mut env), "E_BAD_ARG");
        code(
            call_with_input(name, vec![], Some(s("ignored")), &mut env),
            "E_BAD_ARG",
        );
    }
    for name in ["cron_remove", "cron_enable", "cron_disable"] {
        code(call(name, vec![], &mut env), "E_BAD_ARG");
        code(call(name, vec![s("")], &mut env), "E_BAD_ARG");
        code(
            call_with_input(name, vec![s("job")], Some(s("ignored")), &mut env),
            "E_BAD_ARG",
        );
    }
    for name in ["cron_list", "at_list"] {
        code(call(name, vec![s("ignored")], &mut env), "E_BAD_ARG");
        code(
            call_with_input(name, vec![], Some(s("ignored")), &mut env),
            "E_BAD_ARG",
        );
    }
}

#[test]
fn scheduler_lifecycle_is_gated_before_native_backend_access() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::new();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_POLICY", "strict");
    #[cfg(windows)]
    for name in ["cron_remove", "cron_enable", "cron_disable"] {
        code(call(name, vec![s("*")], &mut env), "E_BAD_ARG");
    }
    #[cfg(unix)]
    for name in ["cron_remove", "cron_enable", "cron_disable"] {
        code(
            call(name, vec![s("not a cron entry")], &mut env),
            "E_BAD_ARG",
        );
    }
    for name in ["cron_remove", "cron_enable", "cron_disable"] {
        code(
            call(
                name,
                vec![s("@daily echo AetherShell-owned-fixture")],
                &mut env,
            ),
            "E_NEEDS_APPROVAL",
        );
    }
    for (name, args) in [
        ("cron_add", vec![s("@daily"), s("echo owned-fixture")]),
        (
            "at_schedule",
            vec![s("2099-01-01T12:00"), s("echo owned-fixture")],
        ),
        (
            "at_remove",
            vec![s(if cfg!(windows) {
                "AetherShell-at-12345678-1234-4234-8234-123456789012"
            } else {
                "12"
            })],
        ),
    ] {
        code(call(name, args, &mut env), "E_NEEDS_APPROVAL");
    }
    for (name, args) in [
        ("cron_add", vec![s("@daily"), s("echo x\necho y")]),
        ("at_schedule", vec![s("2000-01-01T12:00"), s("echo x")]),
        ("at_remove", vec![s("*")]),
    ] {
        code(call(name, args, &mut env), "E_BAD_ARG");
    }
}

#[test]
fn rust_documentation_extracts_real_declarations_without_running_source() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::new();
    let source = r#"// fn fake() {}
        const TEXT: &str = "struct Fake; fn pretend() {}";
        struct Zed<T> { value: T }
        enum Choice { A, B }
        type Alias = Vec<u8>;
        trait Named { fn name(&self); }
        union Bits { integer: u32, float: f32 }
        async fn zeta<T: Clone>(x: T) -> T where T: Send { x }
        fn alpha(x: i64) -> i64 { panic!("must not execute") }
        mod nested { pub fn hidden() {} }
    "#;
    let Value::Array(types) = call("docs_types", vec![s(source)], &mut env).unwrap() else {
        panic!("array")
    };
    assert_eq!(types.len(), 5);
    let Value::Record(first) = &types[0] else {
        panic!("record")
    };
    assert_eq!(first.get("name"), Some(&s("Alias")));
    let Value::Array(functions) = call("docs_signatures", vec![s(source)], &mut env).unwrap()
    else {
        panic!("array")
    };
    assert_eq!(functions.len(), 2);
    let Value::Record(first) = &functions[0] else {
        panic!("record")
    };
    assert_eq!(first.get("name"), Some(&s("alpha")));
    let Value::Record(second) = &functions[1] else {
        panic!("record")
    };
    assert_eq!(second.get("async"), Some(&Value::Bool(true)));
    let Some(Value::Str(signature)) = second.get("signature") else {
        panic!("signature")
    };
    assert!(
        signature.contains("where") && signature.contains("Clone") && signature.contains("Send")
    );
    for name in ["docs_types", "docs_signatures"] {
        for args in [
            vec![],
            vec![Value::Int(1)],
            vec![s("fn broken(")],
            vec![s(source), s("ignored")],
        ] {
            code(call(name, args, &mut env), "E_BAD_ARG");
        }
        code(
            call_with_input(name, vec![s(source)], Some(s("ignored")), &mut env),
            "E_BAD_ARG",
        );
        code(
            call(name, vec![s(&" ".repeat(1024 * 1024 + 1))], &mut env),
            "E_BUDGET_EXCEEDED",
        );
        code(
            call(name, vec![s(&"(".repeat(129))], &mut env),
            "E_BUDGET_EXCEEDED",
        );
        assert!(matches!(call(name,vec![s("")],&mut env).unwrap(),Value::Array(a) if a.is_empty()));
    }
}
#[test]
fn tcp_binary_transfer_eof_timeout_and_handle_lifecycle() {
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener};
    use std::time::Duration;
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut peer, _) = listener.accept().unwrap();
        peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        peer.set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 5];
        peer.read_exact(&mut bytes).unwrap();
        assert_eq!(bytes, [0, 255, 65, 195, 169]);
        peer.write_all(&bytes).unwrap();
        peer.shutdown(Shutdown::Write).unwrap();
    });
    let mut env = Env::default();
    let handle = call(
        "net_connect",
        vec![s("127.0.0.1"), Value::Int(port as i64), Value::Int(500)],
        &mut env,
    )
    .unwrap();
    assert!(matches!(
        call(
            "net_send",
            vec![
                handle.clone(),
                Value::Array(vec![Value::Int(0), Value::Int(255)])
            ],
            &mut env
        )
        .unwrap(),
        Value::Int(2)
    ));
    assert!(matches!(
        call("net_send", vec![handle.clone(), s("Aé")], &mut env).unwrap(),
        Value::Int(3)
    ));
    let mut received = Vec::new();
    loop {
        let Value::Record(result) =
            call("net_recv", vec![handle.clone(), Value::Int(2)], &mut env).unwrap()
        else {
            panic!("record required")
        };
        let Value::Array(bytes) = &result["bytes"] else {
            panic!("bytes required")
        };
        received.extend(bytes.iter().map(|v| {
            if let Value::Int(n) = v {
                *n
            } else {
                panic!("byte Int required")
            }
        }));
        if matches!(result["eof"], Value::Bool(true)) {
            assert!(bytes.is_empty());
            break;
        }
        assert!(!bytes.is_empty());
    }
    assert_eq!(received, vec![0, 255, 65, 195, 169]);
    server.join().unwrap();
    assert!(matches!(
        call("net_close", vec![handle.clone()], &mut env).unwrap(),
        Value::Bool(true)
    ));
    for name in ["net_recv", "net_close"] {
        code(call(name, vec![handle.clone()], &mut env), "E_BAD_STATE");
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let handle = call(
        "net_connect",
        vec![
            s("127.0.0.1"),
            Value::Int(listener.local_addr().unwrap().port() as i64),
            Value::Int(30),
        ],
        &mut env,
    )
    .unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    code(call("net_recv", vec![handle.clone()], &mut env), "E_IO");
    // Timeout leaves the handle usable and does not report EOF.
    peer.write_all(&[42]).unwrap();
    let Value::Record(result) = call("net_recv", vec![handle.clone()], &mut env).unwrap() else {
        panic!()
    };
    assert!(matches!(result["eof"], Value::Bool(false)));
    call("net_close", vec![handle], &mut env).unwrap();
}

#[test]
fn tcp_contracts_policy_and_transactions_gate_actual_peer() {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let mut env = Env::default();
    for args in [
        vec![],
        vec![s("localhost"), Value::Int(80)],
        vec![s("127.0.0.1"), Value::Int(0)],
        vec![s("127.0.0.1"), Value::Int(65536)],
        vec![s("127.0.0.1"), Value::Int(80), Value::Int(10001)],
    ] {
        for name in ["net_connect", "net_listen"] {
            code(call(name, args.clone(), &mut env), "E_BAD_ARG");
        }
    }
    for data in [
        Value::Bool(false),
        Value::Array(vec![Value::Int(-1)]),
        Value::Array(vec![Value::Int(256)]),
        s(&"x".repeat(1048577)),
    ] {
        code(
            call("net_send", vec![s("missing"), data], &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call("net_recv", vec![s("missing"), Value::Int(0)], &mut env),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "net_close",
            vec![s("missing")],
            Some(s("ignored")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let args = vec![
        s("127.0.0.1"),
        Value::Int(listener.local_addr().unwrap().port() as i64),
        Value::Int(50),
    ];
    aethershell::tx::begin().unwrap();
    code(
        call("net_connect", args.clone(), &mut env),
        "E_UNIMPLEMENTED",
    );
    aethershell::tx::rollback().unwrap();
    std::env::set_var("AETHER_NET_ALLOW", "192.0.2.1");
    code(call("net_connect", args.clone(), &mut env), "E_POLICY_DENY");
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let handle = call("net_connect", args, &mut env).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    peer.set_read_timeout(Some(Duration::from_millis(30)))
        .unwrap();
    std::env::set_var("AETHER_NET_ALLOW", "192.0.2.1");
    code(
        call("net_send", vec![handle.clone(), s("denied")], &mut env),
        "E_POLICY_DENY",
    );
    code(
        call("net_recv", vec![handle.clone()], &mut env),
        "E_POLICY_DENY",
    );
    code(
        call("net_close", vec![handle.clone()], &mut env),
        "E_POLICY_DENY",
    );
    let mut byte = [0];
    assert!(peer.read(&mut byte).is_err());
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    aethershell::tx::begin().unwrap();
    code(
        call("net_send", vec![handle.clone(), s("denied")], &mut env),
        "E_UNIMPLEMENTED",
    );
    aethershell::tx::rollback().unwrap();
    peer.write_all(&[99]).unwrap();
    call("net_recv", vec![handle.clone()], &mut env).unwrap();
    call("net_close", vec![handle], &mut env).unwrap();
    let audit = std::fs::read_to_string(fixture.root.join("audit.log")).unwrap();
    assert!(audit.contains("tcp://127.0.0.1:"));
}

#[test]
fn tcp_one_shot_listener_accepts_and_releases_port_on_timeout() {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reservation.local_addr().unwrap();
    drop(reservation);
    let server = std::thread::spawn(move || {
        let mut env = Env::default();
        let handle = call(
            "net_listen",
            vec![
                s("127.0.0.1"),
                Value::Int(addr.port() as i64),
                Value::Int(2000),
            ],
            &mut env,
        )
        .unwrap();
        let Value::Record(result) = call("net_recv", vec![handle.clone()], &mut env).unwrap()
        else {
            panic!()
        };
        assert!(
            matches!(&result["bytes"],Value::Array(bytes) if matches!(bytes.as_slice(),[Value::Int(7)]))
        );
        call(
            "net_send",
            vec![handle.clone(), Value::Array(vec![Value::Int(255)])],
            &mut env,
        )
        .unwrap();
        call("net_close", vec![handle], &mut env).unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut peer = loop {
        match TcpStream::connect_timeout(&addr, Duration::from_millis(50)) {
            Ok(peer) => break peer,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            Err(e) => panic!("listener not available: {e}"),
        }
    };
    peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    std::thread::sleep(Duration::from_millis(30));
    peer.write_all(&[7]).unwrap();
    let mut byte = [0];
    peer.read_exact(&mut byte).unwrap();
    assert_eq!(byte, [255]);
    server.join().unwrap();
    drop(peer);
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reservation.local_addr().unwrap();
    drop(reservation);
    let mut env = Env::default();
    code(
        call(
            "net_listen",
            vec![
                s("127.0.0.1"),
                Value::Int(addr.port() as i64),
                Value::Int(20),
            ],
            &mut env,
        ),
        "E_IO",
    );
    assert!(TcpListener::bind(addr).is_ok());
}
#[test]
fn tcp_handle_budget_releases_capacity_and_close_interrupts_reader() {
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    struct Handles(Vec<Value>);
    impl Drop for Handles {
        fn drop(&mut self) {
            let mut env = Env::default();
            for handle in self.0.drain(..) {
                let _ = call("net_close", vec![handle], &mut env);
            }
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let args = vec![
        s("127.0.0.1"),
        Value::Int(listener.local_addr().unwrap().port() as i64),
        Value::Int(1000),
    ];
    let mut env = Env::default();
    let mut handles = Handles(Vec::new());
    let mut peers = Vec::new();
    for _ in 0..128 {
        handles
            .0
            .push(call("net_connect", args.clone(), &mut env).unwrap());
        peers.push(listener.accept().unwrap().0);
    }
    code(
        call("net_connect", args.clone(), &mut env),
        "E_BUDGET_EXCEEDED",
    );
    let closed = handles.0.pop().unwrap();
    call("net_close", vec![closed], &mut env).unwrap();
    let handle = call("net_connect", args, &mut env).unwrap();
    peers.push(listener.accept().unwrap().0);
    handles.0.push(handle.clone());
    let read_handle = handle.clone();
    let reader =
        std::thread::spawn(move || call("net_recv", vec![read_handle], &mut Env::default()));
    // Observe the stream mutex held by the reader before closing it. Empty
    // sends transmit nothing and return zero until the read owns the stream.
    let ready_deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match call("net_send", vec![handle.clone(), s("")], &mut env) {
            Err(e) if e.to_string().contains("E_BAD_STATE") => break,
            Ok(Value::Int(0)) if Instant::now() < ready_deadline => {
                std::thread::sleep(Duration::from_millis(1))
            }
            other => panic!("reader failed to enter blocking read: {other:?}"),
        }
    }
    let started = Instant::now();
    call("net_close", vec![handle], &mut env).unwrap();
    match reader.join().unwrap() {
        Ok(Value::Record(record)) => assert!(matches!(record["eof"], Value::Bool(true))),
        Err(e) => assert!(
            e.to_string().contains("E_BAD_STATE") || e.to_string().contains("E_IO"),
            "{e}"
        ),
        other => panic!("unexpected read outcome: {other:?}"),
    }
    assert!(started.elapsed() < Duration::from_millis(750));
    handles.0.pop();
    drop(handles);
    drop(peers);
}

#[test]
fn tcp_waits_respect_short_and_expired_evaluation_deadlines() {
    use std::net::TcpListener;
    use std::time::{Duration, Instant};
    fn short_deadline(result: anyhow::Result<Value>) {
        // Scheduling may consume the deadline before I/O starts. Both paths
        // refuse the wait; a timeout after I/O starts is E_IO.
        let error = result.unwrap_err().to_string();
        assert!(
            error.contains("E_IO") || error.contains("E_BUDGET_EXCEEDED"),
            "{error}"
        );
    }
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_NET_ALLOW", "127.0.0.1");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut env = Env::default();
    let args = vec![
        s("127.0.0.1"),
        Value::Int(listener.local_addr().unwrap().port() as i64),
        Value::Int(5000),
    ];
    let handle = call("net_connect", args.clone(), &mut env).unwrap();
    let _peer = listener.accept().unwrap().0;
    {
        let _deadline = aethershell::safety::enter_deadline(Duration::from_millis(20));
        let started = Instant::now();
        short_deadline(call("net_recv", vec![handle.clone()], &mut env));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
    {
        let _deadline = aethershell::safety::enter_deadline(Duration::ZERO);
        code(
            call("net_recv", vec![handle.clone()], &mut env),
            "E_BUDGET_EXCEEDED",
        );
        code(
            call("net_connect", args.clone(), &mut env),
            "E_BUDGET_EXCEEDED",
        );
        // This address is already bound; deadline refusal must precede bind.
        code(call("net_listen", args, &mut env), "E_BUDGET_EXCEEDED");
    }
    call("net_close", vec![handle], &mut env).unwrap();
    let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = reservation.local_addr().unwrap().port();
    drop(reservation);
    let _deadline = aethershell::safety::enter_deadline(Duration::from_millis(20));
    let started = Instant::now();
    short_deadline(call(
        "net_listen",
        vec![s("127.0.0.1"), Value::Int(port as i64), Value::Int(5000)],
        &mut env,
    ));
    assert!(started.elapsed() < Duration::from_secs(1));
}
#[test]
fn source_navigation_inspects_real_bindings_references_and_signatures_without_effects() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &fixture.root);
    let mut env = Env::default();
    let source="# side in a comment\nside = fn(x) => file.write(\"must-not-exist\", x); other = fn(y) => side(y); literal = \"side\"; result = other(1)";
    let Value::Array(definitions) =
        call("code_definition", vec![s(source), s("side")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(definitions.len(), 1);
    let Value::Record(definition) = &definitions[0] else {
        panic!()
    };
    assert_eq!(definition["kind"], s("lambda"));
    assert_eq!(definition["params"], Value::Array(vec![s("x")]));
    assert!(matches!(&definition["value_ast"],Value::Record(ast) if ast.contains_key("Lambda")));
    let Value::Array(references) =
        call("code_references", vec![s(source), s("side")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(references.len(), 2); // The comment and String do not create references.
    let Value::Array(callers) = call("code_callers", vec![s(source), s("side")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(callers, vec![s("other")]);
    let Value::Record(callees) =
        call("code_callees", vec![s(source), s("side")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(callees["callees"], Value::Array(vec![s("file.write")]));
    assert_eq!(callees["dynamic_calls"], Value::Int(0));
    assert_eq!(callees["found"], Value::Bool(true));
    let Value::Record(signature) =
        call("code_signature", vec![s(source), s("side")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        signature["signatures"],
        Value::Array(vec![s("side(x) -> Any")])
    );
    assert_eq!(signature["origin"], s("source"));
    let Value::Record(builtin) = call("code_signature", vec![s(""), s("round")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(builtin["origin"], s("builtin"));
    assert_eq!(builtin["found"], Value::Bool(true));
    let Value::Record(module_sig) =
        call("code_signature", vec![s(""), s("math.abs")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(module_sig["found"], Value::Bool(true));
    let Value::Record(shadow) = call(
        "code_signature",
        vec![s("round = 42"), s("round")],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(shadow["origin"], s("source"));
    assert_eq!(shadow["found"], Value::Bool(false));
    assert!(!fixture.root.join("must-not-exist").exists());
}

#[test]
fn source_navigation_counts_dynamic_calls_pipelines_unicode_positions_and_completions() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let source = "f = fn(x) => [abs(x), len(x), (fn(y) => y)(x), x | len]";
    let Value::Record(callees) = call("code_callees", vec![s(source), s("f")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(callees["callees"], Value::Array(vec![s("abs"), s("len")]));
    assert_eq!(callees["dynamic_calls"], Value::Int(1));
    let Value::Array(callers) = call(
        "code_callers",
        vec![s("f = fn(x) => abs(x); result = abs(1)"), s("abs")],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(callers, vec![s("<top-level>"), s("f")]);
    let source = "v = \"α\"; result = v";
    let Value::Array(refs) = call("code_references", vec![s(source), s("v")], &mut env).unwrap()
    else {
        panic!()
    };
    let Value::Record(last) = &refs[1] else {
        panic!()
    };
    assert_eq!(last["column"], Value::Int(19));
    assert_eq!(last["end_column"], Value::Int(20));
    let Value::Record(hover) = call(
        "code_hover",
        vec![s(source), Value::Int(1), Value::Int(19)],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(hover["name"], s("v"));
    assert_eq!(hover["found"], Value::Bool(true));
    let Value::Record(hover) = call(
        "code_hover",
        vec![s("# α"), Value::Int(1), Value::Int(3)],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(hover["found"], Value::Bool(false));
    let Value::Array(refs) = call(
        "code_references",
        vec![s("math.abs(1)"), s("math.abs")],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(refs.len(), 1);
    let Value::Record(reference) = &refs[0] else {
        panic!()
    };
    assert_eq!(reference["name"], s("abs"));
    let Value::Array(fields) = call(
        "code_references",
        vec![s("r = {from: 42}; r.from"), s("from")],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(fields.len(), 2);
    let Value::Array(keywords) =
        call("code_references", vec![s("x = true"), s("true")], &mut env).unwrap()
    else {
        panic!()
    };
    assert!(keywords.is_empty());

    let Value::Array(completions) = call(
        "code_completions",
        vec![s("map = 42; map_extra = 1"), s("map")],
        &mut env,
    )
    .unwrap() else {
        panic!()
    };
    let Value::Record(first) = &completions[0] else {
        panic!()
    };
    assert_eq!(first["name"], s("map"));
    assert_eq!(first["kind"], s("variable"));
    assert!(completions
        .iter()
        .any(|v| matches!(v,Value::Record(r) if r["name"]==s("map_extra"))));
    let Value::Array(missing) =
        call("code_definition", vec![s(source), s("missing")], &mut env).unwrap()
    else {
        panic!()
    };
    assert!(missing.is_empty());
    let Value::Record(missing) =
        call("code_callees", vec![s(source), s("missing")], &mut env).unwrap()
    else {
        panic!()
    };
    assert_eq!(missing["found"], Value::Bool(false));
}

#[test]
fn source_navigation_refuses_invalid_shapes_syntax_and_complexity() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for name in [
        "code_references",
        "code_definition",
        "code_callers",
        "code_callees",
        "code_signature",
        "code_completions",
        "code_hover",
    ] {
        code(call(name, vec![], &mut env), "E_BAD_ARG");
        let args = if name == "code_hover" {
            vec![s("x ="), Value::Int(1), Value::Int(1)]
        } else if name == "code_completions" {
            vec![s("x =")]
        } else {
            vec![s("x ="), s("x")]
        };
        code(call(name, args.clone(), &mut env), "E_BAD_ARG");
        code(
            call_with_input(name, args, Some(s("discarded")), &mut env),
            "E_BAD_ARG",
        );
    }
    code(
        call(
            "code_hover",
            vec![s("x = 1"), Value::Int(2), Value::Int(1)],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call("code_definition", vec![s("x = 1"), s("x; rm")], &mut env),
        "E_BAD_ARG",
    );
    code(
        call(
            "code_completions",
            vec![s(""), s(&"x".repeat(257))],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "code_references",
            vec![s(&format!("#{}", "x".repeat(65536))), s("x")],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call(
            "code_references",
            vec![s(&format!("#{}", ".".repeat(257))), s("x")],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call(
            "code_references",
            vec![s(&"x ".repeat(8193)), s("x")],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
}

#[test]
fn project_scripts_reads_json_jsonc_python_and_cargo_without_running_commands() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    let cases = [
        (
            "package.json",
            r#"{"scripts":{"z":"echo z","a":"rm must-not-exist"}}"#,
            vec!["npm_script", "npm_script"],
        ),
        (
            "deno.jsonc",
            r#"{
          // task descriptions are data
          "tasks": {"a": {"description":"hi","command":"echo https://host/*literal*/,"}, "z":"echo z",},
        }"#,
            vec!["deno_task", "deno_task"],
        ),
        (
            "pyproject.toml",
            "[project.scripts]\nz = 'module:main'\n[project.gui-scripts]\na = 'module:gui'",
            vec!["python_gui_script", "python_console_script"],
        ),
        (
            "config.toml",
            "[alias]\nz = ['test', '--workspace']\na = 'build'",
            vec!["cargo_alias", "cargo_alias"],
        ),
    ];
    for (file, source, kinds) in cases {
        let path = fixture.path(file);
        std::fs::write(&path, source).unwrap();
        let Value::Array(entries) = call("project_scripts", vec![s(&path)], &mut env).unwrap()
        else {
            panic!()
        };
        assert_eq!(entries.len(), 2);
        for (i, kind) in kinds.iter().enumerate() {
            let Value::Record(entry) = &entries[i] else {
                panic!()
            };
            assert_eq!(entry["kind"], s(kind));
            assert_eq!(entry["name"], s(if i == 0 { "a" } else { "z" }));
            assert_eq!(entry["manifest"], s(&path));
        }
        if file == "deno.jsonc" {
            let Value::Record(entry) = &entries[0] else {
                panic!()
            };
            assert_eq!(entry["command"], s("echo https://host/*literal*/,"));
        }
        if file == "config.toml" {
            let Value::Record(entry) = &entries[1] else {
                panic!()
            };
            assert_eq!(
                entry["command"],
                Value::Array(vec![s("test"), s("--workspace")])
            );
        }
    }
    let path = fixture.path("grouped.json");
    std::fs::write(&path,r#"{"tasks":{"build":"echo build","all":{"dependencies":["build"],"description":"Grouped"}}}"#).unwrap();
    let Value::Array(entries) = call("project_scripts", vec![s(&path)], &mut env).unwrap() else {
        panic!()
    };
    let Value::Record(grouped) = &entries[0] else {
        panic!()
    };
    assert_eq!(grouped["name"], s("all"));
    assert_eq!(grouped["command"], Value::Null);
    let Value::Record(definition) = &grouped["definition"] else {
        panic!()
    };
    assert_eq!(definition["dependencies"], Value::Array(vec![s("build")]));
    assert_eq!(definition["description"], s("Grouped"));
    assert!(!fixture.root.join("must-not-exist").exists());
    assert!(std::fs::read_to_string(fixture.root.join("audit.log"))
        .unwrap()
        .contains("project_scripts"));
    assert_eq!(
        aethershell::safety::effect_of("project_scripts"),
        aethershell::safety::Effect::ReadLocal
    );
}

#[test]
fn project_scripts_refuses_malformed_missing_and_oversized_manifests_without_partial_results() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let fixture = Fixture::new();
    let mut env = Env::default();
    code(call("project_scripts", vec![], &mut env), "E_BAD_ARG");
    code(
        call("project_scripts", vec![s("file.yaml")], &mut env),
        "E_BAD_ARG",
    );
    code(
        call_with_input(
            "project_scripts",
            vec![s("file.json")],
            Some(s("discarded")),
            &mut env,
        ),
        "E_BAD_ARG",
    );
    code(
        call(
            "project_scripts",
            vec![s(&fixture.path("missing.json"))],
            &mut env,
        ),
        "E_NOT_FOUND",
    );
    for source in [
        r#"{"scripts":{"valid":"echo ok","bad":42}}"#,
        r#"{"tasks":[]}"#,
        r#"{"tasks":{"bad":{"description":"no command"}}}"#,
        r#"{"scripts":{,}}"#,
        r#"{"scripts":{"a":"ok"}/*"#,
    ] {
        let path = fixture.path("invalid.jsonc");
        std::fs::write(&path, source).unwrap();
        code(
            call("project_scripts", vec![s(&path)], &mut env),
            "E_BAD_STATE",
        );
    }
    let path = fixture.path("invalid.toml");
    std::fs::write(&path, "[alias]\na = [1]").unwrap();
    code(
        call("project_scripts", vec![s(&path)], &mut env),
        "E_BAD_STATE",
    );
    let path = fixture.path("large.json");
    std::fs::write(&path, " ".repeat(1048577)).unwrap();
    code(
        call("project_scripts", vec![s(&path)], &mut env),
        "E_BUDGET_EXCEEDED",
    );
    let path = fixture.path("empty.json");
    std::fs::write(&path, "{}").unwrap();
    assert_eq!(
        call("project_scripts", vec![s(&path)], &mut env).unwrap(),
        Value::Array(vec![])
    );
    let jail = fixture.root.join("jail");
    std::fs::create_dir(&jail).unwrap();
    std::env::set_var("AETHER_MODE", "agent");
    std::env::set_var("AETHER_WORKSPACE", &jail);
    code(
        call("project_scripts", vec![s(&path)], &mut env),
        "E_OUTSIDE_WORKSPACE",
    );
}

fn ed_fixture() -> (Value, Value, Value) {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let seed = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    let public = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    let signature = concat!(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e06522490155",
        "5fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b"
    );
    let der = hex::decode(format!(
        "3051020101300506032b657004220420{seed}812100{public}"
    ))
    .unwrap();
    (
        s(&format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            STANDARD.encode(der)
        )),
        s(&STANDARD.encode(hex::decode(public).unwrap())),
        s(&STANDARD.encode(hex::decode(signature).unwrap())),
    )
}
#[test]
fn ed25519_matches_rfc8032_vector_and_rejects_tampering() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let (private, public, signature) = ed_fixture();
    for message in [s(""), Value::Array(vec![])] {
        let signed = call(
            "crypto_sign",
            vec![private.clone(), message.clone()],
            &mut env,
        )
        .unwrap();
        let Value::Record(fields) = signed else {
            panic!("expected signature record")
        };
        assert_eq!(fields["algorithm"], s("ed25519"));
        assert_eq!(fields["public_key"], public);
        assert_eq!(fields["signature"], signature);
        assert_eq!(
            call(
                "crypto_verify_signature",
                vec![public.clone(), message, signature.clone()],
                &mut env
            )
            .unwrap(),
            Value::Bool(true)
        );
    }
    assert_eq!(
        call(
            "crypto_verify_signature",
            vec![public.clone(), s("changed"), signature.clone()],
            &mut env
        )
        .unwrap(),
        Value::Bool(false)
    );
    use base64::{engine::general_purpose::STANDARD, Engine};
    let Value::Str(encoded) = signature else {
        unreachable!()
    };
    let mut bytes = STANDARD.decode(encoded).unwrap();
    bytes[0] ^= 1;
    assert_eq!(
        call(
            "crypto_verify_signature",
            vec![public, s(""), s(&STANDARD.encode(bytes))],
            &mut env
        )
        .unwrap(),
        Value::Bool(false)
    );
}
#[test]
fn ed25519_generated_keys_sign_binary_and_language_examples() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let key = call("crypto_generate_key", vec![s("ED25519")], &mut env).unwrap();
    let next = call("crypto_generate_key", vec![s("ed25519")], &mut env).unwrap();
    assert_ne!(key, next);
    let message = Value::Array(vec![Value::Int(0), Value::Int(255), Value::Int(128)]);
    let signed = call("crypto_sign", vec![key.clone(), message.clone()], &mut env).unwrap();
    assert_eq!(
        signed,
        call("crypto_sign", vec![key, message.clone()], &mut env).unwrap()
    );
    let Value::Record(fields) = signed else {
        unreachable!()
    };
    assert_eq!(
        call(
            "crypto_verify_signature",
            vec![
                fields["public_key"].clone(),
                message,
                fields["signature"].clone()
            ],
            &mut env
        )
        .unwrap(),
        Value::Bool(true)
    );
    for source in ["typeof(crypto_sign(crypto_generate_key(\"ed25519\"), \"example\"))", "key = crypto_generate_key(\"ed25519\"); signed = crypto_sign(key, \"example\"); crypto_verify_signature(signed.public_key, \"example\", signed.signature)"] {
        let statements = aethershell::parser::parse_program(source).unwrap();
        let result = aethershell::eval::eval_program(&statements, &mut Env::default()).unwrap();
        assert!(result == s("Record") || result == Value::Bool(true));
    }
}
#[test]
fn ed25519_rejects_bad_encodings_arguments_and_inconsistent_keys() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let (private, public, signature) = ed_fixture();
    for invalid in [
        s("secret-invalid-key"),
        Value::Null,
        s("-----BEGIN PRIVATE KEY-----\nAA==\n-----END PRIVATE KEY-----"),
        s("x.pem"),
    ] {
        let error = call("crypto_sign", vec![invalid, s("")], &mut env)
            .unwrap_err()
            .to_string();
        assert!(error.contains("E_BAD_ARG"));
        assert!(!error.contains("secret-invalid-key"));
    }
    use base64::{engine::general_purpose::STANDARD, Engine};
    let Value::Str(pem) = &private else {
        unreachable!()
    };
    let mut der = STANDARD.decode(pem.lines().nth(1).unwrap()).unwrap();
    *der.last_mut().unwrap() ^= 1;
    code(
        call(
            "crypto_sign",
            vec![
                s(&format!(
                    "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
                    STANDARD.encode(der)
                )),
                s(""),
            ],
            &mut env,
        ),
        "E_BAD_ARG",
    );
    for invalid in [Value::Int(-1), Value::Int(256), Value::Float(1.0), s("1")] {
        code(
            call(
                "crypto_sign",
                vec![private.clone(), Value::Array(vec![invalid])],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    for invalid in [s(""), s("AA=="), Value::Null, s(&"A".repeat(129))] {
        code(
            call(
                "crypto_verify_signature",
                vec![public.clone(), s(""), invalid],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    let Value::Str(pk) = &public else {
        unreachable!()
    };
    for invalid in [s(pk.trim_end_matches('=')), s(&format!("{pk}\n"))] {
        code(
            call(
                "crypto_verify_signature",
                vec![invalid, s(""), signature.clone()],
                &mut env,
            ),
            "E_BAD_ARG",
        );
    }
    for (name, args) in [
        ("crypto_sign", vec![private.clone(), s("")]),
        ("crypto_verify_signature", vec![public, s(""), signature]),
        ("crypto_generate_key", vec![s("ed25519")]),
    ] {
        code(
            call_with_input(name, args.clone(), Some(Value::Null), &mut env),
            "E_BAD_ARG",
        );
        let mut surplus = args;
        surplus.push(Value::Null);
        code(call(name, surplus, &mut env), "E_BAD_ARG");
    }
    code(
        call(
            "crypto_sign",
            vec![private, s(&"x".repeat(1048577))],
            &mut env,
        ),
        "E_BUDGET_EXCEEDED",
    );
    code(
        call("crypto_sign", vec![s(&"x".repeat(4097)), s("")], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn ed25519_fips_policy_refuses_unvalidated_backend() {
    let _lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _fixture = Fixture::new();
    std::env::set_var("AETHER_FIPS", "1");
    let mut env = Env::default();
    let (private, public, signature) = ed_fixture();
    for (name, args) in [
        ("crypto_generate_key", vec![s("ed25519")]),
        ("crypto_sign", vec![private, s("")]),
        ("crypto_verify_signature", vec![public, s(""), signature]),
    ] {
        code(call(name, args, &mut env), "E_POLICY_DENY");
    }
}
