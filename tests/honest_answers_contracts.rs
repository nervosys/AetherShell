//! Builtins that used to answer without doing the work.
//!
//! cluster_create answered a record and stored nothing; cloud_instances read a
//! store nothing wrote and cloud_regions returned a fixed list no provider
//! supplied; ai_routes returned registry totals instead of its rules; noop was a
//! placeholder; capabilities had no Windows answer; archive_test covers
//! bzip2, xz and zstd through their own tools.
//!
//! Every test here runs on every platform, except where a Windows-only or a
//! tool-present check says so. The tools are never run through a shell, every
//! archive is written under the system temp dir, and each test removes its own
//! files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use aethershell::value::Value;

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call_with_input(name, args, Some(input), &mut env)
}

fn code_of(e: &anyhow::Error) -> String {
    let s = e.to_string();
    match s.find("E_") {
        Some(i) => s[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {s}"),
    }
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(r) => r
            .get(key)
            .unwrap_or_else(|| panic!("no field {key:?} in {v:?}")),
        other => panic!("expected a Record, got {other:?}"),
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A directory under the system temp dir, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ae_honest_answers_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---------------------------------------------------------------- cluster

/// One test owns every cluster_create call in this file: the registry is
/// process-wide, and the bound below would be order-dependent if another test
/// stored clusters too.
#[test]
fn cluster_create_stores_the_cluster_and_refuses_what_it_cannot_hold() {
    // Refusals come before the registry is touched.
    for args in [
        vec![Value::Int(5)],
        vec![s("")],
        vec![s(&"x".repeat(129))],
        vec![s("a"), s("b")],
    ] {
        assert_eq!(
            code_of(&call("cluster_create", args).expect_err("refused")),
            "E_BAD_ARG"
        );
    }

    let pid = std::process::id();
    let name = format!("honest-{pid}-alpha");
    let r = call("cluster_create", vec![s(&name)]).expect("a valid name is stored");
    assert_eq!(field(&r, "name"), &s(&name));
    assert_eq!(field(&r, "status"), &s("created"));
    let Value::Record(rec) = &r else {
        panic!("not a record")
    };
    assert!(
        !rec.contains_key("node_count"),
        "a record claiming a node count nothing tracks"
    );

    assert_eq!(
        code_of(&call("cluster_create", vec![s(&name)]).expect_err("a duplicate")),
        "E_BAD_ARG"
    );

    let status = call("cluster_status", vec![]).expect("status");
    let Value::Array(names) = field(&status, "cluster_names") else {
        panic!("cluster_names is not an Array")
    };
    assert!(
        names.contains(&s(&name)),
        "cluster_status does not see {name}"
    );
    assert!(matches!(field(&status, "clusters"), Value::Int(n) if *n >= 1));

    // Fill to the ceiling. Some clusters may already be stored, so the loop
    // stops at the first refusal and asserts it is the budget one.
    let mut refused = false;
    for i in 0..=64 {
        match call("cluster_create", vec![s(&format!("honest-{pid}-fill-{i}"))]) {
            Ok(_) => {}
            Err(e) => {
                assert_eq!(code_of(&e), "E_BUDGET_EXCEEDED", "{e}");
                refused = true;
                break;
            }
        }
    }
    assert!(refused, "65 clusters were stored; the ceiling is 64");
    let status = call("cluster_status", vec![]).expect("status");
    assert_eq!(field(&status, "clusters"), &Value::Int(64));
}

/// A node added by cluster_add_node is registered, and the status reported
/// for it is the one stored: cluster_status must not count it as online.
#[test]
fn a_registered_node_is_not_counted_as_online() {
    let id = format!("honest-node-{}", std::process::id());
    let before = call("cluster_status", vec![]).expect("status");
    let online_before = field(&before, "online_nodes").clone();
    let registered_before = field(&before, "registered_nodes").clone();

    let r = call("cluster_add_node", vec![s(&id), s("10.9.8.7:8080")]).expect("add");
    assert_eq!(field(&r, "status"), &s("registered"));

    let after = call("cluster_status", vec![]).expect("status");
    assert_eq!(field(&after, "online_nodes"), &online_before);
    let (Value::Int(rb), Value::Int(ra)) = (&registered_before, field(&after, "registered_nodes"))
    else {
        panic!("registered_nodes is not an Int")
    };
    assert_eq!(*ra, rb + 1, "the new node is counted as registered");

    let Value::Array(nodes) = call("cluster_nodes", vec![]).expect("nodes") else {
        panic!("cluster_nodes is not an Array")
    };
    let mine = nodes
        .iter()
        .find(|n| field(n, "id") == &s(&id))
        .expect("the node is listed");
    assert_eq!(field(mine, "status"), &s("registered"));
    assert!(call("cluster_remove_node", vec![s(&id)]).is_ok());
}

// ------------------------------------------------------------------ cloud

#[test]
fn cloud_instances_and_regions_answer_not_implemented_not_a_guess() {
    for name in ["cloud_instances", "cloud_regions"] {
        assert_eq!(
            code_of(&call(name, vec![]).expect_err("no provider is wired in")),
            "E_UNIMPLEMENTED",
            "{name}"
        );
        assert_eq!(
            code_of(&call(name, vec![s("x")]).expect_err("an argument")),
            "E_BAD_ARG",
            "{name}"
        );
        assert_eq!(
            code_of(&piped(name, vec![], s("x")).expect_err("pipeline input")),
            "E_BAD_ARG",
            "{name}"
        );
    }
}

// --------------------------------------------------------------- ai_routes

/// One test owns the routing rules this binary adds, for the same reason as
/// the cluster test.
#[test]
fn ai_routes_lists_the_rules_the_registry_holds() {
    let name = format!("honest_route_{}", std::process::id());
    let mut rule = BTreeMap::new();
    rule.insert("name".to_string(), s(&name));
    rule.insert("priority".to_string(), Value::Int(77));
    rule.insert("condition".to_string(), s("has_tools"));
    rule.insert("target".to_string(), s("ollama"));
    rule.insert("allow_fallback".to_string(), Value::Bool(false));
    call("ai_add_route", vec![Value::Record(rule)]).expect("the rule is added");

    let routes = call("ai_routes", vec![]).expect("ai_routes answers");
    let Value::Array(rules) = field(&routes, "rules") else {
        panic!("rules is not an Array")
    };
    assert_eq!(
        field(&routes, "rule_count"),
        &Value::Int(rules.len() as i64)
    );

    let mine = rules
        .iter()
        .find(|r| field(r, "name") == &s(&name))
        .expect("the added rule is listed");
    assert_eq!(field(mine, "priority"), &Value::Int(77));
    assert_eq!(field(mine, "condition"), &s("has_tools"));
    assert_eq!(field(mine, "target"), &s("ollama"));
    assert_eq!(field(mine, "allow_fallback"), &Value::Bool(false));

    let Value::Record(rec) = &routes else {
        panic!("not a record")
    };
    assert!(
        !rec.contains_key("note"),
        "a note about an empty rule list, with a rule listed"
    );
}

// ------------------------------------------------------------------- noop

#[test]
fn noop_is_not_a_builtin() {
    assert!(!aethershell::builtins::is_dispatched("noop"));
    assert!(call("noop", vec![]).is_err());
}

// ----------------------------------------------------------- capabilities

#[cfg(windows)]
#[test]
fn capabilities_on_windows_lists_this_process_privileges() {
    let r = call("capabilities", vec![]).expect("whoami /priv runs on Windows");
    let rows = match field(&r, "privileges") {
        Value::Array(rows) => rows,
        other => panic!("privileges is not an Array: {other:?}"),
    };
    assert!(!rows.is_empty(), "whoami /priv listed no privileges");
    for row in rows {
        let Value::Str(name) = field(row, "name") else {
            panic!("name is not a String")
        };
        assert!(name.starts_with("Se"), "{name}");
        assert!(matches!(field(row, "state"), Value::Str(s) if !s.is_empty()));
        assert!(matches!(field(row, "description"), Value::Str(_)));
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
#[test]
fn capabilities_elsewhere_is_unimplemented() {
    assert_eq!(
        code_of(&call("capabilities", vec![]).expect_err("no capability sets here")),
        "E_UNIMPLEMENTED"
    );
}

// ---------------------------------------------------------- archive_test

/// Whether the tool can be spawned at all. A missing tool is E_TOOL_MISSING;
/// a tool present but failing is not the absent case.
fn tool_present(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// About 64 KiB of letters: large enough that a flipped run of bytes in the
/// middle of its compressed form cannot land in padding.
fn payload() -> Vec<u8> {
    let mut x: u32 = 0x1234_5678;
    (0..65_536)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
            b'a' + ((x >> 16) % 26) as u8
        })
        .collect()
}

/// Flips a run of bytes in the middle of an archive.
fn corrupt(mut bytes: Vec<u8>) -> Vec<u8> {
    let mid = bytes.len() / 2;
    for b in &mut bytes[mid..mid + 32] {
        *b ^= 0xA5;
    }
    bytes
}

/// The tool's own compressor, run on `src`, whose compressed bytes are read
/// back from stdout.
fn compress(tool: &str, flags: &[&str], src: &Path) -> Vec<u8> {
    let out = Command::new(tool)
        .args(flags)
        .arg(src)
        .stdin(Stdio::null())
        .output()
        .expect("the tool runs");
    assert!(
        out.status.success(),
        "{tool} could not compress the fixture: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out.stdout
}

/// `(tool, compress flags, extension, magic bytes that name the codec)`.
const CODECS: &[(&str, &[&str], &str, &[u8])] = &[
    (
        "xz",
        &["-c"],
        "xz",
        &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0, 0],
    ),
    ("bzip2", &["-c"], "bz2", b"BZh9\0\0\0\0"),
    (
        "zstd",
        &["-c", "-q"],
        "zst",
        &[0x28, 0xB5, 0x2F, 0xFD, 0, 0, 0, 0],
    ),
];

#[test]
fn xz_bzip2_and_zstd_plain_archives_are_tested_by_their_tools() {
    for (tool, flags, ext, magic) in CODECS {
        let dir = Scratch::new(&format!("plain_{tool}"));
        let file = dir.path(&format!("sample.{ext}"));
        if !tool_present(tool) {
            // Right magic and no tool: the call reaches the tool and says so.
            std::fs::write(&file, magic).unwrap();
            let e = call("archive_test", vec![s(&path_str(&file))])
                .expect_err("the tool is not installed here");
            assert_eq!(code_of(&e), "E_TOOL_MISSING", "{tool}: {e}");
            continue;
        }

        let src = dir.path("sample.txt");
        std::fs::write(&src, payload()).unwrap();
        let good = compress(tool, flags, &src);
        std::fs::write(&file, &good).unwrap();
        assert_eq!(
            call("archive_test", vec![s(&path_str(&file))]).unwrap(),
            Value::Bool(true),
            "{tool}: a valid archive"
        );

        let bad = dir.path(&format!("bad.{ext}"));
        std::fs::write(&bad, corrupt(good)).unwrap();
        assert_eq!(
            call("archive_test", vec![s(&path_str(&bad))]).unwrap(),
            Value::Bool(false),
            "{tool}: a corrupt archive is an answer, not an error"
        );
    }
}

/// A tar archive inside each codec, built with the tar crate and compressed
/// with the tool, is read to its end and answers true; a damaged one answers
/// false.
#[test]
fn tar_archives_inside_xz_bzip2_and_zstd_are_tested() {
    let mut builder = tar::Builder::new(Vec::new());
    let data = payload();
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o644);
    builder
        .append_data(&mut header, "notes.txt", &data[..])
        .unwrap();
    let tar_bytes = builder.into_inner().unwrap();

    for (tool, flags, ext, magic) in CODECS {
        let dir = Scratch::new(&format!("tar_{tool}"));
        let file = dir.path(&format!("bundle.tar.{ext}"));
        if !tool_present(tool) {
            std::fs::write(&file, magic).unwrap();
            let e = call("archive_test", vec![s(&path_str(&file))])
                .expect_err("the tool is not installed here");
            assert_eq!(code_of(&e), "E_TOOL_MISSING", "{tool}: {e}");
            continue;
        }

        let tar_path = dir.path("bundle.tar");
        std::fs::write(&tar_path, &tar_bytes).unwrap();
        let good = compress(tool, flags, &tar_path);
        std::fs::write(&file, &good).unwrap();
        assert_eq!(
            call("archive_test", vec![s(&path_str(&file))]).unwrap(),
            Value::Bool(true),
            "{tool}: a valid tar archive"
        );

        let bad = dir.path(&format!("damaged.tar.{ext}"));
        std::fs::write(&bad, corrupt(good)).unwrap();
        assert_eq!(
            call("archive_test", vec![s(&path_str(&bad))]).unwrap(),
            Value::Bool(false),
            "{tool}: a damaged tar archive"
        );
    }
}

#[test]
fn a_codec_file_that_is_not_its_codec_is_refused_before_any_tool_runs() {
    let dir = Scratch::new("wrong_codec");
    let f = dir.path("text.zst");
    std::fs::write(&f, b"plain text, not zstd\n").unwrap();
    let e = call("archive_test", vec![s(&path_str(&f))]).expect_err("text under .zst");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}
