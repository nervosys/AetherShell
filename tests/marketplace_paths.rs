//! Path safety and integrity for the agent marketplace and the package cache.
//!
//! Every test uses a temp directory and, where a registry is needed, a loopback
//! listener. The clients are built with an explicit cache directory
//! (`with_cache_dir`, or the public `cache_dir` field), so the user's real cache
//! is never read or written. No real registry is contacted.
//!
//! The agent API handlers are private to `agent_api`, so their refusal is tested
//! in that module's unit tests (`marketplace_refuses_a_traversal_name_as_bad_arg`).

#![cfg(feature = "native")]

use iron_shell::marketplace::{
    validate_package_name, AgentDefinition, AgentManifest, AuthorInfo, PublishedAgent,
    RegistryClient,
};
use iron_shell::packages::RegistryClient as PkgClient;
use iron_shell::safety::{ErrorCode, SafetyError};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

fn coded(err: &anyhow::Error) -> &SafetyError {
    err.downcast_ref::<SafetyError>()
        .unwrap_or_else(|| panic!("not a coded error: {err:#}"))
}

fn temp_root(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("ish-mp-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Serve `n` requests on a loopback port. `route` maps a request target to a
/// status and body. Returns the base URL. The thread exits after `n` requests.
fn serve(n: usize, route: impl Fn(&str) -> (u16, Vec<u8>) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for _ in 0..n {
            let Ok((mut sock, _)) = listener.accept() else {
                return;
            };
            let mut head = Vec::new();
            let mut chunk = [0u8; 512];
            while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                match sock.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(k) => head.extend_from_slice(&chunk[..k]),
                }
            }
            let text = String::from_utf8_lossy(&head);
            let target = text.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (status, body) = route(&target);
            let reply = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = sock.write_all(reply.as_bytes());
            let _ = sock.write_all(&body);
        }
    });
    base
}

fn manifest(version: &str) -> AgentManifest {
    AgentManifest {
        name: "demo".into(),
        display_name: "Demo".into(),
        description: "a test agent".into(),
        version: version.into(),
        author: AuthorInfo {
            name: "test".into(),
            email: None,
            url: None,
        },
        license: "MIT".into(),
        repository: None,
        homepage: None,
        keywords: vec![],
        categories: vec![],
        capabilities: vec![],
        ironshell_version: ">=1.0.0".into(),
        dependencies: vec![],
        agent: AgentDefinition {
            system_prompt: "test".into(),
            model: None,
            tools: vec![],
            tool_choice: None,
            temperature: None,
            max_tokens: None,
        },
    }
}

/// An agent registry: `GET .../agents/demo` answers the metadata with `version`
/// and `checksum`, and `.../download` answers `bytes`.
fn agent_registry(version: &str, checksum: String, bytes: Vec<u8>, n: usize) -> String {
    let meta = serde_json::to_vec(&PublishedAgent {
        manifest: manifest(version),
        downloads: 0,
        stars: 0,
        forks: 0,
        verified: false,
        published_at: 0,
        updated_at: 0,
        checksum,
        size: bytes.len() as u64,
    })
    .unwrap();
    serve(n, move |t| {
        if t.ends_with("/download") {
            (200, bytes.clone())
        } else if t.starts_with("/api/v1/agents/demo") {
            (200, meta.clone())
        } else {
            (404, Vec::new())
        }
    })
}

/// A package tarball (`.tar.gz`) from raw entries. Names are written into the
/// header directly, so a `..` or absolute name reaches the reader unchanged.
fn tgz(entries: &[(&str, tar::EntryType, &[u8], Option<&str>)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, kind, data, link) in entries {
        let mut h = tar::Header::new_ustar();
        h.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
        h.set_entry_type(*kind);
        h.set_size(data.len() as u64);
        h.set_mode(0o644);
        if let Some(l) = link {
            h.set_link_name(l).unwrap();
        }
        h.set_cksum();
        builder.append(&h, *data).unwrap();
    }
    let tar_bytes = builder.into_inner().unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&tar_bytes).unwrap();
    gz.finish().unwrap()
}

fn pkg_client(base: String, cache: PathBuf) -> PkgClient {
    PkgClient {
        base_url: base,
        registry: "ironshell".into(),
        cache_dir: cache,
    }
}

// ---- the validator -------------------------------------------------------

#[test]
fn validator_refuses_each_unsafe_shape_with_bad_arg() {
    let long = "x".repeat(129);
    let refused: Vec<&str> = vec![
        "", ".", "..", "../x", "a/b", "a\\b", "C:x", "a\0b", ".hidden", "-rf", "con", "NUL.txt",
        "LPT1", "a b", "é", &long,
    ];
    for name in refused {
        let err = validate_package_name(name, "agent name")
            .expect_err(&format!("{name:?} must be refused"));
        assert_eq!(coded(&err).code, ErrorCode::BadArg, "{name:?}");
    }
    let edge = "x".repeat(128);
    for ok in [
        "code-reviewer",
        "a_b.c",
        "1.0.0",
        "1.0.0-rc.1",
        edge.as_str(),
    ] {
        validate_package_name(ok, "agent name").unwrap_or_else(|e| panic!("{ok:?}: {e:#}"));
    }
}

// ---- install, get, uninstall: names refused before any request --------------

#[tokio::test]
async fn install_get_and_uninstall_refuse_traversal_before_any_request() {
    let root = temp_root("pre");
    let cache = root.join("agents");
    // A dead loopback port: a request would fail as a network error, not E_BAD_ARG.
    let mut client =
        RegistryClient::with_cache_dir(Some("http://127.0.0.1:1".into()), cache.clone()).unwrap();

    let cases: [(&str, Option<&str>); 5] = [
        ("../../escape", None),
        ("a/b", None),
        ("demo", Some("../x")),
        (".", None),
        ("", None),
    ];
    for (name, version) in cases {
        let err = client
            .install(name, version)
            .await
            .expect_err(&format!("install {name:?} {version:?} must be refused"));
        assert_eq!(
            coded(&err).code,
            ErrorCode::BadArg,
            "install {name:?}: {err:#}"
        );
    }
    let err = client.get("..", None).await.expect_err("get must refuse");
    assert_eq!(coded(&err).code, ErrorCode::BadArg, "get: {err:#}");
    let err = client
        .uninstall("../../x")
        .expect_err("uninstall must refuse");
    assert_eq!(coded(&err).code, ErrorCode::BadArg, "uninstall: {err:#}");

    // Nothing was created under the cache.
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
    let _ = std::fs::remove_dir_all(root);
}

// ---- registry-supplied version -----------------------------------------------

#[tokio::test]
async fn registry_supplied_version_is_refused_before_it_names_a_directory() {
    let root = temp_root("regver");
    let cache = root.join("agents");
    let bytes = b"print 1".to_vec();
    // The registry names version "../../pwn"; the checksum is valid for the bytes.
    let base = agent_registry("../../pwn", sha256_hex(&bytes), bytes, 1);
    let mut client = RegistryClient::with_cache_dir(Some(base), cache.clone()).unwrap();

    let err = client
        .install("demo", None)
        .await
        .expect_err("a traversal version from the registry must be refused");
    assert_eq!(coded(&err).code, ErrorCode::BadArg, "{err:#}");
    assert!(!cache.join("demo").exists());
    assert!(!cache.join("pwn").exists());
    assert!(!root.join("pwn").exists());
    let _ = std::fs::remove_dir_all(root);
}

// ---- digests -----------------------------------------------------------------

#[tokio::test]
async fn md5_digest_is_refused_even_when_the_bytes_match_it() {
    let root = temp_root("md5");
    let cache = root.join("agents");
    let bytes = b"agent body".to_vec();
    let md5_hex = format!("{:x}", md5::compute(&bytes));
    let base = agent_registry("1.0.0", md5_hex, bytes, 2);
    let mut client = RegistryClient::with_cache_dir(Some(base), cache.clone()).unwrap();

    let err = client
        .install("demo", None)
        .await
        .expect_err("an MD5 digest must be refused");
    let msg = format!("{err:#}");
    assert!(msg.contains("MD5"), "{msg}");
    assert_eq!(coded(&err).code, ErrorCode::Io, "{msg}");
    assert!(!cache.join("demo").exists(), "nothing may be written");
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn sha256_digest_installs_and_loads() {
    // Positive control for the test above: the same flow with SHA-256 succeeds.
    let root = temp_root("sha");
    let cache = root.join("agents");
    let bytes = b"agent body".to_vec();
    let base = agent_registry("1.0.0", sha256_hex(&bytes), bytes.clone(), 2);
    let mut client = RegistryClient::with_cache_dir(Some(base), cache.clone()).unwrap();

    let installed = client
        .install("demo", None)
        .await
        .expect("SHA-256 installs");
    assert_eq!(installed.manifest.version, "1.0.0");
    assert_eq!(
        std::fs::read(cache.join("demo").join("1.0.0").join("agent.ish")).unwrap(),
        bytes
    );
    assert_eq!(
        client.load_agent_code("demo").unwrap().as_bytes(),
        &bytes[..]
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn package_without_a_digest_is_refused_and_nothing_is_written() {
    let root = temp_root("pkg-none");
    let cache = root.join("packages");
    let client = pkg_client("http://127.0.0.1:1".into(), cache.clone());
    let err = client
        .download_package("demo", "1.0.0", None)
        .expect_err("no digest must be refused");
    assert_eq!(coded(&err).code, ErrorCode::PolicyDeny, "{err:#}");
    assert!(!cache.join("demo").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn package_with_a_mismatched_digest_is_refused_and_nothing_is_written() {
    let root = temp_root("pkg-mismatch");
    let cache = root.join("packages");
    let archive = tgz(&[("agent.ish", tar::EntryType::Regular, &b"print 1"[..], None)]);
    let wrong = sha256_hex(b"some other archive");
    let base = serve(1, move |_| (200, archive.clone()));
    let client = pkg_client(base, cache.clone());
    let err = client
        .download_package("demo", "1.0.0", Some(&wrong))
        .expect_err("a mismatched SHA-256 must be refused");
    assert_eq!(coded(&err).code, ErrorCode::PolicyDeny, "{err:#}");
    assert!(
        !cache.join("demo").exists(),
        "no tarball or unpacked file may be written on mismatch"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn package_with_an_md5_shaped_digest_is_refused() {
    let root = temp_root("pkg-md5");
    let cache = root.join("packages");
    let archive = tgz(&[("agent.ish", tar::EntryType::Regular, &b"print 1"[..], None)]);
    let md5_hex = format!("{:x}", md5::compute(&archive));
    let base = serve(1, move |_| (200, archive.clone()));
    let client = pkg_client(base, cache.clone());
    let err = client
        .download_package("demo", "1.0.0", Some(&md5_hex))
        .expect_err("an MD5-shaped digest must be refused");
    assert_eq!(coded(&err).code, ErrorCode::PolicyDeny, "{err:#}");
    assert!(!cache.join("demo").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn package_with_a_matching_digest_unpacks() {
    // Positive control for the three refusals above.
    let root = temp_root("pkg-ok");
    let cache = root.join("packages");
    let archive = tgz(&[("agent.ish", tar::EntryType::Regular, &b"print 1"[..], None)]);
    let good = sha256_hex(&archive);
    let base = serve(1, move |_| (200, archive.clone()));
    let client = pkg_client(base, cache.clone());
    let dir = client
        .download_package("demo", "1.0.0", Some(&good))
        .expect("a matching SHA-256 installs");
    assert_eq!(std::fs::read(dir.join("agent.ish")).unwrap(), b"print 1");
    let _ = std::fs::remove_dir_all(root);
}

// ---- archive entries ---------------------------------------------------------

#[test]
fn archive_entries_cannot_escape_or_link_out() {
    let root = temp_root("archive");
    let client = pkg_client("http://127.0.0.1:1".into(), root.join("packages"));
    let extract = |name: &str, entries: &[(&str, tar::EntryType, &[u8], Option<&str>)]| {
        let file = root.join(format!("{name}.tgz"));
        std::fs::write(&file, tgz(entries)).unwrap();
        let dest = root.join("packages").join(name);
        std::fs::create_dir_all(&dest).unwrap();
        (client.extract_tarball(&file, &dest), dest)
    };

    // `..` in a name: refused, and the file is not written beside the package.
    let (res, dest) = extract(
        "dotdot",
        &[("../escape.txt", tar::EntryType::Regular, &b"x"[..], None)],
    );
    assert!(res.is_err(), "a '..' entry must be refused");
    assert!(!dest.parent().unwrap().join("escape.txt").exists());

    // A symlink to outside: refused outright, so no later entry can write through it.
    // Short and fixed: a tar header holds 100 bytes, and a temp path can exceed that.
    let target = "/ae-outside/outside.txt".to_string();
    // Alone, so the refusal is ours (the message names a link entry), not a
    // platform error from creating the link (Windows needs privilege for it).
    let (res, dest) = extract(
        "symlink",
        &[(
            "link",
            tar::EntryType::Symlink,
            &b""[..],
            Some(target.as_str()),
        )],
    );
    let err = res.expect_err("a symlink entry must be refused");
    assert!(format!("{err:#}").contains("link entry"), "{err:#}");
    assert!(!dest.join("link").exists(), "the link must not be created");

    // A file written through a link is refused too: the link is never created.
    let (res, dest2) = extract(
        "symlink-then-file",
        &[
            (
                "link",
                tar::EntryType::Symlink,
                &b""[..],
                Some(target.as_str()),
            ),
            ("link/written.txt", tar::EntryType::Regular, &b"x"[..], None),
        ],
    );
    assert!(
        res.is_err(),
        "a link followed by a write through it must be refused"
    );
    assert!(!dest2.join("link").exists(), "the link must not be created");
    assert!(
        !root.join("written.txt").exists(),
        "nothing may be written through it"
    );

    // An absolute name: the leading '/' is stripped, so it lands inside the package.
    let (res, dest) = extract(
        "absolute",
        &[("/abs-escape.txt", tar::EntryType::Regular, &b"x"[..], None)],
    );
    assert!(
        res.is_ok() && dest.join("abs-escape.txt").exists(),
        "an absolute name must be unpacked inside the package: {res:?}"
    );
    assert!(!root.join("abs-escape.txt").exists());

    // A device node: refused.
    let (res, _) = extract("device", &[("dev", tar::EntryType::Char, &b""[..], None)]);
    assert!(res.is_err(), "a character device entry must be refused");

    let _ = std::fs::remove_dir_all(root);
}

// ---- the installed list on disk ----------------------------------------------

#[test]
fn an_installed_path_outside_the_cache_is_refused_and_kept() {
    let root = temp_root("edited");
    let cache = root.join("agents");
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("marker.txt"), b"keep").unwrap();
    std::fs::write(outside.join("agent.ish"), b"secret").unwrap();
    std::fs::create_dir_all(cache.join("good").join("1.0.0")).unwrap();
    std::fs::write(cache.join("good").join("1.0.0").join("agent.ish"), b"x").unwrap();

    // Two edited records: one absolute path outside, one reaching outside with `..`.
    let escape_dotdot = cache.join("..").join("outside");
    let record = |path: &Path| {
        serde_json::json!({
            "manifest": serde_json::to_value(manifest("1.0.0")).unwrap(),
            "installed_at": 0,
            "path": path,
        })
    };
    let index = serde_json::json!({
        "demo": record(&outside),
        "walk": record(&escape_dotdot),
        "good": record(&cache.join("good").join("1.0.0")),
    });
    std::fs::write(cache.join("installed.json"), index.to_string()).unwrap();

    let mut client = RegistryClient::with_cache_dir(None, cache.clone()).unwrap();
    for name in ["demo", "walk"] {
        let err = client
            .uninstall(name)
            .expect_err(&format!("{name}: a path outside the cache must be refused"));
        assert_eq!(coded(&err).code, ErrorCode::BadState, "{name}: {err:#}");
    }
    assert_eq!(std::fs::read(outside.join("marker.txt")).unwrap(), b"keep");
    assert!(
        client.load_agent_code("demo").is_err(),
        "the code of an agent outside the cache must not be read"
    );

    // A path inside the cache is still removed: the control for the refusals.
    client
        .uninstall("good")
        .expect("an in-cache path uninstalls");
    assert!(
        !cache.join("good").join("1.0.0").exists(),
        "the installed directory is removed"
    );
    assert!(outside.join("marker.txt").exists());
    let _ = std::fs::remove_dir_all(root);
}
