//! TLS crypto provider: IronCrypto's rustls provider as the process default.
//!
//! reqwest is built without its own provider (no ring), so it panics when a
//! client is built and no process default is installed. tungstenite's rustls
//! config also takes the process default. Both therefore need a provider
//! installed first, and this is the one that is installed.

use std::sync::Once;

/// Install `ic_rustls::provider()` as the process-default rustls crypto
/// provider. Safe to call any number of times: it runs once, and an existing
/// default (installed by someone else first) is left in place.
pub fn install_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::CryptoProvider::install_default(ic_rustls::provider());
    });
}
