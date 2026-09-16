//! The path denylist has to block the files it names and nothing else.
//!
//! It did neither. `validate_safe_path` matched each blocked pattern with
//! `path.to_lowercase().contains(&pattern.to_lowercase())`, and that single
//! line failed in both directions simultaneously:
//!
//! **Nothing it was written to protect was protected.** The entries `*.key`,
//! `*.pem`, `*.p12` and `*.pfx` were compared as literal text. No filename
//! contains the characters `*.pem`, so `cat("server.pem")` returned the
//! private key. Four of the thirteen patterns then in the list -- the four aimed
//! squarely at credential exfiltration by an agent -- had no effect at all.
//!
//! **Ordinary source files were unreadable.** `SAM`, `SYSTEM`, `SECURITY` and
//! `SOFTWARE` name Windows registry hives, but as substrings they matched any
//! path containing those letters:
//!
//! ```text
//! cat("src/security.rs")    error: cat: path validation failed
//! cat("filesystem.rs")      error: cat: path validation failed
//! cat("samples/data.txt")   error: cat: path validation failed
//! ```
//!
//! This repository has a `samples/` directory, so the shell could not read its
//! own examples. It was found by asking the shell to count the lines in each
//! file of `src/` -- thirty-three files succeeded and the thirty-fourth,
//! `security.rs`, did not.
//!
//! A denylist that is wrong in the permissive direction is a security defect
//! and one that is wrong in the restrictive direction is a usability defect;
//! the same line of code produced both, which is why this file asserts both.

use aethershell::security::validate_read_path;
use std::fs;
use std::path::{Path, PathBuf};

fn sandbox(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ae-blocked-{name}-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("create sandbox");
    dir
}

fn write(dir: &Path, name: &str, body: &str) -> String {
    let p = dir.join(name);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).expect("create parent");
    }
    fs::write(&p, body).expect("write fixture");
    p.to_string_lossy().into_owned()
}

// ── the hole: credentials the denylist named and did not block ──────────

#[test]
fn key_material_is_refused_by_extension() {
    let dir = sandbox("keys");
    for name in ["server.pem", "id.key", "bundle.p12", "cert.pfx"] {
        let path = write(&dir, name, "-----BEGIN PRIVATE KEY-----\n");
        let err = validate_read_path(&path).expect_err(&format!(
            "{name} must be refused: it is what the denylist is for"
        ));
        assert!(
            err.to_string().contains("blocked pattern"),
            "{name} was refused for the wrong reason: {err}"
        );
    }
}

#[test]
fn the_extension_match_is_case_insensitive_and_anchored() {
    let dir = sandbox("case");
    let upper = write(&dir, "SERVER.PEM", "x");
    assert!(
        validate_read_path(&upper).is_err(),
        "SERVER.PEM must be refused"
    );

    // `*.pem` blocks the extension, not the letters. A file that merely
    // mentions pem in its name is ordinary.
    let ok = write(&dir, "pemberton-notes.txt", "x");
    assert!(
        validate_read_path(&ok).is_ok(),
        "pemberton-notes.txt is not key material"
    );
}

// ── the breakage: ordinary files the denylist blocked ───────────────────

#[test]
fn source_files_named_after_a_registry_hive_are_readable() {
    let dir = sandbox("src");
    for name in [
        "security.rs",      // "SECURITY"
        "filesystem.rs",    // "SYSTEM"
        "system_info.rs",   // "SYSTEM"
        "software_list.rs", // "SOFTWARE"
        "samples/data.txt", // "SAM"
        "sample.json",      // "SAM"
    ] {
        let path = write(&dir, name, "ordinary content\n");
        assert!(
            validate_read_path(&path).is_ok(),
            "{name} was refused; substring matching on hive names is back"
        );
    }
}

// ── what must still be blocked ──────────────────────────────────────────

#[test]
fn literal_sensitive_paths_are_still_refused() {
    for path in ["/etc/passwd", "/etc/shadow", "/etc/sudoers"] {
        assert!(
            validate_read_path(path).is_err(),
            "{path} must remain blocked"
        );
    }
}

#[test]
fn a_separator_pattern_matches_on_a_component_boundary() {
    let dir = sandbox("sep");
    // `.ssh/id_rsa` must block the real thing...
    let real = write(&dir, ".ssh/id_rsa", "key");
    assert!(
        validate_read_path(&real).is_err(),
        ".ssh/id_rsa must be refused"
    );

    // ...and not a file that merely contains those letters run together.
    let note = write(&dir, "notes-about-ssh-id_rsa.md", "prose");
    assert!(
        validate_read_path(&note).is_ok(),
        "a document discussing id_rsa is not id_rsa"
    );
}

#[test]
fn a_bare_hive_name_is_refused_as_a_whole_component() {
    let dir = sandbox("hive");
    let hive = write(&dir, "SAM", "hive bytes");
    assert!(
        validate_read_path(&hive).is_err(),
        "a file named exactly SAM is a registry hive and must stay blocked"
    );
}

// ── non-vacuity ─────────────────────────────────────────────────────────

#[test]
fn non_vacuity_the_validator_is_reachable_and_discriminating() {
    // If `validate_read_path` accepted everything, half of this file passes
    // trivially; if it rejected everything, the other half does.
    let dir = sandbox("vac");
    let ok = write(&dir, "plain.txt", "hello");
    assert!(
        validate_read_path(&ok).is_ok(),
        "an ordinary file must validate"
    );
    assert!(
        validate_read_path("/etc/shadow").is_err(),
        "a blocked path must not validate"
    );
    assert!(
        validate_read_path("").is_err(),
        "the validator is not running at all"
    );
}
