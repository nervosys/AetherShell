//! Contracts for the crypto, archive and ssh builtins: ssh_*, scp_*, gpg_*,
//! age_*, openssl_*, jq and jq_filter, xz_*, zstd_*, and zip_create/zip_add.
//!
//! The ordinary tests run everywhere. They cover refusals, which happen before
//! any tool starts, the pipeline-input refusal, and offline real runs in a fresh
//! temporary directory. A real run that needs a tool the machine lacks must
//! fail as E_TOOL_MISSING, and the test accepts exactly that.
//!
//! No test contacts a remote host. ssh and scp are only ever pointed at
//! 127.0.0.1 port 1, with BatchMode, where the connection is refused at once.
//! No test reads or writes ~/.ssh, ~/.gnupg or a keyring.

use aethershell::env::Env;
use aethershell::value::Value;
use std::path::{Path, PathBuf};

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn call_piped(name: &str, args: Vec<Value>, input: Value) -> anyhow::Result<Value> {
    let mut env = Env::new();
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

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A directory under the system temp dir, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ae_crypto_archive_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn file(&self, name: &str, text: &str) -> PathBuf {
        let p = self.path(name);
        std::fs::write(&p, text).expect("write scratch file");
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Asserts a call is refused with `code`, and returns the message.
fn refused(name: &str, args: Vec<Value>, code: &str) -> String {
    match call(name, args) {
        Ok(v) => panic!("{name} accepted a call it should refuse, and answered {v}"),
        Err(e) => {
            assert_eq!(code_of(&e), code, "{name} refused with the wrong code: {e}");
            e.to_string()
        }
    }
}

/// `Some(value)` when the call ran, or `None` when the tool is not installed
/// here, which must be the coded E_TOOL_MISSING and nothing else.
fn ran_or_tool_missing(name: &str, r: anyhow::Result<Value>) -> Option<Value> {
    match r {
        Ok(v) => Some(v),
        Err(e) => {
            assert_eq!(
                code_of(&e),
                "E_TOOL_MISSING",
                "{name} failed for a reason other than a missing tool: {e}"
            );
            None
        }
    }
}

/// A value's display text without ANSI colour. `Value`'s Display writes colour
/// unconditionally, so a comparison against plain text must strip it.
fn text(v: &Value) -> String {
    let mut out = String::new();
    let mut chars = format!("{v}").chars().collect::<Vec<_>>().into_iter();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

fn field<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    match v {
        Value::Record(m) => m.get(key),
        _ => None,
    }
}

// ── host, user and option arguments ─────────────────────────────────────────

#[test]
fn an_option_shaped_host_is_refused_before_any_tool_runs() {
    // `-oProxyCommand=…` is a command-execution vector for ssh and scp.
    refused(
        "ssh_exec",
        vec![s("-oProxyCommand=id"), s("true")],
        "E_BAD_ARG",
    );
    refused(
        "ssh_tunnel",
        vec![
            Value::Int(15432),
            s("db.internal"),
            Value::Int(5432),
            s("-oProxyCommand=id"),
        ],
        "E_BAD_ARG",
    );
    refused("ssh_copy_id", vec![s("-oProxyCommand=id")], "E_BAD_ARG");
    refused(
        "scp_upload",
        vec![s("notes.txt"), s("-oProxyCommand=id:/tmp/x")],
        "E_BAD_ARG",
    );
    // The option can also sit after the user name, where scp hands it to ssh.
    refused(
        "scp_upload",
        vec![s("notes.txt"), s("user@-oProxyCommand=id:/tmp/x")],
        "E_BAD_ARG",
    );
    refused("scp_download", vec![s("-oX:/a"), s("y.txt")], "E_BAD_ARG");
}

#[test]
fn a_remote_path_with_shell_or_glob_characters_is_refused() {
    refused(
        "scp_upload",
        vec![s("notes.txt"), s("host:/tmp/a;rm -rf x")],
        "E_BAD_ARG",
    );
    refused(
        "scp_upload",
        vec![s("notes.txt"), s("host:/tmp/$(id)")],
        "E_BAD_ARG",
    );
    refused(
        "scp_download",
        vec![s("host:/tmp/*.txt"), s("x.txt")],
        "E_BAD_ARG",
    );
}

#[test]
fn a_local_scp_path_with_a_colon_is_refused_as_a_host_would_be() {
    refused(
        "scp_upload",
        vec![s("x/a:b.txt"), s("host:/tmp/a")],
        "E_BAD_ARG",
    );
}

#[test]
fn ports_must_be_in_range() {
    refused(
        "ssh_exec",
        vec![s("host"), s("true"), Value::Int(0)],
        "E_BAD_ARG",
    );
    refused(
        "ssh_tunnel",
        vec![
            Value::Int(0),
            s("db.internal"),
            Value::Int(5432),
            s("bastion"),
        ],
        "E_BAD_ARG",
    );
    refused(
        "ssh_tunnel",
        vec![
            Value::Int(15432),
            s("db.internal"),
            Value::Int(70000),
            s("bastion"),
        ],
        "E_BAD_ARG",
    );
    refused(
        "scp_upload",
        vec![s("notes.txt"), s("host:/a"), Value::Int(0)],
        "E_BAD_ARG",
    );
}

#[test]
fn an_empty_remote_command_is_refused() {
    refused("ssh_exec", vec![s("user@host"), s("")], "E_BAD_ARG");
}

#[test]
fn ssh_keygen_refuses_an_unknown_type_and_an_option_file() {
    refused("ssh_keygen", vec![s("dsa")], "E_BAD_ARG");
    refused("ssh_keygen", vec![s("ed25519"), s("-oX")], "E_BAD_ARG");
}

#[test]
fn ssh_copy_id_refuses_a_private_key_without_showing_it() {
    let dir = Scratch::new("copy_id_private");
    let key = dir.file(
        "id_ed25519",
        "-----BEGIN OPENSSH PRIVATE KEY-----\nSECRETBLOB\n-----END OPENSSH PRIVATE KEY-----\n",
    );
    let msg = refused(
        "ssh_copy_id",
        vec![s("user@host"), s(&path_str(&key))],
        "E_BAD_ARG",
    );
    assert!(
        !msg.contains("SECRETBLOB"),
        "the key content was echoed: {msg}"
    );
    assert!(!msg.contains("BEGIN"), "the key header was echoed: {msg}");
}

#[test]
fn ssh_copy_id_refuses_a_public_key_line_with_quote_characters() {
    // A quote would break out of the quoting of the Windows remote command.
    let dir = Scratch::new("copy_id_quote");
    let key = dir.file(
        "id.pub",
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMq comment'; rm -rf x\n",
    );
    refused(
        "ssh_copy_id",
        vec![s("user@host"), s(&path_str(&key))],
        "E_BAD_ARG",
    );
}

#[test]
fn a_piped_subject_is_refused_by_every_builtin_that_takes_none() {
    let subject = s("piped text");
    assert_eq!(
        code_of(&call_piped("ssh_exec", vec![s("host"), s("true")], subject.clone()).unwrap_err()),
        "E_BAD_ARG"
    );
    assert_eq!(
        code_of(
            &call_piped(
                "zip_create",
                vec![s("out.zip"), s("a.txt")],
                subject.clone()
            )
            .unwrap_err()
        ),
        "E_BAD_ARG"
    );
    assert_eq!(
        code_of(&call_piped("xz_compress", vec![s("a.txt")], subject.clone()).unwrap_err()),
        "E_BAD_ARG"
    );
    assert_eq!(
        code_of(&call_piped("age_keygen", vec![s("id.txt")], subject).unwrap_err()),
        "E_BAD_ARG"
    );
}

// ── crypto: age, gpg, openssl ───────────────────────────────────────────────

#[test]
fn age_keygen_refuses_to_print_a_private_key_and_to_overwrite() {
    refused("age_keygen", vec![], "E_BAD_ARG");

    let dir = Scratch::new("age_keygen_exists");
    let existing = dir.file("id.txt", "keep me");
    refused("age_keygen", vec![s(&path_str(&existing))], "E_BAD_STATE");
    assert_eq!(std::fs::read_to_string(&existing).unwrap(), "keep me");
}

#[test]
fn age_recipients_and_decrypt_output_are_checked_before_a_tool_runs() {
    let dir = Scratch::new("age_checks");
    let input = dir.file("plain.txt", "hello");
    let input = path_str(&input);
    let out = path_str(&dir.path("out.age"));
    refused(
        "age_encrypt",
        vec![s(&input), s(&out), s("not-a-key")],
        "E_BAD_ARG",
    );
    refused(
        "age_encrypt",
        vec![s("-x"), s(&out), s("age1example")],
        "E_BAD_ARG",
    );
    // No .age suffix and no output file: there is nowhere safe to write.
    refused("age_decrypt", vec![s(&input)], "E_BAD_ARG");
    refused("age_decrypt", vec![s("-i"), s("key.txt")], "E_BAD_ARG");
}

#[test]
fn gpg_refuses_option_paths_and_existing_outputs() {
    refused("gpg_encrypt", vec![s("-x"), s("alice")], "E_BAD_ARG");
    refused("gpg_encrypt", vec![s("-x.txt"), s("-oX")], "E_BAD_ARG");

    let dir = Scratch::new("gpg_exists");
    let plain = dir.file("f.txt", "data");
    dir.file("f.txt.gpg", "already here");
    refused(
        "gpg_encrypt",
        vec![s(&path_str(&plain)), s("alice")],
        "E_BAD_STATE",
    );

    let cipher = dir.file("g.gpg", "not really gpg");
    dir.file("g", "already here");
    refused("gpg_decrypt", vec![s(&path_str(&cipher))], "E_BAD_STATE");
    assert_eq!(
        std::fs::read_to_string(dir.path("g")).unwrap(),
        "already here"
    );
}

#[test]
fn openssl_refuses_small_keys_options_and_existing_outputs() {
    refused(
        "openssl_genrsa",
        vec![s("key.pem"), Value::Int(1024)],
        "E_BAD_ARG",
    );
    refused("openssl_genrsa", vec![s("-engine")], "E_BAD_ARG");
    refused("openssl_cert_info", vec![s("-noout")], "E_BAD_ARG");

    let dir = Scratch::new("openssl_exists");
    let key = dir.file("key.pem", "keep me");
    refused("openssl_genrsa", vec![s(&path_str(&key))], "E_BAD_STATE");
    assert_eq!(std::fs::read_to_string(&key).unwrap(), "keep me");
}

// ── archives ────────────────────────────────────────────────────────────────

#[test]
fn xz_and_zstd_refuse_the_wrong_suffix_and_existing_outputs() {
    refused("xz_decompress", vec![s("notes.txt")], "E_BAD_ARG");
    refused("zstd_decompress", vec![s("a.txt")], "E_BAD_ARG");
    refused("xz_compress", vec![s("-k")], "E_BAD_ARG");

    let dir = Scratch::new("xz_exists");
    let plain = dir.file("f.txt", "data");
    dir.file("f.txt.xz", "already here");
    refused("xz_compress", vec![s(&path_str(&plain))], "E_BAD_STATE");
    dir.file("f.txt.zst", "already here");
    refused("zstd_compress", vec![s(&path_str(&plain))], "E_BAD_STATE");
    assert_eq!(
        std::fs::read_to_string(dir.path("f.txt.xz")).unwrap(),
        "already here"
    );
}

#[test]
fn zip_create_needs_files_and_refuses_an_existing_archive() {
    refused("zip_create", vec![s("out.zip")], "E_BAD_ARG");
    refused("zip_create", vec![s("out.zip"), Value::Int(5)], "E_BAD_ARG");
    refused("zip_add", vec![s("-TT"), s("a.txt")], "E_BAD_ARG");

    let dir = Scratch::new("zip_exists");
    let archive = dir.file("out.zip", "already here");
    let file = dir.file("a.txt", "a");
    refused(
        "zip_create",
        vec![s(&path_str(&archive)), s(&path_str(&file))],
        "E_BAD_STATE",
    );
    refused(
        "zip_add",
        vec![s(&path_str(&dir.path("missing.zip"))), s(&path_str(&file))],
        "E_NOT_FOUND",
    );
}

// ── jq ──────────────────────────────────────────────────────────────────────

#[test]
fn jq_native_evaluates_its_subset_and_refuses_the_rest() {
    let seven = call("jq", vec![s(".a.b"), s("{\"a\": {\"b\": 7}}")]).unwrap();
    assert_eq!(text(&seven), "7");

    let three = call_piped("jq", vec![s("length")], s("[1,2,3]")).unwrap();
    assert_eq!(text(&three), "3");

    // Code points, not bytes: "héllo" is five characters and six bytes.
    let five = call("jq", vec![s("length"), s("\"héllo\"")]).unwrap();
    assert_eq!(text(&five), "5");

    // jq's length of a number is its absolute value.
    let abs = call("jq", vec![s("length"), s("-4")]).unwrap();
    assert_eq!(text(&abs), "4");

    refused("jq", vec![s("length"), s("true")], "E_BAD_ARG");
    refused("jq", vec![s(".a[x]"), s("{\"a\": [1]}")], "E_BAD_ARG");
    refused("jq", vec![s(".[5]"), s("[1]")], "E_BAD_ARG");
    refused("jq", vec![s("map(.)"), s("[1]")], "E_BAD_ARG");
    refused("jq", vec![s(".nope"), s("{}")], "E_NOT_FOUND");
}

#[test]
fn jq_filter_refuses_an_option_filter_and_never_falls_back() {
    // A leading '-' would be read by jq as an option, such as --from-file.
    refused(
        "jq_filter",
        vec![s("--from-file=/etc/passwd"), s("{}")],
        "E_BAD_ARG",
    );
    refused("jq_filter", vec![s("-n")], "E_BAD_ARG");

    // With jq installed the filter runs. Without it the answer is E_TOOL_MISSING,
    // which is the one honest outcome; it must not be the native subset's guess.
    if let Some(v) = ran_or_tool_missing(
        "jq_filter",
        call("jq_filter", vec![s(".a"), s("{\"a\": 5}")]),
    ) {
        assert_eq!(text(&v), "5");
    }
}

// ── real runs, offline, in a fresh temporary directory ──────────────────────

#[test]
fn xz_round_trip_restores_the_file() {
    let dir = Scratch::new("xz_round_trip");
    let original = dir.file("a.txt", "hello xz\n");
    let input = path_str(&original);
    let Some(done) = ran_or_tool_missing("xz_compress", call("xz_compress", vec![s(&input)]))
    else {
        return;
    };
    assert_eq!(done, Value::Bool(true));
    assert!(
        dir.path("a.txt.xz").is_file(),
        "xz_compress wrote no a.txt.xz"
    );
    assert!(original.is_file(), "xz_compress removed its input");

    std::fs::remove_file(&original).unwrap();
    let back = call("xz_decompress", vec![s(&path_str(&dir.path("a.txt.xz")))]).unwrap();
    assert_eq!(back, Value::Bool(true));
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "hello xz\n");
}

#[test]
fn zstd_round_trip_restores_the_file() {
    let dir = Scratch::new("zstd_round_trip");
    let original = dir.file("b.txt", "hello zstd\n");
    let input = path_str(&original);
    let Some(done) = ran_or_tool_missing("zstd_compress", call("zstd_compress", vec![s(&input)]))
    else {
        return;
    };
    assert_eq!(done, Value::Bool(true));
    std::fs::remove_file(&original).unwrap();
    let back = call(
        "zstd_decompress",
        vec![s(&path_str(&dir.path("b.txt.zst")))],
    )
    .unwrap();
    assert_eq!(back, Value::Bool(true));
    assert_eq!(std::fs::read_to_string(&original).unwrap(), "hello zstd\n");
}

#[test]
fn age_keygen_then_encrypt_and_decrypt_round_trip_without_printing_the_key() {
    let dir = Scratch::new("age_round_trip");
    let identity = path_str(&dir.path("identity.txt"));
    let Some(made) = ran_or_tool_missing("age_keygen", call("age_keygen", vec![s(&identity)]))
    else {
        return;
    };
    // The result carries the public key only, never the identity.
    assert!(
        !text(&made).contains("AGE-SECRET-KEY"),
        "the private key was returned"
    );
    let public = match field(&made, "public_key") {
        Some(Value::Str(p)) => p.clone(),
        other => panic!("no public_key in {made}: {other:?}"),
    };
    assert!(
        public.starts_with("age1"),
        "unexpected public key: {public}"
    );
    let secret = std::fs::read_to_string(&identity).unwrap();
    assert!(
        secret.contains("AGE-SECRET-KEY-1"),
        "identity file has no key"
    );

    let plain = dir.file("plain.txt", "age payload\n");
    let cipher = path_str(&dir.path("plain.age"));
    let enc = ran_or_tool_missing(
        "age_encrypt",
        call(
            "age_encrypt",
            vec![s(&path_str(&plain)), s(&cipher), s(&public)],
        ),
    );
    if enc.is_none() {
        return;
    }
    std::fs::remove_file(&plain).unwrap();
    let back = path_str(&dir.path("plain.txt"));
    call("age_decrypt", vec![s(&cipher), s(&back), s(&identity)]).unwrap();
    assert_eq!(std::fs::read_to_string(&back).unwrap(), "age payload\n");
}

#[test]
fn openssl_genrsa_writes_a_key_file_and_returns_no_key() {
    let dir = Scratch::new("genrsa");
    let key = path_str(&dir.path("key.pem"));
    let Some(made) = ran_or_tool_missing(
        "openssl_genrsa",
        call("openssl_genrsa", vec![s(&key), Value::Int(2048)]),
    ) else {
        return;
    };
    assert!(!text(&made).contains("BEGIN"), "the key was returned");
    let text = std::fs::read_to_string(&key).unwrap();
    assert!(text.contains("PRIVATE KEY"), "no key was written");
    // The file exists now, so the same call is refused rather than replaced.
    refused(
        "openssl_genrsa",
        vec![s(&key), Value::Int(2048)],
        "E_BAD_STATE",
    );
}

#[test]
fn zip_create_then_zip_add_lists_every_member() {
    let dir = Scratch::new("zip_round_trip");
    let archive = path_str(&dir.path("out.zip"));
    let a = dir.file("a.txt", "a\n");
    let b = dir.file("b.txt", "b\n");
    let Some(created) = ran_or_tool_missing(
        "zip_create",
        call("zip_create", vec![s(&archive), s(&path_str(&a))]),
    ) else {
        return;
    };
    assert_eq!(created, Value::Bool(true));
    let added = call("zip_add", vec![s(&archive), s(&path_str(&b))]).unwrap();
    assert_eq!(added, Value::Bool(true));
    let listing = text(&call("zip_list", vec![s(&archive)]).unwrap());
    assert!(listing.contains("a.txt"), "a.txt missing from {listing}");
    assert!(listing.contains("b.txt"), "b.txt missing from {listing}");
}

// ── remote tools, pointed at a closed local port only ───────────────────────

#[test]
fn ssh_and_scp_to_a_closed_local_port_fail_coded() {
    // 127.0.0.1 port 1 refuses at once. Nothing leaves the machine.
    if let Err(e) = call("ssh_exec", vec![s("127.0.0.1"), s("true"), Value::Int(1)]) {
        let code = code_of(&e);
        assert!(
            code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
            "ssh_exec failed uncoded or with {code}: {e}"
        );
    } else {
        panic!("ssh_exec reported success against a closed port");
    }

    let dir = Scratch::new("scp_closed_port");
    let file = dir.file("a.txt", "a\n");
    if let Err(e) = call(
        "scp_upload",
        vec![
            s(&path_str(&file)),
            s("127.0.0.1:/nonexistent/a.txt"),
            Value::Int(1),
        ],
    ) {
        let code = code_of(&e);
        assert!(
            code == "E_TOOL_FAILED" || code == "E_TOOL_MISSING",
            "scp_upload failed uncoded or with {code}: {e}"
        );
    } else {
        panic!("scp_upload reported success against a closed port");
    }
}

#[test]
fn ssh_keygen_writes_a_new_pair_and_refuses_to_replace_it() {
    let dir = Scratch::new("ssh_keygen");
    let key = path_str(&dir.path("id_test"));
    let Some(made) = ran_or_tool_missing(
        "ssh_keygen",
        call("ssh_keygen", vec![s("ed25519"), s(&key)]),
    ) else {
        return;
    };
    assert_eq!(field(&made, "passphrase"), Some(&Value::Bool(false)));
    assert!(dir.path("id_test").is_file(), "no private key");
    assert!(dir.path("id_test.pub").is_file(), "no public key");
    refused("ssh_keygen", vec![s("ed25519"), s(&key)], "E_BAD_STATE");
}
