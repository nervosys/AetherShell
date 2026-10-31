//! The POSIX-shell subset, run on the host.
//!
//! The subset and its utilities live in the `ironshell-posix` crate, which
//! builds without `std`: every file, environment, process, clock, locale and
//! stream operation goes through its `Os` trait, and every policy decision
//! through its `Gate`. This module is the host: it runs the subset on
//! [`HostOs`](crate::posix_os::HostOs), whose gate is the shell's own effect
//! gate and journal, so `ish -b` behaves as it always did.

use crate::posix_os::HostOs;

pub use ironshell_posix::{bre_to_regex, has_glob_meta, supports};

/// Run `src` as a shell script on the host, writing to the real stdout and
/// stderr.
///
/// Returns the exit status, or `None` when the script uses something this
/// subset does not run -- decided before anything executes, so the caller can
/// fall back without a half-run script behind it.
pub fn run(src: &str) -> Option<i32> {
    ironshell_posix::run_on(&HostOs, src)
}

/// Pathname expansion for the typed builtins, on the host. See
/// [`ironshell_posix::glob_paths`].
pub fn glob_paths(pattern: &str) -> Vec<String> {
    ironshell_posix::glob_paths(&HostOs, pattern)
}

/// [`ironshell_posix::glob_paths_capped`], on the host.
pub fn glob_paths_capped(pattern: &str, max: usize) -> (Vec<String>, bool) {
    ironshell_posix::glob_paths_capped(&HostOs, pattern, max)
}
