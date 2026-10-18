//! Proves the disposable runner reaches root in the marked distro, and that
//! the guard refuses everywhere else.
#[path = "support/disposable.rs"]
mod disposable;

#[test]
#[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
fn the_runner_reaches_root_in_the_marked_distro() {
    disposable::require();
}

#[test]
fn the_guard_refuses_outside_the_disposable_distro() {
    if std::env::var("AETHER_DISPOSABLE").as_deref() == Ok("1") {
        return; // inside the runner, the ignored test above is the check
    }
    assert!(std::panic::catch_unwind(disposable::require).is_err());
}
