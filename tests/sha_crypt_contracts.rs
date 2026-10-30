//! SHA-crypt ($5$ / $6$) verification through `crypto_password_verify`.
//! Vectors are from Ulrich Drepper's "Unix crypt using SHA-256 and SHA-512"
//! (the sha256crypt.c and sha512crypt.c test tables).
use iron_shell::{builtins::call, env::Env, value::Value};
use std::sync::{Mutex, MutexGuard};

static LOCK: Mutex<()> = Mutex::new(());

/// Locks the shared test mutex, recovering from poisoning so one failure does not cascade.
fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn s(text: &str) -> Value {
    Value::Str(text.into())
}

fn verify_result(password: &str, hash: &str) -> anyhow::Result<Value> {
    let mut env = Env::default();
    call(
        "crypto_password_verify",
        vec![s(password), s(hash)],
        &mut env,
    )
}

fn expect(password: &str, hash: &str, expected: bool) {
    match verify_result(password, hash) {
        Ok(Value::Bool(actual)) => assert_eq!(actual, expected, "{hash}"),
        other => panic!("{hash}: expected Bool, got {other:?}"),
    }
}

fn code(password: &str, hash: &str, expected: &str) {
    let err = verify_result(password, hash).unwrap_err();
    assert!(err.to_string().contains(expected), "{hash}: {err}");
}

/// Runs `body` with `IRONSHELL_FIPS` set to `value`, restoring the previous value.
fn with_fips<T>(value: Option<&str>, body: impl FnOnce() -> T) -> T {
    let previous = std::env::var_os("IRONSHELL_FIPS");
    match value {
        Some(v) => std::env::set_var("IRONSHELL_FIPS", v),
        None => std::env::remove_var("IRONSHELL_FIPS"),
    }
    let out = body();
    match previous {
        Some(v) => std::env::set_var("IRONSHELL_FIPS", v),
        None => std::env::remove_var("IRONSHELL_FIPS"),
    }
    out
}

const SHA512_HELLO: &str = "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1";
const SHA256_HELLO: &str = "$5$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5";

// ---- SHA-512 ($6$) published vectors ----

#[test]
fn sha512_published_vectors_verify() {
    let _lock = lock();
    with_fips(None, || {
        expect("Hello world!", SHA512_HELLO, true);
        expect(
            "Hello world!",
            "$6$rounds=10000$saltstringsaltst$OW1/O6BYHV6BcXZu8QVeXbDWra3Oeqh0sbHbbMCVNSnCM/UrjmM0Dp8vOuZeHBy/YTBmSK6H9qs/y3RnOaw5v.",
            true,
        );
        expect(
            "This is just a test",
            "$6$rounds=5000$toolongsaltstrin$lQ8jolhgVRVhY4b5pZKaysCLi0QBxGoNeKQzQ3glMhwllF7oGDZxUhx1yxdYcz/e1JSbq3y6JMxxl8audkUEm0",
            true,
        );
        expect(
            "a very much longer text to encrypt.  This one even stretches over morethan one line.",
            "$6$rounds=1400$anotherlongsalts$POfYwTEok97VWcjxIiSOjiykti.o/pQs.wPvMxQ6Fm7I6IoYN3CmLs66x9t0oSwbtEW7o7UmJEiDwGqd8p4ur1",
            true,
        );
        expect(
            "we have a short salt string but not a short password",
            "$6$rounds=77777$short$WuQyW2YR.hBNpjjRhpYD/ifIw05xdfeEyQoMxIXbkvr0gge1a1x3yRULJ5CCaUeOxFmtlcGZelFl5CxtgfiAc0",
            true,
        );
        expect(
            "a short string",
            "$6$rounds=123456$asaltof16chars..$BtCwjqMJGx5hrJhZywWvt0RLE8uZ4oPwcelCjmw2kSYu.Ec6ycULevoBK25fs2xXgMNrCzIMVcgEJAstJeonj1",
            true,
        );
        // rounds=10 is clamped to 1000; the published output records rounds=1000.
        expect(
            "the minimum number is still observed",
            "$6$rounds=1000$roundstoolow$kUMsbe306n21p9R.FRkW3IGn.S9NPN0x50YhH1xhLsPuWGsUSklZt58jaTfF4ZEQpyUNGc0dqbpBYYBaHHrsX.",
            true,
        );
        // The setting as written (rounds=10) verifies the same digest, since it is clamped.
        expect(
            "the minimum number is still observed",
            "$6$rounds=10$roundstoolow$kUMsbe306n21p9R.FRkW3IGn.S9NPN0x50YhH1xhLsPuWGsUSklZt58jaTfF4ZEQpyUNGc0dqbpBYYBaHHrsX.",
            true,
        );
    });
}

#[test]
fn sha512_cross_checked_against_openssl() {
    // Generated with: openssl passwd -6 -salt saltsalt hello
    // (OpenSSL 3.5.7, default 5000 rounds).
    let _lock = lock();
    with_fips(None, || {
        let hash = "$6$saltsalt$YTeBOLnmm3CoeJTzBuijUEtOEWCqrw/nQ8/AeMmON4LGp3k0ZiMjC1OdqzWFUAfDQBvYkOch9BWtQTBcZ1E/p0";
        expect("hello", hash, true);
        expect("hellO", hash, false);
    });
}

// ---- SHA-256 ($5$) published vectors ----

#[test]
fn sha256_published_vectors_verify() {
    let _lock = lock();
    with_fips(None, || {
        expect("Hello world!", SHA256_HELLO, true);
        expect(
            "Hello world!",
            "$5$rounds=10000$saltstringsaltst$3xv.VbSHBb41AL9AvLeujZkZRBAwqFMz2.opqey6IcA",
            true,
        );
        expect(
            "This is just a test",
            "$5$rounds=5000$toolongsaltstrin$Un/5jzAHMgOGZ5.mWJpuVolil07guHPvOW8mGRcvxa5",
            true,
        );
        expect(
            "a very much longer text to encrypt.  This one even stretches over morethan one line.",
            "$5$rounds=1400$anotherlongsalts$Rx.j8H.h8HjEDGomFU8bDkXm3XIUnzyxf12oP84Bnq1",
            true,
        );
        expect(
            "we have a short salt string but not a short password",
            "$5$rounds=77777$short$JiO1O3ZpDAxGJeaDIuqCoEFysAe1mZNJRs3pw0KQRd/",
            true,
        );
        expect(
            "a short string",
            "$5$rounds=123456$asaltof16chars..$gP3VQ/6X7UUEW3HkBn2w1/Ptq2jxPyzV/cZKmF/wJvD",
            true,
        );
        expect(
            "the minimum number is still observed",
            "$5$rounds=1000$roundstoolow$yfvwcWrQ8l/K0DAWyuPMDNHpIVlTQebY9l/gL972bIC",
            true,
        );
        expect(
            "the minimum number is still observed",
            "$5$rounds=10$roundstoolow$yfvwcWrQ8l/K0DAWyuPMDNHpIVlTQebY9l/gL972bIC",
            true,
        );
    });
}

// ---- Behaviour ----

#[test]
fn wrong_password_is_false_not_an_error() {
    let _lock = lock();
    with_fips(None, || {
        expect("Hello world?", SHA512_HELLO, false);
        expect("Hello world?", SHA256_HELLO, false);
        expect("", SHA512_HELLO, false);
        expect("Hello world!", SHA256_HELLO, true);
    });
}

#[test]
fn sha_crypt_is_refused_when_fips_is_on() {
    // SHA-crypt uses SHA-2 but is not an approved password-hashing construction;
    // under IRONSHELL_FIPS only PBKDF2 is, as with the legacy hex and Argon2 forms.
    // The refusal does not depend on whether the password matches.
    let _lock = lock();
    with_fips(Some("1"), || {
        code("Hello world!", SHA512_HELLO, "E_POLICY_DENY");
        code("Hello world!", SHA256_HELLO, "E_POLICY_DENY");
        code("Hello world?", SHA512_HELLO, "E_POLICY_DENY");
    });
}

#[test]
fn malformed_sha_crypt_hashes_are_coded_bad_arg() {
    let _lock = lock();
    with_fips(None, || {
        let good6 = &SHA512_HELLO["$6$saltstring$".len()..];
        let good5 = &SHA256_HELLO["$5$saltstring$".len()..];
        let cases = [
            "$6$".to_string(),
            "$6$saltstring".to_string(),
            "$6$saltstring$".to_string(),
            format!("$6$${good6}"),
            format!("$6$bad!salt${good6}"),
            format!("$6$saltstring${}", &good6[..85]),
            format!("$6${good6}0"),
            format!("$6$saltstring${}!", &good6[..85]),
            format!("$6$saltstring${good5}"),
            format!("$5$saltstring${good6}"),
            format!("$5$saltstring${}", &good5[..42]),
            format!("$6$rounds=$saltstring${good6}"),
            format!("$6$rounds=abc$saltstring${good6}"),
            format!("$6$rounds=5000{good6}"),
            format!("$6$rounds=-5000$saltstring${good6}"),
            format!("$6$rounds=5000${good6}"),
            format!("$7$saltstring${good6}"),
        ];
        for hash in cases {
            code("Hello world!", &hash, "E_BAD_ARG");
        }
    });
}

#[test]
fn excessive_rounds_are_refused_before_hashing() {
    let _lock = lock();
    with_fips(None, || {
        let digest = &SHA512_HELLO["$6$saltstring$".len()..];
        code(
            "Hello world!",
            &format!("$6$rounds=2000000$saltstring${digest}"),
            "E_BUDGET_EXCEEDED",
        );
        code(
            "Hello world!",
            "$5$rounds=1000001$saltstring$5B8vYYiY.CVt1RlTTf8KbXBH3hsxY/GNooZaBBGWEc5",
            "E_BUDGET_EXCEEDED",
        );
        code(
            "Hello world!",
            &format!("$6$rounds=99999999999999999999999$saltstring${digest}"),
            "E_BUDGET_EXCEEDED",
        );
    });
}

#[test]
fn phc_argon2_path_is_unchanged() {
    let _lock = lock();
    with_fips(None, || {
        let mut env = Env::default();
        let hash = call("crypto_password_hash", vec![s("example")], &mut env).unwrap();
        assert!(matches!(
            call(
                "crypto_password_verify",
                vec![s("example"), hash.clone()],
                &mut env
            )
            .unwrap(),
            Value::Bool(true)
        ));
        assert!(matches!(
            call("crypto_password_verify", vec![s("nope"), hash], &mut env).unwrap(),
            Value::Bool(false)
        ));
    });
}
