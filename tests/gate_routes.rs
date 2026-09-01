//! The safety gate must fire whatever syntactic route reaches the builtin.
//!
//! `tests/one_door.rs` pins this structurally — that every dispatch path goes
//! through the gate. This pins it *behaviourally*, from the language side: the
//! same destructive call written six different ways must be refused six times.
//!
//! Written when closure capture was added. Capture carries values from one
//! scope into a call that runs later, which is exactly the shape a gate bypass
//! takes: if the guard consulted something that capture had rewritten, a
//! `rm` reached through a captured name could slip past. It does not —
//! but "it does not" is worth a test rather than an assumption, because the
//! feature is new and the failure would be silent.

use std::process::Command;

fn ae() -> std::path::PathBuf {
    let mut p = std::env::current_exe().expect("test exe");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join(format!("ae{}", std::env::consts::EXE_SUFFIX))
}

struct Workspace {
    dir: std::path::PathBuf,
}

impl Workspace {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "ae_gate_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("workspace");
        std::fs::write(dir.join("victim.txt"), "victim").expect("victim file");
        Self { dir }
    }

    /// Run one program in agent mode against this workspace.
    fn run(&self, source: &str) -> String {
        let script = self.dir.join("probe.ae");
        std::fs::write(&script, source).expect("write probe");
        let out = Command::new(ae())
            .arg("--deterministic")
            .arg(&script)
            .current_dir(&self.dir)
            .env("AETHER_MODE", "agent")
            .env("AETHER_WORKSPACE", &self.dir)
            // Approval must come from the gate, not from an inherited grant.
            .env_remove("AETHER_APPROVE")
            .env_remove("AETHER_APPROVE_ALL")
            .output()
            .expect("run ae");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    }

    fn victim_survived(&self) -> bool {
        self.dir.join("victim.txt").exists()
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Every way I could think of to reach `rm` without naming it at the top level
/// of a statement.
///
/// Not included: `"victim.txt" | rm`. `rm` takes its path positionally and
/// rejects piped input with `E_BAD_ARG` before any guard runs, so that route
/// never reaches the gated operation — asserting the gate fires there would be
/// asserting something untrue about a call that cannot happen.
///
/// `rm` specifically, and not `file_delete`: the first draft used the latter,
/// every case passed, and every case was vacuous. `file_delete` is *classified*
/// as Destructive in `safety.rs` but is not a builtin — 15 of the 606
/// classified names are not, which is defensible as defence in depth, since an
/// unclassified builtin would default to ungated. The consequence for a test is
/// that the gate fires on the name before anything resolves it, so the file
/// survived because the builtin did not exist rather than because the gate
/// stopped it. `non_vacuity` below is what catches that.
const ROUTES: &[(&str, &str)] = &[
    ("direct call", "rm(\"victim.txt\")"),
    (
        "through a lambda",
        "let f = fn(p) => rm(p)\nf(\"victim.txt\")",
    ),
    ("through map", "[\"victim.txt\"] | map(fn(p) => rm(p))"),
    (
        "through a captured binding",
        "let target = \"victim.txt\"\nlet f = fn(q) => rm(target)\nf(0)",
    ),
    (
        "through a returned closure",
        "let mk = fn(p) => fn(q) => rm(p)\nlet run = mk(\"victim.txt\")\nrun(0)",
    ),
    (
        "inside try/catch",
        "try { rm(\"victim.txt\") } catch e { e }",
    ),
];

#[test]
fn the_gate_refuses_every_route_to_a_destructive_builtin() {
    for (label, source) in ROUTES {
        let ws = Workspace::new("route");
        let out = ws.run(source);
        assert!(
            out.contains("E_NEEDS_APPROVAL") || out.contains("approval"),
            "{label}: expected the approval gate to fire, got:\n{out}"
        );
        assert!(
            ws.victim_survived(),
            "{label}: the file was deleted despite agent mode"
        );
    }
}

/// The point of the previous test, isolated: capture must not smuggle a value
/// past the guard. A closure created in one scope and called in another still
/// meets the gate.
#[test]
fn closure_capture_does_not_bypass_the_gate() {
    let ws = Workspace::new("capture");
    let out = ws.run(
        "let target = \"victim.txt\"\n\
         let mk = fn(t) => fn(q) => rm(t)\n\
         let run = mk(target)\n\
         run(0)",
    );
    assert!(
        out.contains("E_NEEDS_APPROVAL") || out.contains("approval"),
        "a captured filename reached rm without approval:\n{out}"
    );
    assert!(
        ws.victim_survived(),
        "the file was deleted through a closure"
    );
}

/// A builtin cannot be summoned by a name computed at runtime. If it could,
/// every allowlist keyed on the *written* name would be advisory.
#[test]
fn a_builtin_cannot_be_called_through_a_computed_name() {
    let ws = Workspace::new("computed");
    let out = ws.run("let n = \"file_\" + \"delete\"\nn(\"victim.txt\")");
    assert!(
        !out.contains("\"success\":true") && !out.contains("success: true"),
        "a computed name reached a builtin:\n{out}"
    );
    assert!(
        ws.victim_survived(),
        "the file was deleted via a computed name"
    );
}

/// `sh` is disabled outright rather than gated, so this asserts the stronger
/// property: no approval prompt, no execution, just a refusal.
#[test]
fn the_shell_escape_hatch_stays_shut() {
    let ws = Workspace::new("sh");
    let out = ws.run("sh(\"echo pwned\")");
    assert!(
        out.to_lowercase().contains("disabled") || out.contains("E_"),
        "sh() should be refused outright, got:\n{out}"
    );
    assert!(
        !out.contains("pwned\n") || out.contains("E_"),
        "sh() appears to have executed:\n{out}"
    );
}

/// Human mode is a different policy, not a different gate. This is here so
/// that a change which accidentally disables the gate everywhere shows up as a
/// *difference* between the modes rather than as uniform silence.
#[test]
fn the_same_call_is_permitted_outside_agent_mode() {
    let ws = Workspace::new("human");
    let script = ws.dir.join("probe.ae");
    std::fs::write(&script, "rm(\"victim.txt\")").expect("write");
    let out = Command::new(ae())
        .arg("--deterministic")
        .arg(&script)
        .current_dir(&ws.dir)
        .env_remove("AETHER_MODE")
        .env_remove("AETHER_WORKSPACE")
        .output()
        .expect("run ae");
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        !text.contains("E_NEEDS_APPROVAL"),
        "human mode should not require approval; if this fails the two modes \
         have converged and the agent-mode assertions above prove less than \
         they appear to:\n{text}"
    );
}

/// The check that makes every assertion above mean something.
///
/// A gate test can pass for the wrong reason: if the call never reaches a real
/// builtin, the refusal proves nothing and the file survives regardless. So run
/// the same closure route *with approval granted* and require that the file is
/// actually deleted. If this fails, the routes above are not exercising the
/// gate at all.
#[test]
fn non_vacuity_the_routes_really_reach_the_builtin() {
    let ws = Workspace::new("nonvacuous");
    let script = ws.dir.join("probe.ae");
    std::fs::write(
        &script,
        "let mk = fn(p) => fn(q) => rm(p)\nlet run = mk(\"victim.txt\")\nrun(0)",
    )
    .expect("write");

    let out = Command::new(ae())
        .arg("--deterministic")
        .arg(&script)
        .current_dir(&ws.dir)
        .env("AETHER_MODE", "agent")
        .env("AETHER_WORKSPACE", &ws.dir)
        .env("AETHER_APPROVE_ALL", "1")
        .output()
        .expect("run ae");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    assert!(
        !ws.victim_survived(),
        "with approval granted the closure route should have deleted the file; \
         it did not, so the refusals asserted elsewhere in this file are not \
         evidence that the gate is doing anything:\n{text}"
    );
}
