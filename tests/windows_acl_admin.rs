//! Contracts for the Windows `acl_get` and `acl_set` backends (icacls).
//!
//! Compiled on Windows only. Each test changes the ACL of a file or directory
//! that the test itself creates in the temp directory, and removes it in a
//! Drop guard. The tests are `#[ignore]`d and also return at once unless
//! AE_DISPOSABLE_HOST=1, which only the disposable Windows runner sets.
//!
//! The builtins are checked against PowerShell's Get-Acl, not against icacls
//! or against the builtins' own output. The principal is the built-in Users
//! group, named by its SID S-1-5-32-545 and resolved to its account name at run
//! time, so a localized Windows still works. The oracle lists the explicit
//! (not inherited) ACEs for that SID. The test path reaches PowerShell in an
//! environment variable, not in the script text.

#![cfg(windows)]

use aethershell::env::Env;
use aethershell::value::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Every test takes this lock, so ACL changes never overlap.
static SERIAL: Mutex<()> = Mutex::new(());

/// The built-in Users group. Its SID is the same on every Windows install.
const USERS_SID: &str = "S-1-5-32-545";

/// Lists the explicit ACEs for the Users SID on the path in AE_ACL_PATH, one
/// line each: `<Allow|Deny> <FileSystemRights>`.
const EXPLICIT_ACES: &str = r#"(Get-Acl -LiteralPath $env:AE_ACL_PATH).Access | Where-Object { -not $_.IsInherited -and $_.IdentityReference.Translate([System.Security.Principal.SecurityIdentifier]).Value -eq 'S-1-5-32-545' } | ForEach-Object { "$($_.AccessControlType) $($_.FileSystemRights)" }"#;

/// True only on the disposable Windows runner, which sets AE_DISPOSABLE_HOST=1.
fn disposable() -> bool {
    std::env::var("AE_DISPOSABLE_HOST").as_deref() == Ok("1")
}

fn s(text: &str) -> Value {
    Value::Str(text.to_string())
}

fn rec(pairs: &[(&str, Value)]) -> Value {
    Value::Record(
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect::<BTreeMap<String, Value>>(),
    )
}

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_ok(name: &str, args: Vec<Value>) -> Value {
    call(name, args).unwrap_or_else(|e| panic!("{name} refused a valid call: {e}"))
}

/// The coded refusal an error carries: the first `E_` token in its text.
fn code_of(e: &anyhow::Error) -> String {
    let text = e.to_string();
    match text.find("E_") {
        Some(i) => text[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {text}"),
    }
}

fn expect_code(result: anyhow::Result<Value>, want: &str) {
    match result {
        Ok(v) => panic!("answered {v:?}; expected {want}"),
        Err(e) => assert_eq!(code_of(&e), want, "wrong refusal: {e}"),
    }
}

fn field<'a>(v: &'a Value, key: &str) -> &'a Value {
    match v {
        Value::Record(m) => m.get(key).unwrap_or_else(|| panic!("no {key} in {v:?}")),
        other => panic!("not a record: {other:?}"),
    }
}

fn text(v: &Value, key: &str) -> String {
    match field(v, key) {
        Value::Str(s) => s.clone(),
        other => panic!("{key} is not a String: {other:?}"),
    }
}

fn flag(v: &Value, key: &str) -> bool {
    match field(v, key) {
        Value::Bool(b) => *b,
        other => panic!("{key} is not a Bool: {other:?}"),
    }
}

/// Runs PowerShell with `script`. The path, when given, is passed in AE_ACL_PATH.
fn powershell(script: &str, path: Option<&Path>) -> String {
    let mut cmd = Command::new("powershell.exe");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", script]);
    if let Some(path) = path {
        cmd.env("AE_ACL_PATH", path);
    }
    let out = cmd.output().expect("powershell.exe should start");
    assert!(
        out.status.success(),
        "PowerShell oracle failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The account name of the Users SID on this machine, from the SID itself.
fn users_name() -> String {
    let script = format!(
        "(New-Object System.Security.Principal.SecurityIdentifier('{USERS_SID}')).Translate([System.Security.Principal.NTAccount]).Value"
    );
    let name = powershell(&script, None).trim().to_string();
    assert!(!name.is_empty(), "the Users SID has no account name here");
    assert!(
        !name.contains(|c| matches!(c, ':' | '(' | ')')),
        "the Users account name {name:?} would not parse as a principal"
    );
    name
}

/// The explicit ACEs for the Users SID: each is (AccessControlType, rights).
fn explicit_aces(path: &Path) -> Vec<(String, Vec<String>)> {
    powershell(EXPLICIT_ACES, Some(path))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (kind, rights) = line
                .split_once(' ')
                .unwrap_or_else(|| panic!("unexpected oracle line {line:?}"));
            (
                kind.to_string(),
                rights.split(", ").map(str::to_string).collect(),
            )
        })
        .collect()
}

/// The explicit (not inherited) entries for `principal` in an acl_get record,
/// as their perms text.
fn explicit_perms(listing: &Value, principal: &str) -> Vec<String> {
    let Value::Array(entries) = field(listing, "entries") else {
        panic!("entries is not an Array: {listing:?}");
    };
    entries
        .iter()
        .filter(|entry| {
            text(entry, "principal").eq_ignore_ascii_case(principal) && !flag(entry, "inherited")
        })
        .map(|entry| text(entry, "perms"))
        .collect()
}

fn unique_path(kind: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is after 1970")
        .as_nanos();
    std::env::temp_dir().join(format!("aeacl-{}-{nanos}-{kind}", std::process::id()))
}

/// Deletes what a test created, even when an assertion failed first.
struct Owned(PathBuf);

impl Drop for Owned {
    fn drop(&mut self) {
        if self.0.is_dir() {
            let _ = std::fs::remove_dir_all(&self.0);
        } else {
            let _ = std::fs::remove_file(&self.0);
        }
    }
}

/// Points the audit log at a temp file and clears the mode variables, so that
/// the calls run under the default policy. Everything is restored on drop.
struct AuditSandbox {
    saved: Vec<(&'static str, Option<std::ffi::OsString>)>,
    log: PathBuf,
}

const SANDBOX_KEYS: &[&str] = &[
    "AETHER_MODE",
    "AETHER_AGENT",
    "AETHER_POLICY",
    "AETHER_AUDIT_LOG",
    "AETHER_AUDIT_KEY",
];

impl AuditSandbox {
    fn new() -> Self {
        let saved = SANDBOX_KEYS
            .iter()
            .map(|key| (*key, std::env::var_os(key)))
            .collect();
        for key in SANDBOX_KEYS {
            std::env::remove_var(key);
        }
        let log = unique_path("audit.log");
        std::env::set_var("AETHER_AUDIT_LOG", &log);
        std::env::set_var("AETHER_AUDIT_KEY", "11".repeat(32));
        Self { saved, log }
    }
}

impl Drop for AuditSandbox {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
        let _ = std::fs::remove_file(&self.log);
    }
}

/// Grant Users Modify, read it back two ways, then remove it. The oracle is
/// checked before, after the grant, and after the removal.
fn grant_read_and_remove(path: &Path, principal: &str, is_dir: bool) {
    let path_text = path.to_str().expect("temp path is UTF-8");
    assert!(
        explicit_aces(path).is_empty(),
        "a fresh {} has an explicit ACE for the Users SID before the grant",
        if is_dir { "directory" } else { "file" }
    );

    let grant = call_ok("acl_set", vec![s(path_text), s(&format!("{principal}:M"))]);
    assert!(flag(&grant, "applied"), "acl_set did not report applied");
    assert_eq!(text(&grant, "path"), path_text);

    let aces = explicit_aces(path);
    assert!(
        aces.iter()
            .any(|(kind, rights)| kind == "Allow" && rights.iter().any(|r| r == "Modify")),
        "oracle shows no Allow Modify ACE after the grant: {aces:?}"
    );

    let listing = call_ok("acl_get", vec![s(path_text)]);
    assert_eq!(text(&listing, "path"), path_text);
    assert_eq!(
        explicit_perms(&listing, principal),
        vec!["M".to_string()],
        "acl_get does not list the Modify grant as an explicit entry"
    );

    call_ok(
        "acl_set",
        vec![
            s(path_text),
            s(principal),
            rec(&[("remove", Value::Bool(true))]),
        ],
    );
    assert!(
        explicit_aces(path).is_empty(),
        "oracle still shows an explicit ACE after the removal"
    );
    assert!(
        explicit_perms(&call_ok("acl_get", vec![s(path_text)]), principal).is_empty(),
        "acl_get still lists an explicit entry after the removal"
    );
}

/// A file's ACL as the oracle reports it, for comparing before and after a refusal.
fn snapshot(path: &Path) -> Vec<(String, Vec<String>)> {
    explicit_aces(path)
}

#[test]
#[ignore = "changes the ACL of a file the test creates: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn acl_grant_and_remove_on_a_file_are_seen_by_the_oracle() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _audit = AuditSandbox::new();
    let principal = users_name();
    let file = unique_path("file.txt");
    std::fs::write(&file, b"acl test\n").expect("create the test file");
    let _owned = Owned(file.clone());

    grant_read_and_remove(&file, &principal, false);
}

#[test]
#[ignore = "changes the ACL of a directory the test creates: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn acl_grant_and_remove_on_a_directory_are_seen_by_the_oracle() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _audit = AuditSandbox::new();
    let principal = users_name();
    let dir = unique_path("dir");
    std::fs::create_dir(&dir).expect("create the test directory");
    let _owned = Owned(dir.clone());

    grant_read_and_remove(&dir, &principal, true);
}

/// A specific-rights grant, `Name:RX,D`, takes the rights the oracle reports:
/// ReadAndExecute and Delete, and not Modify.
#[test]
#[ignore = "changes the ACL of a file the test creates: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn acl_specific_rights_grant_is_seen_by_the_oracle() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _audit = AuditSandbox::new();
    let principal = users_name();
    let file = unique_path("specific.txt");
    std::fs::write(&file, b"acl test\n").expect("create the test file");
    let _owned = Owned(file.clone());
    let path_text = file.to_str().expect("temp path is UTF-8");

    let grant = call_ok(
        "acl_set",
        vec![s(path_text), s(&format!("{principal}:RX,D"))],
    );
    assert!(flag(&grant, "applied"));
    assert_eq!(text(&grant, "entry"), format!("{principal}:(RX,D)"));

    let aces = explicit_aces(&file);
    assert!(
        aces.iter().any(|(kind, rights)| {
            kind == "Allow"
                && rights.iter().any(|r| r == "ReadAndExecute")
                && rights.iter().any(|r| r == "Delete")
                && !rights.iter().any(|r| r == "Modify")
        }),
        "oracle does not show Allow ReadAndExecute, Delete: {aces:?}"
    );

    call_ok(
        "acl_set",
        vec![
            s(path_text),
            s(&principal),
            rec(&[("remove", Value::Bool(true))]),
        ],
    );
    assert!(explicit_aces(&file).is_empty());
}

/// Each refusal is raised before anything changes: the missing path creates
/// nothing, and the oracle shows the same ACL before and after every refusal.
#[test]
#[ignore = "runs the file ACL builtins against a file the test creates: disposable Windows runner only (AE_DISPOSABLE_HOST=1)"]
fn acl_refusals_change_nothing() {
    if !disposable() {
        return;
    }
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let _audit = AuditSandbox::new();
    let principal = users_name();
    let file = unique_path("refusal.txt");
    std::fs::write(&file, b"acl test\n").expect("create the test file");
    let _owned = Owned(file.clone());
    let path_text = file.to_str().expect("temp path is UTF-8").to_string();
    let before = snapshot(&file);
    assert!(
        before.is_empty(),
        "a fresh file has an explicit ACE: {before:?}"
    );

    // A path that does not exist: E_NOT_FOUND, and nothing is created.
    let missing = unique_path("missing.txt");
    let missing_text = missing.to_str().expect("temp path is UTF-8").to_string();
    expect_code(call("acl_get", vec![s(&missing_text)]), "E_NOT_FOUND");
    expect_code(
        call(
            "acl_set",
            vec![s(&missing_text), s(&format!("{principal}:M"))],
        ),
        "E_NOT_FOUND",
    );
    assert!(
        !missing.exists(),
        "a refused acl_set created the missing path"
    );
    assert_eq!(snapshot(&file), before);

    // A wildcard in the path: E_BAD_ARG, before icacls runs.
    let starred = format!("{path_text}*");
    expect_code(call("acl_get", vec![s(&starred)]), "E_BAD_ARG");
    expect_code(
        call("acl_set", vec![s(&starred), s(&format!("{principal}:M"))]),
        "E_BAD_ARG",
    );
    assert_eq!(snapshot(&file), before);

    // The Linux `user::perms` form: E_BAD_ARG.
    expect_code(
        call("acl_set", vec![s(&path_text), s("u::rw-")]),
        "E_BAD_ARG",
    );
    assert_eq!(snapshot(&file), before);

    // A default (inheritance) option: E_BAD_ARG.
    expect_code(
        call(
            "acl_set",
            vec![
                s(&path_text),
                s(&format!("{principal}:M")),
                rec(&[("default", Value::Bool(true))]),
            ],
        ),
        "E_BAD_ARG",
    );
    assert_eq!(snapshot(&file), before);
}
