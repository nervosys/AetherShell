//! Each redirect hop of the direct-URL model download is judged (AS-2026-25,
//! AS-2026-28). The decision itself is unit-tested in `src/ai_api/downloader.rs`;
//! these tests drive a real download through hand-written HTTP responses on
//! loopback listeners.
//!
//! This is its own test binary because the loopback test seam is process-wide:
//! `IRONSHELL_TEST_ALLOW_LOOPBACK=1` is set in `init`, and in the same process it
//! would change the loopback refusals that `aimodel_hardening` checks.
//!
//! The refused hop points at `0.0.0.0`, not at the metadata address. Refusal
//! happens before any connection, so nothing is contacted; but if the check
//! were removed, the download would connect to this machine rather than to a
//! link-local address that might route somewhere.

use iron_shell::ai_api::downloader::{DownloadRequest, ModelDownloader};
use iron_shell::ai_api::models::ModelSource;
use iron_shell::ai_api::storage::{ModelStorage, StorageConfig};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Once};
use std::time::Duration;

fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base = std::env::var_os("IRONSHELL_SESSION_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join(format!("aimodel-redirect-hops-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::env::set_var("IRONSHELL_SESSION_DIR", base.join("session"));
        std::env::set_var("IRONSHELL_TEST_ALLOW_LOOPBACK", "1");
    });
}

/// Serve every connection on a loopback port with the bytes `respond()` gives,
/// counting connections in `hits`. The thread lives for the test process.
fn serve(hits: Arc<AtomicUsize>, respond: impl Fn() -> Vec<u8> + Send + 'static) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut conn) = conn else { continue };
            hits.fetch_add(1, Ordering::SeqCst);
            conn.set_read_timeout(Some(Duration::from_secs(10))).ok();
            // Read the request head up to the blank line, then answer and close.
            let mut head = Vec::new();
            let mut buf = [0u8; 1024];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match conn.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => head.extend_from_slice(&buf[..n]),
                }
            }
            let _ = conn.write_all(&respond());
        }
    });
    addr
}

fn redirect_to(location: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )
    .into_bytes()
}

fn file_of(body: &'static [u8]) -> Vec<u8> {
    let mut out = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    out.extend_from_slice(body);
    out
}

/// A downloader whose storage is a fresh temp directory.
fn downloader(name: &str) -> ModelDownloader {
    let data = std::env::temp_dir()
        .join("aimodel-redirect-hops")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let config = StorageConfig {
        custom_data_dir: Some(data),
        ..StorageConfig::default()
    };
    ModelDownloader::new(ModelStorage::new(&config).unwrap()).unwrap()
}

fn url_request(url: &str) -> DownloadRequest {
    DownloadRequest {
        model_id: "hop-test".to_string(),
        source: ModelSource {
            origin: "url".to_string(),
            url: Some(url.to_string()),
            repository: None,
            commit: None,
            license: None,
        },
        format_preference: None,
        quantization: None,
        validate_checksum: false,
    }
}

#[test]
fn a_redirect_from_a_loopback_url_to_an_internal_address_is_refused() {
    init();
    // The first URL is loopback, which the seam allows. The hop is not.
    let first_hits = Arc::new(AtomicUsize::new(0));
    let first = serve(first_hits.clone(), || {
        redirect_to("http://0.0.0.0:9/latest/meta-data/")
    });
    let url = format!("http://{first}/model.gguf");

    let mut dl = downloader("refused");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let err = rt
        .block_on(dl.download_model(url_request(&url)))
        .expect_err("a redirect to 0.0.0.0 must not be followed");
    let text = format!("{err:#}");
    assert!(
        text.contains("redirect to http://0.0.0.0:9/latest/meta-data/ refused"),
        "the refusal should come from the hop check: {text}"
    );
    assert_eq!(
        first_hits.load(Ordering::SeqCst),
        1,
        "the first URL was requested once"
    );
}

#[test]
fn a_redirect_from_a_loopback_url_to_a_permitted_address_is_followed() {
    init();
    let body = b"GGUF-HOP-TEST";
    let final_hits = Arc::new(AtomicUsize::new(0));
    let last = serve(final_hits.clone(), move || file_of(body));
    let middle_hits = Arc::new(AtomicUsize::new(0));
    let middle = serve(middle_hits.clone(), move || {
        redirect_to(&format!("http://{last}/final.gguf"))
    });
    let url = format!("http://{middle}/model.gguf");

    let mut dl = downloader("followed");
    let rt = tokio::runtime::Runtime::new().unwrap();
    let meta = rt
        .block_on(dl.download_model(url_request(&url)))
        .expect("a redirect to a permitted address must be followed");
    assert_eq!(meta.size_bytes, body.len() as u64);
    assert_eq!(middle_hits.load(Ordering::SeqCst), 1);
    assert_eq!(
        final_hits.load(Ordering::SeqCst),
        1,
        "the redirect target was fetched"
    );
}
