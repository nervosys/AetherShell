//! The openssl-backed crypto builtins, checked against a real OpenSSL.
//!
//! `crypto_cert_info` and `crypto_cert_verify` no longer run openssl: they
//! parse and validate in-process with IronSocketLayer's X.509, and their
//! answers are checked here against openssl's own output. The legacy path of
//! `crypto_decrypt` still runs the `openssl` CLI on every OS.
//! `crypto_generate_key` generates natively, so its output is checked here by
//! openssl, but it needs no openssl on PATH.
//! On Windows that is `openssl.exe`, which Git for Windows ships. Every effect
//! is set up here (a key, a certificate, a legacy ciphertext) and checked with
//! openssl itself rather than by trusting the builtin's own answer.
//!
//! All checks that depend on the process PATH live in ONE test, because PATH
//! and the other environment variables are shared by every test in the binary.

use aethershell::builtins::call;
use aethershell::env::Env;
use aethershell::value::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const AE: &str = env!("CARGO_BIN_EXE_ae");
/// Where Git for Windows keeps its openssl.exe.
const GIT_OPENSSL_DIR: &str = r"C:\Program Files\Git\mingw64\bin";
/// A child `ae` that has not finished by then is killed.
const CHILD_DEADLINE: Duration = Duration::from_secs(30);

/// A scratch directory, removed on drop even when the test panics.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("ae-openssl-{label}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        ScratchDir(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A child process killed on drop, so a panic cannot leave it running.
struct KillOnDrop(std::process::Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Removes an environment variable on drop.
struct UnsetOnDrop(&'static str);

impl Drop for UnsetOnDrop {
    fn drop(&mut self) {
        std::env::remove_var(self.0);
    }
}

/// Puts PATH back on drop.
struct RestorePath(Option<OsString>);

impl Drop for RestorePath {
    fn drop(&mut self) {
        match self.0.take() {
            Some(value) => std::env::set_var("PATH", value),
            None => std::env::remove_var("PATH"),
        }
    }
}

fn s(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}

fn text(value: &str) -> Value {
    Value::Str(value.to_string())
}

fn openssl(args: &[&str]) -> std::process::Output {
    Command::new("openssl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run openssl")
}

fn assert_openssl_ok(out: &std::process::Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A self-signed certificate with its key, for the given common name.
fn self_signed(dir: &ScratchDir, stem: &str, common_name: &str) -> PathBuf {
    let key = dir.path(&format!("{stem}.key.pem"));
    let cert = dir.path(&format!("{stem}.pem"));
    let subject = format!("/CN={common_name}");
    let out = openssl(&[
        "req",
        "-x509",
        "-newkey",
        "ec",
        "-pkeyopt",
        "ec_paramgen_curve:prime256v1",
        "-nodes",
        "-keyout",
        s(&key),
        "-out",
        s(&cert),
        "-days",
        "30",
        "-subj",
        &subject,
    ]);
    assert_openssl_ok(&out, "openssl req -x509");
    cert
}

/// `crypto_cert_verify(cert)` with `trust` as OpenSSL's default trust store.
/// The builtin takes one argument, so the trust anchor reaches `openssl verify`
/// through SSL_CERT_FILE, which OpenSSL reads for its default store on every OS.
fn verify_with_trust(trust: &Path, cert: &Path) -> Value {
    std::env::set_var("SSL_CERT_FILE", trust);
    let _unset = UnsetOnDrop("SSL_CERT_FILE");
    let mut env = Env::new();
    call("crypto_cert_verify", vec![text(s(cert))], &mut env)
        .unwrap_or_else(|e| panic!("crypto_cert_verify failed: {e}"))
}

#[test]
fn openssl_backed_crypto_builtins_on_this_host() {
    if !ensure_openssl_on_path() {
        println!("openssl absent; skipped");
        return;
    }
    let dir = ScratchDir::new("main");
    let mut env = Env::new();

    // Generated RSA and EC keys are real PEM private keys that openssl parses.
    let rsa = call(
        "crypto_generate_key",
        vec![text("rsa"), Value::Int(2048)],
        &mut env,
    );
    let ec = call("crypto_generate_key", vec![text("EC")], &mut env);
    for (kind, result) in [("rsa", rsa), ("ec", ec)] {
        let pem = match result {
            Ok(Value::Str(pem)) => pem,
            other => panic!("crypto_generate_key({kind}) returned {other:?}"),
        };
        let key = dir.path(&format!("{kind}.key.pem"));
        std::fs::write(&key, &pem).expect("write generated key");
        assert_openssl_ok(
            &openssl(&["pkey", "-in", s(&key), "-noout"]),
            &format!("openssl pkey on the generated {kind} key"),
        );
    }

    // cert_info: one record for the certificate. Its subject and self-issued
    // flag are read from the certificate, and its SHA-256 fingerprint is
    // checked against openssl's.
    let cert = self_signed(&dir, "leaf", "ae-leaf.invalid");
    let records = match call("crypto_cert_info", vec![text(s(&cert))], &mut env) {
        Ok(Value::Array(records)) => records,
        other => panic!("crypto_cert_info returned {other:?}"),
    };
    let [Value::Record(info)] = records.as_slice() else {
        panic!("expected one certificate record, got {records:?}");
    };
    assert_eq!(
        info.get("subject"),
        Some(&Value::Str("CN=ae-leaf.invalid".into())),
        "subject in {info:?}"
    );
    assert_eq!(info.get("self_issued"), Some(&Value::Bool(true)));
    let fp = openssl(&["x509", "-in", s(&cert), "-noout", "-fingerprint", "-sha256"]);
    assert_openssl_ok(&fp, "openssl x509 -fingerprint");
    let expected_fp: String = String::from_utf8_lossy(&fp.stdout)
        .split('=')
        .nth(1)
        .unwrap_or_default()
        .trim()
        .replace(':', "")
        .to_lowercase();
    assert_eq!(
        info.get("sha256_fingerprint"),
        Some(&Value::Str(expected_fp))
    );

    // cert_verify: a self-signed certificate is a CA (openssl's req -x509
    // default), so it is not a valid end entity even against itself; an
    // unrelated CA does not verify it. Chains that validate are covered in
    // tests/cert_isl_contracts.rs.
    let other = self_signed(&dir, "other-ca", "ae-other-ca.invalid");
    assert_eq!(verify_with_trust(&cert, &cert), Value::Bool(false));
    assert_eq!(verify_with_trust(&other, &cert), Value::Bool(false));
    let absent = dir.path("absent-cert.pem");
    let e = call("crypto_cert_verify", vec![text(s(&absent))], &mut env)
        .expect_err("a missing certificate must not be a verdict");
    assert!(e.to_string().contains("E_NOT_FOUND"), "{e}");

    // Legacy CBC: the ciphertext comes from the openssl CLI, as the old
    // builtin wrote it, and is refused unless the operator opts in.
    let plain = dir.path("plain.txt");
    std::fs::write(&plain, "legacy line one\nlegacy line two\n").expect("write plaintext");
    let legacy = dir.path("legacy.b64");
    let enc = Command::new("openssl")
        .args([
            "enc",
            "-aes-256-cbc",
            "-pbkdf2",
            "-base64",
            "-A",
            "-pass",
            "env:AE_TEST_LEGACY_PASS",
            "-in",
            s(&plain),
            "-out",
            s(&legacy),
        ])
        .env("AE_TEST_LEGACY_PASS", "legacy-pass")
        .stdin(Stdio::null())
        .output()
        .expect("run openssl enc");
    assert_openssl_ok(&enc, "openssl enc -aes-256-cbc -pbkdf2");
    let ciphertext = std::fs::read_to_string(&legacy)
        .expect("read legacy ciphertext")
        .trim()
        .to_string();

    {
        let _unset = UnsetOnDrop("AETHER_CRYPTO_LEGACY_DECRYPT");
        std::env::remove_var("AETHER_CRYPTO_LEGACY_DECRYPT");
        let e = call(
            "crypto_decrypt",
            vec![text(&ciphertext), text("legacy-pass")],
            &mut env,
        )
        .expect_err("legacy ciphertext decrypted without the opt-in");
        assert!(e.to_string().contains("E_DECRYPT_FAILED"), "{e}");
        assert!(
            e.to_string().contains("AETHER_CRYPTO_LEGACY_DECRYPT"),
            "{e}"
        );
    }
    {
        let _unset = UnsetOnDrop("AETHER_CRYPTO_LEGACY_DECRYPT");
        std::env::set_var("AETHER_CRYPTO_LEGACY_DECRYPT", "1");
        match call(
            "crypto_decrypt",
            vec![text(&ciphertext), text("legacy-pass")],
            &mut env,
        ) {
            Ok(Value::Str(plaintext)) => {
                assert_eq!(plaintext, "legacy line one\nlegacy line two\n")
            }
            other => panic!("legacy decrypt with the opt-in returned {other:?}"),
        }

        // Plaintext that is not UTF-8 is refused, never returned lossily.
        let binary = dir.path("binary.bin");
        std::fs::write(&binary, [0xffu8, 0xfe, 0x00, 0x80]).expect("write binary plaintext");
        let binary_b64 = dir.path("binary.b64");
        let enc = Command::new("openssl")
            .args([
                "enc",
                "-aes-256-cbc",
                "-pbkdf2",
                "-base64",
                "-A",
                "-pass",
                "env:AE_TEST_LEGACY_PASS",
                "-in",
                s(&binary),
                "-out",
                s(&binary_b64),
            ])
            .env("AE_TEST_LEGACY_PASS", "legacy-pass")
            .stdin(Stdio::null())
            .output()
            .expect("run openssl enc");
        assert_openssl_ok(&enc, "openssl enc of binary plaintext");
        let binary_ct = std::fs::read_to_string(&binary_b64)
            .expect("read binary ciphertext")
            .trim()
            .to_string();
        let e = call(
            "crypto_decrypt",
            vec![text(&binary_ct), text("legacy-pass")],
            &mut env,
        )
        .expect_err("non-UTF-8 plaintext was returned");
        assert!(e.to_string().contains("E_DECRYPT_FAILED"), "{e}");
        assert!(e.to_string().contains("UTF-8"), "{e}");
    }

    // With no openssl on PATH: cert_info still answers, since it needs no
    // tool, and the legacy decrypt answers too, since it is in-process now.
    let empty = ScratchDir::new("no-openssl");
    let _restore = RestorePath(std::env::var_os("PATH"));
    std::env::set_var("PATH", empty.0.as_os_str());
    let _opt_in = UnsetOnDrop("AETHER_CRYPTO_LEGACY_DECRYPT");
    std::env::set_var("AETHER_CRYPTO_LEGACY_DECRYPT", "1");
    assert!(
        matches!(
            call("crypto_cert_info", vec![text(s(&cert))], &mut env),
            Ok(Value::Array(ref a)) if a.len() == 1
        ),
        "crypto_cert_info needs no openssl on PATH"
    );
    match call(
        "crypto_decrypt",
        vec![text(&ciphertext), text("legacy-pass")],
        &mut env,
    )
    .expect("legacy decrypt is in-process and needs no openssl on PATH")
    {
        Value::Str(plaintext) => {
            assert_eq!(plaintext, "legacy line one\nlegacy line two\n");
        }
        other => panic!("legacy decrypt returned {}", other.type_name()),
    }
}

/// Makes `openssl` resolvable: it is already on PATH, or Git for Windows'
/// copy is prepended to PATH. False when neither exists.
fn ensure_openssl_on_path() -> bool {
    let on_path = Command::new("openssl")
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if on_path {
        return true;
    }
    let git = Path::new(GIT_OPENSSL_DIR);
    if !git.join("openssl.exe").is_file() {
        return false;
    }
    let mut dirs = vec![git.to_path_buf()];
    if let Some(old) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&old));
    }
    std::env::set_var("PATH", std::env::join_paths(dirs).expect("join PATH"));
    true
}

/// `ae -c` with no openssl on its PATH inspects a file that is not a
/// certificate and refuses it as E_BAD_ARG: the path needs no tool, and a file
/// that is not a certificate is a refusal, never an empty success.
/// It runs everywhere, and never touches this test process's environment.
#[test]
fn ae_cert_info_without_openssl_on_path_is_bad_arg() {
    let dir = ScratchDir::new("child");
    let cert = dir.path("cert.pem");
    std::fs::write(&cert, "not really a certificate\n").expect("write cert placeholder");
    let empty = ScratchDir::new("child-path");
    let expr = format!("crypto_cert_info(\"{}\")", s(&cert).replace('\\', "/"));
    let stdout_path = dir.path("child.out");
    let stderr_path = dir.path("child.err");
    let mut cmd = Command::new(AE);
    cmd.args(["-c", &expr])
        .current_dir(&empty.0)
        .env("PATH", &empty.0)
        .env("AETHER_SESSION_DIR", &dir.0)
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout_path).expect("create stdout file"))
        .stderr(std::fs::File::create(&stderr_path).expect("create stderr file"));
    let mut child = KillOnDrop(cmd.spawn().expect("spawn ae"));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("poll ae") {
            break status;
        }
        assert!(
            started.elapsed() < CHILD_DEADLINE,
            "ae did not finish within 30 s"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let stdout = std::fs::read_to_string(&stdout_path).unwrap_or_default();
    let stderr = std::fs::read_to_string(&stderr_path).unwrap_or_default();
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("E_BAD_ARG"),
        "expected E_BAD_ARG (status {status}), got:\n{combined}"
    );
    assert!(!status.success(), "a refused call exited 0: {combined}");
}
