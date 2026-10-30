//! Compatibility with what the RustCrypto-backed builds wrote (IronCrypto move).
//!
//! Every literal here was produced by the pre-IronCrypto crates, `argon2` 0.5.3,
//! `pbkdf2` 0.12.2 and `aes-gcm` 0.10.3, or by `openssl enc` 3.5, and was pasted
//! in as a constant. The generator is not in the repository. A ciphertext or a
//! stored hash written by an older release must keep verifying, and one written
//! now must keep verifying in an older release, so these tests are the contract.
//!
//! Environment variables are process-wide, so each test takes `ENV_LOCK` and
//! points `IRONSHELL_SESSION_DIR` at a scratch directory before building an `Env`.

use std::sync::{Mutex, MutexGuard};

use iron_shell::env::Env;
use iron_shell::value::Value;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Hold the environment for one test, with FIPS and legacy decrypt set as given.
fn configure(fips: bool, legacy_decrypt: bool) -> MutexGuard<'static, ()> {
    let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!("ae-crypto-compat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("scratch session dir");
    std::env::set_var("IRONSHELL_SESSION_DIR", &dir);
    if fips {
        std::env::set_var("IRONSHELL_FIPS", "1");
    } else {
        std::env::remove_var("IRONSHELL_FIPS");
    }
    if legacy_decrypt {
        std::env::set_var("IRONSHELL_CRYPTO_LEGACY_DECRYPT", "1");
    } else {
        std::env::remove_var("IRONSHELL_CRYPTO_LEGACY_DECRYPT");
    }
    guard
}

fn call(name: &str, args: &[&str]) -> anyhow::Result<Value> {
    let mut env = Env::new();
    iron_shell::builtins::call(
        name,
        args.iter().map(|a| Value::Str(a.to_string())).collect(),
        &mut env,
    )
}

fn verify(password: &str, stored: &str) -> bool {
    match call("crypto_password_verify", &[password, stored]) {
        Ok(Value::Bool(b)) => b,
        other => panic!("crypto_password_verify failed: {other:?}"),
    }
}

// --- Stored password hashes -------------------------------------------------

const ARGON2_DEFAULT_1: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU";
const ARGON2_DEFAULT_2: &str = "$argon2id$v=19$m=19456,t=2,p=1$gIGCg4SFhoeIiYqLjI2Ojw$3QMV47HYxBCzJUrQTS6njNOU/5f4GzvVs/qEm0Gojcs";
const ARGON2_CUSTOM: &str = "$argon2id$v=19$m=4096,t=3,p=2$ICEiIyQlJicoKSorLC0uLw$goVf4dWyyeylXc+CzM/YCU3/zX/17l5gSKlKGNhyRlo";
const PBKDF2_1000: &str =
    "$pbkdf2-sha256$i=1000,l=32$QEFCQ0RFRkdISUpLTE1OTw$wjfXzyJ4EYivw62TLSzmP1gcQ08iJ0Ii6aYSMUorh48";

#[test]
fn argon2id_hashes_from_the_old_crate_verify() {
    let _env = configure(false, false);
    assert!(verify("correct horse battery staple", ARGON2_DEFAULT_1));
    assert!(!verify("correct horse battery stapl", ARGON2_DEFAULT_1));
    assert!(verify("pässwörd ✓ 密码", ARGON2_DEFAULT_2));
    assert!(verify("custom-params", ARGON2_CUSTOM));
    assert!(!verify("custom-param", ARGON2_CUSTOM));
}

#[test]
fn pbkdf2_hash_from_the_old_crate_verifies_with_and_without_fips() {
    for fips in [false, true] {
        let _env = configure(fips, false);
        assert!(verify("pbkdf2 fixture", PBKDF2_1000), "fips={fips}");
        assert!(!verify("pbkdf2 fixtur", PBKDF2_1000), "fips={fips}");
    }
}

#[test]
fn stored_pbkdf2_hashes_ironcrypto_would_not_write_still_verify() {
    // IronCrypto's pbkdf2 refuses an 8-byte salt and fewer than 1000 rounds
    // for a new hash; the pbkdf2 crate verified both, so a stored one must
    // still verify. The digests are Python's hashlib.pbkdf2_hmac("sha256").
    const SHORT_SALT: &str =
        "$pbkdf2-sha256$i=1000,l=32$OGJ5dGVzYWw$wT5SxnJh2d8Il6K53xfSLtm99Goy6y6EmnBzWijREgs";
    const FEW_ROUNDS: &str =
        "$pbkdf2-sha256$i=500,l=32$c2l4dGVlbi1ieXRlLXNsdA$jAezz6jY+ORIUsLy7LnFcCr9tUtsRAO2sh2RVdUfRgU";
    let _env = configure(false, false);
    for stored in [SHORT_SALT, FEW_ROUNDS] {
        assert!(verify("old hash", stored), "{stored}");
        assert!(!verify("old hasH", stored), "{stored}");
    }
}

#[test]
fn argon2_hash_is_refused_under_fips_as_before() {
    let _env = configure(true, false);
    assert!(call(
        "crypto_password_verify",
        &["correct horse battery staple", ARGON2_DEFAULT_1]
    )
    .is_err());
}

#[test]
fn new_argon2id_hash_has_the_old_shape_and_verifies() {
    let _env = configure(false, false);
    let stored = match call("crypto_password_hash", &["a new password"]).unwrap() {
        Value::Str(s) => s,
        other => panic!("expected a string, got {other:?}"),
    };
    assert!(
        stored.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"),
        "{stored}"
    );
    // Salt: 16 bytes, unpadded base64 is 22 characters; hash: 32 bytes, 43.
    let fields: Vec<&str> = stored.split('$').collect();
    assert_eq!(fields.len(), 6, "{stored}");
    assert_eq!(fields[4].len(), 22, "{stored}");
    assert_eq!(fields[5].len(), 43, "{stored}");
    assert!(verify("a new password", &stored));
    assert!(!verify("a new passwords", &stored));
}

#[test]
fn new_pbkdf2_hash_under_fips_has_the_old_shape_and_verifies() {
    let _env = configure(true, false);
    let stored = match call("crypto_password_hash", &["a fips password"]).unwrap() {
        Value::Str(s) => s,
        other => panic!("expected a string, got {other:?}"),
    };
    assert!(
        stored.starts_with("$pbkdf2-sha256$i=600000,l=32$"),
        "{stored}"
    );
    assert!(verify("a fips password", &stored));
    assert!(!verify("a fips passwort", &stored));
}

#[test]
fn stored_hash_bounds_are_still_enforced() {
    let _env = configure(false, false);
    // Over the PBKDF2 iteration ceiling: a budget error, not a silent verify.
    let excessive =
        "$pbkdf2-sha256$i=1200001,l=32$c2FsdHNhbHQ$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    assert!(call("crypto_password_verify", &["password", excessive]).is_err());
    // Argon2 with a version other than 1.3 is malformed.
    let v16 = "$argon2id$v=16$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU";
    assert!(call(
        "crypto_password_verify",
        &["correct horse battery staple", v16]
    )
    .is_err());
    // Argon2 over the memory ceiling is a budget error.
    let big = "$argon2id$v=19$m=65537,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU";
    assert!(call(
        "crypto_password_verify",
        &["correct horse battery staple", big]
    )
    .is_err());
}

// --- crypto.encrypt envelopes -----------------------------------------------

const ENV_AE1_PW: &str = "envelope password";
const ENV_AE1: &str = "AE1.oKGio6SlpqeoqaqrrK2urw==.AQIDBAUGBwgJCgsM.eksUYv6mpYJzuooHOB1FUBZJNYS7XJAWmHleG16p2o3CZDUiuGNFVD7aiub680Mcz/PeURQ=";
const ENV_AE1_PLAINTEXT: &str = r#"{"api_key": "abc123", "debug": false}"#;
const ENV_AE1F_PW: &str = "fips envelope";
const ENV_AE1F: &str =
    "AE1F.wMHCw8TFxsfIycrLzM3Ozw==.DAsKCQgHBgUEAwIB.Y8XQBXtgBUkHWWQQnTwSuEFEltK/WmljRFkP8UeN";
const ENV_AE1F_PLAINTEXT: &str = "fips plaintext";

fn decrypt(data: &str, password: &str) -> anyhow::Result<String> {
    match call("crypto_decrypt", &[data, password])? {
        Value::Str(s) => Ok(s),
        other => panic!("expected a string, got {other:?}"),
    }
}

#[test]
fn ae1_envelope_from_the_old_build_decrypts() {
    let _env = configure(false, false);
    assert_eq!(decrypt(ENV_AE1, ENV_AE1_PW).unwrap(), ENV_AE1_PLAINTEXT);
    assert!(decrypt(ENV_AE1, "envelope passwore").is_err());
}

#[test]
fn ae1f_envelope_from_the_old_build_decrypts_with_and_without_fips() {
    for fips in [false, true] {
        let _env = configure(fips, false);
        assert_eq!(
            decrypt(ENV_AE1F, ENV_AE1F_PW).unwrap(),
            ENV_AE1F_PLAINTEXT,
            "fips={fips}"
        );
    }
}

#[test]
fn a_modified_envelope_is_rejected_not_decrypted() {
    let _env = configure(false, false);
    // Flip the first character of the ciphertext field.
    let mut parts: Vec<String> = ENV_AE1.split('.').map(str::to_string).collect();
    let body = parts[3].clone();
    let flipped = if body.starts_with('A') { 'B' } else { 'A' };
    parts[3] = format!("{flipped}{}", &body[1..]);
    let tampered = parts.join(".");
    let err = decrypt(&tampered, ENV_AE1_PW).unwrap_err().to_string();
    assert!(err.contains("authentication failed"), "{err}");
}

#[test]
fn new_envelopes_round_trip_in_both_modes() {
    for fips in [false, true] {
        let _env = configure(fips, false);
        let sealed = match call(
            "crypto_encrypt",
            &["round trip body", "round trip password"],
        )
        .unwrap()
        {
            Value::Str(s) => s,
            other => panic!("expected a string, got {other:?}"),
        };
        let expected_tag = if fips { "AE1F." } else { "AE1." };
        assert!(sealed.starts_with(expected_tag), "{sealed}");
        assert_eq!(sealed.split('.').count(), 4, "{sealed}");
        assert_eq!(
            decrypt(&sealed, "round trip password").unwrap(),
            "round trip body"
        );
    }
}

// --- AES-256-GCM against the RustCrypto output -------------------------------

#[test]
fn aes_256_gcm_with_aad_matches_the_old_crate_vector() {
    use ironcrypto::core_types::traits::Aead;
    let key =
        hex::decode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f").unwrap();
    let nonce = hex::decode("deadbeef0001020304050607").unwrap();
    let plaintext = b"raw gcm plaintext spanning more than one AES block";
    let expected = "957691e40d36e14a88b1e7960851ebde140b266b01066b0ab1325099341c8185e246075b3a926b4b0a68546d7794864cacbbf12a1f15d68d3400390a8044b11bd1eb";

    let cipher = ironcrypto::cipher::Aes256Gcm::new(&key).unwrap();
    let mut buf = plaintext.to_vec();
    let mut tag = [0u8; 16];
    cipher
        .seal_detached(&nonce, b"raw-gcm-aad", &mut buf, &mut tag)
        .unwrap();
    buf.extend_from_slice(&tag);
    assert_eq!(hex::encode(&buf), expected);

    // And the same bytes open, with the same AAD, and not with other AAD.
    let (ct, tag) = buf.split_at(buf.len() - 16);
    let mut opened = ct.to_vec();
    cipher
        .open_detached(&nonce, b"raw-gcm-aad", &mut opened, tag)
        .unwrap();
    assert_eq!(opened, plaintext);
    let mut wrong_aad = ct.to_vec();
    assert!(cipher
        .open_detached(&nonce, b"raw-gcm-AAD", &mut wrong_aad, tag)
        .is_err());
}

// --- Legacy CBC (IRONSHELL_CRYPTO_LEGACY_DECRYPT=1) -----------------------------

/// `openssl enc -aes-256-cbc -pbkdf2 -base64 -pass pass:legacy-pass`, 3.5.9,
/// wrapped at 64 characters as OpenSSL writes it.
const LEGACY_CIPHER: &str = "U2FsdGVkX190zeVwk1Sl96U/0p39thwVslDGDf0LepbMl7UWUj9sfye8lU0Mhywt\n3AnwYQ79mFD1LFVOsvuPmCiH4V4RwaMJfrFLb7Uz4MpgUgMY4n6XAgUQYZJ+76Cn\n";
const LEGACY_PASSWORD: &str = "legacy-pass";
const LEGACY_PLAINTEXT: &str =
    "legacy plaintext: the quick brown fox, jumps over 13 lazy dogs.\nsecond line\n";

#[test]
fn legacy_openssl_cbc_decrypts_natively_when_opted_in() {
    let _env = configure(false, true);
    assert_eq!(
        decrypt(LEGACY_CIPHER, LEGACY_PASSWORD).unwrap(),
        LEGACY_PLAINTEXT
    );
    // The same bytes with whitespace stripped, as a one-line ciphertext.
    let one_line: String = LEGACY_CIPHER.split_whitespace().collect();
    assert_eq!(
        decrypt(&one_line, LEGACY_PASSWORD).unwrap(),
        LEGACY_PLAINTEXT
    );
}

#[test]
fn legacy_wrong_password_is_refused() {
    let _env = configure(false, true);
    let err = decrypt(LEGACY_CIPHER, "legacy-passwor")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("legacy (unauthenticated) decrypt failed"),
        "{err}"
    );
}

#[test]
fn legacy_stays_off_without_the_opt_in() {
    let _env = configure(false, false);
    let err = decrypt(LEGACY_CIPHER, LEGACY_PASSWORD)
        .unwrap_err()
        .to_string();
    assert!(err.contains("IRONSHELL_CRYPTO_LEGACY_DECRYPT"), "{err}");
}
