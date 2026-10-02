//! Native Ed25519 signatures. Key material is supplied as data, never a path.
use crate::{safety, value::Value};
use anyhow::Result;
use base64::{engine::general_purpose::STANDARD, Engine};
use ring::signature::{Ed25519KeyPair, KeyPair, UnparsedPublicKey, ED25519};
use std::collections::BTreeMap;
use zeroize::Zeroizing;
fn bad(name: &str, message: &str) -> anyhow::Error {
    safety::arg_err(format!("{name}: {message}"))
}
fn policy(name: &str) -> Result<()> {
    if safety::fips_enabled() {
        return Err(safety::policy_deny(
            name,
            "native Ed25519 backend is not FIPS validated",
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
fn private_key(name: &str, value: &Value) -> Result<Ed25519KeyPair> {
    let pem = text(name, value)?;
    if pem.len() > 4096 {
        return Err(safety::budget_exceeded(
            name,
            "private key envelope exceeds 4096 bytes".into(),
            "supply an Ed25519 PKCS#8 v2 PEM key",
        ));
    }
    let body = pem
        .trim_matches(|c: char| c.is_ascii_whitespace())
        .strip_prefix("-----BEGIN PRIVATE KEY-----")
        .and_then(|body| body.strip_suffix("-----END PRIVATE KEY-----"))
        .ok_or_else(|| {
            bad(
                name,
                "expected one unencrypted PKCS#8 v2 PRIVATE KEY PEM envelope",
            )
        })?;
    let body = Zeroizing::new(
        body.chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect::<String>(),
    );
    let bytes = Zeroizing::new(
        STANDARD
            .decode(body.as_bytes())
            .map_err(|_| bad(name, "invalid private key encoding"))?,
    );
    Ed25519KeyPair::from_pkcs8(&bytes).map_err(|_| {
        bad(
            name,
            "invalid or inconsistent Ed25519 PKCS#8 v2 private key",
        )
    })
}
fn decoded(name: &str, value: &Value, length: usize, max_encoded: usize) -> Result<Vec<u8>> {
    let encoded = text(name, value)?;
    if encoded.len() > max_encoded {
        return Err(bad(
            name,
            "key or signature encoding exceeds its fixed-size limit",
        ));
    }
    let bytes = STANDARD.decode(encoded).map_err(|_| {
        bad(
            name,
            "key and signature must be canonical padded standard Base64",
        )
    })?;
    if bytes.len() != length {
        return Err(bad(
            name,
            "public key must be 32 bytes and signature must be 64 bytes",
        ));
    }
    Ok(bytes)
}
pub(super) fn sign(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const NAME: &str = "crypto_sign";
    shape(NAME, &args, &input, 2)?;
    policy(NAME)?;
    let message = message(NAME, &args[1])?;
    let key = private_key(NAME, &args[0])?;
    let signature = key.sign(&message);
    Ok(Value::Record(BTreeMap::from([
        ("algorithm".into(), Value::Str("ed25519".into())),
        (
            "public_key".into(),
            Value::Str(STANDARD.encode(key.public_key().as_ref())),
        ),
        (
            "signature".into(),
            Value::Str(STANDARD.encode(signature.as_ref())),
        ),
    ])))
}
pub(super) fn verify(args: Vec<Value>, input: Option<Value>) -> Result<Value> {
    const NAME: &str = "crypto_verify_signature";
    shape(NAME, &args, &input, 3)?;
    policy(NAME)?;
    let key = decoded(NAME, &args[0], 32, 64)?;
    let signature = decoded(NAME, &args[2], 64, 128)?;
    let message = message(NAME, &args[1])?;
    Ok(Value::Bool(
        UnparsedPublicKey::new(&ED25519, key)
            .verify(&message, &signature)
            .is_ok(),
    ))
}
pub(super) fn generate_key() -> Result<Value> {
    const NAME: &str = "crypto_generate_key";
    policy(NAME)?;
    let document =
        Ed25519KeyPair::generate_pkcs8(&ring::rand::SystemRandom::new()).map_err(|_| {
            safety::bad_state(
                NAME,
                "secure key generation failed",
                "check operating-system randomness availability",
            )
        })?;
    let encoded = Zeroizing::new(STANDARD.encode(document.as_ref()));
    let mut pem = String::from("-----BEGIN PRIVATE KEY-----\n");
    for chunk in encoded.as_bytes().chunks(64) {
        pem.push_str(std::str::from_utf8(chunk).expect("Base64 is ASCII"));
        pem.push('\n');
    }
    pem.push_str("-----END PRIVATE KEY-----\n");
    Ok(Value::Str(pem))
}
