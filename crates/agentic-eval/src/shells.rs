//! Evaluating **shells** for agentic AI use.
//!
//! [`languages`](crate::languages) profiles the language a program is written
//! in. This profiles the shell an agent *drives* — the thing that stands
//! between a model and a machine for most of an autonomous session. Same four
//! axes, same 0.0–1.0 scale:
//!
//! - **token efficiency** — the command an agent writes plus the output it must
//!   read back, which is the term that dominates a long transcript.
//! - **determinism** — the same command twice, and on a second machine: can the
//!   result be cached, diffed or replayed?
//! - **reliability** — when the agent gets it wrong, can it tell *what* went
//!   wrong from the bytes returned, without parsing prose?
//! - **safety** — what a mistaken command can reach before anything stops it.
//!
//! # These scores are anchored, not invented
//!
//! `crates/agentic-eval/README.md` draws a line between **measured** numbers
//! and **curated** ones, and warns that the curated tables rank this
//! ecosystem's own systems first. That warning applies here, so the scores
//! below are pinned to executed measurements wherever one exists. Every axis of
//! every shell cites a run from `benches/agentic/`, on 500 real GitHub records
//! and this repository's own source tree, with exact cl100k token counts:
//!
//! | Evidence | Experiment |
//! | --- | --- |
//! | command + output tokens, scalar answers | E1 (`run.mjs`) |
//! | command + output tokens, tabular answers | E2 (`run-shellops.mjs`) |
//! | machine-readable codes, repair hints, byte cost | E3 (`errors.mjs`) |
//! | dangerous operations contained, by consequence | E4 (`safety.mjs`) |
//! | byte-stability across locale, timezone, width | E5 (`environment.mjs`) |
//!
//! Judgment still enters in mapping a measurement onto 0.0–1.0, and in the
//! properties nothing here executes (interactive ergonomics, install base,
//! how a shell behaves on a machine unlike the test host). Two informed people
//! would place these differently. The measurements they rest on are in
//! `benches/agentic/results/` and do not move.
//!
//! ```
//! use agentic_eval::shells::{profile, rank_shells, Shell};
//! let bash = profile(Shell::Bash);
//! assert!(bash.safety < 0.2); // E4: 0 of 5 dangerous operations contained
//! let ranked = rank_shells();
//! assert_eq!(ranked.len(), Shell::all().len());
//! assert!(ranked[0].fitness() >= ranked[ranked.len() - 1].fitness());
//! ```

/// Shells with curated agentic profiles.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Nushell,
    /// AetherShell in its default (human) rendering.
    AetherShell,
    /// AetherShell under `--agent --workspace`: AECON output, effect gating on.
    /// Profiled separately because the two differ on three of the four axes,
    /// and quoting only the better one would be sleight of hand.
    AetherShellAgent,
}

impl Shell {
    /// All profiled shells, in fixed (deterministic) order.
    pub fn all() -> [Shell; 7] {
        [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Nushell,
            Shell::AetherShell,
            Shell::AetherShellAgent,
        ]
    }

    /// Canonical lowercase name.
    pub fn name(self) -> &'static str {
        match self {
            Shell::Bash => "bash",
            Shell::Zsh => "zsh",
            Shell::Fish => "fish",
            Shell::PowerShell => "powershell",
            Shell::Nushell => "nushell",
            Shell::AetherShell => "aethershell",
            Shell::AetherShellAgent => "aethershell-agent",
        }
    }

    /// Parse a (case-insensitive) name; accepts common aliases.
    pub fn from_name(name: &str) -> Option<Shell> {
        match name.to_ascii_lowercase().as_str() {
            "bash" | "sh" => Some(Shell::Bash),
            "zsh" => Some(Shell::Zsh),
            "fish" => Some(Shell::Fish),
            "powershell" | "pwsh" | "ps" => Some(Shell::PowerShell),
            "nushell" | "nu" => Some(Shell::Nushell),
            "aethershell" | "ae" => Some(Shell::AetherShell),
            "aethershell-agent" | "ae-agent" | "aeagent" => Some(Shell::AetherShellAgent),
            _ => None,
        }
    }

    /// Was this shell executed in the benchmarks the scores cite, or scored by
    /// analogy to one that was?
    ///
    /// zsh and fish are **not** executed anywhere in `benches/agentic/`: their
    /// profiles are inherited from bash, whose POSIX core and `ls -l` output
    /// they share. That is an assumption, and it is marked rather than hidden —
    /// fish in particular diverges (no POSIX compatibility, different error
    /// text) and a reader should discount it accordingly.
    pub fn measured(self) -> bool {
        !matches!(self, Shell::Zsh | Shell::Fish)
    }
}

/// A curated agentic profile of a shell: four 0.0–1.0 axis scores, the evidence
/// behind them, and whether the shell was actually executed.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct ShellProfile {
    /// Which shell this profiles.
    pub shell: Shell,
    /// Command + output tokens for the same answers (1.0 = cheapest observed).
    pub token_efficiency: f64,
    /// Byte-stability of output across repetitions and environments.
    pub determinism: f64,
    /// Whether a failure carries something an agent can branch on.
    pub reliability: f64,
    /// Fraction of dangerous operations the shell itself contains.
    pub safety: f64,
    /// Whether the scores rest on an execution of this shell, or on analogy.
    pub measured: bool,
    /// Why: one evidence string per axis, citing the run it comes from.
    pub evidence: Vec<&'static str>,
}

impl ShellProfile {
    /// Composite agentic fitness: the unweighted mean of the four axes.
    pub fn fitness(&self) -> f64 {
        (self.token_efficiency + self.determinism + self.reliability + self.safety) / 4.0
    }
}

impl std::fmt::Display for ShellProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: fitness {:.2} (tokens {:.2}, determinism {:.2}, reliability {:.2}, safety {:.2}){}",
            self.shell.name(),
            self.fitness(),
            self.token_efficiency,
            self.determinism,
            self.reliability,
            self.safety,
            if self.measured { "" } else { "  [by analogy to bash]" }
        )
    }
}

/// The curated profile for `shell`.
pub fn profile(shell: Shell) -> ShellProfile {
    match shell {
        Shell::Bash => ShellProfile {
            shell,
            // E2: 1,463 tokens for eight tabular answers, the most of any
            // engine; E1: 345 with jq, second cheapest, because a scalar
            // answer has no structure to be efficient about.
            token_efficiency: 0.35,
            // E5: output changed with the timezone (`ls -l` prints mtime in
            // local time), so the same unchanged directory diffs against
            // itself across two machines.
            determinism: 0.55,
            // E3: 0/10 machine-readable codes, 1/10 repair hints — but the
            // cheapest failures of any shell at 42 mean bytes, and the only
            // shell with useful exit-status granularity (5/10 distinct).
            reliability: 0.35,
            // E4: 0 of 5 dangerous operations contained. Nothing in the shell
            // stops a mistaken command; containment is the container's job.
            safety: 0.05,
            measured: true,
            evidence: vec![
                "E2: 1,463 tokens for eight tabular answers (1,395 of them output) — most of any engine; six fields nobody asked for on every `ls -l` line",
                "E5: varies with timezone; an unchanged file reads Sep 16 18:56 in UTC and Sep 17 03:56 in Asia/Tokyo",
                "E3: 0/10 machine-readable codes and 1/10 repair hints, at 42 mean bytes — the cheapest errors measured, and the least actionable",
                "E3: the only shell with exit-status granularity worth having (127 command-not-found, 2 syntax, 1 permission)",
                "E4: 0/5 contained — write, delete, truncate outside the workspace, arbitrary exec and network egress all executed",
                "E1: with jq available it is the second-cheapest engine on scalar answers (345 tokens); without jq, the most expensive (704)",
            ],
        },

        // zsh and fish share bash's POSIX core, `ls -l` output and untyped
        // pipeline, and neither was executed. Scored as bash with a small
        // reliability credit for better interactive diagnostics — which is an
        // assumption about a property nothing here measured.
        Shell::Zsh => ShellProfile {
            shell,
            token_efficiency: 0.35,
            determinism: 0.55,
            reliability: 0.38,
            safety: 0.05,
            measured: false,
            evidence: vec![
                "NOT EXECUTED: profiled by analogy to bash, whose POSIX core, `ls -l` output format and untyped pipeline it shares",
                "small reliability credit for richer interactive diagnostics; this is judgment, not a measurement",
                "the timezone and containment findings carry over unchanged, because they are properties of coreutils and of having no effect model",
            ],
        },
        Shell::Fish => ShellProfile {
            shell,
            token_efficiency: 0.35,
            determinism: 0.55,
            reliability: 0.40,
            safety: 0.05,
            measured: false,
            evidence: vec![
                "NOT EXECUTED: profiled by analogy to bash. Discount this one further — fish deliberately breaks POSIX compatibility and has its own error text, so the analogy is weaker than for zsh",
                "reliability credit for the clearest human-facing errors of the POSIX-family shells; again judgment, not measurement",
                "same containment posture: no effect model, nothing refused",
            ],
        },

        Shell::PowerShell => ShellProfile {
            shell,
            // E2: 735 tokens, second cheapest — objects rather than text, and
            // `Select-Object` returns the two fields asked for.
            token_efficiency: 0.60,
            // E5: 1 of 6 distinct outputs, stable across every environment
            // varied. Ties AetherShell on this axis.
            determinism: 0.90,
            // E3: 8/10 failures actually failed — two returned exit 0 with an
            // empty or zero result, which is the worst shape available.
            reliability: 0.25,
            // E4: 0 of 5 contained.
            safety: 0.05,
            measured: true,
            evidence: vec![
                "E2: 735 tokens for eight tabular answers, second cheapest — typed objects, and Select-Object emits the fields requested",
                "E5: byte-stable across all six locale/timezone/width environments, tying AetherShell",
                "E3: only 8/10 induced failures failed at all; a missing field and an out-of-range index both returned exit 0",
                "E3: 0/10 machine-readable codes, 0/10 distinct exit statuses",
                "E4: 0/5 contained",
                "start-up dominates: 2.3–3.6 s per invocation on Linux, almost all .NET initialisation (E1, E2)",
            ],
        },

        Shell::Nushell => ShellProfile {
            shell,
            // E2: 1,274 tokens — more expensive than bash, because its table
            // renderer targets a human at a terminal.
            token_efficiency: 0.40,
            // E5: size column is locale-formatted (27.2 kB / 27,2 kB), and
            // already rounded to three significant figures, so the cheapest
            // path to the answer is lossy as well as unstable.
            determinism: 0.50,
            // E3: 10/10 machine-readable codes — the best of any shell
            // measured, ours included at the time — at 294 mean bytes, 2.1x
            // ours and 7x bash's.
            reliability: 0.70,
            safety: 0.05,
            measured: true,
            evidence: vec![
                "E3: 10/10 machine-readable codes (nu::shell::division_by_zero and the like) — it got to a full error taxonomy before AetherShell did",
                "E3: 294 mean bytes per failure, 2.1x AetherShell and 7x bash, most of it source-span art that is valuable to a human and pure cost to a model",
                "E2: 1,274 tokens for eight tabular answers — more expensive than bash, on box-drawing and column padding",
                "E5: size column is locale-formatted — 27.2 kB under C, 27,2 kB under de_DE — and rounded to three significant figures, so it is both unstable and lossy",
                "E4: 0/5 contained; typed values, no effect model",
                "the only engine measured whose cheapest path to an answer destroys information",
            ],
        },

        Shell::AetherShell => ShellProfile {
            shell,
            // E2: 842 tokens in the human rendering, 1.7x cheaper than bash.
            // E1: 448, fourth of six — scalar answers give typing nothing to do.
            token_efficiency: 0.50,
            determinism: 0.90,
            reliability: 0.75,
            // Human mode is allow-all by design: E4 contained 1 of 6, and the
            // one was `sh()` being disabled rather than any gate firing.
            safety: 0.15,
            measured: true,
            evidence: vec![
                "E2: 842 tokens for eight tabular answers in the human renderer, 1.7x cheaper than bash",
                "E1: 448 tokens, FOURTH OF SIX, behind SQLite (258) and bash+jq (345) — a scalar answer has no structure for typing to be efficient about",
                "E5: byte-stable across all six environments",
                "E3: 10/10 codes and 10/10 repair hints at 141 mean bytes, but 0/10 distinct exit statuses — everything exits 1",
                "E4: 1/6 contained in human mode, and that one is `sh()` being off, not a gate firing. Containment is opt-in; a deployment that forgets the flags is as unbounded as bash",
                "E6: `ae -b` ran 1 of 32 ordinary bash commands natively — the documented Bash-compatibility feature does not carry a shell-native benchmark",
            ],
        },

        Shell::AetherShellAgent => ShellProfile {
            shell,
            // E2: 497 tokens, cheapest measured; output alone 334 against
            // bash's 1,395.
            token_efficiency: 0.85,
            determinism: 0.90,
            reliability: 0.75,
            // E4: 6 of 6 contained, scored by whether the file outside the
            // jail actually changed.
            safety: 0.80,
            measured: true,
            evidence: vec![
                "E2: 497 tokens for eight tabular answers, cheapest measured; output alone 334 against bash's 1,395 (4.2x)",
                "E2: AECON factors repeated structure out of a column (`@suffix name: .rs`), losslessly — sizes stay exact",
                "E1: still fourth of six on scalar answers; the advantage is proportional to how much data comes back",
                "E5: byte-stable across all six environments",
                "E3: 10/10 codes, 10/10 repair hints, 141 mean bytes; refusals carry a `retryable` flag an agent can branch on",
                "E4: 6/6 dangerous operations contained, scored by consequence — write, delete and truncate outside the jail, arbitrary exec, privilege self-grant, network egress",
                "safety is 0.80 not higher: network is metered by request count rather than denied by destination, and these are language-level gates in one process, not a sandbox",
            ],
        },
    }
}

/// Every profile, ranked best-first by composite fitness.
pub fn rank_shells() -> Vec<ShellProfile> {
    let mut all: Vec<ShellProfile> = Shell::all().iter().copied().map(profile).collect();
    all.sort_by(|a, b| {
        b.fitness()
            .partial_cmp(&a.fitness())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.shell.name().cmp(b.shell.name()))
    });
    all
}

/// Per-axis deltas between two shells (positive = `a` fits agentic use better).
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct ShellComparison {
    /// The shell on the left of the comparison.
    pub a: Shell,
    /// The shell on the right.
    pub b: Shell,
    /// `a.fitness() - b.fitness()`.
    pub fitness: f64,
    /// `a.token_efficiency - b.token_efficiency`.
    pub token_efficiency: f64,
    /// `a.determinism - b.determinism`.
    pub determinism: f64,
    /// `a.reliability - b.reliability`.
    pub reliability: f64,
    /// `a.safety - b.safety`.
    pub safety: f64,
}

/// Compare two shells axis by axis.
pub fn compare_shells(a: Shell, b: Shell) -> ShellComparison {
    let (pa, pb) = (profile(a), profile(b));
    ShellComparison {
        a,
        b,
        fitness: pa.fitness() - pb.fitness(),
        token_efficiency: pa.token_efficiency - pb.token_efficiency,
        determinism: pa.determinism - pb.determinism,
        reliability: pa.reliability - pb.reliability,
        safety: pa.safety - pb.safety,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_has_a_profile_and_evidence() {
        for s in Shell::all() {
            let p = profile(s);
            assert_eq!(p.shell, s);
            assert!(!p.evidence.is_empty(), "{} has no evidence", s.name());
            for axis in [p.token_efficiency, p.determinism, p.reliability, p.safety] {
                assert!(
                    (0.0..=1.0).contains(&axis),
                    "{} has an axis outside 0..=1",
                    s.name()
                );
            }
        }
    }

    #[test]
    fn names_round_trip() {
        for s in Shell::all() {
            assert_eq!(
                Shell::from_name(s.name()),
                Some(s),
                "{} does not parse back",
                s.name()
            );
        }
        assert_eq!(Shell::from_name("PWSH"), Some(Shell::PowerShell));
        assert_eq!(Shell::from_name("nu"), Some(Shell::Nushell));
        assert_eq!(Shell::from_name("perl"), None);
    }

    #[test]
    fn the_unexecuted_shells_are_marked_as_such() {
        // The honesty property the module docs promise: a reader must be able
        // to tell which rows rest on a run and which on an analogy.
        assert!(!Shell::Zsh.measured());
        assert!(!Shell::Fish.measured());
        for s in [
            Shell::Bash,
            Shell::Nushell,
            Shell::PowerShell,
            Shell::AetherShell,
            Shell::AetherShellAgent,
        ] {
            assert!(s.measured(), "{} was executed and should say so", s.name());
        }
        // And the profile must carry the caveat where the reader will see it.
        for s in [Shell::Zsh, Shell::Fish] {
            assert!(
                profile(s)
                    .evidence
                    .iter()
                    .any(|e| e.contains("NOT EXECUTED")),
                "{} does not disclose that it was not run",
                s.name()
            );
        }
    }

    #[test]
    fn ranking_is_ordered_and_total() {
        let ranked = rank_shells();
        assert_eq!(ranked.len(), Shell::all().len());
        for w in ranked.windows(2) {
            assert!(w[0].fitness() >= w[1].fitness(), "ranking is not ordered");
        }
    }

    #[test]
    fn the_measurements_the_scores_claim_are_the_ones_reported() {
        // Guards against the scores drifting away from the runs they cite.
        // bash contained nothing (E4) and must score near zero on safety;
        // agent mode contained everything and must score high.
        assert!(profile(Shell::Bash).safety < 0.2);
        assert!(profile(Shell::AetherShellAgent).safety >= 0.75);
        // nushell beat everything on error codes (E3) and must lead the
        // POSIX shells on reliability.
        assert!(profile(Shell::Nushell).reliability > profile(Shell::Bash).reliability);
        // AetherShell's human renderer is more expensive than its agent mode
        // (E2: 842 vs 497) and must score lower on tokens.
        assert!(
            profile(Shell::AetherShell).token_efficiency
                < profile(Shell::AetherShellAgent).token_efficiency
        );
        // E1 is the result that goes against us: on scalar answers we are
        // fourth of six. Nothing in this module may imply otherwise by
        // scoring the human renderer above nushell on tokens.
        assert!(
            profile(Shell::AetherShell).token_efficiency > profile(Shell::Nushell).token_efficiency,
            "E2 has AetherShell's human renderer cheaper than nushell; if that \
             changes, update the score and the evidence together"
        );
    }

    #[test]
    fn comparison_is_antisymmetric() {
        let ab = compare_shells(Shell::AetherShellAgent, Shell::Bash);
        let ba = compare_shells(Shell::Bash, Shell::AetherShellAgent);
        assert!((ab.fitness + ba.fitness).abs() < 1e-9);
        assert!(ab.safety > 0.0, "agent mode contains more than bash (E4)");
    }
}
