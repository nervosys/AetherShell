//! Unimplemented cryptographic builtins must fail *closed*.
//!
//! `eval::is_truthy` maps `Value::Str(s)` to `!s.is_empty()` and `Value::Error`
//! to `false`. A stub that returns an explanatory string is therefore
//! indistinguishable from success at a branch:
//!
//! ```text
//! if crypto.verify_signature(sig, data, key) { deploy() }   # taken, always
//! ```
//!
//! Until 2026-07-30 every one of the builtins below did exactly that. The
//! `verify*` ones granted trust with nothing verified (CWE-347); `encrypt`
//! returned the sentence "Encryption requires OpenSSL" where the caller
//! expected ciphertext, so writing the result to disk stored plaintext-adjacent
//! prose rather than an encrypted secret (CWE-311).
//!
//! Most of these builtins shell out to the `openssl` CLI under `#[cfg(unix)]`
//! only, so on Windows the stub path is the *only* path — which is where this
//! test runs in CI as well as on Unix, where `openssl` may be absent or may
//! fail. `crypto_encrypt`/`crypto_decrypt` are the exception since 10.0.0: they
//! are in-process AES-256-GCM and behave identically on every platform.

use aethershell::value::Value;

/// Call a builtin and return the raw `Result`, so the error case is observable
/// rather than unwrapped away.
fn try_call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = aethershell::env::Env::new();
    aethershell::builtins::call(name, args, &mut env)
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

/// The property that actually matters: whatever these return, a script must not
/// be able to mistake it for success. Either an `Err`, or a falsy value — never
/// a truthy one.
fn assert_not_mistakable_for_success(name: &str, args: Vec<Value>) {
    match try_call(name, args) {
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("E_UNIMPLEMENTED"),
                "{name} failed, but not with the E_UNIMPLEMENTED code the shell's \
                 error convention expects: {msg}"
            );
        }
        // A real implementation (openssl present on Unix) is free to answer
        // Bool(false)/Null — both falsy, so a gate on them denies. Bool(true) is
        // only legitimate from a genuine verification, which cannot happen for
        // the bogus inputs used here.
        Ok(Value::Bool(false)) | Ok(Value::Null) => {}
        Ok(other) => panic!(
            "{name} returned {other:?}, which is truthy — a caller writing \
             `if {name}(...)` would take the trusted branch with nothing verified"
        ),
    }
}

// Note the registered names: the dispatch table knows `crypto_verify_signature`
// and `crypto_cert_verify`. `crypto.verify` — which modules.rs advertises — maps
// to `crypto_verify`, which no builtin implements; see the dangling-alias test
// in tests/module_aliases.rs.
#[test]
fn signature_verification_never_reports_success_when_unimplemented() {
    assert_not_mistakable_for_success(
        "crypto_verify_signature",
        vec![s("c2lnbmF0dXJl"), s("payload"), s("key")],
    );
}

#[test]
fn certificate_checks_never_report_success_when_unimplemented() {
    assert_not_mistakable_for_success("crypto_cert_verify", vec![s("/nonexistent/cert.pem")]);
    assert_not_mistakable_for_success("crypto_cert_info", vec![s("/nonexistent/cert.pem")]);
}

/// `encrypt` is not a gate, so the hazard is different: the caller wants
/// ciphertext and must not receive prose. Assert it does not hand back a value
/// that merely *looks* like output.
#[test]
fn encrypt_and_decrypt_do_not_return_prose_in_place_of_ciphertext() {
    for name in ["crypto_encrypt", "crypto_decrypt"] {
        match try_call(name, vec![s("secret data"), s("password")]) {
            // Two failures are legitimate here and they are platform-dependent,
            // which is why this assertion used to be wrong on Unix only:
            //
            //   * no openssl on the host          -> E_UNIMPLEMENTED
            //   * openssl present and it refused  -> E_DECRYPT_FAILED
            //
            // Since 10.0.0 there is a third, and it is now the one this call
            // actually produces: the input here is plaintext, so it is not an
            // authenticated `AE1.` envelope, and `crypto_decrypt` refuses it as
            // E_DECRYPT_UNAUTHENTICATED rather than falling back to the
            // unauthenticated CBC path — that fallback would be a downgrade
            // (AS-2026-04). `crypto_encrypt` no longer fails at all here; it is
            // in-process and cross-platform, so it returns real ciphertext,
            // which the Ok arm below checks is not prose.
            //
            // What the test is really for is unchanged: the failure must be a
            // branchable coded error, never prose handed back as data.
            Err(e) => {
                let text = e.to_string();
                assert!(
                    text.contains("E_UNIMPLEMENTED")
                        || text.contains("E_DECRYPT_FAILED")
                        || text.contains("E_DECRYPT_UNAUTHENTICATED"),
                    "{name}: failure must carry a recognisable code, got: {e}"
                );
            }
            Ok(Value::Str(out)) => {
                // A real openssl run is fine; the failure mode is the sentinel
                // message leaking through as though it were data.
                let lower = out.to_ascii_lowercase();
                assert!(
                    !lower.contains("requires") && !lower.contains("openssl"),
                    "{name} returned an explanatory message as if it were \
                     ciphertext, which a caller would persist verbatim: {out:?}"
                );
            }
            Ok(Value::Null) => {}
            Ok(other) => panic!("{name} returned an unexpected shape: {other:?}"),
        }
    }
}

#[test]
fn signing_does_not_return_prose_in_place_of_a_signature() {
    match try_call("crypto_sign", vec![s("payload"), s("key")]) {
        Err(e) => assert!(
            e.to_string().contains("E_UNIMPLEMENTED"),
            "crypto_sign: unexpected error: {e}"
        ),
        Ok(Value::Str(out)) => {
            let lower = out.to_ascii_lowercase();
            assert!(
                !lower.contains("require") && !lower.contains("openssl directly"),
                "crypto_sign returned prose where a signature belongs: {out:?}"
            );
        }
        Ok(Value::Null) => {}
        Ok(other) => panic!("crypto_sign returned an unexpected shape: {other:?}"),
    }
}
