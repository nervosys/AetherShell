//! archive_test for bzip2, xz and zstd, and for the tar archives inside them.
//!
//! The ordinary tests run everywhere. They cover refusals, which happen before
//! any tool starts, and the zip and tar.gz paths that stay in-process.
//!
//! The `#[ignore]`d Linux tests create real archives with the bzip2, xz, zstd
//! and tar CLIs, and run only in the disposable distro
//! (scripts/disposable_linux_tests.sh). Each one checks the verdict against the
//! tool run directly, so the expected answer does not come from this crate.

#[cfg(target_os = "linux")]
#[path = "support/disposable.rs"]
mod disposable;

use iron_shell::value::Value;
use std::path::{Path, PathBuf};

fn call(name: &str, args: Vec<Value>) -> anyhow::Result<Value> {
    let mut env = iron_shell::env::Env::new();
    iron_shell::builtins::call(name, args, &mut env)
}

fn code_of(e: &anyhow::Error) -> String {
    let s = e.to_string();
    match s.find("E_") {
        Some(i) => s[i..]
            .split(|c: char| !(c.is_ascii_uppercase() || c == '_'))
            .next()
            .unwrap_or("E_?")
            .to_string(),
        None => format!("NO_CODE: {s}"),
    }
}

fn s(v: &str) -> Value {
    Value::Str(v.to_string())
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// A directory under the system temp dir, removed when dropped.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("ae_archive_test_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch(dir)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- ordinary tests: every platform -------------------------------------

#[test]
fn archive_test_refuses_a_missing_path() {
    let e = call("archive_test", vec![]).expect_err("no path given");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn archive_test_refuses_a_non_string_path() {
    let e = call("archive_test", vec![Value::Int(3)]).expect_err("an Int is not a path");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn archive_test_refuses_surplus_arguments() {
    let e = call("archive_test", vec![s("a.xz"), s("b.xz")]).expect_err("two paths");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn archive_test_refuses_pipeline_input() {
    let mut env = iron_shell::env::Env::new();
    let e = iron_shell::builtins::call_with_input(
        "archive_test",
        vec![s("a.xz")],
        Some(s("a.xz")),
        &mut env,
    )
    .expect_err("pipeline input is not accepted");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn archive_test_refuses_an_option_like_path() {
    let e = call("archive_test", vec![s("-t.xz")]).expect_err("a leading dash is an option");
    assert_eq!(code_of(&e), "E_BAD_ARG");
}

#[test]
fn an_xz_name_with_zstd_magic_is_refused() {
    let dir = Scratch::new("xz_zstd_magic");
    let f = dir.path("fake.xz");
    std::fs::write(&f, [0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x00, 0x00, 0x00]).unwrap();
    let e = call("archive_test", vec![s(&path_str(&f))]).expect_err("zstd bytes under .xz");
    assert_eq!(code_of(&e), "E_BAD_ARG");
    assert!(
        e.to_string().contains("xz"),
        "the refusal names the expected format: {e}"
    );
}

#[test]
fn plain_text_named_like_a_compressed_archive_is_refused() {
    let dir = Scratch::new("plain_text");
    for name in [
        "notes.bz2",
        "notes.xz",
        "notes.zst",
        "notes.tar.bz2",
        "notes.tar.xz",
        "notes.tar.zst",
    ] {
        let f = dir.path(name);
        std::fs::write(&f, b"just some text, not an archive\n").unwrap();
        let e = call("archive_test", vec![s(&path_str(&f))])
            .expect_err("text is not a compressed archive");
        assert_eq!(code_of(&e), "E_BAD_ARG", "{name}: {e}");
    }
}

#[test]
fn a_missing_compressed_file_is_not_found() {
    let dir = Scratch::new("missing");
    let f = dir.path("absent.tar.xz");
    let e = call("archive_test", vec![s(&path_str(&f))]).expect_err("no such file");
    assert_eq!(code_of(&e), "E_NOT_FOUND");
}

#[test]
fn a_missing_tool_is_e_tool_missing() {
    // The magic bytes are right, so the call reaches the tool, which is not on
    // PATH. PATH is restored before the assertion.
    let dir = Scratch::new("no_tool");
    let f = dir.path("ok.xz");
    std::fs::write(&f, [0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0x00, 0x00]).unwrap();
    let empty = dir.path("empty_path");
    std::fs::create_dir_all(&empty).unwrap();
    let saved = std::env::var_os("PATH");
    std::env::set_var("PATH", &empty);
    let result = call("archive_test", vec![s(&path_str(&f))]);
    match saved {
        Some(v) => std::env::set_var("PATH", v),
        None => std::env::remove_var("PATH"),
    }
    let e = result.expect_err("xz is not on the empty PATH");
    assert_eq!(code_of(&e), "E_TOOL_MISSING", "{e}");
}

#[test]
fn zip_and_tar_gz_are_still_tested_in_process() {
    let fixtures = |name: &str| format!("tests/fixtures/archives/{name}");
    assert_eq!(
        call("archive_test", vec![s(&fixtures("sample.zip"))]).unwrap(),
        Value::Bool(true)
    );
    assert_eq!(
        call("archive_test", vec![s(&fixtures("damaged.zip"))]).unwrap(),
        Value::Bool(false)
    );
    assert_eq!(
        call("archive_test", vec![s(&fixtures("sample.tar.gz"))]).unwrap(),
        Value::Bool(true)
    );
}

// ---- disposable Linux tests: real tools, real archives ------------------

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::fs::File;
    use std::process::{Command, Stdio};

    /// (tool, arguments that make it write to stdout, archive extension)
    const STREAMS: [(&str, &[&str], &str); 3] = [
        ("bzip2", &["-c"], "bz2"),
        ("xz", &["-c"], "xz"),
        ("zstd", &["-q", "-c"], "zst"),
    ];

    /// A deterministic text payload and a plain tar of it.
    struct Fixture {
        dir: Scratch,
        payload: PathBuf,
        tar: PathBuf,
    }

    impl Fixture {
        fn new(name: &str) -> Fixture {
            let dir = Scratch::new(name);
            let payload = dir.path("data.txt");
            let text: String = (0..4096)
                .map(|i| format!("line {i} of the fixture\n"))
                .collect();
            std::fs::write(&payload, text).unwrap();
            let tar = dir.path("data.tar");
            let status = Command::new("tar")
                .arg("-cf")
                .arg(&tar)
                .arg("-C")
                .arg(&dir.0)
                .arg("data.txt")
                .status()
                .expect("run tar");
            assert!(status.success(), "tar could not build the fixture");
            Fixture { dir, payload, tar }
        }
    }

    /// Compresses `input` into `out` with the CLI, writing its stdout to `out`.
    fn make(tool: &str, args: &[&str], input: &Path, out: &Path) {
        let file = File::create(out).expect("create archive");
        let status = Command::new(tool)
            .args(args)
            .arg(input)
            .stdout(Stdio::from(file))
            .stderr(Stdio::null())
            .status()
            .expect("run the archive tool");
        assert!(status.success(), "{tool} could not write {}", out.display());
    }

    /// The tool's own integrity test, run directly.
    fn tool_accepts(tool: &str, archive: &Path) -> bool {
        let mut command = Command::new(tool);
        if tool == "zstd" {
            command.arg("-q");
        }
        command
            .args(["-t", "--"])
            .arg(archive)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Decompresses with the CLI and lists the result with tar, run directly.
    fn tar_accepts(tool: &str, archive: &Path, scratch: &Path) -> bool {
        let inner = scratch.join("oracle_inner.tar");
        let file = File::create(&inner).expect("create oracle output");
        let decompressed = Command::new(tool)
            .arg("-dc")
            .arg(archive)
            .stdout(Stdio::from(file))
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        decompressed
            && Command::new("tar")
                .arg("-tf")
                .arg(&inner)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
    }

    fn verdict(path: &Path) -> Value {
        call("archive_test", vec![s(&path_str(path))]).expect("a valid call answers")
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn valid_bzip2_xz_and_zstd_streams_pass() {
        disposable::require();
        let fx = Fixture::new("valid_streams");
        for (tool, args, ext) in STREAMS {
            let archive = fx.dir.path(&format!("data.txt.{ext}"));
            make(tool, args, &fx.payload, &archive);
            assert!(
                tool_accepts(tool, &archive),
                "{tool} itself rejects its own output"
            );
            assert_eq!(
                verdict(&archive),
                Value::Bool(true),
                "{tool}: data.txt.{ext}"
            );
        }
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn valid_tar_archives_inside_bzip2_xz_and_zstd_pass() {
        disposable::require();
        let fx = Fixture::new("valid_tars");
        for (tool, args, ext) in STREAMS {
            let archive = fx.dir.path(&format!("data.tar.{ext}"));
            make(tool, args, &fx.tar, &archive);
            assert!(
                tar_accepts(tool, &archive, &fx.dir.0),
                "{tool} and tar do not accept their own output"
            );
            assert_eq!(
                verdict(&archive),
                Value::Bool(true),
                "{tool}: data.tar.{ext}"
            );
        }
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn truncated_archives_report_false() {
        disposable::require();
        let fx = Fixture::new("truncated");
        for (tool, args, ext) in STREAMS {
            for (inner, label) in [(&fx.payload, "txt"), (&fx.tar, "tar")] {
                let good = fx.dir.path(&format!("good.{label}.{ext}"));
                make(tool, args, inner, &good);
                let bytes = std::fs::read(&good).unwrap();
                let cut = fx.dir.path(&format!("cut.{label}.{ext}"));
                std::fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
                assert!(!tool_accepts(tool, &cut), "{tool} accepts a truncated file");
                assert_eq!(
                    verdict(&cut),
                    Value::Bool(false),
                    "{tool} {label}: truncated"
                );
            }
        }
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn byte_flipped_archives_report_false() {
        disposable::require();
        let fx = Fixture::new("flipped");
        for (tool, args, ext) in STREAMS {
            for (inner, label) in [(&fx.payload, "txt"), (&fx.tar, "tar")] {
                let good = fx.dir.path(&format!("good.{label}.{ext}"));
                make(tool, args, inner, &good);
                let mut bytes = std::fs::read(&good).unwrap();
                let middle = bytes.len() / 2;
                bytes[middle] ^= 0xFF;
                let flipped = fx.dir.path(&format!("flip.{label}.{ext}"));
                std::fs::write(&flipped, &bytes).unwrap();
                assert!(
                    !tool_accepts(tool, &flipped),
                    "{tool} accepts a flipped byte"
                );
                assert_eq!(
                    verdict(&flipped),
                    Value::Bool(false),
                    "{tool} {label}: flipped"
                );
            }
        }
    }

    #[test]
    #[ignore = "disposable environment only: scripts/disposable_linux_tests.sh"]
    fn tar_xz_with_a_corrupt_inner_tar_reports_false() {
        disposable::require();
        let fx = Fixture::new("corrupt_inner");
        // Valid xz around bytes that are not a tar archive.
        let garbage: Vec<u8> = (0..2048u32)
            .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
            .collect();
        let bad_tar = fx.dir.path("bad.tar");
        std::fs::write(&bad_tar, &garbage).unwrap();
        let archive = fx.dir.path("bad.tar.xz");
        make("xz", &["-c"], &bad_tar, &archive);
        assert!(
            !tar_accepts("xz", &archive, &fx.dir.0),
            "tar accepts the garbage inside the xz"
        );
        assert_eq!(verdict(&archive), Value::Bool(false));
    }
}
