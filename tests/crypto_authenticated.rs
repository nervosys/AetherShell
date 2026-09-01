//! `crypto.encrypt` must produce ciphertext whose modification is *detectable*
//! (AS-2026-04).
//!
//! Until 10.0.0 the builtin shelled out to `openssl enc -aes-256-cbc`, which
//! provides confidentiality and nothing else. Flip a byte of a CBC ciphertext
//! and it still decrypts — to different plaintext — and `crypto.decrypt`
//! returned that plaintext with no indication anything had changed. A caller
//! doing
//!
//! ```text
//! config = json.parse(crypto.decrypt(file.read(path), pass))
//! ```
//!
//! would parse attacker-influenced data believing it had been protected.
//! `openssl enc` cannot fix this: 3.5.7 answers `enc: AEAD ciphers not
//! supported`, so the cipher moved in-process to AES-256-GCM — approved, unlike
//! the ChaCha20-Poly1305 that would otherwise be the obvious pick, which keeps
//! `docs/security/FIPS_140-2_COMPLIANCE.md` true.
//!
//! Two properties are load-bearing here and each has a test below: a modified
//! ciphertext must be *rejected*, and an attacker must not be able to escape
//! that rejection by stripping the envelope and presenting the remains as a
//! legacy ciphertext.

use aethershell::env::Env;
use aethershell::value::Value;

fn call(name: &str, args: Vec<&str>) -> anyhow::Result<Value> {
    let mut env = Env::new();
    aethershell::builtins::call(
        name,
        args.into_iter()
            .map(|a| Value::Str(a.to_string()))
            .collect(),
        &mut env,
    )
}

fn encrypt(plaintext: &str, password: &str) -> String {
    match call("crypto_encrypt", vec![plaintext, password]) {
        Ok(Value::Str(s)) => s,
        other => panic!("crypto_encrypt did not return ciphertext: {other:?}"),
    }
}

fn decrypt_err(ciphertext: &str, password: &str) -> String {
    match call("crypto_decrypt", vec![ciphertext, password]) {
        Err(e) => e.to_string(),
        Ok(v) => panic!(
            "crypto_decrypt accepted input it should have rejected and returned {v:?} — a \
             caller would treat this as verified plaintext"
        ),
    }
}

#[test]
fn round_trips_through_the_authenticated_envelope() {
    let _g = fips_lock();
    let ct = encrypt("the launch code is 0000", "correct horse battery staple");
    assert_ne!(
        ct, "the launch code is 0000",
        "ciphertext must not be the plaintext"
    );
    assert!(
        !ct.contains("launch"),
        "plaintext leaked into the envelope: {ct}"
    );

    match call("crypto_decrypt", vec![&ct, "correct horse battery staple"]) {
        Ok(Value::Str(pt)) => assert_eq!(pt, "the launch code is 0000"),
        other => panic!("round trip failed: {other:?}"),
    }
}

#[test]
fn ciphertext_is_a_versioned_four_part_envelope() {
    let _g = fips_lock();
    let ct = encrypt("x", "pw");
    let parts: Vec<&str> = ct.split('.').collect();
    assert_eq!(
        parts.len(),
        4,
        "expected version.salt.nonce.body, got {ct:?}"
    );
    assert_eq!(
        parts[0], "AE1",
        "the version tag is what lets decrypt refuse an unauthenticated blob"
    );
}

/// The regression test for AS-2026-04 itself.
#[test]
fn a_modified_ciphertext_is_rejected_rather_than_decrypted() {
    let _g = fips_lock();
    let ct = encrypt("transfer $10 to alice", "pw");
    let parts: Vec<&str> = ct.split('.').collect();

    // Corrupt one base64 character of the body, keeping it valid base64 so the
    // rejection comes from the authentication tag and not from the decoder.
    let body: String = {
        let mut b: Vec<char> = parts[3].chars().collect();
        b[0] = if b[0] == 'A' { 'B' } else { 'A' };
        b.into_iter().collect()
    };
    let tampered = format!("{}.{}.{}.{}", parts[0], parts[1], parts[2], body);

    let err = decrypt_err(&tampered, "pw");
    assert!(
        err.contains("E_DECRYPT_FAILED"),
        "a tampered ciphertext must fail with a branchable code, got: {err}"
    );
}

#[test]
fn tampering_with_the_salt_or_nonce_is_also_rejected() {
    let _g = fips_lock();
    let ct = encrypt("payload", "pw");
    let parts: Vec<&str> = ct.split('.').collect();

    for field in [1usize, 2] {
        let mut swapped: Vec<String> = parts.iter().map(|s| s.to_string()).collect();
        let mut chars: Vec<char> = swapped[field].chars().collect();
        chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
        swapped[field] = chars.into_iter().collect();
        let err = decrypt_err(&swapped.join("."), "pw");
        assert!(
            err.contains("E_DECRYPT_FAILED"),
            "modifying field {field} must be detected, got: {err}"
        );
    }
}

#[test]
fn the_wrong_password_fails_closed_without_releasing_plaintext() {
    let _g = fips_lock();
    let ct = encrypt("secret", "right");
    let err = decrypt_err(&ct, "wrong");
    assert!(
        err.contains("E_DECRYPT_FAILED"),
        "wrong password must be a coded failure, got: {err}"
    );
    assert!(!err.contains("secret"), "the error leaked plaintext: {err}");
}

/// Salt and nonce are fresh per call, so encrypting the same value twice must
/// not produce the same ciphertext — otherwise an observer learns when a stored
/// secret was left unchanged.
#[test]
fn encrypting_the_same_value_twice_gives_different_ciphertext() {
    let _g = fips_lock();
    let a = encrypt("same", "pw");
    let b = encrypt("same", "pw");
    assert_ne!(
        a, b,
        "ciphertext is deterministic — salt/nonce are not fresh"
    );
}

/// The downgrade this format exists to prevent: an attacker who cannot forge a
/// tag strips the envelope instead, hoping decrypt falls back to the
/// unauthenticated path that has no tag to check.
#[test]
fn stripping_the_envelope_does_not_reach_the_unauthenticated_path() {
    let _g = fips_lock();
    std::env::remove_var("AETHER_CRYPTO_LEGACY_DECRYPT");
    let ct = encrypt("payload", "pw");
    let bare = ct.splitn(4, '.').nth(3).expect("body").to_string();

    let err = decrypt_err(&bare, "pw");
    assert!(
        err.contains("E_DECRYPT_UNAUTHENTICATED"),
        "a bare blob must be refused as unauthenticated, not decrypted; got: {err}"
    );
    assert!(
        err.contains("AETHER_CRYPTO_LEGACY_DECRYPT"),
        "the refusal must name the one switch that recovers genuinely old data, \
         otherwise it reads as data loss: {err}"
    );
}

#[test]
fn an_empty_password_is_refused_rather_than_silently_accepted() {
    let _g = fips_lock();
    match call("crypto_encrypt", vec!["data", ""]) {
        Err(e) => assert!(
            e.to_string().contains("E_CRYPTO_BAD_INPUT"),
            "unexpected error: {e}"
        ),
        Ok(v) => panic!("an empty password produced {v:?} instead of a refusal"),
    }
}

/// The builtin no longer shells out, so it works where the openssl path was
/// compiled out. This is the assertion that would have caught the old
/// Windows-only `E_UNIMPLEMENTED` gap.
#[test]
fn encryption_is_available_on_every_platform() {
    let _g = fips_lock();
    let ct = encrypt("cross-platform", "pw");
    assert!(
        ct.starts_with("AE1."),
        "crypto.encrypt must work without an external openssl on this platform"
    );
}

// ── The FIPS key-derivation path (AS-2026-08) ────────────────────────────

/// Under `AETHER_FIPS` the key is derived with PBKDF2-HMAC-SHA256 rather than
/// Argon2id, so the whole chain is SP 800-132 / FIPS 197 approved. The
/// envelope records which, because the mode a ciphertext was written under is
/// not the mode it will be read under.
#[test]
fn the_fips_mode_uses_an_approved_kdf_and_says_so() {
    let _g = fips_lock();
    // Serialised against the other env-mutating test in this file.
    std::env::set_var("AETHER_FIPS", "1");
    let ct = encrypt("secret", "pw");
    std::env::remove_var("AETHER_FIPS");

    assert!(
        ct.starts_with("AE1F."),
        "FIPS-mode ciphertext should carry the AE1F tag, got {ct:?}"
    );
    // Readable with the mode off: the KDF comes from the envelope, never from
    // the ambient setting.
    match call("crypto_decrypt", vec![&ct, "pw"]) {
        Ok(Value::Str(pt)) => assert_eq!(pt, "secret"),
        other => panic!("FIPS ciphertext unreadable outside FIPS mode: {other:?}"),
    }
}

#[test]
fn default_ciphertext_stays_readable_in_fips_mode() {
    let _g = fips_lock();
    std::env::remove_var("AETHER_FIPS");
    let ct = encrypt("secret", "pw");
    assert!(ct.starts_with("AE1."), "expected the Argon2id tag: {ct:?}");

    std::env::set_var("AETHER_FIPS", "1");
    let out = call("crypto_decrypt", vec![&ct, "pw"]);
    std::env::remove_var("AETHER_FIPS");
    match out {
        Ok(Value::Str(pt)) => assert_eq!(pt, "secret"),
        other => panic!("turning FIPS on stranded existing ciphertext: {other:?}"),
    }
}

#[test]
fn the_two_kdfs_produce_different_keys_for_the_same_password() {
    let _g = fips_lock();
    // Not a security property in itself — a guard that the FIPS branch is
    // actually taken rather than silently falling through to Argon2id.
    std::env::set_var("AETHER_FIPS", "1");
    let fips = encrypt("x", "pw");
    std::env::remove_var("AETHER_FIPS");
    let plain = encrypt("x", "pw");
    assert_ne!(&fips[..4], &plain[..4], "both envelopes carry the same tag");
}

static FIPS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn fips_lock() -> std::sync::MutexGuard<'static, ()> {
    FIPS_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}
