//! SHA-crypt verification (`$5$` = SHA-256, `$6$` = SHA-512) per Ulrich Drepper's
//! "Unix crypt using SHA-256 and SHA-512" specification.
//!
//! Verification only: this module never generates a hash. Hash format:
//! `$6$[rounds=N$]salt$digest`. The salt is at most 16 characters (longer salts
//! are truncated to the first 16), the rounds default to 5000 and are clamped to
//! 1000..=999999999 as the specification requires. To bound CPU, a stored
//! `rounds=` above [`ROUNDS_REFUSE_ABOVE`] (1,000,000) is refused with
//! `E_BUDGET_EXCEEDED` before any hashing happens. Malformed hashes (bad
//! alphabet, missing fields, wrong digest length) are `E_BAD_ARG`. A wrong
//! password is `Ok(false)`, not an error.

use anyhow::Result;
use sha2::{Digest, Sha256, Sha512};

/// The crypt alphabet, `./0-9A-Za-z`, in the order the encoding indexes it.
const ALPHABET: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const SALT_MAX: usize = 16;
const ROUNDS_DEFAULT: u64 = 5000;
const ROUNDS_MIN: u64 = 1000;
const ROUNDS_MAX: u64 = 999_999_999;
/// Verification refuses any stored `rounds=` above this, to bound CPU.
const ROUNDS_REFUSE_ABOVE: u64 = 1_000_000;

/// Marker index for the zero padding that the specification's encoding adds.
const ZERO: usize = usize::MAX;

/// Each entry is `b64_from_24bit(B2, B1, B0, N)` from the specification, as
/// `(B2, B1, B0, N)` byte indices into the digest (or `ZERO`).
const SHA512_ORDER: [(usize, usize, usize, usize); 22] = [
    (0, 21, 42, 4),
    (22, 43, 1, 4),
    (44, 2, 23, 4),
    (3, 24, 45, 4),
    (25, 46, 4, 4),
    (47, 5, 26, 4),
    (6, 27, 48, 4),
    (28, 49, 7, 4),
    (50, 8, 29, 4),
    (9, 30, 51, 4),
    (31, 52, 10, 4),
    (53, 11, 32, 4),
    (12, 33, 54, 4),
    (34, 55, 13, 4),
    (56, 14, 35, 4),
    (15, 36, 57, 4),
    (37, 58, 16, 4),
    (59, 17, 38, 4),
    (18, 39, 60, 4),
    (40, 61, 19, 4),
    (62, 20, 41, 4),
    (ZERO, ZERO, 63, 2),
];

const SHA256_ORDER: [(usize, usize, usize, usize); 11] = [
    (0, 10, 20, 4),
    (21, 1, 11, 4),
    (12, 22, 2, 4),
    (3, 13, 23, 4),
    (24, 4, 14, 4),
    (15, 25, 5, 4),
    (6, 16, 26, 4),
    (27, 7, 17, 4),
    (18, 28, 8, 4),
    (9, 19, 29, 4),
    (ZERO, 31, 30, 3),
];

#[derive(Clone, Copy)]
enum Variant {
    Sha256,
    Sha512,
}

impl Variant {
    fn digest_len(self) -> usize {
        match self {
            Variant::Sha256 => 32,
            Variant::Sha512 => 64,
        }
    }

    /// Length of the encoded digest field: 43 characters for SHA-256 and 86 for SHA-512.
    fn encoded_len(self) -> usize {
        match self {
            Variant::Sha256 => 43,
            Variant::Sha512 => 86,
        }
    }
}

struct Parsed<'a> {
    variant: Variant,
    rounds: u64,
    salt: &'a [u8],
    digest: &'a str,
}

fn malformed(name: &str) -> anyhow::Error {
    crate::safety::bad_arg(
        name,
        "SHA-crypt hash ($5$ or $6$) with a salt and a complete digest",
        "invalid stored hash",
    )
}

fn in_alphabet(text: &str) -> bool {
    text.bytes().all(|b| ALPHABET.contains(&b))
}

fn parse<'a>(name: &str, hash: &'a str) -> Result<Parsed<'a>> {
    let (variant, rest) = if let Some(rest) = hash.strip_prefix("$6$") {
        (Variant::Sha512, rest)
    } else if let Some(rest) = hash.strip_prefix("$5$") {
        (Variant::Sha256, rest)
    } else {
        return Err(malformed(name));
    };
    let (rounds, rest) = match rest.strip_prefix("rounds=") {
        Some(after) => {
            let end = after.find('$').ok_or_else(|| malformed(name))?;
            let digits = &after[..end];
            if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(malformed(name));
            }
            // A value too large for u64 is certainly above the refusal bound.
            let requested = digits.parse::<u64>().unwrap_or(u64::MAX);
            if requested > ROUNDS_REFUSE_ABOVE {
                return Err(crate::safety::budget_exceeded(
                    name,
                    format!("stored SHA-crypt rounds={requested} exceed the verification limit"),
                    "maximum rounds=1000000; rehash the password with crypto_password_hash",
                ));
            }
            (requested.clamp(ROUNDS_MIN, ROUNDS_MAX), &after[end + 1..])
        }
        None => (ROUNDS_DEFAULT, rest),
    };
    let end = rest.find('$').ok_or_else(|| malformed(name))?;
    let salt_field = &rest[..end];
    if salt_field.is_empty() || !in_alphabet(salt_field) {
        return Err(malformed(name));
    }
    let digest = &rest[end + 1..];
    if digest.len() != variant.encoded_len() || !in_alphabet(digest) {
        return Err(malformed(name));
    }
    Ok(Parsed {
        variant,
        rounds,
        salt: &salt_field.as_bytes()[..salt_field.len().min(SALT_MAX)],
        digest,
    })
}

/// Repeats `source` cyclically to exactly `len` bytes. This is the specification's
/// "full copies of the digest, then the first `len % DS` bytes" construction.
fn repeat_to(source: &[u8], len: usize) -> Vec<u8> {
    source.iter().copied().cycle().take(len).collect()
}

/// The specification's SHA-crypt core, generic over SHA-256 or SHA-512.
fn sha_crypt_digest<D: Digest>(key: &[u8], salt: &[u8], rounds: u64) -> Vec<u8> {
    // Step 1: B = H(key || salt || key).
    let mut h = D::new();
    h.update(key);
    h.update(salt);
    h.update(key);
    let b = h.finalize().to_vec();
    let ds = b.len();

    // Step 2: A = H(key || salt || B-bytes for len(key), then bits of len(key)).
    let mut a = D::new();
    a.update(key);
    a.update(salt);
    let mut cnt = key.len();
    while cnt > ds {
        a.update(&b);
        cnt -= ds;
    }
    a.update(&b[..cnt]);
    let mut cnt = key.len();
    while cnt > 0 {
        if cnt & 1 == 1 {
            a.update(&b);
        } else {
            a.update(key);
        }
        cnt >>= 1;
    }
    let a = a.finalize().to_vec();

    // Step 3: P = repeat(H(key repeated len(key) times), len(key)).
    let mut h = D::new();
    for _ in 0..key.len() {
        h.update(key);
    }
    let p = repeat_to(&h.finalize(), key.len());

    // Step 4: S = repeat(H(salt repeated 16 + A[0] times), len(salt)).
    let mut h = D::new();
    for _ in 0..(16 + usize::from(a[0])) {
        h.update(salt);
    }
    let s = repeat_to(&h.finalize(), salt.len());

    // Step 5: the rounds.
    let mut c = a;
    for i in 0..rounds {
        let mut h = D::new();
        if i % 2 == 1 {
            h.update(&p);
        } else {
            h.update(&c);
        }
        if i % 3 != 0 {
            h.update(&s);
        }
        if i % 7 != 0 {
            h.update(&p);
        }
        if i % 2 == 1 {
            h.update(&c);
        } else {
            h.update(&p);
        }
        c = h.finalize().to_vec();
    }
    c
}

/// Encodes a digest with the specification's per-variant byte permutation.
fn encode(bytes: &[u8], order: &[(usize, usize, usize, usize)]) -> String {
    let at = |index: usize| {
        if index == ZERO {
            0u32
        } else {
            u32::from(bytes[index])
        }
    };
    let mut out = String::new();
    for &(b2, b1, b0, n) in order {
        let mut w = (at(b2) << 16) | (at(b1) << 8) | at(b0);
        for _ in 0..n {
            out.push(char::from(ALPHABET[(w & 0x3f) as usize]));
            w >>= 6;
        }
    }
    out
}

/// Compares two byte strings without an early exit on the first difference.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Verifies `password` against a `$5$` or `$6$` SHA-crypt `hash`. `name` is the
/// calling builtin, used in coded errors. Returns `Ok(false)` on a mismatch and
/// `E_BAD_ARG` for malformed hashes or `E_BUDGET_EXCEEDED` for `rounds=` above
/// 1,000,000.
pub(super) fn verify(name: &str, password: &str, hash: &str) -> Result<bool> {
    let parsed = parse(name, hash)?;
    let key = password.as_bytes();
    let (bytes, order): (Vec<u8>, &[(usize, usize, usize, usize)]) = match parsed.variant {
        Variant::Sha256 => (
            sha_crypt_digest::<Sha256>(key, parsed.salt, parsed.rounds),
            &SHA256_ORDER,
        ),
        Variant::Sha512 => (
            sha_crypt_digest::<Sha512>(key, parsed.salt, parsed.rounds),
            &SHA512_ORDER,
        ),
    };
    debug_assert_eq!(bytes.len(), parsed.variant.digest_len());
    let encoded = encode(&bytes, order);
    Ok(constant_time_eq(
        encoded.as_bytes(),
        parsed.digest.as_bytes(),
    ))
}
