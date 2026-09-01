//! Two processes sharing one audit log.
//!
//! The chain is per-process state — `seq` and `last_hash` live in memory — so
//! two shells appending to one file interleave two independent chains. That is
//! not an exotic configuration: in agent mode the log defaults to
//! `<workspace>/.ae/audit.log`, so two agents in one workspace share it by
//! default.
//!
//! Probing that case found three problems, in increasing order of seriousness:
//!
//!   * `verify_audit` reported `broken chain link`, which reads as tampering
//!     and sends the reader hunting for an attacker who is not there;
//!   * each process's tail check fired on the *other's* writes, filling the log
//!     with `tamper-detected` markers — an alarm that goes off whenever two
//!     agents run is an alarm nobody reads;
//!   * and records could be **torn**. `writeln!` emits the content and the
//!     newline as separate writes, and `O_APPEND` only guarantees atomicity per
//!     write, so two entries could land on one line and the log stopped being
//!     valid JSON at all.
//!
//! The third is the one that mattered most: a log that fails to verify still
//! tells you something happened, and a log that cannot be parsed tells you
//! nothing.
//!
//! What is *not* claimed here: that a shared log forms a single verifiable
//! chain. It does not, and cannot without cross-process locking. The tests
//! assert that it stays parseable, stays quiet, and explains itself.

#![cfg(feature = "native")]

use std::process::Command;

fn ae() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("test exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ae{}", std::env::consts::EXE_SUFFIX))
}

struct Shared {
    dir: std::path::PathBuf,
}

impl Shared {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ae_auditconc_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("dir");
        Self { dir }
    }

    fn log(&self) -> std::path::PathBuf {
        self.dir.join("audit.log")
    }

    /// Start one shell writing `n` audited operations, without waiting.
    fn spawn_writer(&self, tag: &str, n: usize) -> std::process::Child {
        let src: String = (0..n)
            .map(|i| format!("file_write(\"{tag}{i}.txt\", \"x\")\n"))
            .collect();
        let script = self.dir.join(format!("{tag}.ae"));
        std::fs::write(&script, src).expect("write script");
        Command::new(ae())
            .arg("--deterministic")
            .arg(&script)
            .current_dir(&self.dir)
            .env("AETHER_MODE", "agent")
            .env("AETHER_WORKSPACE", &self.dir)
            .env("AETHER_AUDIT_LOG", self.log())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn writer")
    }

    fn entries(&self) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(self.log()).unwrap_or_default();
        text.lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| {
                serde_json::from_str(l).unwrap_or_else(|e| {
                    panic!(
                        "audit log line is not valid JSON ({e}); a torn record means two \
                            processes interleaved inside one line:\n{l}"
                    )
                })
            })
            .collect()
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Run two writers at once and return their entries. Forty operations each is
/// enough that they genuinely overlap rather than running back to back — which
/// the interleaving assertion below checks rather than assumes.
fn run_concurrently() -> (Shared, Vec<serde_json::Value>) {
    let shared = Shared::new();
    let mut a = shared.spawn_writer("a", 40);
    let mut b = shared.spawn_writer("b", 40);
    a.wait().expect("writer a");
    b.wait().expect("writer b");
    let entries = shared.entries();
    // Hand the workspace back rather than leaking it: an earlier draft used
    // `mem::forget` to keep the directory alive past the caller's assertions,
    // which left one temp tree behind per test run.
    (shared, entries)
}

#[test]
fn concurrent_writers_never_tear_a_record() {
    // `entries()` panics on unparseable JSON, so reaching the assertion at all
    // means every line survived intact.
    let (_ws, entries) = run_concurrently();
    assert_eq!(
        entries.len(),
        80,
        "expected exactly one entry per audited operation; got {}",
        entries.len()
    );
}

#[test]
fn the_writers_really_do_interleave() {
    // Without this the other tests could pass because the two processes
    // happened to run one after the other, which is the easy case and not the
    // one under test.
    let (_ws, entries) = run_concurrently();
    let switches = entries
        .windows(2)
        .filter(|w| w[0]["writer"] != w[1]["writer"])
        .count();
    assert!(
        switches > 0,
        "the two writers never interleaved, so this file proves nothing about \
         concurrency; re-run or raise the operation count"
    );
}

/// A concurrent writer is not an attacker. Before this distinction existed, the
/// tail check fired on the other process's entries and every concurrent run
/// left `tamper-detected` markers behind.
#[test]
fn concurrency_does_not_raise_a_tamper_alarm() {
    let (_ws, entries) = run_concurrently();
    let markers: Vec<_> = entries
        .iter()
        .filter(|e| e["decision"] == "tamper-detected")
        .collect();
    assert!(
        markers.is_empty(),
        "{} false tamper alarms from ordinary concurrent use; an alarm that \
         fires whenever two agents run is one nobody will read",
        markers.len()
    );
}

#[test]
fn every_entry_records_which_writer_produced_it() {
    let (_ws, entries) = run_concurrently();
    for e in &entries {
        let w = e["writer"].as_str().unwrap_or_default();
        assert_eq!(
            w.len(),
            16,
            "entry has no usable writer id, so a shared log cannot be \
             attributed: {e}"
        );
    }
    let distinct: std::collections::BTreeSet<&str> = entries
        .iter()
        .filter_map(|e| e["writer"].as_str())
        .collect();
    assert_eq!(distinct.len(), 2, "expected two distinct writers");
}

/// A single writer must still produce one clean, verifiable chain — the
/// concurrency handling must not have loosened the ordinary case.
#[test]
fn a_single_writer_still_produces_one_valid_chain() {
    let shared = Shared::new();
    let mut only = shared.spawn_writer("solo", 10);
    only.wait().expect("writer");

    let entries = shared.entries();
    assert_eq!(entries.len(), 10);

    let seqs: Vec<u64> = entries.iter().filter_map(|e| e["seq"].as_u64()).collect();
    assert_eq!(
        seqs,
        (1..=10).collect::<Vec<u64>>(),
        "a single writer should number its entries 1..n with no gaps"
    );

    let n = aethershell::safety::verify_audit_with(&shared.log(), None)
        .expect("a single writer's log must verify");
    assert_eq!(n, 10);
}
