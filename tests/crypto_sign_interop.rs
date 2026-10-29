//! crypto_sign output checked by openssl itself: a fresh RSA and a fresh EC key
//! are generated through crypto_generate_key, signed by crypto_sign, and the
//! message, signature and SPKI public key are verified by `openssl dgst`.
//! Skipped with a printed line when openssl is not on PATH.

use aethershell::builtins::call;
use aethershell::env::Env;
use aethershell::value::Value;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// An openssl child that has not finished by then is killed.
const OPENSSL_DEADLINE: Duration = Duration::from_secs(120);

/// A scratch directory, removed on drop even when the test panics.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ae-sign-interop-{label}-{}-{nanos}",
            std::process::id()
        ));
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

fn openssl_on_path() -> bool {
    Command::new("openssl")
        .arg("version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Runs openssl under OPENSSL_DEADLINE and returns its output.
fn openssl(args: &[&str]) -> std::process::Output {
    let mut child = Command::new("openssl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run openssl");
    let deadline = Instant::now() + OPENSSL_DEADLINE;
    loop {
        match child.try_wait().expect("poll openssl") {
            Some(_) => break,
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("openssl {args:?} exceeded its deadline");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    child.wait_with_output().expect("collect openssl output")
}

fn assert_openssl_ok(out: &std::process::Output, what: &str) {
    assert!(
        out.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn path_str(path: &Path) -> &str {
    path.to_str().expect("UTF-8 test path")
}

#[test]
fn crypto_sign_output_verifies_under_openssl_dgst() {
    if !openssl_on_path() {
        println!("skipped: openssl is not on PATH; crypto_sign interop not checked");
        return;
    }
    let scratch = ScratchDir::new("dgst");
    // Keep any session state inside the scratch directory, not the user's.
    let session = scratch.path("session");
    std::fs::create_dir_all(&session).expect("create session dir");
    std::env::set_var("AETHER_SESSION_DIR", &session);

    let mut env = Env::new();
    for (kind, algorithm) in [("rsa", "rsa-pkcs1-sha256"), ("ec", "ecdsa-p256-sha256")] {
        let Value::Str(private_pem) = call(
            "crypto_generate_key",
            vec![Value::Str(kind.into())],
            &mut env,
        )
        .expect("crypto_generate_key succeeds") else {
            panic!("crypto_generate_key returns a String")
        };
        let message = format!("interop-{kind}");
        let Value::Record(fields) = call(
            "crypto_sign",
            vec![Value::Str(private_pem.clone()), Value::Str(message.clone())],
            &mut env,
        )
        .expect("crypto_sign succeeds") else {
            panic!("crypto_sign returns a Record")
        };
        assert_eq!(fields["algorithm"], Value::Str(algorithm.into()));
        let signature = STANDARD
            .decode(match &fields["signature"] {
                Value::Str(text) => text.as_str(),
                other => panic!("signature is not a String: {other:?}"),
            })
            .expect("signature is Base64");

        let key_path = scratch.path(&format!("{kind}-key.pem"));
        let public_path = scratch.path(&format!("{kind}-pub.pem"));
        let message_path = scratch.path(&format!("{kind}-msg.txt"));
        let signature_path = scratch.path(&format!("{kind}-sig.bin"));
        std::fs::write(&key_path, private_pem).expect("write key");
        std::fs::write(&message_path, message.as_bytes()).expect("write message");
        std::fs::write(&signature_path, &signature).expect("write signature");

        let out = openssl(&[
            "pkey",
            "-in",
            path_str(&key_path),
            "-pubout",
            "-out",
            path_str(&public_path),
        ]);
        assert_openssl_ok(&out, "openssl pkey -pubout");

        let out = openssl(&[
            "dgst",
            "-sha256",
            "-verify",
            path_str(&public_path),
            "-signature",
            path_str(&signature_path),
            path_str(&message_path),
        ]);
        assert!(
            out.status.success(),
            "openssl dgst rejected the {algorithm} signature: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
