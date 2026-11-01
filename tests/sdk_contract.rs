//! The Python SDK invokes the `ish` binary by command line. Nothing checked that
//! the flags it passes exist.
//!
//! They did not. `IronShellRuntime.eval` ran `ish -e <code> --json`; the binary
//! takes `-c/--command` and has no `--json`, so clap rejected the call and every
//! `eval()` raised `RuntimeError`. The SDK's documented entry point could not
//! have worked against any released build.
//!
//! This test reads the flags out of the SDK source and runs the real binary with
//! them, so the two cannot drift apart again without failing the build.

const SDK: &str = include_str!("../integrations/python/python/ironshell/__init__.py");

#[test]
fn the_sdk_does_not_use_flags_the_binary_rejects() {
    for dead in ["\"-e\"", "\"--json\""] {
        assert!(
            !SDK.contains(dead),
            "the SDK passes {dead}, which `ish` does not accept"
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

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_ish"))
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

/// The LangChain tool text is read by a model, which then calls those names.
/// It said `read_file`, which no builtin answers to.
#[test]
fn the_tools_the_langchain_text_names_exist() {
    const LANGCHAIN: &str = include_str!("../integrations/python/python/ironshell/langchain.py");

    let between = |start: &str, end: &str| -> String {
        let (_, rest) = LANGCHAIN
            .split_once(start)
            .unwrap_or_else(|| panic!("langchain.py no longer contains {start:?}"));
        rest.split(end).next().unwrap().to_string()
    };
    let listed = between("use tools like ", ". ") + ", " + &between("(e.g., [", "]");

    let names: Vec<&str> = listed
        .split(", ")
        .map(|name| name.trim_matches('\''))
        .collect();
    assert!(names.len() >= 4, "parsed too few names: {names:?}");
    for name in names {
        assert!(
            iron_shell::builtins::is_dispatched(name),
            "langchain.py tells the model to call `{name}`, which is not a builtin"
        );
    }
}
