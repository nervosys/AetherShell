//! Guard for tests that change the machine: accounts, mounts, services,
//! packages. They are `#[ignore]`d, so a plain `cargo test` never runs them,
//! and each calls [`require`] first, which refuses unless it is running as
//! root inside the disposable distro that scripts/disposable_linux_tests.sh
//! prepares: AETHER_DISPOSABLE=1 and the /etc/ae-disposable marker.

/// Panics unless this process may change the system it runs on.
pub fn require() {
    let flagged = std::env::var("AETHER_DISPOSABLE").as_deref() == Ok("1");
    let marked = std::path::Path::new("/etc/ae-disposable").is_file();
    #[cfg(unix)]
    let root = unsafe { libc::geteuid() } == 0;
    #[cfg(not(unix))]
    let root = false;
    assert!(
        flagged && marked && root,
        "refusing to change a real system: needs AETHER_DISPOSABLE=1, /etc/ae-disposable and root \
         (flag {flagged}, marker {marked}, root {root}); run scripts/disposable_linux_tests.sh"
    );
}
