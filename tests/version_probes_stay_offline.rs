//! Asking a tool its version must not reach the network.
//!
//! The egress gate (`benches/agentic/network-egress.mjs`) traced every
//! `platform_*` builtin on the CI runner, with AETHER_MAX_NET=0, and found the
//! connections came from the tools being asked: `kubectl version` contacted
//! the API server, packer checked HashiCorp for updates, vcpkg and az sent
//! telemetry, and `bazel` -- bazelisk there -- downloaded Bazel. This puts
//! fake tools first on PATH that record how they were called, and checks the
//! probe asks each one the offline question.

#![cfg(unix)]

use aethershell::value::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn fake(dir: &Path, name: &str, log: &Path) {
    fake_answering(dir, name, log, &format!("{name} version 1.2.3"));
}

fn fake_answering(dir: &Path, name: &str, log: &Path, answer: &str) {
    let script = format!(
        "#!/bin/sh\necho \"{name} $* CHECKPOINT_DISABLE=$CHECKPOINT_DISABLE \
         AZURE_CORE_COLLECT_TELEMETRY=$AZURE_CORE_COLLECT_TELEMETRY\" >> {log}\n\
         echo '{answer}'\n",
        log = log.display()
    );
    let p = dir.join(name);
    std::fs::write(&p, script).unwrap();
    std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn version_probes_ask_the_offline_question() {
    let dir = std::env::temp_dir().join(format!("ae-fake-tools-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let log = dir.join("calls.log");

    fake(&dir, "kubectl", &log);
    fake(&dir, "packer", &log);
    fake_answering(
        &dir,
        "az",
        &log,
        r#"{"azure-cli": "2.64.0", "extensions": {}}"#,
    );
    // `bazel` is a symlink to bazelisk, as on the GitHub runner.
    fake(&dir, "bazelisk-linux_amd64", &log);
    std::os::unix::fs::symlink(dir.join("bazelisk-linux_amd64"), dir.join("bazel")).unwrap();

    // The only test in this binary, so changing PATH races nothing.
    let old = std::env::var("PATH").unwrap_or_default();
    std::env::set_var("PATH", format!("{}:{old}", dir.display()));
    let mut env = aethershell::env::Env::new();
    let result = aethershell::builtins::call("platform_tool_versions", vec![], &mut env);
    std::env::set_var("PATH", old);

    let Value::Record(versions) = result.unwrap() else {
        panic!("platform_tool_versions did not return a record")
    };
    let calls = std::fs::read_to_string(&log).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);

    let call_of = |tool: &str| {
        calls
            .lines()
            .find(|l| l.starts_with(&format!("{tool} ")))
            .unwrap_or_else(|| panic!("{tool} was never asked; calls:\n{calls}"))
            .to_string()
    };
    assert!(
        call_of("kubectl").starts_with("kubectl version --client "),
        "{}",
        call_of("kubectl")
    );
    assert!(call_of("packer").contains("CHECKPOINT_DISABLE=1"));
    // `az --version` checks online for updates; `az version` does not.
    assert!(
        call_of("az").starts_with("az version --output json "),
        "{}",
        call_of("az")
    );
    assert!(call_of("az").contains("AZURE_CORE_COLLECT_TELEMETRY=no"));
    let az = versions
        .iter()
        .find(|(k, _)| k.ends_with(".az"))
        .map(|(_, v)| v.clone());
    assert_eq!(az, Some(Value::Str("azure-cli 2.64.0".into())));
    assert!(
        !calls.contains("bazelisk"),
        "bazelisk was run, which downloads Bazel:\n{calls}"
    );

    // Non-vacuity: not running bazelisk must not hide it. It is installed.
    let bazel = versions
        .iter()
        .find(|(k, _)| k.ends_with(".bazel"))
        .map(|(_, v)| v.clone());
    assert!(
        matches!(&bazel, Some(Value::Str(s)) if s.contains("bazelisk")),
        "bazel: {bazel:?}"
    );
}
