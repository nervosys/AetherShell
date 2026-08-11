//! The Python SDK invokes the `ae` binary by command line. Nothing checked that
//! the flags it passes exist.
//!
//! They did not. `AetherRuntime.eval` ran `ae -e <code> --json`; the binary
//! takes `-c/--command` and has no `--json`, so clap rejected the call and every
//! `eval()` raised `RuntimeError`. The SDK's documented entry point could not
//! have worked against any released build.
//!
//! This test reads the flags out of the SDK source and runs the real binary with
//! them, so the two cannot drift apart again without failing the build.

const SDK: &str = include_str!("../integrations/python/python/aethershell/__init__.py");

#[test]
fn the_sdk_does_not_use_flags_the_binary_rejects() {
    for dead in ["\"-e\"", "\"--json\""] {
        assert!(
            !SDK.contains(dead),
            "the SDK passes {dead}, which `ae` does not accept"
        );
    }
}

#[test]
fn the_flags_the_sdk_passes_actually_work() {
    // Not "are documented" — run them.
    assert!(
        SDK.contains("\"-c\"") && SDK.contains("\"--deterministic\""),
        "expected the SDK to invoke `-c ... --deterministic`; if that changed, \
         update this test *and* verify the new flags against the binary"
    );

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ae"))
        .args(["-c", "2 + 2", "--deterministic"])
        .output()
        .expect("run ae");

    assert!(
        out.status.success(),
        "ae rejected the SDK's flags: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        stdout.trim(),
        "4",
        "the SDK parses this with json.loads, so it must be canonical JSON"
    );
}
