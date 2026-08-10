//! Effect-tagging coverage (docs/AGENTIC_FIRST_DESIGN.md §5.3, §12).
//!
//! `safety::effect_of` decides whether a builtin is gated by policy, approval and
//! the audit log. Its fall-through is `Effect::Pure` — the *least* restrictive
//! class — so a builtin nobody tagged is silently treated as side-effect-free.
//! §12 flagged tagging 1,100+ builtins as unfinished labour and proposed "a lint
//! that fails CI on untagged builtins". This is that lint.
//!
//! It does not demand every builtin be tagged; most genuinely are pure. It demands
//! that no builtin whose *name* advertises a side effect falls through to `Pure`,
//! because that is the failure mode that matters: an ungated destructive call that
//! looks safe to every consumer of `effect_of`, including the agent-facing
//! ontology's `x-effect` annotation.

use aethershell::builtins::BUILTIN_LOOKUP;
use aethershell::safety::{effect_of, Effect};

/// Name fragments that assert a side effect. Deliberately narrow: each one is a
/// verb an implementation has to *act* on, not a noun that merely mentions state.
const DESTRUCTIVE: &[&str] = &["delete", "destroy", "purge", "wipe", "truncate", "drop_"];
const EXECUTING: &[&str] = &["_exec", "exec_", "spawn", "_shell", "shell_", "sudo"];
const KILLING: &[&str] = &["kill", "terminate", "sigkill"];
/// Egress: sends data somewhere it cannot be recalled from. Under-tagging these
/// is the exfiltration blind spot — a `Pure` tag means no `Network` governor
/// accounting and no audit entry.
const NETWORKING: &[&str] = &["upload", "download", "publish", "_post", "post_", "webhook"];
/// Writes that persist outside the current value space.
const WRITING: &[&str] = &["_write", "write_", "_save", "save_", "install", "_mount"];
/// Changing permissions, ownership, or the run-state of a service. These act on
/// the machine rather than on data, so an untagged one is a privilege operation
/// nobody is metering.
const CONTROLLING: &[&str] = &[
    "chmod", "chown", "restart", "deploy", "_service", "service_",
];

/// Names that match a fragment but are genuinely pure, with the reason. Every
/// entry here is a claim someone can check — which is the point of listing them
/// rather than loosening the patterns.
fn is_known_pure(name: &str) -> bool {
    const ALLOWED: &[&str] = &[
        // Verified by reading the implementations: a `which("sudo")` lookup and
        // an env-var read respectively. They report on the platform's shell
        // without invoking one.
        "platform_has_sudo",
        "platform_shell_type",
        // Three names that assert execution and perform none. All three were
        // tagged `Exec` on the first pass of this lint — from the name alone,
        // which is precisely the mistake the lint exists to catch, made while
        // fixing it. Caught by reading each body before wiring a guard to it,
        // which is the only step that would have caught them.
        //   sudo_exec:     returns "use sudo directly in terminal", runs nothing
        //   watchexec_run: returns a suggested `watchexec --` invocation
        //   env_shell:     reads $SHELL / %COMSPEC%
        //   remote_exec:   a stub — "Simulate remote execution (in real impl
        //                  would use SSH/RPC)". It used to report
        //                  `status: "executed"`, a separate honesty problem
        //                  from the effect tag; now reports `simulated`.
        "sudo_exec",
        "watchexec_run",
        "env_shell",
        "remote_exec",
        "exec_remote",
        // Transaction bookkeeping: `tx_savepoint` matches "_save" but only names
        // a point in the journal. It writes no user data.
        "tx_savepoint",
        // A stub with no cloud client and no subprocess: it records a deployment
        // intent locally. It used to report `status: "deployed"` — corrected to
        // `simulated` at the same time this entry was added.
        "cloud_deploy",
        // Predicates and formatters that only *describe* an effect.
        "can_delete",
        "is_executable",
        "shell_quote",
        "shell_escape",
        "shell_split",
        "exec_plan",
        "explain_exec",
    ];
    ALLOWED.contains(&name)
}

fn matches_any(name: &str, fragments: &[&str]) -> bool {
    let lower = name.to_lowercase();
    fragments.iter().any(|f| lower.contains(f))
}

/// Report the shape of the tagging so the number is visible rather than assumed.
#[test]
fn effect_tagging_coverage_is_reported() {
    let mut counts = std::collections::BTreeMap::new();
    for name in BUILTIN_LOOKUP.keys() {
        *counts
            .entry(format!("{:?}", effect_of(name)))
            .or_insert(0usize) += 1;
    }
    let total: usize = counts.values().sum();
    let pure = *counts.get("Pure").unwrap_or(&0);
    println!(
        "effect coverage: {} builtins, {} classified, {} fall through to Pure ({:.0}%)",
        total,
        total - pure,
        pure,
        pure as f64 / total as f64 * 100.0
    );
    for (effect, n) in &counts {
        println!("  {effect}: {n}");
    }
    assert!(total > 1000, "expected the full builtin table, saw {total}");
}

/// The lint. A name that advertises a side effect must not classify as `Pure`.
#[test]
fn no_builtin_that_names_a_side_effect_is_classified_pure() {
    let mut offenders: Vec<(&str, &'static str)> = Vec::new();
    for name in BUILTIN_LOOKUP.keys() {
        if is_known_pure(name) || effect_of(name) != Effect::Pure {
            continue;
        }
        let kind = if matches_any(name, DESTRUCTIVE) {
            "destructive"
        } else if matches_any(name, EXECUTING) {
            "executing"
        } else if matches_any(name, KILLING) {
            "killing"
        } else if matches_any(name, NETWORKING) {
            "networking"
        } else if matches_any(name, WRITING) {
            "writing"
        } else if matches_any(name, CONTROLLING) {
            "controlling"
        } else {
            continue;
        };
        offenders.push((name, kind));
    }
    offenders.sort_unstable();

    assert!(
        offenders.is_empty(),
        "{} builtin(s) name a side effect but classify as Pure, so policy, \
         approval and the audit log do not apply to them:\n{}",
        offenders.len(),
        offenders
            .iter()
            .map(|(n, k)| format!("  {n} ({k})"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
