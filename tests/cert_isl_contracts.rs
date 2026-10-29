//! `crypto_cert_info` and `crypto_cert_verify` on IronSocketLayer's X.509.
//!
//! The fixtures are PEM text generated once with OpenSSL 3.5 (not run here):
//! a P-256 CA "AE Test CA" (serial 0x1001), a leaf "leaf.example" signed by it
//! (serial 0x2002, SAN DNS:leaf.example and IP:127.0.0.1), an expired leaf
//! (2020 to 2021, serial 0x3003), an unrelated CA, and a self-signed RSA-2048
//! certificate. Every validity is fixed, so the expected dates are constants.
//! The fingerprints are openssl's `x509 -fingerprint -sha256`.
//!
//! verify reads the trust store from SSL_CERT_FILE, which is process-wide, so
//! every test that sets it holds ENV_LOCK and restores it.

use aethershell::builtins::call;
use aethershell::env::Env;
use aethershell::value::Value;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

const CA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIBWzCCAQKgAwIBAgICEAEwCgYIKoZIzj0EAwIwFTETMBEGA1UEAwwKQUUgVGVz
dCBDQTAeFw0yNTAxMDEwMDAwMDBaFw0zNTAxMDEwMDAwMDBaMBUxEzARBgNVBAMM
CkFFIFRlc3QgQ0EwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAARGyIyedh38TApZ
NMSzt+sh3WzK/o0VweVjZ7X0KoNJKeLvM28MHUu9jGRPF2jEjyWOvVQx4wqN7m5T
umBZJgTio0IwQDAPBgNVHRMBAf8EBTADAQH/MA4GA1UdDwEB/wQEAwIBBjAdBgNV
HQ4EFgQU7DG8nqLSGWMC9BQNPqVOG1RQhwcwCgYIKoZIzj0EAwIDRwAwRAIgPQhj
KImUoaASN6HoGd+tbBfzEQKtNIBv+Uzr3EArzacCIHBNfWZbafC9HSlFHCReq4fr
S+TcLhqLwON26m7pNpEa
-----END CERTIFICATE-----
"#;

const LEAF_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIBsjCCAVigAwIBAgICIAIwCgYIKoZIzj0EAwIwFTETMBEGA1UEAwwKQUUgVGVz
dCBDQTAeFw0yNTAxMDEwMDAwMDBaFw0zNTAxMDEwMDAwMDBaMBcxFTATBgNVBAMM
DGxlYWYuZXhhbXBsZTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABFWnXOte2Cru
VlyFoXPPDpwZrAIky1za7X5D3cXLxMBq2Jo4+DBAIqrJSMQvfF+vZg+PUcjdralY
oHoZ20mNQWejgZUwgZIwDAYDVR0TAQH/BAIwADAOBgNVHQ8BAf8EBAMCB4AwEwYD
VR0lBAwwCgYIKwYBBQUHAwEwHQYDVR0RBBYwFIIMbGVhZi5leGFtcGxlhwR/AAAB
MB0GA1UdDgQWBBRyecYLCaIXqnVpmejk0PdTVvC+lTAfBgNVHSMEGDAWgBTsMbye
otIZYwL0FA0+pU4bVFCHBzAKBggqhkjOPQQDAgNIADBFAiEA6kohnaT/Q4mgK11x
jFd4Ix/PeAP2I4kJDIAn58PUEJACIHSd5jC4GRvFQ2jES9xgks96/SgkvKMizFn+
7gyWIq2F
-----END CERTIFICATE-----
"#;

const EXPIRED_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIBtTCCAVugAwIBAgICMAMwCgYIKoZIzj0EAwIwFTETMBEGA1UEAwwKQUUgVGVz
dCBDQTAeFw0yMDAxMDEwMDAwMDBaFw0yMTAxMDEwMDAwMDBaMBoxGDAWBgNVBAMM
D2V4cGlyZWQuZXhhbXBsZTBZMBMGByqGSM49AgEGCCqGSM49AwEHA0IABNEUrJ3t
ddDal620zG7+F/IxRKlwLdhUFFSkuzCNJuRNOqn4YQ4j4Ip54k6VYnC1CRbrz+BX
iXKTFZMms9thi8ujgZUwgZIwDAYDVR0TAQH/BAIwADAOBgNVHQ8BAf8EBAMCB4Aw
EwYDVR0lBAwwCgYIKwYBBQUHAwEwHQYDVR0RBBYwFIIMbGVhZi5leGFtcGxlhwR/
AAABMB0GA1UdDgQWBBRkEFDG8AJVqJUmRAyJSb+Bv76LKTAfBgNVHSMEGDAWgBTs
MbyeotIZYwL0FA0+pU4bVFCHBzAKBggqhkjOPQQDAgNIADBFAiA2rmWxUYgryeAC
a87nZgNwj0bii7prulZp4pEdu8TwigIhAMCx+UdtMZq9nNcpD2nLhvT0qCK678vU
zfHzHx6YVM0R
-----END CERTIFICATE-----
"#;

const UNRELATED_CA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIBYDCCAQagAwIBAgICQAQwCgYIKoZIzj0EAwIwFzEVMBMGA1UEAwwMVW5yZWxh
dGVkIENBMB4XDTI1MDEwMTAwMDAwMFoXDTM1MDEwMTAwMDAwMFowFzEVMBMGA1UE
AwwMVW5yZWxhdGVkIENBMFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEG0XgHNpr
HKrdxwt3AS/zjEJ3a7vwLaqLxL0fcIXfNGrjcRkNmnFdI/57Rtz5G+n5xxW+oW3a
P3bP8bXrJeLVe6NCMEAwDwYDVR0TAQH/BAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYw
HQYDVR0OBBYEFBGmVpelmqJ0os7naKvFT1hw2ZbYMAoGCCqGSM49BAMCA0gAMEUC
IBkpiYmr0qerxiUy32ml0hu1o/BbMK+iHDPqu110PmsiAiEA3A+qGXTx/LwShkCN
qy7xWa5NfIk/sULZyyPqZBdbanM=
-----END CERTIFICATE-----
"#;

const RSA_PEM: &str = r#"-----BEGIN CERTIFICATE-----
MIIDKzCCAhOgAwIBAgIUbCY8qWl5/rGnHXdJuL8nqn9PFk0wDQYJKoZIhvcNAQEL
BQAwGTEXMBUGA1UEAwwOYWUtcnNhLmV4YW1wbGUwHhcNMjYxMDI5MTcyOTMzWhcN
MzYxMDI2MTcyOTMzWjAZMRcwFQYDVQQDDA5hZS1yc2EuZXhhbXBsZTCCASIwDQYJ
KoZIhvcNAQEBBQADggEPADCCAQoCggEBANDTnpZ/qJcB0H1JYYg7uFnRkWRnctZy
vpaO98s2QZHQiH8vPNpswqf0UqcSKHA+J3ZFPP7dMbOoF1SgUQwgrZb+3Hb7Dr9g
8vHqkR7Jeibb74aDLRLGWWKqKeKv4g7kZXOt/28unP31pHUgp24GClqPX9Im69/Y
jKk9oTRg3JlTDqP9nB0JuZhTQBs68N2QhVWrenRLHXRrPME6o+xLTKsea6t3cvFO
OG7aJ0GSOH8fnTq3n+/i9rhxgOFzJLvWzb8p3maBD4lX1yYZzwZv+Y9D+TKfWD2Q
/csGV1MnT/+tWP7z/sJKmp/0ZRmaN7RnfrS7UgCG37pIp50v5pgADWcCAwEAAaNr
MGkwHQYDVR0OBBYEFCtgxe1hKcyzjaUj4lPs/W2NaruOMB8GA1UdIwQYMBaAFCtg
xe1hKcyzjaUj4lPs/W2NaruOMAwGA1UdEwEB/wQCMAAwGQYDVR0RBBIwEIIOYWUt
cnNhLmV4YW1wbGUwDQYJKoZIhvcNAQELBQADggEBALI4cbTNIVKuvn82uo8XML1O
idRPEDyvRDAqjYNdst7e8aZKgwIRRu71s5XG8gjzW8AuN8+kYsFSIheInXDmw48P
2g2C4nUtLhccVWUIbqZaAXYZ47bbCjQUmXtzHpXP67hJf6EPQ8QxxXSnj+AadjOz
KfmlXSL5VtzY/ujGZQgHDowaGJhgO+BvCu70UPgAmdG6i/jnu+9PSNgWqBMuXQ8T
ynWLHlBntnLAQ3L6xYLV0dJ3xSOZC4OmhHl/EH4Ybuigec/p6pDmv/VgMl3y1dE6
bjfyD04+IJJ7h6FHPkh+Yjb+xEamoSV6RINF1sIYvbqgrqxutWGmymdPmHKH9gc=
-----END CERTIFICATE-----
"#;

const CA_FINGERPRINT: &str = "59e3ba82091d3742dbac726df757c4d1d78914b5e7d5b1076d34eafdb19bc6d6";
const LEAF_FINGERPRINT: &str = "f835a50c638974b8f7070560d321ad824d86aa2ceecbfb9a8972e49cfd8669cf";
const EXPIRED_FINGERPRINT: &str =
    "7e6b830395651748abacf5b9b90acccb396026a44b1e996ffe4233339d0b4924";

/// A scratch directory of this test, removed on drop.
struct Scratch(PathBuf);

impl Scratch {
    fn new(label: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "ae-cert-isl-{label}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, text).expect("write fixture");
        path.to_string_lossy().into_owned()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The session directory every test uses, so no test touches a real journal.
fn isolated_env() -> Env {
    let session = std::env::temp_dir().join("ae-cert-isl-session");
    std::env::set_var("AETHER_SESSION_DIR", &session);
    Env::new()
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

/// The first `E_...` code in an error's text.
fn code_of(e: &anyhow::Error) -> String {
    let msg = e.to_string();
    let at = msg
        .find("E_")
        .unwrap_or_else(|| panic!("uncoded refusal: {msg}"));
    msg[at..]
        .chars()
        .take_while(|c| c.is_ascii_uppercase() || *c == '_')
        .collect()
}

fn records(v: Value) -> Vec<BTreeMap<String, Value>> {
    match v {
        Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                Value::Record(r) => r,
                other => panic!("expected a Record, got {other:?}"),
            })
            .collect(),
        other => panic!("expected an Array, got {other:?}"),
    }
}

fn strings(r: &BTreeMap<String, Value>, field: &str) -> Vec<String> {
    match r.get(field) {
        Some(Value::Array(items)) => items
            .iter()
            .map(|v| match v {
                Value::Str(s) => s.clone(),
                other => panic!("{field}: expected a String, got {other:?}"),
            })
            .collect(),
        other => panic!("{field}: expected an Array, got {other:?}"),
    }
}

fn str_field<'a>(r: &'a BTreeMap<String, Value>, field: &str) -> &'a str {
    match r.get(field) {
        Some(Value::Str(s)) => s,
        other => panic!("{field}: expected a String, got {other:?}"),
    }
}

/// Converts a single-block PEM to DER with the standard Base64 alphabet.
fn pem_to_der(pem: &str) -> Vec<u8> {
    let body: String = pem.lines().filter(|l| !l.starts_with("-----")).collect();
    STANDARD.decode(body).expect("fixture base64")
}

/// Calls `crypto_cert_verify` with SSL_CERT_FILE set to `trust`, restoring the
/// variable afterwards. The caller holds ENV_LOCK.
fn verify_under(trust: &str, file: &str) -> anyhow::Result<Value> {
    std::env::set_var("SSL_CERT_FILE", trust);
    let mut env = isolated_env();
    let result = call("crypto_cert_verify", vec![text(file)], &mut env);
    std::env::remove_var("SSL_CERT_FILE");
    result
}

#[test]
fn cert_info_returns_one_record_per_certificate_of_a_bundle() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("bundle");
    let bundle = dir.write("bundle.pem", &format!("{CA_PEM}{LEAF_PEM}"));
    let mut env = isolated_env();
    let rs = records(call("crypto_cert_info", vec![text(&bundle)], &mut env).expect("cert_info"));
    assert_eq!(rs.len(), 2, "one record per certificate, in file order");

    let ca = &rs[0];
    assert_eq!(str_field(ca, "subject"), "CN=AE Test CA");
    assert_eq!(str_field(ca, "issuer"), "CN=AE Test CA");
    assert_eq!(ca.get("subject_cn"), Some(&text("AE Test CA")));
    assert_eq!(str_field(ca, "serial"), "1001");
    assert_eq!(ca.get("self_issued"), Some(&Value::Bool(true)));
    assert_eq!(ca.get("is_ca"), Some(&Value::Bool(true)));
    assert_eq!(ca.get("extension_count"), Some(&Value::Int(3)));
    assert_eq!(str_field(ca, "sha256_fingerprint"), CA_FINGERPRINT);
    assert_eq!(str_field(ca, "not_before"), "2025-01-01T00:00:00Z");
    assert_eq!(str_field(ca, "not_after"), "2035-01-01T00:00:00Z");
    assert_eq!(ca.get("not_before_unix"), Some(&Value::Int(1_735_689_600)));
    assert_eq!(ca.get("not_after_unix"), Some(&Value::Int(2_051_222_400)));
    let ca_key = match ca.get("public_key") {
        Some(Value::Record(k)) => k,
        other => panic!("public_key: {other:?}"),
    };
    assert_eq!(ca_key.get("algorithm"), Some(&text("ecdsa-p256")));
    assert_eq!(ca_key.get("bits"), Some(&Value::Int(256)));

    let leaf = &rs[1];
    assert_eq!(str_field(leaf, "subject"), "CN=leaf.example");
    assert_eq!(str_field(leaf, "issuer"), "CN=AE Test CA");
    assert_eq!(str_field(leaf, "issuer_cn"), "AE Test CA");
    assert_eq!(str_field(leaf, "serial"), "2002");
    assert_eq!(strings(leaf, "dns_names"), vec!["leaf.example"]);
    assert_eq!(strings(leaf, "ip_addresses"), vec!["127.0.0.1"]);
    assert!(strings(leaf, "uri_names").is_empty());
    assert_eq!(leaf.get("is_ca"), Some(&Value::Bool(false)));
    assert_eq!(leaf.get("self_issued"), Some(&Value::Bool(false)));
    assert_eq!(leaf.get("extension_count"), Some(&Value::Int(6)));
    assert_eq!(str_field(leaf, "sha256_fingerprint"), LEAF_FINGERPRINT);
    assert_eq!(str_field(leaf, "not_after"), "2035-01-01T00:00:00Z");
    assert_eq!(str_field(leaf, "signature_algorithm"), "ecdsa-p256-sha256");
}

#[test]
fn cert_info_reads_an_expired_rsa_and_a_der_certificate() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("kinds");
    let mut env = isolated_env();

    let rsa = dir.write("rsa.pem", RSA_PEM);
    let rs = records(call("crypto_cert_info", vec![text(&rsa)], &mut env).expect("rsa"));
    let key = match rs[0].get("public_key") {
        Some(Value::Record(k)) => k.clone(),
        other => panic!("public_key: {other:?}"),
    };
    assert_eq!(key.get("algorithm"), Some(&text("rsa")));
    assert_eq!(key.get("bits"), Some(&Value::Int(2048)));
    assert_eq!(rs[0].get("is_ca"), Some(&Value::Bool(false)));

    // One DER certificate, not PEM: the whole file is the certificate.
    let der_path = dir.0.join("leaf.der");
    std::fs::write(&der_path, pem_to_der(LEAF_PEM)).expect("write der");
    let rs = records(
        call(
            "crypto_cert_info",
            vec![text(&der_path.to_string_lossy())],
            &mut env,
        )
        .expect("der"),
    );
    assert_eq!(rs.len(), 1);
    assert_eq!(str_field(&rs[0], "subject"), "CN=leaf.example");
    assert_eq!(str_field(&rs[0], "sha256_fingerprint"), LEAF_FINGERPRINT);

    let expired = dir.write("expired.pem", EXPIRED_PEM);
    let rs = records(call("crypto_cert_info", vec![text(&expired)], &mut env).expect("expired"));
    assert_eq!(str_field(&rs[0], "not_before"), "2020-01-01T00:00:00Z");
    assert_eq!(str_field(&rs[0], "not_after"), "2021-01-01T00:00:00Z");
    assert_eq!(
        rs[0].get("not_after_unix"),
        Some(&Value::Int(1_609_459_200))
    );
    assert_eq!(str_field(&rs[0], "sha256_fingerprint"), EXPIRED_FINGERPRINT);
}

#[test]
fn cert_info_refuses_what_is_not_a_certificate_with_its_index() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("refuse");
    let mut env = isolated_env();

    let garbage = dir.write("garbage.pem", "not a certificate");
    let e = call("crypto_cert_info", vec![text(&garbage)], &mut env).expect_err("garbage");
    assert_eq!(code_of(&e), "E_BAD_ARG", "{e}");

    // A CERTIFICATE block whose body is not base64 is refused by its index,
    // and it is not silently dropped.
    let broken = format!(
        "{CA_PEM}-----BEGIN CERTIFICATE-----\n!!!! not base64 !!!!\n-----END CERTIFICATE-----\n"
    );
    let path = dir.write("broken.pem", &broken);
    let e = call("crypto_cert_info", vec![text(&path)], &mut env).expect_err("broken block");
    assert_eq!(code_of(&e), "E_BAD_ARG", "{e}");
    assert!(e.to_string().contains("#2"), "index named: {e}");

    // A CERTIFICATE block with no END line.
    let open = format!("{CA_PEM}-----BEGIN CERTIFICATE-----\nMIIB\n");
    let path = dir.write("open.pem", &open);
    let e = call("crypto_cert_info", vec![text(&path)], &mut env).expect_err("open block");
    assert_eq!(code_of(&e), "E_BAD_ARG", "{e}");
}

#[test]
fn cert_info_skips_other_pem_labels() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("labels");
    let mut env = isolated_env();
    let text_in =
        format!("-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n{LEAF_PEM}");
    let path = dir.write("mixed.pem", &text_in);
    let rs = records(call("crypto_cert_info", vec![text(&path)], &mut env).expect("mixed"));
    assert_eq!(rs.len(), 1);
    assert_eq!(str_field(&rs[0], "subject"), "CN=leaf.example");
}

#[test]
fn cert_verify_accepts_a_leaf_under_the_trusted_ca() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("verify-ok");
    let trust = dir.write("trust.pem", CA_PEM);
    let leaf = dir.write("leaf.pem", LEAF_PEM);
    // The leaf alone, and the leaf with its CA as an intermediate.
    assert_eq!(
        verify_under(&trust, &leaf).expect("verify"),
        Value::Bool(true)
    );
    let with_ca = dir.write("leaf-and-ca.pem", &format!("{LEAF_PEM}{CA_PEM}"));
    assert_eq!(
        verify_under(&trust, &with_ca).expect("verify"),
        Value::Bool(true)
    );
}

#[test]
fn cert_verify_is_false_for_expired_and_unrelated_chains() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("verify-false");
    let trust = dir.write("trust.pem", CA_PEM);
    let expired = dir.write("expired.pem", EXPIRED_PEM);
    assert_eq!(
        verify_under(&trust, &expired).expect("verify"),
        Value::Bool(false)
    );

    let unrelated = dir.write("unrelated.pem", UNRELATED_CA_PEM);
    let leaf = dir.write("leaf.pem", LEAF_PEM);
    assert_eq!(
        verify_under(&unrelated, &leaf).expect("verify"),
        Value::Bool(false)
    );
}

#[test]
fn cert_verify_never_answers_true_without_a_usable_trust_store() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("verify-empty");
    let empty = dir.write("empty.pem", "");
    let leaf = dir.write("leaf.pem", LEAF_PEM);
    // A set SSL_CERT_FILE is the whole store: an empty, garbage or missing
    // one is E_NOT_FOUND, never a fall back to the system bundle, which would
    // trust public CAs the caller meant to leave out.
    let garbage = dir.write("garbage.pem", "not a bundle");
    let missing = dir.0.join("absent.pem").to_string_lossy().into_owned();
    for trust in [&empty, &garbage, &missing] {
        match verify_under(trust, &leaf) {
            Err(e) if code_of(&e) == "E_NOT_FOUND" => {
                assert!(e.to_string().contains("SSL_CERT_FILE"), "{e}")
            }
            other => panic!("with SSL_CERT_FILE={trust}: {other:?}"),
        }
    }
}

#[test]
fn cert_verify_refuses_a_non_certificate_file_and_a_missing_one() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = Scratch::new("verify-refuse");
    let trust = dir.write("trust.pem", CA_PEM);
    let garbage = dir.write("garbage.pem", "not a certificate");
    let e = verify_under(&trust, &garbage).expect_err("garbage is not a verdict");
    assert_eq!(code_of(&e), "E_BAD_ARG", "{e}");

    let mut env = isolated_env();
    let missing = dir.0.join("absent.pem").to_string_lossy().into_owned();
    let e = call("crypto_cert_verify", vec![text(&missing)], &mut env).expect_err("missing");
    assert_eq!(code_of(&e), "E_NOT_FOUND", "{e}");
}
