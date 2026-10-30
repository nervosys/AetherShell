//! Password-hash strings, computed by IronCrypto.
//!
//! Until 12.x these came from the RustCrypto `argon2` and `pbkdf2` crates. The
//! strings written here are byte-for-byte what those crates wrote, so a hash
//! made by this build still verifies in an older release and the reverse:
//!
//! * Argon2id, version 1.3, `m=19456,t=2,p=1` (19 MiB, two passes, one lane),
//!   which is `Argon2::default()` and IronCrypto's `Argon2Params::DEFAULT`.
//! * PBKDF2-HMAC-SHA256 with `i=600000,l=32`, the defaults of the `pbkdf2`
//!   crate's `Pbkdf2` hasher.
//!
//! Both encode salt and hash as base64 without padding. The unit tests below
//! hold literals made with the old crates, so a change in either algorithm or
//! encoding fails here rather than in someone's stored database.

use anyhow::Result;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use ironcrypto::kdf::argon2::{phc, Argon2Params, Variant, Version};

/// PBKDF2 iterations for a new password hash under `IRONSHELL_FIPS`: the `pbkdf2`
/// crate's default, and the SP 800-132 recommendation at the time it was chosen.
pub(crate) const PBKDF2_HASH_ROUNDS: u32 = 600_000;

/// Bytes of hash written for PBKDF2 password hashes.
const PBKDF2_HASH_LEN: usize = 32;

/// Argon2id with the default cost, over `salt`, as a PHC string.
pub(crate) fn argon2id_hash(builtin: &str, password: &[u8], salt: &[u8]) -> Result<String> {
    phc::hash_password_with(
        Variant::Argon2id,
        Version::V0x13,
        &Argon2Params::DEFAULT,
        password,
        salt,
        &[],
        &[],
        &[],
        32,
    )
    .map_err(|e| crate::safety::ic_failed(builtin, e))
}

/// PBKDF2-HMAC-SHA256 over `salt` at the default cost, as a PHC string.
pub(crate) fn pbkdf2_sha256_hash(builtin: &str, password: &[u8], salt: &[u8]) -> Result<String> {
    let hash = pbkdf2_sha256_derive(builtin, password, salt, PBKDF2_HASH_ROUNDS, PBKDF2_HASH_LEN)?;
    Ok(format!(
        "$pbkdf2-sha256$i={},l={}${}${}",
        PBKDF2_HASH_ROUNDS,
        PBKDF2_HASH_LEN,
        STANDARD_NO_PAD.encode(salt),
        STANDARD_NO_PAD.encode(&hash)
    ))
}

/// PBKDF2-HMAC-SHA256 with the given cost, `out_len` bytes long.
pub(crate) fn pbkdf2_sha256_derive(
    builtin: &str,
    password: &[u8],
    salt: &[u8],
    rounds: u32,
    out_len: usize,
) -> Result<Vec<u8>> {
    let mut out = vec![0u8; out_len];
    ironcrypto::kdf::pbkdf2::<ironcrypto::mac::HmacSha256>(password, salt, rounds, &mut out)
        .map_err(|e| crate::safety::ic_failed(builtin, e))?;
    Ok(out)
}

/// PBKDF2-HMAC-SHA256 for checking a stored hash. IronCrypto's `pbkdf2`
/// refuses salts under 128 bits and fewer than 1000 rounds, which is right for
/// a new hash, but the `pbkdf2` crate verified such strings and a stored one
/// must still verify. When IronCrypto refuses, RFC 8018's F function over
/// IronCrypto's HMAC-SHA256 computes the same bytes.
pub(crate) fn pbkdf2_sha256_verify_derive(
    builtin: &str,
    password: &[u8],
    salt: &[u8],
    rounds: u32,
    out_len: usize,
) -> Result<Vec<u8>> {
    if let Ok(out) = pbkdf2_sha256_derive(builtin, password, salt, rounds, out_len) {
        return Ok(out);
    }
    let mut out = vec![0u8; out_len];
    pbkdf2_sha256_rfc8018(builtin, password, salt, rounds, &mut out)?;
    Ok(out)
}

/// PBKDF2-HMAC-SHA256 for a salt of any length, from IronCrypto's HMAC.
///
/// IronCrypto's `pbkdf2` refuses salts under 128 bits, and OpenSSL's salt is 64
/// bits, as are some stored PHC salts. The derivation is RFC 8018's F function over the same HMAC, so the
/// primitive is still IronCrypto's.
pub(crate) fn pbkdf2_sha256_rfc8018(
    builtin: &str,
    password: &[u8],
    salt: &[u8],
    iterations: u32,
    out: &mut [u8],
) -> Result<()> {
    use ironcrypto::core_types::traits::Mac as _;
    let mac = |key: &[u8], data: &[u8]| -> Result<Vec<u8>> {
        ironcrypto::mac::HmacSha256::mac(key, data)
            .map(|tag| tag.as_ref().to_vec())
            .map_err(|e| crate::safety::ic_failed(builtin, e))
    };
    for (index, block) in out.chunks_mut(32).enumerate() {
        let mut first_input = salt.to_vec();
        first_input.extend_from_slice(&((index as u32) + 1).to_be_bytes());
        let mut u = mac(password, &first_input)?;
        let mut t = u.clone();
        for _ in 1..iterations {
            u = mac(password, &u)?;
            for (a, b) in t.iter_mut().zip(u.iter()) {
                *a ^= b;
            }
        }
        block.copy_from_slice(&t[..block.len()]);
    }
    Ok(())
}

/// A `$pbkdf2-sha256$` PHC string, parsed but not yet checked against policy.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Pbkdf2Phc {
    /// The `i=` field, if the string carries one.
    pub rounds: Option<u32>,
    /// The `l=` field, if the string carries one.
    pub output_length: Option<usize>,
    pub salt: Vec<u8>,
    pub hash: Vec<u8>,
}

/// Parse a `$pbkdf2-sha256$` PHC string the way the `pbkdf2` crate reads one:
/// `i` and `l` in either order, each at most once, decimals without leading
/// zeros, and a stored hash whose length agrees with `l`. Base64 must be the
/// canonical unpadded form. `None` for anything else.
pub(crate) fn parse_pbkdf2_sha256(stored: &str) -> Option<Pbkdf2Phc> {
    let rest = stored.strip_prefix("$pbkdf2-sha256$")?;
    let mut fields = rest.split('$');
    let params = fields.next()?;
    let salt = decode_canonical_b64(fields.next()?)?;
    let hash = decode_canonical_b64(fields.next()?)?;
    if fields.next().is_some() {
        return None;
    }

    let (mut rounds, mut output_length) = (None, None);
    for field in params.split(',') {
        let (name, value) = field.split_once('=')?;
        let value = canonical_decimal(value)?;
        let slot = match name {
            "i" => &mut rounds,
            "l" => &mut output_length,
            _ => return None,
        };
        if slot.is_some() {
            return None;
        }
        *slot = Some(value);
    }
    let output_length = match output_length {
        Some(l) => {
            let l = usize::try_from(l).ok()?;
            if hash.len() != l {
                return None;
            }
            Some(l)
        }
        None => None,
    };
    Some(Pbkdf2Phc {
        rounds,
        output_length,
        salt,
        hash,
    })
}

/// IronCrypto's `phc::PasswordHash::parse` is the canonical Argon2 parser; this
/// is the matching check for the PBKDF2 format. Both accept only what the
/// writers in this module would produce.
fn decode_canonical_b64(s: &str) -> Option<Vec<u8>> {
    let bytes = STANDARD_NO_PAD.decode(s).ok()?;
    (STANDARD_NO_PAD.encode(&bytes) == s).then_some(bytes)
}

fn canonical_decimal(s: &str) -> Option<u32> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if s.len() > 1 && s.starts_with('0') {
        return None;
    }
    s.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixtures from the pre-IronCrypto crates (`argon2` 0.5.3, `pbkdf2` 0.12.2).
    /// The salts are the 16 bytes `start, start+1, ...` from the given base.
    fn salt_from(base: u8) -> [u8; 16] {
        let mut s = [0u8; 16];
        for (i, b) in s.iter_mut().enumerate() {
            *b = base.wrapping_add(i as u8);
        }
        s
    }

    const ARGON2_DEFAULT_1: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAECAwQFBgcICQoLDA0ODw$gYJZtjEAJqjg26xdLmknq8/bB7MiWPrE9hsYuA+SkIU";
    const ARGON2_DEFAULT_2: &str = "$argon2id$v=19$m=19456,t=2,p=1$gIGCg4SFhoeIiYqLjI2Ojw$3QMV47HYxBCzJUrQTS6njNOU/5f4GzvVs/qEm0Gojcs";
    const ARGON2_CUSTOM: &str = "$argon2id$v=19$m=4096,t=3,p=2$ICEiIyQlJicoKSorLC0uLw$goVf4dWyyeylXc+CzM/YCU3/zX/17l5gSKlKGNhyRlo";
    const PBKDF2_1000: &str = "$pbkdf2-sha256$i=1000,l=32$QEFCQ0RFRkdISUpLTE1OTw$wjfXzyJ4EYivw62TLSzmP1gcQ08iJ0Ii6aYSMUorh48";
    const PBKDF2_DEFAULT: &str = "$pbkdf2-sha256$i=600000,l=32$YGFiY2RlZmdoaWprbG1ubw$+epjx17QTBTl+7hf30Ud2f90VcIV9iH6js5gQAGFLaY";

    #[test]
    fn argon2id_hash_reproduces_the_old_crate_output() {
        let salt = salt_from(0x00);
        let h = argon2id_hash("test", b"correct horse battery staple", &salt).unwrap();
        assert_eq!(h, ARGON2_DEFAULT_1);
        let salt = salt_from(0x80);
        let h = argon2id_hash("test", "pässwörd ✓ 密码".as_bytes(), &salt).unwrap();
        assert_eq!(h, ARGON2_DEFAULT_2);
    }

    #[test]
    fn old_argon2_hashes_verify_under_ironcrypto() {
        phc::verify_password(b"correct horse battery staple", ARGON2_DEFAULT_1).unwrap();
        phc::verify_password("pässwörd ✓ 密码".as_bytes(), ARGON2_DEFAULT_2).unwrap();
        phc::verify_password(b"custom-params", ARGON2_CUSTOM).unwrap();
        assert!(phc::verify_password(b"custom-param", ARGON2_CUSTOM).is_err());
    }

    #[test]
    fn pbkdf2_hash_reproduces_the_old_crate_output_at_default_cost() {
        let salt = salt_from(0x60);
        let h = pbkdf2_sha256_hash("test", b"fips default", &salt).unwrap();
        assert_eq!(h, PBKDF2_DEFAULT);
    }

    #[test]
    fn old_pbkdf2_hash_verifies_under_the_parser_and_derivation() {
        let parsed = parse_pbkdf2_sha256(PBKDF2_1000).expect("canonical PHC");
        assert_eq!(parsed.rounds, Some(1000));
        assert_eq!(parsed.output_length, Some(32));
        let derived =
            pbkdf2_sha256_derive("test", b"pbkdf2 fixture", &parsed.salt, 1000, 32).unwrap();
        assert_eq!(derived, parsed.hash);
        assert_eq!(parsed.salt, salt_from(0x40).to_vec());
    }

    #[test]
    fn pbkdf2_parser_refuses_what_the_writers_never_produce() {
        // Padding, a leading zero, a duplicate field, an unknown field, a
        // length that disagrees with the hash, and trailing fields.
        let base = PBKDF2_1000;
        for bad in [
            base.replace(
                "$wjfXzyJ4EYivw62TLSzmP1gcQ08iJ0Ii6aYSMUorh48",
                "$wjfXzyJ4EYivw62TLSzmP1gcQ08iJ0Ii6aYSMUorh48=",
            )
            .as_str(),
            base.replace("i=1000", "i=01000").as_str(),
            base.replace("l=32", "l=32,l=32").as_str(),
            base.replace("l=32", "x=32").as_str(),
            base.replace("l=32", "l=16").as_str(),
            format!("{base}$extra").as_str(),
        ] {
            assert!(parse_pbkdf2_sha256(bad).is_none(), "accepted: {bad}");
        }
    }

    #[test]
    fn argon2_hash_is_refused_by_the_pbkdf2_parser_and_vice_versa() {
        assert!(parse_pbkdf2_sha256(ARGON2_DEFAULT_1).is_none());
        assert!(phc::PasswordHash::parse(PBKDF2_1000).is_err());
    }
}
