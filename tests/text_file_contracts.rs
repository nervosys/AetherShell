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
#[test]
fn chmod_validates_modes_and_preserves_platform_permission_semantics() {
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
fn chmod_codes_missing_paths_guards_aliases_and_refuses_untracked_transaction_changes() {
    let _lock = LOCK.lock().unwrap();
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
    aethershell::tx::begin().unwrap();
    for name in ["fs_chmod", "chmod"] {
        code(
            call(name, vec![s("input"), Value::Int(0)], &mut env),
            "E_UNIMPLEMENTED",
        );
        assert_eq!(std::fs::metadata(&path).unwrap().permissions(), original);
    }
    aethershell::tx::rollback().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
            Value::Array(vec![s("b\ta\tb"), s("d\tc\td")])
        );
        assert_eq!(
            call_with_input(
                name,
                vec![s("/a.b/{print $0, $2}")],
                Some(s("a.b x\naxb y")),
                &mut env
            )
            .unwrap(),
            Value::Array(vec![s("a.b x\tx")])
        );
        assert_eq!(
            call(name, vec![s("/keep/"), s("keep a\nskip b")], &mut env).unwrap(),
            Value::Array(vec![s("keep a")])
        );
        assert_eq!(
            call(name, vec![s("print"), s("  a  b  ")], &mut env).unwrap(),
            Value::Array(vec![s("  a  b  ")])
        );
        assert_eq!(
            call(name, vec![s("$2,$1"), s("a\u{2003}b")], &mut env).unwrap(),
            Value::Array(vec![s("b\ta")])
        );
    }
}
#[test]
fn awk_honours_optional_separators_in_direct_and_piped_forms() {
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for separator in [",", "::", ""] {
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
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    for program in [
        "",
        "{exit}",
        "{print 1}",
        "{print $1; exit}",
        "printf",
        "/a/{}",
    ] {
        // A malformed field token is a bad argument; unsupported actions are explicit refusals.
        let expected = if program.contains("$1;") {
            "E_BAD_ARG"
        } else {
            "E_UNIMPLEMENTED"
        };
        code(call("awk", vec![s(program), s("a")], &mut env), expected);
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
    let _lock = LOCK.lock().unwrap();
    let _fixture = Fixture::new();
    let mut env = Env::default();
    let program = std::iter::repeat_n("$1", 10000)
        .collect::<Vec<_>>()
        .join(",");
    let Value::Array(result) = call("awk", vec![s(&program), s("a")], &mut env).unwrap() else {
        panic!("expected rows")
    };
    assert_eq!(
        result[0],
        s(&std::iter::repeat_n("a", 10000)
            .collect::<Vec<_>>()
            .join("\t"))
    );
    code(
        call("awk", vec![s(&(program + ",$1")), s("a")], &mut env),
        "E_BUDGET_EXCEEDED",
    );
}
#[test]
fn cut_preserves_field_order_duplicates_and_literal_delimiters() {
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
    let _lock = LOCK.lock().unwrap();
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
