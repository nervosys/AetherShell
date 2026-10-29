//! Native signatures over IronCrypto: Ed25519, ECDSA P-256 and P-384 (with
//! SHA-256 and SHA-384) and RSA PKCS#1 v1.5 with SHA-256, plus key generation
//! for the same algorithms. Key material is supplied as data, never a path, and
//! nothing here touches the file system or a subprocess.
//!
//! ECDSA signs with RFC 6979 nonces, so one key and message give one signature.
//! Signatures leave as DER `Ecdsa-Sig-Value`, the form openssl reads.
use crate::{safety, value::Value};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use ironcrypto::core_types::traits::SignatureScheme;
use ironcrypto::core_types::ErrorKind;
use ironcrypto::drbg::Rng;
use ironcrypto::ec::p256::EcdsaP256Sha256;
use ironcrypto::ec::p384::EcdsaP384Sha384;
use ironcrypto::ec::{Ed25519, Ed25519Key};
use ironcrypto::pkix::private_key::parse_rsa_private_key;
use ironcrypto::pkix::{
    ecdsa_signature, parse_rsa_public_key, pem, write_rsa_public_key, KeyAlgorithm, PrivateKeyInfo,
};
use ironcrypto::rsa::{generate as generate_rsa, Pkcs1Sha256, RsaPrivateKey, RsaPublicKey};
use std::collections::BTreeMap;
use zeroize::Zeroizing;

/// Longest private-key PEM accepted, in bytes.
const MAX_PRIVATE_KEY: usize = 16384;
/// Longest public-key text accepted (raw Base64 or SPKI PEM), in bytes.
const MAX_PUBLIC_KEY: usize = 4096;
/// Longest signature Base64 text accepted, in bytes.
const MAX_SIGNATURE_TEXT: usize = 1400;
/// The shortest and longest RSA modulus accepted, in bytes: 2048 to 4096 bits,
/// the range IronCrypto signs and verifies.
const MIN_RSA_MODULUS: usize = 256;
const MAX_RSA_MODULUS: usize = 512;
/// Longest RSA signature accepted: one modulus.
const MAX_RSA_SIGNATURE: usize = MAX_RSA_MODULUS;
const MAX_P256_SIGNATURE: usize = 72;
const MAX_P384_SIGNATURE: usize = 104;
/// RSA public exponents accepted: 65537 up to 2^33 - 1, as before.
const MIN_RSA_EXPONENT: u64 = 65537;
const MAX_RSA_EXPONENT: u64 = (1 << 33) - 1;
/// Room for the largest DER signature (P-384 with a full-width r and s).
const DER_SIGNATURE_BUFFER: usize = 128;
/// PKCS#8 output buffer; the largest key (RSA-4096 with CRT) is about 2.5 KiB.
const PKCS8_BUFFER: usize = 8192;

const PKCS8_SUPPORT: &str = "invalid PKCS#8 private key: supported are Ed25519 (PKCS#8 v2), ECDSA P-256 or P-384, and RSA with a 2048 to 4096-bit modulus and public exponent of at least 65537";
const PKCS1_SUPPORT: &str = "invalid PKCS#1 RSA private key: supported are RSA with a 2048 to 4096-bit modulus and public exponent of at least 65537";
const SEC1_SUPPORT: &str = "invalid SEC1 EC private key: supported are the P-256 (prime256v1) and P-384 (secp384r1) curves, with the public key present";
const RAW_PUBLIC_SUPPORT: &str = "invalid public key: Base64 of 32 Ed25519 bytes, of an uncompressed P-256 (65 bytes) or P-384 (97 bytes) point, or of a DER RSAPublicKey with a 2048 to 4096-bit modulus; or a BEGIN PUBLIC KEY PEM";
const SPKI_SUPPORT: &str = "unsupported or malformed public key PEM: supported are Ed25519, ECDSA P-256 or P-384 (uncompressed points), and rsaEncryption with a 2048 to 4096-bit modulus";
const RSA_SIZE_SUPPORT: &str = "RSA key size must be 2048, 3072 or 4096 bits";

const TAG_INTEGER: u8 = 0x02;
const TAG_BIT_STRING: u8 = 0x03;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_NULL: u8 = 0x05;
const TAG_OID: u8 = 0x06;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_CONTEXT_0: u8 = 0xa0;
const TAG_CONTEXT_1: u8 = 0xa1;

/// DER contents of the object identifiers this module understands.
const OID_RSA_ENCRYPTION: &[u8] = &[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x01, 0x01];
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01];
const OID_P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2b, 0x81, 0x04, 0x00, 0x22];
const OID_ED25519: &[u8] = &[0x2b, 0x65, 0x70];

/// PKCS#8 (version 1, with public key) header for Ed25519, up to the seed. The
/// 32-byte seed and the `[1]` public key follow. Same layout the previous backend produced.
const ED25519_PKCS8_HEAD: [u8; 16] = [
    0x30, 0x53, 0x02, 0x01, 0x01, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}

fn sign_failed(name: &str) -> anyhow::Error {
    safety::bad_state(
        name,
        "signature generation failed",
        "check operating-system randomness availability",
    )
}

fn generation_failed(name: &str) -> anyhow::Error {
    safety::bad_state(
        name,
        "secure key generation failed",
        "check operating-system randomness availability",
    )
}

fn verify_failed(name: &str) -> anyhow::Error {
    safety::bad_state(
        name,
        "signature check could not run",
        "check the cryptographic module's self-test status",
    )
}

/// Turns a verification result into an answer. A signature or key that does
/// not check out is `false`, as it always was. A failure of the module itself
/// is an error, not a `false` that would hide it.
fn verdict(name: &str, result: ironcrypto::core_types::Result<()>) -> Result<bool> {
    match result {
        Ok(()) => Ok(true),
        Err(error) => match error.kind() {
            ErrorKind::AuthenticationFailed
            | ErrorKind::MalformedEncoding
            | ErrorKind::InvalidParameter
            | ErrorKind::InvalidLength => Ok(false),
            _ => Err(verify_failed(name)),
        },
    }
}

fn policy(name: &str) -> Result<()> {
    if safety::fips_enabled() {
        return Err(safety::policy_deny(
            name,
            "native IronCrypto signature backend (Ed25519, ECDSA, RSA) is not FIPS validated",
            "use an independently validated signing provider",
        ));
    }
    Ok(())
}

fn shape(name: &str, args: &[Value], input: &Option<Value>, count: usize) -> Result<()> {
    if args.len() != count || input.is_some() {
        return Err(bad(
            name,
            "invalid argument count or unexpected pipeline input",
        ));
    }
    Ok(())
}

fn text<'a>(name: &str, value: &'a Value) -> Result<&'a str> {
    match value {
        Value::Str(value) => Ok(value),
        _ => Err(bad(name, "expected String")),
    }
}

fn message(name: &str, value: &Value) -> Result<Vec<u8>> {
    let bytes = match value {
        Value::Str(value) => {
            if value.len() > 1048576 {
                return Err(safety::budget_exceeded(
                    name,
                    "message exceeds 1 MiB".into(),
                    "sign or verify a smaller message",
                ));
            }
            value.as_bytes().to_vec()
        }
        Value::Array(values) => {
            if values.len() > 1048576 {
                return Err(safety::budget_exceeded(
                    name,
                    "message exceeds 1 MiB".into(),
                    "sign or verify a smaller message",
                ));
            }
            values
                .iter()
                .map(|value| match value {
                    Value::Int(n) if (0..=255).contains(n) => Ok(*n as u8),
                    _ => Err(bad(name, "message byte arrays require Int elements 0..255")),
                })
                .collect::<Result<_>>()?
        }
        _ => {
            return Err(bad(
                name,
                "message must be a UTF-8 String or byte Int Array",
            ))
        }
    };
    Ok(bytes)
}

/// Splits one PEM envelope out of `pem`. Only the three private-key labels are
/// recognized, and the body may hold no second envelope.
fn envelope(pem: &str) -> Option<(Envelope, &str)> {
    let trimmed = pem.trim_matches(|c: char| c.is_ascii_whitespace());
    for (kind, label) in [
        (Envelope::Pkcs8, "PRIVATE KEY"),
        (Envelope::Pkcs1, "RSA PRIVATE KEY"),
        (Envelope::Sec1, "EC PRIVATE KEY"),
    ] {
        let begin = format!("-----BEGIN {label}-----");
        if let Some(rest) = trimmed.strip_prefix(begin.as_str()) {
            let end = format!("-----END {label}-----");
            let body = rest.strip_suffix(end.as_str())?;
            return (!body.contains("-----")).then_some((kind, body));
        }
    }
    None
}

/// Decodes the Base64 body of a PEM envelope, ignoring line breaks.
fn pem_der(body: &str) -> Option<Zeroizing<Vec<u8>>> {
    let compact = Zeroizing::new(
        body.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>(),
    );
    STANDARD.decode(compact.as_bytes()).ok().map(Zeroizing::new)
}

#[derive(Clone, Copy)]
enum Envelope {
    Pkcs8,
    Pkcs1,
    Sec1,
}

#[derive(Clone, Copy)]
enum Curve {
    P256,
    P384,
}

impl Curve {
    /// The private scalar's length.
    fn private_len(self) -> usize {
        match self {
            Curve::P256 => 32,
            Curve::P384 => 48,
        }
    }

    /// The uncompressed SEC1 point's length.
    fn public_len(self) -> usize {
        match self {
            Curve::P256 => 65,
            Curve::P384 => 97,
        }
    }

    /// The fixed-width `r || s` length that IronCrypto signs in.
    fn raw_signature_len(self) -> usize {
        match self {
            Curve::P256 => 64,
            Curve::P384 => 96,
        }
    }

    /// The algorithm name crypto_sign reports.
    fn algorithm(self) -> &'static str {
        match self {
            Curve::P256 => "ecdsa-p256-sha256",
            Curve::P384 => "ecdsa-p384-sha384",
        }
    }
}

/// A parsed signing key. The variant fixes the algorithm name and the output.
enum Signer {
    Ed25519(Box<Ed25519Key>),
    Ec(EcKey),
    Rsa(Box<RsaPrivateKey>),
}

/// An ECDSA private key, with its public point derived once when it is read.
struct EcKey {
    curve: Curve,
    scalar: Zeroizing<Vec<u8>>,
    public: Vec<u8>,
}

fn private_key(name: &str, value: &Value) -> Result<Signer> {
    let pem = text(name, value)?;
    if pem.len() > MAX_PRIVATE_KEY {
        return Err(safety::budget_exceeded(
            name,
            "private key envelope exceeds 16384 bytes".into(),
            "supply an unencrypted PKCS#8, PKCS#1 or SEC1 private key PEM of at most 16384 bytes",
        ));
    }
    let (kind, body) = envelope(pem).ok_or_else(|| {
        bad(
            name,
            "expected one unencrypted PEM envelope: PRIVATE KEY (PKCS#8), RSA PRIVATE KEY (PKCS#1) or EC PRIVATE KEY (SEC1)",
        )
    })?;
    let der = pem_der(body).ok_or_else(|| bad(name, "invalid private key encoding"))?;
    match kind {
        Envelope::Pkcs8 => pkcs8_signer(name, &der),
        Envelope::Pkcs1 => {
            let info = parse_rsa_private_key(&der).map_err(|_| bad(name, PKCS1_SUPPORT))?;
            rsa_signer(name, info, PKCS1_SUPPORT)
        }
        Envelope::Sec1 => sec1_signer(name, &der),
    }
}

/// Tries Ed25519 (PKCS#8 version 1, with its public key), then the general
/// PKCS#8 parser for ECDSA P-256 or P-384 and RSA. An Ed25519 key without its
/// public key is refused, as it always was.
fn pkcs8_signer(name: &str, der: &[u8]) -> Result<Signer> {
    if let Some((seed, public)) = ed25519_pkcs8(der) {
        let key = Ed25519Key::from_seed(seed).map_err(|_| bad(name, PKCS8_SUPPORT))?;
        if key.public_key().as_slice() != public {
            return Err(bad(name, PKCS8_SUPPORT));
        }
        return Ok(Signer::Ed25519(Box::new(key)));
    }
    let info = PrivateKeyInfo::from_der(der).map_err(|_| bad(name, PKCS8_SUPPORT))?;
    match info {
        PrivateKeyInfo::Rsa { .. } => rsa_signer(name, info, PKCS8_SUPPORT),
        PrivateKeyInfo::Ec {
            algorithm,
            private_key,
            public_key,
        } => {
            let curve = match algorithm {
                KeyAlgorithm::EcP256 => Curve::P256,
                KeyAlgorithm::EcP384 => Curve::P384,
                _ => return Err(bad(name, PKCS8_SUPPORT)),
            };
            ec_signer(name, curve, private_key, public_key, PKCS8_SUPPORT)
        }
        _ => Err(bad(name, PKCS8_SUPPORT)),
    }
}

/// The Ed25519 PKCS#8 form this module accepts: version 1, with the public key
/// present. Returns the seed and the public key, which the caller checks.
fn ed25519_pkcs8(der: &[u8]) -> Option<(&[u8], &[u8])> {
    let body = whole(der, TAG_SEQUENCE)?;
    let (version, rest) = take(body, TAG_INTEGER)?;
    if version != [1u8].as_slice() {
        return None;
    }
    let (algorithm, rest) = take(rest, TAG_SEQUENCE)?;
    if whole(algorithm, TAG_OID)? != OID_ED25519 {
        return None;
    }
    let (private, rest) = take(rest, TAG_OCTET_STRING)?;
    let seed = whole(private, TAG_OCTET_STRING)?;
    let (public, rest) = take(rest, TAG_CONTEXT_1)?;
    if !rest.is_empty() {
        return None;
    }
    let key = bit_string(whole(public, TAG_BIT_STRING)?)?;
    (seed.len() == 32 && key.len() == 32).then_some((seed, key))
}

fn sec1_signer(name: &str, der: &[u8]) -> Result<Signer> {
    let (scalar, curve, point) = sec1_parts(der).ok_or_else(|| bad(name, SEC1_SUPPORT))?;
    ec_signer(name, curve, scalar, Some(point), SEC1_SUPPORT)
}

/// Builds an ECDSA signer from a private scalar. A public point, when the
/// encoding carried one, must be the one the scalar derives.
fn ec_signer(
    name: &str,
    curve: Curve,
    scalar: &[u8],
    public: Option<&[u8]>,
    support: &str,
) -> Result<Signer> {
    if scalar.len() != curve.private_len() {
        return Err(bad(name, support));
    }
    let mut derived = vec![0u8; curve.public_len()];
    let computed = match curve {
        Curve::P256 => EcdsaP256Sha256::public_key(scalar, &mut derived),
        Curve::P384 => EcdsaP384Sha384::public_key(scalar, &mut derived),
    };
    computed.map_err(|_| bad(name, support))?;
    if public.is_some_and(|point| point != derived.as_slice()) {
        return Err(bad(name, support));
    }
    Ok(Signer::Ec(EcKey {
        curve,
        scalar: Zeroizing::new(scalar.to_vec()),
        public: derived,
    }))
}

/// Builds an RSA signer from a parsed key. The primes and the public exponent
/// are used, and the modulus they give must be the one the encoding states; the
/// private exponent and CRT values in the input are recomputed, not trusted.
fn rsa_signer(name: &str, info: PrivateKeyInfo<'_>, support: &str) -> Result<Signer> {
    let PrivateKeyInfo::Rsa {
        modulus,
        public_exponent,
        prime1,
        prime2,
        ..
    } = info
    else {
        return Err(bad(name, support));
    };
    let stated = strip_leading_zeros(modulus);
    if !(MIN_RSA_MODULUS..=MAX_RSA_MODULUS).contains(&stated.len())
        || public_exponent < MIN_RSA_EXPONENT
    {
        return Err(bad(name, support));
    }
    let key = RsaPrivateKey::from_primes(prime1, prime2, public_exponent)
        .map_err(|_| bad(name, support))?;
    let mut derived = vec![0u8; key.size()];
    key.public_key()
        .modulus_bytes(&mut derived)
        .map_err(|_| bad(name, support))?;
    if derived.as_slice() != stated {
        return Err(bad(name, support));
    }
    Ok(Signer::Rsa(Box::new(key)))
}

fn strip_leading_zeros(bytes: &[u8]) -> &[u8] {
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len());
    &bytes[first..]
}

/// Reads one definite-length TLV from the front of `input`. Returns the tag,
/// the contents and the bytes after it. Indefinite lengths, non-minimal length
/// encodings, lengths beyond four bytes and high-tag-number forms are refused.
fn tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let tag = *input.first()?;
    if tag & 0x1f == 0x1f {
        return None;
    }
    let first = *input.get(1)?;
    let (length, header) = if first < 0x80 {
        (usize::from(first), 2)
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return None;
        }
        let digits = input.get(2..2 + count)?;
        if *digits.first()? == 0 {
            return None;
        }
        let length = digits
            .iter()
            .fold(0usize, |acc, &byte| (acc << 8) | usize::from(byte));
        if length < 0x80 {
            return None;
        }
        (length, 2 + count)
    };
    let end = header.checked_add(length)?;
    let contents = input.get(header..end)?;
    let rest = input.get(end..)?;
    Some((tag, contents, rest))
}

/// One TLV with the expected tag, and the bytes after it.
fn take(input: &[u8], tag: u8) -> Option<(&[u8], &[u8])> {
    let (found, contents, rest) = tlv(input)?;
    (found == tag).then_some((contents, rest))
}

/// Exactly one TLV with the expected tag, and nothing after it.
fn whole(input: &[u8], tag: u8) -> Option<&[u8]> {
    let (contents, rest) = take(input, tag)?;
    rest.is_empty().then_some(contents)
}

/// BIT STRING contents with zero unused bits, as the key bytes.
fn bit_string(contents: &[u8]) -> Option<&[u8]> {
    let (unused, rest) = contents.split_first()?;
    (*unused == 0).then_some(rest)
}

/// INTEGER contents that are a positive, minimally encoded value.
fn positive_integer(contents: &[u8]) -> Option<&[u8]> {
    let first = *contents.first()?;
    if first & 0x80 != 0 {
        return None;
    }
    if first == 0 && contents.get(1)? & 0x80 == 0 {
        return None;
    }
    Some(contents)
}

fn curve_of(oid: &[u8]) -> Option<Curve> {
    if oid == OID_P256 {
        Some(Curve::P256)
    } else if oid == OID_P384 {
        Some(Curve::P384)
    } else {
        None
    }
}

/// RFC 5915 ECPrivateKey: SEQUENCE { INTEGER 1, OCTET STRING privateKey,
/// [0] OID namedCurve, [1] BIT STRING publicKey }. The curve and the public key
/// are required. Returns the scalar, the curve and the uncompressed point.
fn sec1_parts(der: &[u8]) -> Option<(&[u8], Curve, &[u8])> {
    let body = whole(der, TAG_SEQUENCE)?;
    let (version, rest) = take(body, TAG_INTEGER)?;
    if version != [1u8].as_slice() {
        return None;
    }
    let (scalar, rest) = take(rest, TAG_OCTET_STRING)?;
    let (parameters, rest) = take(rest, TAG_CONTEXT_0)?;
    let curve = curve_of(whole(parameters, TAG_OID)?)?;
    let (public, rest) = take(rest, TAG_CONTEXT_1)?;
    if !rest.is_empty() {
        return None;
    }
    let point = bit_string(whole(public, TAG_BIT_STRING)?)?;
    Some((scalar, curve, point))
}

/// RFC 8017 RSAPublicKey: SEQUENCE { INTEGER n, INTEGER e }, with a modulus of
/// 256 to 512 bytes (2048 to 4096 bits).
fn valid_rsa_public_key(der: &[u8]) -> bool {
    let Some(body) = whole(der, TAG_SEQUENCE) else {
        return false;
    };
    let Some((n, rest)) = take(body, TAG_INTEGER) else {
        return false;
    };
    let Some((e, rest)) = take(rest, TAG_INTEGER) else {
        return false;
    };
    if !rest.is_empty() || positive_integer(e).is_none() {
        return false;
    }
    let Some(modulus) = positive_integer(n) else {
        return false;
    };
    let length = match modulus.first() {
        Some(0) => modulus.len() - 1,
        _ => modulus.len(),
    };
    (MIN_RSA_MODULUS..=MAX_RSA_MODULUS).contains(&length)
}

fn uncompressed_point(point: &[u8], length: usize) -> bool {
    point.len() == length && point.first() == Some(&0x04)
}

/// A parsed public key. The variant fixes the verification algorithm.
enum Verifier {
    Ed25519(Vec<u8>),
    EcdsaP256(Vec<u8>),
    EcdsaP384(Vec<u8>),
    /// The DER RSAPublicKey, checked for shape when it is read.
    Rsa(Vec<u8>),
}

/// Raw public-key bytes as crypto_sign returns them. Dispatch is by length and
/// the leading byte, so no key type needs a label.
fn verifier_from_raw(bytes: Vec<u8>) -> Option<Verifier> {
    match (bytes.len(), bytes.first().copied()) {
        (32, _) => Some(Verifier::Ed25519(bytes)),
        (65, Some(0x04)) => Some(Verifier::EcdsaP256(bytes)),
        (97, Some(0x04)) => Some(Verifier::EcdsaP384(bytes)),
        (_, Some(TAG_SEQUENCE)) => valid_rsa_public_key(&bytes).then_some(Verifier::Rsa(bytes)),
        _ => None,
    }
}

/// SubjectPublicKeyInfo: SEQUENCE { SEQUENCE { OID, parameters }, BIT STRING }.
fn spki_verifier(der: &[u8]) -> Option<Verifier> {
    let body = whole(der, TAG_SEQUENCE)?;
    let (algorithm, rest) = take(body, TAG_SEQUENCE)?;
    let (key_bits, rest) = take(rest, TAG_BIT_STRING)?;
    if !rest.is_empty() {
        return None;
    }
    let key = bit_string(key_bits)?;
    let (oid, parameters) = take(algorithm, TAG_OID)?;
    if oid == OID_RSA_ENCRYPTION {
        if !whole(parameters, TAG_NULL)?.is_empty() || !valid_rsa_public_key(key) {
            return None;
        }
        Some(Verifier::Rsa(key.to_vec()))
    } else if oid == OID_ED25519 {
        if !parameters.is_empty() || key.len() != 32 {
            return None;
        }
        Some(Verifier::Ed25519(key.to_vec()))
    } else if oid == OID_EC_PUBLIC_KEY {
        match curve_of(whole(parameters, TAG_OID)?)? {
            Curve::P256 if uncompressed_point(key, 65) => Some(Verifier::EcdsaP256(key.to_vec())),
            Curve::P384 if uncompressed_point(key, 97) => Some(Verifier::EcdsaP384(key.to_vec())),
            _ => None,
        }
    } else {
        None
    }
}

fn verifier(name: &str, value: &Value) -> Result<Verifier> {
    let encoded = text(name, value)?;
    if encoded.len() > MAX_PUBLIC_KEY {
        return Err(bad(name, "public key text exceeds 4096 bytes"));
    }
    let trimmed = encoded.trim_matches(|c: char| c.is_ascii_whitespace());
    if let Some(body) = trimmed
        .strip_prefix("-----BEGIN PUBLIC KEY-----")
        .and_then(|rest| rest.strip_suffix("-----END PUBLIC KEY-----"))
    {
        if body.contains("-----") {
            return Err(bad(name, SPKI_SUPPORT));
        }
        let der = pem_der(body).ok_or_else(|| bad(name, SPKI_SUPPORT))?;
        return spki_verifier(&der).ok_or_else(|| bad(name, SPKI_SUPPORT));
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| {
        bad(
            name,
            "public key must be canonical padded standard Base64 or a BEGIN PUBLIC KEY PEM",
        )
    })?;
    verifier_from_raw(bytes).ok_or_else(|| bad(name, RAW_PUBLIC_SUPPORT))
}

/// Decodes the signature and checks its length against the key's algorithm.
fn signature_bytes(name: &str, value: &Value, key: &Verifier) -> Result<Vec<u8>> {
    let encoded = text(name, value)?;
    if encoded.len() > MAX_SIGNATURE_TEXT {
        return Err(bad(name, "signature text exceeds 1400 bytes"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| bad(name, "signature must be canonical padded standard Base64"))?;
    let fits = match key {
        Verifier::Ed25519(_) => bytes.len() == 64,
        Verifier::EcdsaP256(_) => (1..=MAX_P256_SIGNATURE).contains(&bytes.len()),
        Verifier::EcdsaP384(_) => (1..=MAX_P384_SIGNATURE).contains(&bytes.len()),
        Verifier::Rsa(_) => (1..=MAX_RSA_SIGNATURE).contains(&bytes.len()),
    };
    if !fits {
        return Err(bad(
            name,
            "signature length does not fit the key's algorithm: Ed25519 is 64 bytes, ECDSA P-256 DER is at most 72 bytes, ECDSA P-384 DER is at most 104 bytes, RSA is at most 512 bytes",
        ));
    }
    Ok(bytes)
}

/// Signs with RFC 6979 nonces and returns the DER `Ecdsa-Sig-Value`.
fn ec_sign(name: &str, key: &EcKey, message: &[u8]) -> Result<Vec<u8>> {
    let mut raw = vec![0u8; key.curve.raw_signature_len()];
    let signed = match key.curve {
        Curve::P256 => EcdsaP256Sha256::sign(&key.scalar, message, &mut raw),
        Curve::P384 => EcdsaP384Sha384::sign(&key.scalar, message, &mut raw),
    };
    signed.map_err(|_| sign_failed(name))?;
    let mut der = vec![0u8; DER_SIGNATURE_BUFFER];
    let written = ecdsa_signature::to_der(&raw, &mut der).map_err(|_| sign_failed(name))?;
    der.truncate(written);
    Ok(der)
}

/// Verifies a DER `Ecdsa-Sig-Value`. A malformed signature is `false`.
fn ec_verify(name: &str, curve: Curve, public: &[u8], message: &[u8], der: &[u8]) -> Result<bool> {
    let mut raw = vec![0u8; curve.raw_signature_len()];
    if ecdsa_signature::from_der(der, &mut raw).is_err() {
        return Ok(false);
    }
    let checked = match curve {
        Curve::P256 => EcdsaP256Sha256::verify(public, message, &raw),
        Curve::P384 => EcdsaP384Sha384::verify(public, message, &raw),
    };
    verdict(name, checked)
}

fn rsa_sign(name: &str, key: &RsaPrivateKey, message: &[u8]) -> Result<Vec<u8>> {
    let mut signature = vec![0u8; key.size()];
    Pkcs1Sha256::sign(key, message, &mut signature).map_err(|_| sign_failed(name))?;
    Ok(signature)
}

/// The DER RSAPublicKey of a signing key, as crypto_sign returns it.
fn rsa_public_der(name: &str, key: &RsaPrivateKey) -> Result<Vec<u8>> {
    let public = key.public_key();
    let mut modulus = vec![0u8; public.size()];
    public
        .modulus_bytes(&mut modulus)
        .map_err(|_| sign_failed(name))?;
    let mut der = vec![0u8; modulus.len() + 16];
    let written = write_rsa_public_key(&modulus, public.exponent(), &mut der)
        .map_err(|_| sign_failed(name))?;
    der.truncate(written);
    Ok(der)
}

/// RSASSA-PKCS1-v1_5 over SHA-256 with a DER RSAPublicKey. A key or signature
/// that does not check out is `false`.
fn rsa_verify(name: &str, der: &[u8], message: &[u8], signature: &[u8]) -> Result<bool> {
    let Ok((modulus, exponent)) = parse_rsa_public_key(der) else {
        return Ok(false);
    };
    if !(MIN_RSA_EXPONENT..=MAX_RSA_EXPONENT).contains(&exponent) {
        return Ok(false);
    }
    let Ok(key) = RsaPublicKey::from_components(modulus, exponent) else {
        return Ok(false);
    };
    verdict(name, Pkcs1Sha256::verify(&key, message, signature))
}

pub(super) fn sign(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const NAME: &str = "crypto_sign";
    shape(NAME, &args, &input, 2)?;
    policy(NAME)?;
    let message = message(NAME, &args[1])?;
    let key = private_key(NAME, &args[0])?;
    let (algorithm, public_key, signature) = match &key {
        Signer::Ed25519(pair) => {
            let mut signature = [0u8; 64];
            pair.sign(&message, &mut signature)
                .map_err(|_| sign_failed(NAME))?;
            ("ed25519", pair.public_key().to_vec(), signature.to_vec())
        }
        Signer::Ec(ec) => (
            ec.curve.algorithm(),
            ec.public.clone(),
            ec_sign(NAME, ec, &message)?,
        ),
        Signer::Rsa(pair) => (
            "rsa-pkcs1-sha256",
            rsa_public_der(NAME, pair)?,
            rsa_sign(NAME, pair, &message)?,
        ),
    };
    Ok(Value::Record(BTreeMap::from([
        ("algorithm".into(), Value::Str(algorithm.into())),
        ("public_key".into(), Value::Str(STANDARD.encode(public_key))),
        ("signature".into(), Value::Str(STANDARD.encode(signature))),
    ])))
}

pub(super) fn verify(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const NAME: &str = "crypto_verify_signature";
    shape(NAME, &args, &input, 3)?;
    policy(NAME)?;
    let key = verifier(NAME, &args[0])?;
    let signature = signature_bytes(NAME, &args[2], &key)?;
    let message = message(NAME, &args[1])?;
    let valid = match &key {
        Verifier::Ed25519(bytes) => verdict(NAME, Ed25519::verify(bytes, &message, &signature))?,
        Verifier::EcdsaP256(bytes) => ec_verify(NAME, Curve::P256, bytes, &message, &signature)?,
        Verifier::EcdsaP384(bytes) => ec_verify(NAME, Curve::P384, bytes, &message, &signature)?,
        Verifier::Rsa(bytes) => rsa_verify(NAME, bytes, &message, &signature)?,
    };
    Ok(Value::Bool(valid))
}

fn rng(name: &str) -> Result<Rng> {
    Rng::from_os().map_err(|_| generation_failed(name))
}

/// Encodes a private key as an unencrypted PKCS#8 document.
fn pkcs8_der(name: &str, info: &PrivateKeyInfo<'_>) -> Result<Zeroizing<Vec<u8>>> {
    let mut der = Zeroizing::new(vec![0u8; PKCS8_BUFFER]);
    let written = info
        .to_der(&mut der[..])
        .map_err(|_| generation_failed(name))?;
    der.truncate(written);
    Ok(der)
}

fn ed25519_generate(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let mut rng = rng(name)?;
    let mut seed = Zeroizing::new([0u8; 32]);
    rng.fill(&mut seed[..])
        .map_err(|_| generation_failed(name))?;
    let key = Ed25519Key::from_seed(&seed[..]).map_err(|_| generation_failed(name))?;
    let mut der = Zeroizing::new(Vec::with_capacity(ED25519_PKCS8_HEAD.len() + 37));
    der.extend_from_slice(&ED25519_PKCS8_HEAD);
    der.extend_from_slice(&seed[..]);
    der.extend_from_slice(&[0xa1, 0x23, 0x03, 0x21, 0x00]);
    der.extend_from_slice(&key.public_key()[..]);
    Ok(der)
}

/// Draws scalars until one is a valid P-256 private key; the chance of a
/// refusal is about 2^-32, so the bound is never the limit in practice.
fn ec_generate(name: &str) -> Result<Zeroizing<Vec<u8>>> {
    let mut rng = rng(name)?;
    for _ in 0..32 {
        let mut scalar = Zeroizing::new([0u8; 32]);
        rng.fill(&mut scalar[..])
            .map_err(|_| generation_failed(name))?;
        let mut public = [0u8; 65];
        if EcdsaP256Sha256::public_key(&scalar[..], &mut public).is_err() {
            continue;
        }
        let info = PrivateKeyInfo::Ec {
            algorithm: KeyAlgorithm::EcP256,
            private_key: &scalar[..],
            public_key: Some(&public[..]),
        };
        return pkcs8_der(name, &info);
    }
    Err(generation_failed(name))
}

fn rsa_generate(name: &str, bits: i64) -> Result<Zeroizing<Vec<u8>>> {
    let bits = usize::try_from(bits)
        .ok()
        .filter(|bits| matches!(*bits, 2048 | 3072 | 4096))
        .ok_or_else(|| bad(name, RSA_SIZE_SUPPORT))?;
    let mut rng = rng(name)?;
    let key = generate_rsa(bits, &mut rng).map_err(|_| generation_failed(name))?;
    let public = key.public_key();
    let size = key.size();
    let half = size / 2;
    let mut modulus = vec![0u8; size];
    public
        .modulus_bytes(&mut modulus)
        .map_err(|_| generation_failed(name))?;
    let mut private_exponent = Zeroizing::new(vec![0u8; size]);
    key.exponent_bytes(&mut private_exponent[..])
        .map_err(|_| generation_failed(name))?;
    let mut prime1 = Zeroizing::new(vec![0u8; half]);
    let mut prime2 = Zeroizing::new(vec![0u8; half]);
    key.prime_bytes(&mut prime1[..], &mut prime2[..])
        .map_err(|_| generation_failed(name))?;
    let mut exponent1 = Zeroizing::new(vec![0u8; half]);
    let mut exponent2 = Zeroizing::new(vec![0u8; half]);
    let mut coefficient = Zeroizing::new(vec![0u8; half]);
    key.crt_exponent_bytes(&mut exponent1[..], &mut exponent2[..], &mut coefficient[..])
        .map_err(|_| generation_failed(name))?;
    let info = PrivateKeyInfo::Rsa {
        modulus: &modulus,
        public_exponent: public.exponent(),
        private_exponent: &private_exponent[..],
        prime1: &prime1[..],
        prime2: &prime2[..],
        exponent1: &exponent1[..],
        exponent2: &exponent2[..],
        coefficient: &coefficient[..],
    };
    pkcs8_der(name, &info)
}

/// Generates a private key and returns it as an unencrypted PKCS#8
/// `PRIVATE KEY` PEM. `key_type` is `rsa` (with `bits` of 2048, 3072 or 4096),
/// `ec` or `ecdsa` (P-256), or `ed25519`; the caller has checked the names.
pub(super) fn generate_key(key_type: &str, bits: i64) -> Result<Value> {
    const NAME: &str = "crypto_generate_key";
    policy(NAME)?;
    let der = match key_type {
        "ed25519" => ed25519_generate(NAME)?,
        "ec" | "ecdsa" => ec_generate(NAME)?,
        "rsa" => rsa_generate(NAME, bits)?,
        _ => return Err(bad(NAME, "key type rsa, ec, ecdsa or ed25519")),
    };
    let mut pem_text = Zeroizing::new(vec![0u8; pem::encoded_len("PRIVATE KEY", der.len())]);
    let written =
        pem::encode("PRIVATE KEY", &der, &mut pem_text[..]).map_err(|_| generation_failed(NAME))?;
    let text = std::str::from_utf8(&pem_text[..written])
        .map_err(|_| generation_failed(NAME))?
        .to_owned();
    Ok(Value::Str(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSA_PKCS8_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIIEvAIBADANBgkqhkiG9w0BAQEFAASCBKYwggSiAgEAAoIBAQDV4+qd6oHhI1CB
A1ZOdl6TF7Qo0MaQDfz7L9mnKoUOofoofiENhAc+Bt+UBioDo6gCu21KFX4w/O8l
sCeV5epZPnVnNhpNTCl6QS4z2iRg/NWu8KoJTn0JYqPFe5/GguBlvxDjPPmer4Y+
BmhIbvOZXRAbOoIWnRYgCiCTgwOjRaraHNq1hihPcgqPSIUCLvYuZ/3irbTlnD6/
k2+ou/LIRmP5GqPKHS8efHBKcTL9WloHymy/XGLhesMGKSHeQ2l011tKwEsaYHZ8
afpSRloTlKIT8gquTJaLdqYU6Uj1tZKsVKNYpGe6MoUnccECezj6uUYflmiLcw12
9QV2YfG9AgMBAAECggEAVhAJHVV4wSA2ilLPbW7Pixwkj6/B1cDrJB8S0uKW+1ZN
k/F8VzXivCCD4cnBKgm11rOXvBF93zRG/cEtjHEgpJwvHUJ1vtiAcAK+FnsV3Dif
um533JrKUPZxs0qU9Yk/xXDl2Kl2P1WdDGQDnfGW7wS1MF+QbIz0lBu/E9vJoO3w
4adyJekSTJL+KY0JrcbZKPp2hmf3ufLV9lDI5M0jkpL9aTZo37rAxrW7WH5FcYUn
dJ/JxqBAb0KlRT4HDISBba9FjNK4X6khiFUZe8ikr0vWVfzPJND7UnhyWI9s/B30
L6vKz6YL1Fr+17GVJ0fgzRJ8QENd76Hgzsa3fQHDOQKBgQDz9Wp/IohkPS+WbbiM
EBjRrImmbghub2IsYgsfcWulIMMKn7YfrDmRQifrNfUfDBMIGv7om2+hbRkTIIo+
H9faHyCirJ1VSA6SeG0Vvf4uvmEqphe+i1I4GQ6opWMFerD1Ix/oGWhIvZPO4J1Z
s0I3Hf53LLPdt8KZnwbW4Z0gWwKBgQDgcpD2z7Zy6nax6FTvlEhc2q5Uprvus8oe
s+gyfoMIohwnMhitJDP0hP32d6Bi7cclQDuey2M4ykR681rGhHe7+Gm+f/Yi8M7N
OfVNkoiCZQ8QqXLgdYib9k0mLhhBxsAwwHHZLCj2Z0iSLPalKRRjTUqNN+SjJT0A
LQ4FidNRxwKBgEX8ZLXeQUSIbQt5pJTB1WzpU3/kVxTfbUmQQuKdViVEFdAJa2UA
sm8ijHMkwpJKk3ciz3gJh2fG9YeQMTiLSBAyF9mzLVuW1V9dnuW64J2mThe/GFa4
fzjKr3LgJ10eCn/4uKGMEGe1wx55P4rJKzjGqQalU2BK5E/P2FVy2fG7AoGATSOS
5wCkeAJVIPf0mRKhfCg8WiOj23o3MS4r047fggwVGPOCv579rEA4+IhMuzN1ijv1
QbItO8tolcklnazRae0Kisp30bJpoOcWg76k5AvPja5vKO28Mb4I7eJRu+mPDDJX
7ofa3DTLIFILwt/HfgmGYdqf3eJigLj7ZLPlyOkCgYADAEbRULvXrNIayoEX80bE
CdNY17PnWgSLceM2olt/ViIteGDlVBDFypvZOGN/3qV0mj5P3Ohe9OXrFyY7cEmO
d9KVFIV4OGzKkftHr94t9vqrUq9Qpr3tTB7/odQih5ufAvjc5jp16C8SFHCUxr50
QlFEGKktgipaJDTCLOO6fA==
-----END PRIVATE KEY-----
"#;

    const RSA_PKCS1_PEM: &str = r#"-----BEGIN RSA PRIVATE KEY-----
MIIEogIBAAKCAQEA1ePqneqB4SNQgQNWTnZekxe0KNDGkA38+y/ZpyqFDqH6KH4h
DYQHPgbflAYqA6OoArttShV+MPzvJbAnleXqWT51ZzYaTUwpekEuM9okYPzVrvCq
CU59CWKjxXufxoLgZb8Q4zz5nq+GPgZoSG7zmV0QGzqCFp0WIAogk4MDo0Wq2hza
tYYoT3IKj0iFAi72Lmf94q205Zw+v5NvqLvyyEZj+Rqjyh0vHnxwSnEy/VpaB8ps
v1xi4XrDBikh3kNpdNdbSsBLGmB2fGn6UkZaE5SiE/IKrkyWi3amFOlI9bWSrFSj
WKRnujKFJ3HBAns4+rlGH5Zoi3MNdvUFdmHxvQIDAQABAoIBAFYQCR1VeMEgNopS
z21uz4scJI+vwdXA6yQfEtLilvtWTZPxfFc14rwgg+HJwSoJtdazl7wRfd80Rv3B
LYxxIKScLx1Cdb7YgHACvhZ7Fdw4n7pud9yaylD2cbNKlPWJP8Vw5dipdj9VnQxk
A53xlu8EtTBfkGyM9JQbvxPbyaDt8OGnciXpEkyS/imNCa3G2Sj6doZn97ny1fZQ
yOTNI5KS/Wk2aN+6wMa1u1h+RXGFJ3SfycagQG9CpUU+BwyEgW2vRYzSuF+pIYhV
GXvIpK9L1lX8zyTQ+1J4cliPbPwd9C+rys+mC9Ra/texlSdH4M0SfEBDXe+h4M7G
t30BwzkCgYEA8/VqfyKIZD0vlm24jBAY0ayJpm4Ibm9iLGILH3FrpSDDCp+2H6w5
kUIn6zX1HwwTCBr+6JtvoW0ZEyCKPh/X2h8goqydVUgOknhtFb3+Lr5hKqYXvotS
OBkOqKVjBXqw9SMf6BloSL2TzuCdWbNCNx3+dyyz3bfCmZ8G1uGdIFsCgYEA4HKQ
9s+2cup2sehU75RIXNquVKa77rPKHrPoMn6DCKIcJzIYrSQz9IT99negYu3HJUA7
nstjOMpEevNaxoR3u/hpvn/2IvDOzTn1TZKIgmUPEKly4HWIm/ZNJi4YQcbAMMBx
2Swo9mdIkiz2pSkUY01KjTfkoyU9AC0OBYnTUccCgYBF/GS13kFEiG0LeaSUwdVs
6VN/5FcU321JkELinVYlRBXQCWtlALJvIoxzJMKSSpN3Is94CYdnxvWHkDE4i0gQ
MhfZsy1bltVfXZ7luuCdpk4XvxhWuH84yq9y4CddHgp/+LihjBBntcMeeT+KySs4
xqkGpVNgSuRPz9hVctnxuwKBgE0jkucApHgCVSD39JkSoXwoPFojo9t6NzEuK9OO
34IMFRjzgr+e/axAOPiITLszdYo79UGyLTvLaJXJJZ2s0WntCorKd9GyaaDnFoO+
pOQLz42ubyjtvDG+CO3iUbvpjwwyV+6H2tw0yyBSC8Lfx34JhmHan93iYoC4+2Sz
5cjpAoGAAwBG0VC716zSGsqBF/NGxAnTWNez51oEi3HjNqJbf1YiLXhg5VQQxcqb
2Thjf96ldJo+T9zoXvTl6xcmO3BJjnfSlRSFeDhsypH7R6/eLfb6q1KvUKa97Uwe
/6HUIoebnwL43OY6degvEhRwlMa+dEJRRBipLYIqWiQ0wizjunw=
-----END RSA PRIVATE KEY-----
"#;

    const RSA_SPKI_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA1ePqneqB4SNQgQNWTnZe
kxe0KNDGkA38+y/ZpyqFDqH6KH4hDYQHPgbflAYqA6OoArttShV+MPzvJbAnleXq
WT51ZzYaTUwpekEuM9okYPzVrvCqCU59CWKjxXufxoLgZb8Q4zz5nq+GPgZoSG7z
mV0QGzqCFp0WIAogk4MDo0Wq2hzatYYoT3IKj0iFAi72Lmf94q205Zw+v5NvqLvy
yEZj+Rqjyh0vHnxwSnEy/VpaB8psv1xi4XrDBikh3kNpdNdbSsBLGmB2fGn6UkZa
E5SiE/IKrkyWi3amFOlI9bWSrFSjWKRnujKFJ3HBAns4+rlGH5Zoi3MNdvUFdmHx
vQIDAQAB
-----END PUBLIC KEY-----
"#;

    const P256_SEC1_PEM: &str = r#"-----BEGIN EC PRIVATE KEY-----
MHcCAQEEILraSGVwY/B+WSIELgkYbTo3DcnufQREAP7WDMK062dNoAoGCCqGSM49
AwEHoUQDQgAEjMzc1UI19shCUF4cmVAMgWtZloumY9ED3YrsVoMcLw6gV+5csukO
awRWD81MDwUL61JFnNqG4KOGi9qAFQyDfQ==
-----END EC PRIVATE KEY-----
"#;

    const P256_PKCS8_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgutpIZXBj8H5ZIgQu
CRhtOjcNye59BEQA/tYMwrTrZ02hRANCAASMzNzVQjX2yEJQXhyZUAyBa1mWi6Zj
0QPdiuxWgxwvDqBX7lyy6Q5rBFYPzUwPBQvrUkWc2obgo4aL2oAVDIN9
-----END PRIVATE KEY-----
"#;

    const P256_SPKI_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEjMzc1UI19shCUF4cmVAMgWtZloum
Y9ED3YrsVoMcLw6gV+5csukOawRWD81MDwUL61JFnNqG4KOGi9qAFQyDfQ==
-----END PUBLIC KEY-----
"#;

    const P384_SEC1_PEM: &str = r#"-----BEGIN EC PRIVATE KEY-----
MIGkAgEBBDC2TI9LwGEvSIMwgXnaxogvHQXkOe7xTxb1sTOAydQyvr9ps1M3Vq6O
4SqCf/INbT2gBwYFK4EEACKhZANiAASBOCAf6WkCGtZg2cksRcTPI2XQC3yS+zho
3v2TGUa5Q/myq1OUd37bhqRdXeXR7Zh8wOEZkgNH4SByY9429Vmie51l2FFRghwE
M6OfcBZMD1365GdZJU9ABYD29yfJWZg=
-----END EC PRIVATE KEY-----
"#;

    const P384_PKCS8_PEM: &str = r#"-----BEGIN PRIVATE KEY-----
MIG2AgEAMBAGByqGSM49AgEGBSuBBAAiBIGeMIGbAgEBBDC2TI9LwGEvSIMwgXna
xogvHQXkOe7xTxb1sTOAydQyvr9ps1M3Vq6O4SqCf/INbT2hZANiAASBOCAf6WkC
GtZg2cksRcTPI2XQC3yS+zho3v2TGUa5Q/myq1OUd37bhqRdXeXR7Zh8wOEZkgNH
4SByY9429Vmie51l2FFRghwEM6OfcBZMD1365GdZJU9ABYD29yfJWZg=
-----END PRIVATE KEY-----
"#;

    const P384_SPKI_PEM: &str = r#"-----BEGIN PUBLIC KEY-----
MHYwEAYHKoZIzj0CAQYFK4EEACIDYgAEgTggH+lpAhrWYNnJLEXEzyNl0At8kvs4
aN79kxlGuUP5sqtTlHd+24akXV3l0e2YfMDhGZIDR+EgcmPeNvVZonudZdhRUYIc
BDOjn3AWTA9d+uRnWSVPQAWA9vcnyVmY
-----END PUBLIC KEY-----
"#;

    const SECP256K1_SEC1_PEM: &str = r#"-----BEGIN EC PRIVATE KEY-----
MHQCAQEEILfd1mFYMeWq/yuhAUQB+iOVVHk7/EVn23XlNGR/z6uYoAcGBSuBBAAK
oUQDQgAECEeTcZaVbpT7gw6+9t7F7FfNS1RDboY21RDpP8VNguNPYmCR2v7aCmhd
/ZLiwtSbU6L0I09v1JK/jlyhK0eulg==
-----END EC PRIVATE KEY-----
"#;

    /// `openssl dgst -sha256 -sign` over the message `interop`, base64.
    const RSA_INTEROP_SIGNATURE: &str = "Z87P0K0YuhmnpNvwgEQCxTH1l85mpUiuuzyP41AVybwgTwkrHT60HwwqFUn6o3nEB0InWbEjIKrFtUvVLVenvhGJ0iE2mFP2i2Mcqb9ENhE4IZ8D+RqQWTtYEhzC0EI7nYBpX+DuQJO25Ik+QpxsSQoxWdXLTGw04hNpzqUQrAbDl4m0qGVF6TVkl+taFTKadCSkEuQob5FUcMu82DGR3Q+taMKpk4YIxv6GZtPfr/6Eb83o/kAXu1qvVyA0YyiQUoUqtbPGCFstrmgMRNSxsa9wJ93y0/Ffc6tJjAi9PuTGmwU4Qre19njqvs5hFIgonF+MWl2qPz1yrCf3Tj0XMw==";

    /// `openssl dgst -sha256 -sign` over the message `interop` with the P-256 key, base64.
    const P256_INTEROP_SIGNATURE: &str = "MEUCIElaCpzi27WWgwBxcEbwOUpM7aKX8IbEDTDGOzE+CvYCAiEA+203xxcQbbgQx6wD0Ql2PMynqVl1nTZnh3oTb29DO+8=";

    fn s(value: &str) -> Value {
        Value::Str(value.to_string())
    }

    fn text_of(value: &Value) -> String {
        match value {
            Value::Str(text) => text.clone(),
            other => panic!("expected String, got {other:?}"),
        }
    }

    /// (algorithm, public_key, signature) from crypto_sign.
    fn signed(key_pem: &str, message: &str) -> (String, String, String) {
        let Value::Record(fields) =
            sign(vec![s(key_pem), s(message)], None).expect("sign succeeds")
        else {
            panic!("crypto_sign returns a Record")
        };
        (
            text_of(&fields["algorithm"]),
            text_of(&fields["public_key"]),
            text_of(&fields["signature"]),
        )
    }

    fn verify_with(public: &str, message: &str, signature: &str) -> Result<Value> {
        verify(vec![s(public), s(message), s(signature)], None)
    }

    fn verifies(public: &str, message: &str, signature: &str) -> bool {
        match verify_with(public, message, signature).expect("verify succeeds") {
            Value::Bool(valid) => valid,
            other => panic!("expected Bool, got {other:?}"),
        }
    }

    /// The coded error name (for example E_BAD_ARG), or None when uncoded.
    fn code(error: &anyhow::Error) -> Option<&'static str> {
        error
            .downcast_ref::<crate::safety::SafetyError>()
            .map(|coded| coded.code.as_str())
    }

    fn assert_bad_arg(result: Result<Value>) {
        let error = result.expect_err("expected an error");
        assert_eq!(code(&error), Some("E_BAD_ARG"), "{error}");
    }

    fn flip_last_byte(encoded: &str) -> String {
        let mut bytes = STANDARD.decode(encoded).expect("fixture Base64");
        let last = bytes.last_mut().expect("non-empty signature");
        *last ^= 1;
        STANDARD.encode(bytes)
    }

    /// A DER TLV with a short-form length (contents under 128 bytes).
    fn tlv_bytes(tag: u8, contents: &[u8]) -> Vec<u8> {
        assert!(contents.len() < 128);
        let mut out = vec![tag, contents.len() as u8];
        out.extend_from_slice(contents);
        out
    }

    fn pem_public(der: &[u8]) -> String {
        format!(
            "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----\n",
            STANDARD.encode(der)
        )
    }

    #[test]
    fn round_trip_for_every_algorithm_and_envelope() {
        let cases = [
            (RSA_PKCS8_PEM, "rsa-pkcs1-sha256"),
            (RSA_PKCS1_PEM, "rsa-pkcs1-sha256"),
            (P256_SEC1_PEM, "ecdsa-p256-sha256"),
            (P256_PKCS8_PEM, "ecdsa-p256-sha256"),
            (P384_SEC1_PEM, "ecdsa-p384-sha384"),
            (P384_PKCS8_PEM, "ecdsa-p384-sha384"),
        ];
        for (pem, algorithm) in cases {
            let (returned, public, signature) = signed(pem, "round trip");
            assert_eq!(returned, algorithm);
            assert!(verifies(&public, "round trip", &signature), "{algorithm}");
            assert!(!verifies(&public, "round tripX", &signature), "{algorithm}");
        }
        let generated = generate_key("ed25519", 0).expect("Ed25519 generation");
        let generated = text_of(&generated);
        let (returned, public, signature) = signed(&generated, "round trip");
        assert_eq!(returned, "ed25519");
        assert_eq!(STANDARD.decode(&public).expect("Base64").len(), 32);
        assert!(verifies(&public, "round trip", &signature));
    }

    #[test]
    fn pkcs1_and_pkcs8_rsa_are_the_same_key_and_sign_identically() {
        let (_, public8, signature8) = signed(RSA_PKCS8_PEM, "same");
        let (_, public1, signature1) = signed(RSA_PKCS1_PEM, "same");
        assert_eq!(public8, public1);
        // PKCS#1 v1.5 signing is deterministic.
        assert_eq!(signature8, signature1);
        assert!(verifies(&public8, "same", &signature8));
    }

    #[test]
    fn spki_public_keys_verify_round_trip_signatures() {
        for (pem, key_pem) in [
            (RSA_PKCS8_PEM, RSA_SPKI_PEM),
            (P256_PKCS8_PEM, P256_SPKI_PEM),
            (P384_PKCS8_PEM, P384_SPKI_PEM),
        ] {
            let (_, raw, signature) = signed(pem, "spki");
            assert!(verifies(key_pem, "spki", &signature));
            assert!(verifies(&raw, "spki", &signature));
        }
    }

    #[test]
    fn openssl_signatures_verify_under_both_public_key_forms() {
        assert!(verifies(RSA_SPKI_PEM, "interop", RSA_INTEROP_SIGNATURE));
        assert!(verifies(P256_SPKI_PEM, "interop", P256_INTEROP_SIGNATURE));
        let (_, rsa_raw, _) = signed(RSA_PKCS8_PEM, "interop");
        assert!(verifies(&rsa_raw, "interop", RSA_INTEROP_SIGNATURE));
        let (_, p256_raw, _) = signed(P256_SEC1_PEM, "interop");
        assert!(verifies(&p256_raw, "interop", P256_INTEROP_SIGNATURE));
    }

    #[test]
    fn flipped_message_or_signature_byte_is_false_not_an_error() {
        let cases = [
            (RSA_PKCS8_PEM, "rsa-pkcs1-sha256"),
            (P256_SEC1_PEM, "ecdsa-p256-sha256"),
            (P384_SEC1_PEM, "ecdsa-p384-sha384"),
        ];
        for (pem, algorithm) in cases {
            let (_, public, signature) = signed(pem, "tamper");
            assert!(verifies(&public, "tamper", &signature), "{algorithm}");
            assert!(!verifies(&public, "tampex", &signature), "{algorithm}");
            assert!(!verifies(&public, "", &signature), "{algorithm}");
            assert!(
                !verifies(&public, "tamper", &flip_last_byte(&signature)),
                "{algorithm}"
            );
        }
    }

    #[test]
    fn key_of_one_algorithm_against_another_algorithms_signature() {
        let (_, rsa_public, _) = signed(RSA_PKCS8_PEM, "cross");
        let (_, p256_public, p256_signature) = signed(P256_SEC1_PEM, "cross");
        let (_, _, rsa_signature) = signed(RSA_PKCS8_PEM, "cross");
        // A DER P-256 signature (about 70 bytes) fits the RSA length rule, so IronCrypto
        // runs RSA verification on it and reports a plain false.
        assert_eq!(
            verify_with(&rsa_public, "cross", &p256_signature)
                .expect("RSA key with a P-256 DER signature is well-formed"),
            Value::Bool(false)
        );
        assert_bad_arg(verify_with(&p256_public, "cross", &rsa_signature));
    }

    #[test]
    fn returned_records_carry_exact_algorithm_strings() {
        assert_eq!(signed(RSA_PKCS8_PEM, "x").0, "rsa-pkcs1-sha256");
        assert_eq!(signed(P256_PKCS8_PEM, "x").0, "ecdsa-p256-sha256");
        assert_eq!(signed(P384_SEC1_PEM, "x").0, "ecdsa-p384-sha384");
        let (_, public, _) = signed(P384_SEC1_PEM, "x");
        assert_eq!(STANDARD.decode(public).expect("Base64").len(), 97);
        let (_, public, _) = signed(P256_SEC1_PEM, "x");
        assert_eq!(STANDARD.decode(public).expect("Base64").len(), 65);
    }

    #[test]
    fn signature_longer_than_its_algorithm_allows_is_bad_arg() {
        let (_, p256_public, _) = signed(P256_SEC1_PEM, "long");
        assert_bad_arg(verify_with(
            &p256_public,
            "long",
            &STANDARD.encode(vec![0x30u8; 73]),
        ));
        let (_, rsa_public, _) = signed(RSA_PKCS8_PEM, "long");
        assert_bad_arg(verify_with(
            &rsa_public,
            "long",
            &STANDARD.encode(vec![0u8; 513]),
        ));
        let (_, ed_public, _) = signed(&text_of(&generate_key("ed25519", 0).expect("key")), "long");
        assert_bad_arg(verify_with(
            &ed_public,
            "long",
            &STANDARD.encode(vec![0u8; 65]),
        ));
        assert_bad_arg(verify_with(&p256_public, "long", &"A".repeat(1401)));
    }

    #[test]
    fn der_reader_rejects_indefinite_non_minimal_trailing_and_truncated() {
        // Indefinite length.
        assert!(whole(&[0x30, 0x80, 0x02, 0x01, 0x01, 0x00, 0x00], TAG_SEQUENCE).is_none());
        // Long form for a length that fits the short form.
        assert!(whole(&[0x30, 0x81, 0x01, 0x00], TAG_SEQUENCE).is_none());
        // Leading zero in the long-form length.
        assert!(whole(&[0x30, 0x82, 0x00, 0x80], TAG_SEQUENCE).is_none());
        // Trailing bytes after the element.
        assert!(whole(&[0x05, 0x00, 0x00], TAG_NULL).is_none());
        // Truncated: no length, short contents, and a long form without digits.
        assert!(whole(&[0x30], TAG_SEQUENCE).is_none());
        assert!(whole(&[0x30, 0x05, 0x02], TAG_SEQUENCE).is_none());
        assert!(whole(&[0x30, 0x84], TAG_SEQUENCE).is_none());
        assert!(whole(&[], TAG_SEQUENCE).is_none());
        // Five-byte long form is refused rather than read.
        assert!(whole(
            &[0x30, 0x85, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00],
            TAG_SEQUENCE
        )
        .is_none());
        // A valid element, and a BIT STRING with unused bits, behave as specified.
        assert!(whole(&[0x05, 0x00], TAG_NULL).is_some_and(|contents| contents.is_empty()));
        assert!(bit_string(&[0x01, 0x00]).is_none());
        assert!(bit_string(&[0x00, 0xaa]).is_some());
    }

    #[test]
    fn spki_parser_never_panics_on_truncation_or_trailing_bytes() {
        let key = STANDARD.decode(
            RSA_SPKI_PEM
                .lines()
                .filter(|line| !line.starts_with("-----"))
                .collect::<String>(),
        );
        let der = key.expect("fixture Base64");
        assert!(spki_verifier(&der).is_some());
        for end in 0..der.len() {
            assert!(spki_verifier(&der[..end]).is_none(), "prefix {end}");
        }
        let mut trailing = der.clone();
        trailing.push(0);
        assert!(spki_verifier(&trailing).is_none());
    }

    #[test]
    fn unsupported_curve_is_bad_arg_for_sign_and_for_spki() {
        assert_bad_arg(sign(vec![s(SECP256K1_SEC1_PEM), s("x")], None));
        // secp256k1 SPKI: id-ecPublicKey with the secp256k1 OID (1.3.132.0.10).
        let curve = [0x2b, 0x81, 0x04, 0x00, 0x0a];
        let algorithm = [
            tlv_bytes(TAG_OID, OID_EC_PUBLIC_KEY),
            tlv_bytes(TAG_OID, &curve),
        ]
        .concat();
        let mut point = vec![0x04];
        point.extend_from_slice(&[7u8; 64]);
        let mut bits = vec![0x00];
        bits.extend_from_slice(&point);
        let body = [
            tlv_bytes(TAG_SEQUENCE, &algorithm),
            tlv_bytes(TAG_BIT_STRING, &bits),
        ]
        .concat();
        let spki = tlv_bytes(TAG_SEQUENCE, &body);
        assert_bad_arg(verify_with(
            &pem_public(&spki),
            "x",
            &STANDARD.encode([0u8; 70]),
        ));
    }

    #[test]
    fn unknown_spki_algorithm_is_bad_arg() {
        // Ed448: 1.3.101.113, which this module does not support.
        let algorithm = tlv_bytes(TAG_SEQUENCE, &tlv_bytes(TAG_OID, &[0x2b, 0x65, 0x71]));
        let mut bits = vec![0x00];
        bits.extend_from_slice(&[9u8; 57]);
        let body = [algorithm, tlv_bytes(TAG_BIT_STRING, &bits)].concat();
        let spki = tlv_bytes(TAG_SEQUENCE, &body);
        assert_bad_arg(verify_with(
            &pem_public(&spki),
            "x",
            &STANDARD.encode([0u8; 114]),
        ));
    }

    #[test]
    fn ed25519_behavior_is_unchanged() {
        let key = text_of(&generate_key("ed25519", 0).expect("key"));
        let (algorithm, public, signature) = signed(&key, "ed");
        assert_eq!(algorithm, "ed25519");
        assert_eq!(STANDARD.decode(&public).expect("Base64").len(), 32);
        assert_eq!(STANDARD.decode(&signature).expect("Base64").len(), 64);
        assert!(verifies(&public, "ed", &signature));
        assert!(!verifies(&public, "ee", &signature));
        assert!(!verifies(&public, "ed", &flip_last_byte(&signature)));
        assert_bad_arg(verify_with(&public, "ed", &STANDARD.encode([0u8; 63])));
    }

    #[test]
    fn private_key_envelopes_and_caps_are_coded() {
        assert_bad_arg(sign(vec![s("secret-invalid-key"), s("x")], None));
        assert_bad_arg(sign(
            vec![
                s("-----BEGIN EC PRIVATE KEY-----\nAA==\n-----END EC PRIVATE KEY-----\n"),
                s("x"),
            ],
            None,
        ));
        let oversized = format!(
            "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----\n",
            "A".repeat(MAX_PRIVATE_KEY)
        );
        let error = sign(vec![s(&oversized), s("x")], None).expect_err("oversized key");
        assert_eq!(code(&error), Some("E_BUDGET_EXCEEDED"), "{error}");
    }

    #[test]
    fn generated_keys_are_64_column_pkcs8_pem_that_sign_and_verify() {
        for (kind, algorithm) in [("ec", "ecdsa-p256-sha256"), ("ed25519", "ed25519")] {
            let pem = text_of(&generate_key(kind, 0).expect("key generation"));
            let lines: Vec<&str> = pem.lines().collect();
            assert_eq!(lines.first(), Some(&"-----BEGIN PRIVATE KEY-----"));
            assert_eq!(lines.last(), Some(&"-----END PRIVATE KEY-----"));
            assert!(
                lines[1..lines.len() - 1]
                    .iter()
                    .all(|line| line.len() <= 64),
                "{kind}: body lines are wider than 64 columns"
            );
            let (returned, public, signature) = signed(&pem, "generated");
            assert_eq!(returned, algorithm);
            assert!(verifies(&public, "generated", &signature), "{kind}");
        }
    }

    #[test]
    fn ecdsa_signatures_are_deterministic_rfc6979() {
        let first = signed(P256_PKCS8_PEM, "same message");
        let second = signed(P256_PKCS8_PEM, "same message");
        assert_eq!(first, second);
        let (_, _, other) = signed(P256_PKCS8_PEM, "other message");
        assert_ne!(first.2, other);
    }

    #[test]
    fn rsa_generation_sizes_are_restricted() {
        for bits in [2047, 4097, 8192, 16384] {
            assert_bad_arg(generate_key("rsa", bits).map(|_| Value::Null));
        }
    }
}
