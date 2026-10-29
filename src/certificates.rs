//! Certificate inspection and chain verification for `crypto_cert_info` and
//! `crypto_cert_verify`, on IronSocketLayer's X.509 (IronPKI, over IronCrypto).
//!
//! No external process is run. Parsing, names, validity and chain validation
//! are IronPKI's; PEM decoding and SHA-256 are IronCrypto's. This module adds
//! the two things IronPKI does not provide for display: an RFC 4514 rendering
//! of a Name, which a small bounds-checked DER walker performs here, and the
//! mapping of IronPKI's error kinds onto AetherShell's coded errors.

use std::collections::BTreeMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::time::{SystemTime, UNIX_EPOCH};

use ironsocketlayer::x509::{
    Certificate, Limits, Purpose, RootStore, VerifyOptions, VERIFY_ALGORITHMS,
};

use crate::safety::{self, ErrorCode, SafetyError};
use crate::value::Value;

/// The most certificates one file may hold: PEM blocks, or the one DER.
pub const MAX_CERTIFICATES: usize = 1000;

/// The trust-store bound `crypto_cert_verify` raises to, so that a full
/// system bundle (about 150 anchors) loads. Every other bound is IronPKI's
/// default.
const VERIFY_MAX_ANCHORS: usize = 4096;

const PEM_BEGIN: &[u8] = b"-----BEGIN CERTIFICATE-----";

/// The DER certificates of a file, each parsed once to check it.
///
/// A file containing `-----BEGIN CERTIFICATE-----` is read as PEM: every
/// `CERTIFICATE` block must decode strictly, and other labels are skipped. Any
/// other file is one DER certificate. A block or certificate that fails is
/// `E_BAD_ARG` naming its 1-based index.
pub fn certificates_in(builtin: &str, bytes: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
    let ders = if bytes.windows(PEM_BEGIN.len()).any(|w| w == PEM_BEGIN) {
        pem_certificates(builtin, bytes)?
    } else {
        vec![bytes.to_vec()]
    };
    for (i, der) in ders.iter().enumerate() {
        if let Err(e) = Certificate::parse(der) {
            return Err(safety::arg_err(format!(
                "{builtin}: certificate #{} is not a valid X.509 certificate ({})",
                i + 1,
                e.id()
            )));
        }
    }
    Ok(ders)
}

fn pem_certificates(builtin: &str, bytes: &[u8]) -> anyhow::Result<Vec<Vec<u8>>> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines = text.lines().map(str::trim);
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut index = 0usize;
    while let Some(line) = lines.next() {
        let Some(label) = line
            .strip_prefix("-----BEGIN ")
            .and_then(|rest| rest.strip_suffix("-----"))
        else {
            continue;
        };
        let end = format!("-----END {label}-----");
        let mut body: Vec<&str> = Vec::new();
        let mut closed = false;
        for l in lines.by_ref() {
            if l == end {
                closed = true;
                break;
            }
            body.push(l);
        }
        if label != "CERTIFICATE" {
            continue;
        }
        index += 1;
        if !closed {
            return Err(safety::arg_err(format!(
                "{builtin}: CERTIFICATE block #{index} has no matching END line"
            )));
        }
        if out.len() >= MAX_CERTIFICATES {
            return Err(safety::budget_exceeded(
                builtin,
                format!("the file holds more than {MAX_CERTIFICATES} certificates"),
                "split the bundle into smaller files",
            ));
        }
        let block = format!(
            "-----BEGIN CERTIFICATE-----\n{}\n-----END CERTIFICATE-----\n",
            body.join("\n")
        );
        let mut der = vec![0u8; block.len()];
        let n = ironcrypto::pkix::pem::decode("CERTIFICATE", block.as_bytes(), &mut der).map_err(
            |_| {
                safety::arg_err(format!(
                    "{builtin}: CERTIFICATE block #{index} is not valid strict PEM"
                ))
            },
        )?;
        der.truncate(n);
        out.push(der);
    }
    if out.is_empty() {
        return Err(safety::arg_err(format!(
            "{builtin}: no CERTIFICATE block found in the file"
        )));
    }
    Ok(out)
}

/// `crypto_cert_info`: one record per certificate, in file order.
pub fn inspect(builtin: &str, bytes: &[u8]) -> anyhow::Result<Vec<Value>> {
    let ders = certificates_in(builtin, bytes)?;
    let mut out = Vec::with_capacity(ders.len());
    for (i, der) in ders.iter().enumerate() {
        let cert = Certificate::parse(der).map_err(|e| {
            safety::arg_err(format!(
                "{builtin}: certificate #{} is not a valid X.509 certificate ({})",
                i + 1,
                e.id()
            ))
        })?;
        out.push(record(builtin, i + 1, der, &cert)?);
    }
    Ok(out)
}

fn record(
    builtin: &str,
    index: usize,
    der: &[u8],
    cert: &Certificate<'_>,
) -> anyhow::Result<Value> {
    let name_error = |which: &str, why: String| {
        safety::arg_err(format!(
            "{builtin}: certificate #{index} has a {which} name that cannot be rendered ({why})"
        ))
    };
    let subject = render_name(cert.subject_der()).map_err(|w| name_error("subject", w))?;
    let issuer = render_name(cert.issuer_der()).map_err(|w| name_error("issuer", w))?;
    let not_before = unix_secs(builtin, cert.not_before())?;
    let not_after = unix_secs(builtin, cert.not_after())?;
    let digest = <ironcrypto::hash::Sha256 as ironcrypto::core_types::traits::Digest>::digest(der)
        .map_err(|e| safety::ic_failed(builtin, e))?;
    let optional_str = |s: Option<&str>| match s {
        Some(s) => Value::Str(s.to_string()),
        None => Value::Null,
    };
    let strings = |items: Vec<&str>| {
        Value::Array(
            items
                .into_iter()
                .map(|s| Value::Str(s.to_string()))
                .collect(),
        )
    };
    let ips: Vec<Value> = cert
        .ip_addresses()
        .iter()
        .map(|ip| Value::Str(ip_text(ip.octets())))
        .collect();
    let signature_algorithm = match cert.signature_scheme() {
        Ok(scheme) => scheme.id().to_string(),
        Err(_) => "unsupported".to_string(),
    };
    Ok(rec(vec![
        ("subject", Value::Str(subject)),
        ("issuer", Value::Str(issuer)),
        ("subject_cn", optional_str(cert.common_name())),
        ("issuer_cn", optional_str(cert.issuer_common_name())),
        ("serial", Value::Str(hex::encode(cert.serial()))),
        ("not_before", Value::Str(rfc3339(not_before))),
        ("not_after", Value::Str(rfc3339(not_after))),
        ("not_before_unix", Value::Int(not_before)),
        ("not_after_unix", Value::Int(not_after)),
        ("dns_names", strings(cert.dns_names())),
        ("uri_names", strings(cert.uri_names())),
        ("ip_addresses", Value::Array(ips)),
        ("is_ca", Value::Bool(cert.is_ca())),
        (
            "self_issued",
            Value::Bool(cert.issuer_der() == cert.subject_der()),
        ),
        ("public_key", public_key_record(cert)),
        ("signature_algorithm", Value::Str(signature_algorithm)),
        ("sha256_fingerprint", Value::Str(hex::encode(digest))),
        ("extension_count", Value::Int(cert.extension_count() as i64)),
    ]))
}

/// The public key's algorithm and size. `algorithm` is one of the names below
/// or `unknown`; `bits` is the RSA modulus size or the curve size (Ed25519
/// reports 256), and Null where no size is meaningful (ML-DSA, or unknown).
fn public_key_record(cert: &Certificate<'_>) -> Value {
    let (algorithm, bits) = match cert.subject_public_key() {
        Ok(key) => {
            let kind = key.kind_id();
            let bits = match kind {
                "rsa" => key.rsa_bits().map(|b| Value::Int(b as i64)),
                "ecdsa-p256" => Some(Value::Int(256)),
                "ecdsa-p384" => Some(Value::Int(384)),
                "ecdsa-p521" => Some(Value::Int(521)),
                "ed25519" => Some(Value::Int(256)),
                _ => None,
            };
            let algorithm = match kind {
                "rsa" | "ecdsa-p256" | "ecdsa-p384" | "ecdsa-p521" | "ed25519" | "ml-dsa-44"
                | "ml-dsa-65" | "ml-dsa-87" => kind,
                _ => "unknown",
            };
            (algorithm, bits.unwrap_or(Value::Null))
        }
        Err(_) => ("unknown", Value::Null),
    };
    rec(vec![
        ("algorithm", Value::Str(algorithm.to_string())),
        ("bits", bits),
    ])
}

/// `crypto_cert_verify`: whether the first certificate of the file chains to a
/// trust anchor of the system store, the rest being intermediates.
///
/// Returns `Ok(false)` for the outcomes that mean the chain does not validate.
/// Everything else is a coded error, because no answer can be given.
pub fn verify_file(builtin: &str, bytes: &[u8]) -> anyhow::Result<bool> {
    let ders = certificates_in(builtin, bytes)?;
    let Some((leaf, rest)) = ders.split_first() else {
        return Err(safety::arg_err(format!(
            "{builtin}: no certificate found in the file"
        )));
    };
    let intermediates: Vec<&[u8]> = rest.iter().map(Vec::as_slice).collect();
    let store = trust_store(builtin)?;
    let mut limits = Limits::DEFAULT;
    limits.max_anchors = VERIFY_MAX_ANCHORS;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| {
            safety::crypto_failed(
                builtin,
                "the system clock is before 1970",
                "fix the clock, then retry",
            )
        })?
        .as_secs();
    let opts = VerifyOptions::new(now, Purpose::TlsServer, VERIFY_ALGORITHMS, limits);
    match ironsocketlayer::x509::verify_chain(leaf, &intermediates, &store, &opts) {
        Ok(_) => Ok(true),
        Err(e) => verdict(builtin, e.kind().id()),
    }
}

/// The IronPKI error kind's stable identifier, mapped to the builtin's answer.
/// Matched on `ErrorKind::id()`, never on message text.
fn verdict(builtin: &str, id: &str) -> anyhow::Result<bool> {
    match id {
        "bad-certificate"
        | "certificate-expired"
        | "certificate-revoked"
        | "certificate-usage"
        | "certificate-name-mismatch"
        | "unknown-ca"
        | "bad-certificate-status"
        | "policy-violation"
        | "bad-signature"
        | "key-algorithm-mismatch" => Ok(false),
        "unsupported-certificate" => Err(safety::unimplemented(
            builtin,
            "the chain uses a key or algorithm this verifier does not implement, such as an RSA key over 4096 bits or an unknown critical extension",
            "verify it with a tool that supports that key or extension",
        )),
        "capacity-exceeded" => Err(safety::budget_exceeded(
            builtin,
            "the chain or the trust store exceeds the verifier's bounds (4096 trust anchors, or a longer path than 8 certificates)".to_string(),
            "use a shorter chain or a smaller trust store",
        )),
        other => Err(safety::crypto_failed(
            builtin,
            format!("IronPKI could not decide the chain ({other})"),
            "nothing was decided; check the certificate and trust store, then retry",
        )),
    }
}

/// The largest `SSL_CERT_FILE` read; system bundles are a few hundred KiB.
const MAX_TRUST_FILE: u64 = 16 * 1024 * 1024;

/// The trust store. When `SSL_CERT_FILE` is set it is the whole store, and a
/// file that cannot be read or holds no usable certificate is `E_NOT_FOUND`:
/// IronPKI's `from_system` would go on to the system bundle, so a mistyped
/// private-CA bundle would quietly trust every public CA instead. Unset, the
/// system bundle is used, and none is `E_NOT_FOUND` too.
fn trust_store(builtin: &str) -> anyhow::Result<RootStore> {
    let Some(path) = std::env::var_os("SSL_CERT_FILE").filter(|p| !p.is_empty()) else {
        return RootStore::from_system().map_err(|_| {
            no_store(
                builtin,
                format!("{builtin}: no usable trust store was found"),
                "none".to_string(),
            )
        });
    };
    let shown = std::path::Path::new(&path).display().to_string();
    let unusable = |why: String| {
        no_store(
            builtin,
            format!("{builtin}: SSL_CERT_FILE names {shown}, which {why}"),
            shown.clone(),
        )
    };
    let len = std::fs::metadata(&path)
        .map_err(|e| unusable(format!("cannot be read ({e})")))?
        .len();
    if len > MAX_TRUST_FILE {
        return Err(safety::budget_exceeded(
            builtin,
            format!("{builtin}: SSL_CERT_FILE {shown} is {len} bytes, over {MAX_TRUST_FILE}"),
            "point SSL_CERT_FILE at a smaller PEM bundle",
        ));
    }
    let text =
        std::fs::read_to_string(&path).map_err(|e| unusable(format!("cannot be read ({e})")))?;
    let mut store = RootStore::new();
    store
        .add_pem_bundle(&text)
        .map_err(|_| unusable("holds no usable PEM certificate".to_string()))?;
    Ok(store)
}

fn no_store(builtin: &str, message: String, got: String) -> anyhow::Error {
    anyhow::Error::new(SafetyError {
        code: ErrorCode::NotFound,
        message,
        builtin: builtin.to_string(),
        hint: "set SSL_CERT_FILE to a PEM bundle of trusted CA certificates, then retry"
            .to_string(),
        approval: None,
        did_you_mean: Vec::new(),
        expected: "a PEM trust store".to_string(),
        got,
    })
}

fn unix_secs(builtin: &str, secs: u64) -> anyhow::Result<i64> {
    i64::try_from(secs).map_err(|_| {
        safety::crypto_failed(
            builtin,
            "a validity time is outside the representable range",
            "the certificate's dates are not usable",
        )
    })
}

fn rec(pairs: Vec<(&str, Value)>) -> Value {
    Value::Record(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn ip_text(octets: &[u8]) -> String {
    if let Ok(v4) = <[u8; 4]>::try_from(octets) {
        return Ipv4Addr::from(v4).to_string();
    }
    match <[u8; 16]>::try_from(octets) {
        Ok(v6) => Ipv6Addr::from(v6).to_string(),
        Err(_) => hex::encode(octets),
    }
}

/// RFC 3339 UTC, to the second: `YYYY-MM-DDTHH:MM:SSZ`.
fn rfc3339(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Days since 1970-01-01 to (year, month, day), proleptic Gregorian.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

// ---------------------------------------------------------------------------
// Names: a strict DER walker and an RFC 4514 rendering.
// ---------------------------------------------------------------------------

/// One DER element: its tag, its content, its whole encoding, and the bytes
/// after it.
struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
    whole: &'a [u8],
    rest: &'a [u8],
}

/// Reads one definite-length DER element. Bounds-checked: any truncation,
/// indefinite length, or non-minimal long form is `None`.
fn tlv(input: &[u8]) -> Option<Tlv<'_>> {
    let (&tag, after_tag) = input.split_first()?;
    let (&first, after_first) = after_tag.split_first()?;
    let (len, body) = if first < 0x80 {
        (usize::from(first), after_first)
    } else {
        let n = usize::from(first & 0x7f);
        if n == 0 || n > 4 || after_first.len() < n {
            return None;
        }
        let mut len = 0usize;
        for &b in &after_first[..n] {
            len = (len << 8) | usize::from(b);
        }
        if len < 0x80 {
            return None;
        }
        (len, &after_first[n..])
    };
    if body.len() < len {
        return None;
    }
    let header = input.len() - body.len();
    Some(Tlv {
        tag,
        content: &body[..len],
        whole: &input[..header + len],
        rest: &body[len..],
    })
}

/// A Name, rendered as RFC 4514 does: RDNs last-first, joined by `,`, and the
/// attributes of a multi-valued RDN joined by `+`. Known attribute types use
/// their short names; any other is `dotted.oid=#hex`, where the hex is the
/// attribute value's whole DER encoding. Errors describe what was malformed.
fn render_name(name: &[u8]) -> Result<String, String> {
    let seq = tlv(name).ok_or("truncated Name")?;
    if seq.tag != 0x30 || !seq.rest.is_empty() {
        return Err("Name is not one SEQUENCE".into());
    }
    let mut rdns: Vec<String> = Vec::new();
    let mut body = seq.content;
    while !body.is_empty() {
        let set = tlv(body).ok_or("truncated RDN")?;
        body = set.rest;
        if set.tag != 0x31 || set.content.is_empty() {
            return Err("RDN is not a non-empty SET".into());
        }
        let mut attrs: Vec<String> = Vec::new();
        let mut ab = set.content;
        while !ab.is_empty() {
            let atv = tlv(ab).ok_or("truncated attribute")?;
            ab = atv.rest;
            if atv.tag != 0x30 {
                return Err("attribute is not a SEQUENCE".into());
            }
            let oid = tlv(atv.content).ok_or("truncated attribute type")?;
            if oid.tag != 0x06 {
                return Err("attribute type is not an OBJECT IDENTIFIER".into());
            }
            let value = tlv(oid.rest).ok_or("truncated attribute value")?;
            if !value.rest.is_empty() {
                return Err("attribute has trailing data".into());
            }
            attrs.push(render_attribute(oid.content, &value)?);
        }
        rdns.push(attrs.join("+"));
    }
    rdns.reverse();
    Ok(rdns.join(","))
}

fn render_attribute(oid_content: &[u8], value: &Tlv<'_>) -> Result<String, String> {
    let dotted = oid_dotted(oid_content).ok_or("malformed attribute type")?;
    match short_name(&dotted) {
        Some(short) => match string_value(value) {
            Some(text) => Ok(format!("{short}={}", escape(&text))),
            None => Ok(format!("{short}=#{}", hex::encode(value.whole))),
        },
        None => Ok(format!("{dotted}=#{}", hex::encode(value.whole))),
    }
}

fn short_name(dotted: &str) -> Option<&'static str> {
    Some(match dotted {
        "2.5.4.3" => "CN",
        "2.5.4.5" => "serialNumber",
        "2.5.4.6" => "C",
        "2.5.4.7" => "L",
        "2.5.4.8" => "ST",
        "2.5.4.10" => "O",
        "2.5.4.11" => "OU",
        "1.2.840.113549.1.9.1" => "emailAddress",
        "0.9.2342.19200300.100.1.25" => "DC",
        _ => return None,
    })
}

/// The text of a string-typed value, or `None` for any other type or for
/// bytes that do not decode in their type.
fn string_value(value: &Tlv<'_>) -> Option<String> {
    match value.tag {
        // UTF8String
        0x0c => std::str::from_utf8(value.content).ok().map(str::to_string),
        // PrintableString, IA5String: ASCII only.
        0x13 | 0x16 => {
            if value.content.is_ascii() {
                std::str::from_utf8(value.content).ok().map(str::to_string)
            } else {
                None
            }
        }
        // TeletexString, read as Latin-1, which is how the common cases use it.
        0x14 => Some(value.content.iter().map(|&b| char::from(b)).collect()),
        // BMPString: big-endian UTF-16.
        0x1e => {
            if !value.content.len().is_multiple_of(2) {
                return None;
            }
            let units: Vec<u16> = value
                .content
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            String::from_utf16(&units).ok()
        }
        _ => None,
    }
}

/// RFC 4514 escaping of a string value.
fn escape(text: &str) -> String {
    let count = text.chars().count();
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        match c {
            ',' | '+' | '"' | '\\' | '<' | '>' | ';' => {
                out.push('\\');
                out.push(c);
            }
            '#' if i == 0 => out.push_str("\\#"),
            ' ' if i == 0 || i + 1 == count => out.push_str("\\ "),
            '\0' => out.push_str("\\00"),
            _ => out.push(c),
        }
    }
    out
}

/// The dotted form of an OBJECT IDENTIFIER's content octets.
fn oid_dotted(content: &[u8]) -> Option<String> {
    let mut arcs: Vec<u128> = Vec::new();
    let mut cur: u128 = 0;
    for (i, &b) in content.iter().enumerate() {
        cur = cur.checked_mul(128)?.checked_add(u128::from(b & 0x7f))?;
        if b & 0x80 == 0 {
            arcs.push(cur);
            cur = 0;
        } else if i + 1 == content.len() {
            return None;
        }
    }
    let mut it = arcs.iter();
    let first = *it.next()?;
    let (a, b) = if first < 80 {
        (first / 40, first % 40)
    } else {
        (2, first - 80)
    };
    let mut out = format!("{a}.{b}");
    for arc in it {
        out.push('.');
        out.push_str(&arc.to_string());
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates_match_the_epoch_and_a_leap_day() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        // 2000-02-29T12:34:56Z
        assert_eq!(rfc3339(951_827_696), "2000-02-29T12:34:56Z");
        assert_eq!(rfc3339(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn oid_arcs_decode_in_dotted_form() {
        // 2.5.4.3 (commonName) and 1.2.840.113549.1.9.1 (emailAddress).
        assert_eq!(oid_dotted(&[0x55, 0x04, 0x03]).as_deref(), Some("2.5.4.3"));
        assert_eq!(
            oid_dotted(&[0x2a, 0x86, 0x48, 0x86, 0xf7, 0x0d, 0x01, 0x09, 0x01]).as_deref(),
            Some("1.2.840.113549.1.9.1")
        );
        assert_eq!(oid_dotted(&[0x55, 0x84]), None, "truncated arc");
    }

    #[test]
    fn names_render_last_rdn_first_with_escapes() {
        // Name = SEQUENCE { SET { CN "a,b" }, SET { C "US" } }, built from parts.
        let attr = |oid: &[u8], tag: u8, val: &[u8]| {
            let mut v = vec![
                0x30,
                (2 + oid.len() + 2 + val.len()) as u8,
                0x06,
                oid.len() as u8,
            ];
            v.extend_from_slice(oid);
            v.push(tag);
            v.push(val.len() as u8);
            v.extend_from_slice(val);
            let mut set = vec![0x31, v.len() as u8];
            set.extend(v);
            set
        };
        let mut name_body = attr(&[0x55, 0x04, 0x03], 0x0c, b"a,b");
        name_body.extend(attr(&[0x55, 0x04, 0x06], 0x13, b"US"));
        let mut name = vec![0x30, name_body.len() as u8];
        name.extend(name_body);
        assert_eq!(render_name(&name).as_deref(), Ok("C=US,CN=a\\,b"));
    }
}
