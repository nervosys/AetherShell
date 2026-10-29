//! The process-default rustls provider is IronCrypto's (`ic-rustls`), and the
//! reqwest clients build on it without ring. Offline: no connection is made.
#![cfg(not(target_arch = "wasm32"))]

use rustls::crypto::CryptoProvider;

fn suite_ids(suites: &[rustls::SupportedCipherSuite]) -> Vec<rustls::CipherSuite> {
    suites.iter().map(|s| s.suite()).collect()
}

#[test]
fn install_provider_makes_ic_rustls_the_default() {
    aethershell::tls::install_provider();
    let installed = CryptoProvider::get_default().expect("a default provider is installed");
    let expected = ic_rustls::provider();
    assert_eq!(
        suite_ids(&installed.cipher_suites),
        suite_ids(&expected.cipher_suites)
    );
}

#[test]
fn install_provider_is_idempotent() {
    aethershell::tls::install_provider();
    aethershell::tls::install_provider();
    assert!(CryptoProvider::get_default().is_some());
    // A second explicit install is refused rather than replacing the default.
    assert!(CryptoProvider::install_default(ic_rustls::provider()).is_err());
}

#[test]
fn reqwest_clients_build_on_the_installed_provider() {
    aethershell::tls::install_provider();
    reqwest::blocking::Client::builder()
        .build()
        .expect("blocking client builds");
    reqwest::Client::builder()
        .build()
        .expect("async client builds");
}
