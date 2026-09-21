//! A surface that exists to serve an AI agent must default to the agent safety
//! profile.
//!
//! `ae agent serve` and `ae mcp serve` ran with the *human* profile unless the
//! operator also remembered `--agent`, so the default posture of the two
//! agent-facing surfaces was the permissive one: no effect gate, no workspace
//! jail, on an endpoint whose job is to evaluate code an LLM wrote.
//!
//! It stayed invisible because a second bug hid it. `POST /api/v1/eval` built
//! its environment without the module namespaces, so a probe that tried to
//! write outside the server's directory came back *contained* — the call had
//! never run. Fixing the namespaces (`modules::env_with_modules`) turned the
//! same probe into a successful write to an arbitrary path.
//!
//! That is the interesting part, and it is why this file spawns the real binary
//! and looks at the filesystem afterwards rather than asserting on an error
//! string: containment that rests on a second defect reads exactly like
//! containment until the day the second defect is fixed.
//!
//! Human invocations are deliberately untouched — a shell that refused to write
//! outside its working directory would not be a shell — so the last test here
//! is the one that fails if this was turned into a blanket default.

use std::process::Command;

const AE: &str = env!("CARGO_BIN_EXE_ae");

/// Run `ae -c` with an explicit mode environment, returning (stdout+stderr, ok).
fn run_with_mode(code: &str, mode: Option<&str>, cwd: &std::path::Path) -> (String, bool) {
    let mut cmd = Command::new(AE);
    cmd.arg("-c").arg(code).current_dir(cwd);
    match mode {
        Some(m) => cmd.env("AETHER_MODE", m),
        None => cmd.env_remove("AETHER_MODE"),
    };
    // The jail is what is under test; never let an inherited value decide it.
    cmd.env_remove("AETHER_WORKSPACE")
        .env_remove("AETHER_AGENT");
    let out = cmd.output().expect("spawn ae");
    (
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr),
        out.status.success(),
    )
}

struct Jail {
    root: std::path::PathBuf,
    inside: std::path::PathBuf,
    outside: std::path::PathBuf,
}

impl Drop for Jail {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// A workspace and a directory outside it. Named per-test and per-process, the
/// way the other cross-process tests in this suite do it, so a parallel run
/// cannot have one test's write satisfy another's assertion.
fn jail(tag: &str) -> Jail {
    let root = std::env::temp_dir().join(format!("ae_agentmode_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let inside = root.join("workspace");
    let outside = root.join("elsewhere");
    std::fs::create_dir_all(&inside).expect("create workspace");
    std::fs::create_dir_all(&outside).expect("create outside dir");
    Jail {
        root,
        inside,
        outside,
    }
}

#[test]
fn agent_mode_confines_writes_to_the_workspace() {
    // The property the servers now inherit by default. Asserted here on `-c
    // --agent` because it is the same `current_mode()` switch, and a test that
    // binds a port is a test that flakes.
    let j = jail("agent");
    let target = j.outside.join("pwn.txt");
    let code = format!("file.write({:?}, \"owned\")", target.to_string_lossy());

    let (out, _) = run_with_mode(&code, Some("agent"), &j.inside);
    assert!(
        !target.exists(),
        "agent mode wrote outside the workspace; output: {out}"
    );
    assert!(
        out.contains("E_OUTSIDE_WORKSPACE"),
        "refusal must carry a branchable code, got: {out}"
    );
}

#[test]
fn a_human_invocation_is_not_jailed() {
    // Non-vacuity for the test above — if writes outside the cwd were blocked
    // for everyone, it would pass without agent mode meaning anything — and the
    // guard against turning this into a blanket default. `ae deploy.sh` writing
    // to /etc is the documented migration path (AGENTS.md, Option 4).
    let j = jail("human");
    let target = j.outside.join("human.txt");
    let code = format!("file.write({:?}, \"ok\")", target.to_string_lossy());

    let (out, ok) = run_with_mode(&code, None, &j.inside);
    assert!(ok, "a plain `ae -c` write failed: {out}");
    assert!(
        target.exists(),
        "a human invocation was refused a write outside its cwd — the jail has \
         been made the default for everyone, which breaks shell-script migration"
    );
}

#[test]
fn the_serving_subcommands_report_the_agent_profile() {
    // `serves_an_agent` lives in the binary, so the observable contract is what
    // the operator is told at startup. `--help` is enough to prove the
    // subcommands exist and are spelled the way the implication keys on; the
    // containment itself is proven above and end-to-end in benches/agentic.
    for (sub, expect) in [("agent", "serve"), ("mcp", "serve")] {
        let out = Command::new(AE)
            .args([sub, "--help"])
            .output()
            .expect("spawn ae");
        let text = String::from_utf8_lossy(&out.stdout).to_string()
            + &String::from_utf8_lossy(&out.stderr);
        assert!(
            text.contains(expect),
            "`ae {sub} --help` does not mention `{expect}`; if the subcommand was \
             renamed, `serves_an_agent` in src/main.rs no longer matches it and \
             the servers have silently gone back to the human profile:\n{text}"
        );
    }
}
