//! The audit chain must resist an author, not merely corruption (AS-2026-02).
//!
//! The chain was a plain SHA-256 over each entry. Anyone who could write the
//! file could therefore truncate it and rewrite it end to end with a fresh,
//! internally consistent chain, and `verify_audit` would report it clean —
//! because recomputing an unkeyed hash needs nothing but the entry text. The
//! workspace jail now refuses *jailed filesystem* writes to the log, but an
//! approved `Exec` runs arbitrary code and no jail rule stops
//! `sh -c '> audit.log'`.
//!
//! Keying the chain with HMAC-SHA256 closes the forgery: a rewrite now requires
//! the key, which this process reads once and then removes from its own
//! environment so no spawned child inherits it.
//!
//! What keying cannot do, so that nobody reads more into these tests: code
//! running *inside* this process can still forge, because whatever key the
//! process appends with it can also forge with. That needs an append-only sink
//! the process cannot rewrite, which is a deployment decision.

use aethershell::safety;
use std::path::PathBuf;

/// A fixed 32-byte key, hex-encoded. Every test in this binary installs the
/// same value before touching the audit layer, because the key is resolved once
/// per process: agreeing on one value makes the resolution order irrelevant.
const TEST_KEY_HEX: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

fn key_bytes() -> Vec<u8> {
    (0..TEST_KEY_HEX.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&TEST_KEY_HEX[i..i + 2], 16).expect("hex"))
        .collect()
}

fn tmp_log(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "ae_chain_{tag}_{}_{}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let _ = std::fs::remove_file(&p);
    p
}

/// Write entries through the real audit path, so what is verified is what the
/// shell actually produces rather than a fixture that agrees with the verifier
/// by construction.
fn write_entries(log: &PathBuf, n: usize) {
    std::env::set_var("AETHER_AUDIT_LOG", log);
    for i in 0..n {
        safety::audit(
            "file_write",
            safety::Effect::WriteLocal,
            "allow",
            &format!("/w/file{i}"),
            serde_json::json!({ "i": i }),
        )
        .expect("audit append");
    }
}

/// Every test in this binary mutates process-global state — `AETHER_AUDIT_LOG`
/// and the audit layer's in-memory chain — so they run one at a time. Without
/// this they interleave and the failures look like bugs in the audit code.
static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Install the chain key exactly once per process. The key is resolved once and
/// the variable removed as a side effect, so setting it again afterwards would
/// put back the very thing `the_key_is_removed_from_the_environment` checks for.
fn install_key() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        std::env::set_var("AETHER_AUDIT_KEY", TEST_KEY_HEX);
        // Force resolution now, while the variable is set.
        assert!(
            safety::audit_key().is_some(),
            "the test key did not resolve; every keyed assertion below would pass vacuously"
        );
    });
}

// ── The forgery the finding described ────────────────────────────────────

/// The heart of AS-2026-02: rebuild the log from scratch with a valid *unkeyed*
/// chain — which requires no secret at all — and confirm a keyed verifier
/// refuses it. Before keying, this forgery verified clean.
#[test]
fn an_unkeyed_rewrite_is_refused_by_a_keyed_verifier() {
    let _g = lock();
    let log = tmp_log("forged");

    // What an attacker with write access can always produce: entries whose
    // prev_hash/entry_hash chain is internally consistent under plain SHA-256.
    let mut prev = "0".repeat(64);
    let mut lines = Vec::new();
    for seq in 1..=3u64 {
        let core = serde_json::json!({
            "seq": seq,
            "ts": "2026-08-31T00:00:00+00:00",
            "principal": "attacker",
            "builtin": "file_write",
            "effect": "WriteLocal",
            "decision": "allow",
            "resource": "/w/innocuous",
            "detail": {},
            "prev_hash": prev,
        });
        let hash = {
            use sha2::{Digest, Sha256};
            let mut h = Sha256::new();
            h.update(core.to_string().as_bytes());
            h.finalize()
                .iter()
                .map(|b| format!("{:02x}", b))
                .collect::<String>()
        };
        let mut full = core;
        full["entry_hash"] = serde_json::json!(hash);
        lines.push(full.to_string());
        prev = hash;
    }
    std::fs::write(&log, lines.join("\n") + "\n").expect("write forged log");

    // Unkeyed, this forgery is indistinguishable from a genuine log — that is
    // precisely the finding, asserted rather than assumed.
    assert_eq!(
        safety::verify_audit_with(&log, None),
        Ok(3),
        "an unkeyed chain accepts the forgery; if this ever fails the premise of \
         AS-2026-02 has changed and the rest of this file needs revisiting"
    );

    let err = safety::verify_audit_with(&log, Some(&key_bytes()))
        .expect_err("a keyed verifier must refuse an unkeyed chain");
    assert!(
        err.contains("unkeyed"),
        "the refusal should name the downgrade, got: {err}"
    );

    let _ = std::fs::remove_file(&log);
}

/// The same forgery attempted *with* the mac label present but no key: an
/// attacker cannot relabel their way past the check either, because the label
/// is inside the tagged core.
#[test]
fn a_relabelled_forgery_still_fails_the_tag() {
    let _g = lock();
    let log = tmp_log("relabel");
    let core = serde_json::json!({
        "seq": 1,
        "ts": "2026-08-31T00:00:00+00:00",
        "principal": "attacker",
        "builtin": "file_write",
        "effect": "WriteLocal",
        "decision": "allow",
        "resource": "/w/x",
        "detail": {},
        "prev_hash": "0".repeat(64),
        "mac": "hmac-sha256",
        "key_id": "deadbeefdeadbeef",
    });
    let bogus = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(core.to_string().as_bytes());
        h.finalize()
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };
    let mut full = core;
    full["entry_hash"] = serde_json::json!(bogus);
    std::fs::write(&log, full.to_string() + "\n").expect("write");

    let err = safety::verify_audit_with(&log, Some(&key_bytes()))
        .expect_err("a SHA-256 hash wearing an hmac label must not verify");
    assert!(
        err.contains("mismatch") || err.contains("tampered"),
        "unexpected error: {err}"
    );

    let _ = std::fs::remove_file(&log);
}

// ── Genuine keyed logs ───────────────────────────────────────────────────

#[test]
fn a_keyed_log_verifies_with_the_key_and_not_without_it() {
    let _g = lock();
    install_key();
    let log = tmp_log("genuine");
    write_entries(&log, 3);

    let n = safety::verify_audit_with(&log, Some(&key_bytes()))
        .expect("a genuine keyed log must verify with its key");
    assert_eq!(n, 3);

    // A verifier without the key can see the entries are keyed but cannot
    // confirm them — and must say so rather than silently pass.
    let err = safety::verify_audit_with(&log, None)
        .expect_err("an unkeyed verifier must not report a keyed log as verified");
    assert!(
        err.contains("no chain key is configured"),
        "unexpected error: {err}"
    );

    let _ = std::fs::remove_file(&log);
}

#[test]
fn the_wrong_key_does_not_verify() {
    let _g = lock();
    install_key();
    let log = tmp_log("wrongkey");
    write_entries(&log, 2);

    let mut wrong = key_bytes();
    wrong[0] ^= 0xff;
    let err = safety::verify_audit_with(&log, Some(&wrong))
        .expect_err("a different key must not verify this chain");
    assert!(
        err.contains("mismatch") || err.contains("tampered"),
        "unexpected error: {err}"
    );

    let _ = std::fs::remove_file(&log);
}

#[test]
fn keyed_entries_carry_the_algorithm_and_a_key_id() {
    let _g = lock();
    install_key();
    let log = tmp_log("labels");
    write_entries(&log, 1);

    let content = std::fs::read_to_string(&log).expect("read");
    let obj: serde_json::Value =
        serde_json::from_str(content.lines().next().expect("one line")).expect("json");
    assert_eq!(obj["mac"], "hmac-sha256");
    assert!(
        obj["key_id"].as_str().is_some_and(|s| s.len() == 16),
        "entries need a key id so rotation reads as rotation rather than as tampering: {obj}"
    );

    let _ = std::fs::remove_file(&log);
}

/// The key must not reach a child process, or an approved `Exec` could forge
/// with it and the whole scheme collapses back to the unkeyed case.
#[test]
fn the_key_is_removed_from_the_environment_after_it_is_read() {
    let _g = lock();
    install_key();
    assert!(
        safety::audit_key().is_some(),
        "precondition: a key is configured"
    );
    assert!(
        std::env::var("AETHER_AUDIT_KEY").is_err(),
        "AETHER_AUDIT_KEY is still in this process's environment, so every spawned \
         child inherits the key that is supposed to distinguish us from them"
    );
}

// ── Detection at the next append ─────────────────────────────────────────

/// Keying makes a rewrite *detectable*; this makes it detected promptly. An
/// approved `Exec` that truncates the log leaves the file no longer ending
/// where this process left it, and the next append records that as its own
/// chained entry instead of quietly continuing.
#[test]
fn truncation_between_appends_is_recorded_in_the_log() {
    let _g = lock();
    install_key();
    let log = tmp_log("truncate");
    write_entries(&log, 2);

    // What an approved `Exec` can do and no jail rule prevents.
    std::fs::write(&log, "").expect("truncate");

    write_entries(&log, 1);

    let content = std::fs::read_to_string(&log).expect("read");
    assert!(
        content.contains("tamper-detected"),
        "a truncation went unremarked; the log after the next append was:\n{content}"
    );
    assert!(
        content.contains("audit_chain"),
        "the marker should be attributable to the audit layer itself: {content}"
    );

    let _ = std::fs::remove_file(&log);
}

/// A half-written last line is another process appending, not an attacker.
///
/// `detect_tail_divergence` read the last *line* of the log and, when it failed
/// to parse, fell straight through to the hash comparison: the writer-id check
/// that recognises concurrency sat inside `if let Ok(..)` and was skipped, and
/// an empty hash never matches, so the log recorded `tamper-detected`.
///
/// Two concurrent writers hit this in roughly one run in seven under load --
/// found by CI on ubuntu (81 entries where 80 were expected) after the same
/// suite had passed on Windows and on a quiet Linux box. The mechanism is
/// reproduced deterministically here rather than by racing processes, because
/// the racing version only catches it about 15% of the time.
#[test]
fn a_partially_written_tail_is_not_reported_as_tampering() {
    let _g = lock();
    install_key();
    let log = tmp_log("torn-tail");
    write_entries(&log, 2);

    // Exactly what a reader sees when it catches another process's append in
    // flight: a final line with no terminator and truncated JSON.
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&log)
            .expect("open log for append");
        f.write_all(b"{\"builtin\":\"file_write\",\"seq\":9,\"entry_ha")
            .expect("append a torn line");
    }

    write_entries(&log, 1);

    let content = std::fs::read_to_string(&log).expect("read");
    assert!(
        !content.contains("tamper-detected"),
        "a torn final line was reported as tampering; that is the false alarm \
         the writer id was supposed to end, arriving by another route:\n{content}"
    );

    let _ = std::fs::remove_file(&log);
}

/// The relaxation above must not blind the check to a real rewrite.
///
/// Same shape as the torn-tail case -- an unparseable final line -- except the
/// entries before it have been replaced. The tail check should still find the
/// last complete entry, see that it is this writer's and does not match what
/// this process last wrote, and say so.
#[test]
fn skipping_a_torn_tail_does_not_hide_a_rewrite_beneath_it() {
    let _g = lock();
    install_key();
    let log = tmp_log("torn-over-rewrite");
    write_entries(&log, 2);

    // Rewrite the whole log with an entry this process never wrote, then leave
    // a torn line on the end.
    let forged = std::fs::read_to_string(&log)
        .expect("read")
        .lines()
        .next()
        .expect("at least one entry")
        .replace("file_write", "file_read");
    std::fs::write(&log, format!("{forged}\n{{\"seq\":9,\"entry_ha"))
        .expect("rewrite with a torn tail");

    write_entries(&log, 1);

    let content = std::fs::read_to_string(&log).expect("read");
    assert!(
        content.contains("tamper-detected"),
        "a rewrite hiding under a torn last line went unremarked:\n{content}"
    );

    let _ = std::fs::remove_file(&log);
}

// ── The append-only sink (AS-2026-02 residue) ────────────────────────────

/// Keying stops anyone who can only *write* the log from forging it. It does
/// not stop this process, which holds the key. `AETHER_AUDIT_SINK` is the hook
/// for a destination the shell cannot rewrite — a FIFO drained by a collector,
/// a WORM mount, a directory where the user has append but not write.
///
/// What is asserted here is the mechanism, not the guarantee: the integrity
/// comes from whatever is behind the path.
#[test]
fn every_entry_is_mirrored_to_the_sink() {
    let _g = lock();
    install_key();
    let log = tmp_log("sinklog");
    let sink = tmp_log("sink");
    std::env::set_var("AETHER_AUDIT_SINK", &sink);
    write_entries(&log, 3);
    std::env::remove_var("AETHER_AUDIT_SINK");

    let mirrored = std::fs::read_to_string(&sink).expect("sink was never written");
    let lines: Vec<&str> = mirrored.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 3, "sink holds {} of 3 entries", lines.len());

    // Byte-identical to the log, so the sink can be verified with the same
    // chain check rather than a second, divergent parser.
    let primary = std::fs::read_to_string(&log).expect("read log");
    assert_eq!(primary.lines().filter(|l| !l.trim().is_empty()).count(), 3);
    for (a, b) in primary.lines().zip(lines.iter()) {
        assert_eq!(a, *b, "sink line differs from the log line");
    }

    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_file(&sink);
}

/// A truncated log can be reconstructed from the sink, which is the point of
/// having one: the sink verifies as a chain on its own.
#[test]
fn the_sink_still_verifies_after_the_log_is_truncated() {
    let _g = lock();
    install_key();
    let log = tmp_log("sinktrunc");
    let sink = tmp_log("sinktrunc_out");
    std::env::set_var("AETHER_AUDIT_SINK", &sink);
    write_entries(&log, 3);
    std::env::remove_var("AETHER_AUDIT_SINK");

    // What an approved `Exec` can do to the log and not to a FIFO.
    std::fs::write(&log, "").expect("truncate");

    let n = safety::verify_audit_with(&sink, Some(&key_bytes()))
        .expect("the sink should still verify as a chain");
    assert_eq!(n, 3, "sink lost entries the log had");

    let _ = std::fs::remove_file(&log);
    let _ = std::fs::remove_file(&sink);
}

/// With no sink configured nothing changes — the feature is opt-in and must
/// not add a failure mode for everyone else.
#[test]
fn no_sink_configured_is_not_an_error() {
    let _g = lock();
    std::env::remove_var("AETHER_AUDIT_SINK");
    let log = tmp_log("nosink");
    write_entries(&log, 2);
    assert_eq!(
        std::fs::read_to_string(&log)
            .expect("read")
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count(),
        2
    );
    let _ = std::fs::remove_file(&log);
}
