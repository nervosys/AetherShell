//! Evaluating **programming languages** for agentic AI use.
//!
//! The other modules score a *program*. This module scores the *language* a
//! program is written in — the standing properties that determine how well an
//! LLM agent can write, verify, and recover in it, on the same four axes:
//!
//! - **token efficiency** — how many tokens typical code costs (syntax weight,
//!   boilerplate, type annotations) and how much standing context (imports,
//!   project config) a working snippet drags in.
//! - **determinism** — does the toolchain behave reproducibly (lockfiles,
//!   hermetic builds, stable formatting) so agent-driven edit→run loops converge?
//! - **reliability** — when the agent gets it wrong, does the language *catch* it
//!   (static types, compile errors with spans, no undefined behavior) and is the
//!   error message structured enough to self-correct from?
//! - **safety** — what blast radius does running generated code have by default
//!   (memory safety, sandboxability, capability gating)?
//!
//! Scores are **0.0–1.0 static profiles**: curated, documented judgments encoded
//! as data — deterministic, comparable, and serializable — not measurements of
//! your codebase (use the program-level axes for that). Each profile carries
//! `evidence` strings so an agent can see *why* a score is what it is, and the
//! per-axis rationale survives serialization.
//!
//! ```
//! use agentic_eval::languages::{profile, rank_languages, Language};
//! let rust = profile(Language::Rust);
//! assert!(rust.reliability >= 0.8); // compiler catches agent mistakes
//! let ranked = rank_languages();
//! assert_eq!(ranked.len(), Language::all().len());
//! // Ranked best-first by composite fitness:
//! assert!(ranked[0].fitness() >= ranked[ranked.len() - 1].fitness());
//! ```

/// Languages with curated agentic profiles.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Language {
    Python,
    Rust,
    JavaScript,
    TypeScript,
    Go,
    Bash,
    C,
    Cpp,
    Java,
    /// MechGen — the agentic-first language (token-budgeted syntax, RMIL binary
    /// IR target, self-healing compiler). Included because this crate's parent
    /// ecosystem ships it; scored on the same axes as everything else.
    MechGen,
}

impl Language {
    /// All profiled languages, in fixed (deterministic) order.
    pub fn all() -> [Language; 10] {
        [
            Language::Python,
            Language::Rust,
            Language::JavaScript,
            Language::TypeScript,
            Language::Go,
            Language::Bash,
            Language::C,
            Language::Cpp,
            Language::Java,
            Language::MechGen,
        ]
    }

    /// Canonical lowercase name.
    pub fn name(self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::Rust => "rust",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
            Language::Go => "go",
            Language::Bash => "bash",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Java => "java",
            Language::MechGen => "mechgen",
        }
    }

    /// Parse a (case-insensitive) name; accepts common aliases
    /// (`js`, `ts`, `c++`, `sh`, `golang`, `py`).
    pub fn from_name(name: &str) -> Option<Language> {
        match name.to_ascii_lowercase().as_str() {
            "python" | "py" => Some(Language::Python),
            "rust" | "rs" => Some(Language::Rust),
            "javascript" | "js" | "node" => Some(Language::JavaScript),
            "typescript" | "ts" => Some(Language::TypeScript),
            "go" | "golang" => Some(Language::Go),
            "bash" | "sh" | "shell" => Some(Language::Bash),
            "c" => Some(Language::C),
            "cpp" | "c++" | "cxx" => Some(Language::Cpp),
            "java" => Some(Language::Java),
            "mechgen" | "mg" | "redox" => Some(Language::MechGen),
            _ => None,
        }
    }
}

/// A curated agentic profile of a language: four 0.0–1.0 axis scores plus the
/// evidence behind them.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct LanguageProfile {
    /// Which language this profiles.
    pub language: Language,
    /// Token efficiency of typical agent-written code (1.0 = very compact,
    /// little boilerplate/standing context).
    pub token_efficiency: f64,
    /// Toolchain reproducibility for agent edit→run loops (lockfiles, hermetic
    /// builds, canonical formatting).
    pub determinism: f64,
    /// How much the language catches/structures agent mistakes (static types,
    /// span-quality diagnostics, absence of UB/silent coercion).
    pub reliability: f64,
    /// Default blast-radius posture of running generated code (memory safety,
    /// sandboxability, implicit I/O reach).
    pub safety: f64,
    /// Why: one evidence string per notable factor (serialized with the report).
    pub evidence: Vec<&'static str>,
}

impl LanguageProfile {
    /// Composite agentic fitness: the unweighted mean of the four axes.
    /// (Callers with different priorities should weight the fields directly.)
    pub fn fitness(&self) -> f64 {
        (self.token_efficiency + self.determinism + self.reliability + self.safety) / 4.0
    }
}

impl std::fmt::Display for LanguageProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: fitness {:.2} (tokens {:.2}, determinism {:.2}, reliability {:.2}, safety {:.2})",
            self.language.name(),
            self.fitness(),
            self.token_efficiency,
            self.determinism,
            self.reliability,
            self.safety
        )
    }
}

/// The curated profile for `lang`. Scores are static, documented judgments
/// (see module docs); evidence strings carry the rationale.
pub fn profile(lang: Language) -> LanguageProfile {
    match lang {
        Language::Python => LanguageProfile {
            language: lang,
            token_efficiency: 0.85,
            determinism: 0.45,
            reliability: 0.45,
            safety: 0.35,
            evidence: vec![
                "compact syntax, minimal boilerplate; most-represented language in LLM training data",
                "dynamic typing defers agent mistakes to runtime; tracebacks are readable but late",
                "environment drift (interpreter version, site-packages) breaks reproducibility without lockfile discipline",
                "arbitrary I/O & exec by default; no capability gating; sandboxing requires external containment",
            ],
        },
        Language::Rust => LanguageProfile {
            language: lang,
            token_efficiency: 0.55,
            determinism: 0.9,
            reliability: 0.95,
            safety: 0.8,
            evidence: vec![
                "verbose types/lifetimes cost tokens, but rustc diagnostics (spans + suggested fixes) are the best self-correction signal of any mainstream language",
                "Cargo.lock + rustfmt + stable editions: agent edit→build loops are highly reproducible",
                "borrow checker + no UB in safe code: most agent mistakes are caught before running",
                "memory-safe by default; `unsafe` is greppable/gateable; still full ambient I/O authority",
            ],
        },
        Language::JavaScript => LanguageProfile {
            language: lang,
            token_efficiency: 0.75,
            determinism: 0.5,
            reliability: 0.4,
            safety: 0.4,
            evidence: vec![
                "compact and heavily represented in training data",
                "silent coercion + undefined-not-an-error swallow agent mistakes instead of surfacing them",
                "lockfiles help but ecosystem churn and engine differences hurt reproducibility",
                "ambient filesystem/network in Node; no default sandbox",
            ],
        },
        Language::TypeScript => LanguageProfile {
            language: lang,
            token_efficiency: 0.65,
            determinism: 0.55,
            reliability: 0.7,
            safety: 0.4,
            evidence: vec![
                "types add tokens over JS but catch a large share of agent mistakes at compile time",
                "tsc diagnostics are good though less actionable than rustc's",
                "type erasure at runtime: guarantees end where JS begins (same runtime safety posture)",
                "config sprawl (tsconfig matrix) adds standing context an agent must track",
            ],
        },
        Language::Go => LanguageProfile {
            language: lang,
            token_efficiency: 0.6,
            determinism: 0.85,
            reliability: 0.7,
            safety: 0.55,
            evidence: vec![
                "explicit-but-plain syntax; gofmt is canonical (zero formatting nondeterminism)",
                "go.mod/go.sum + hermetic-ish builds: strong reproducibility",
                "static types + explicit error returns; diagnostics terser than rustc's",
                "memory-safe; ambient I/O authority; goroutine leaks are a quiet failure mode",
            ],
        },
        Language::Bash => LanguageProfile {
            language: lang,
            token_efficiency: 0.9,
            determinism: 0.35,
            reliability: 0.2,
            safety: 0.2,
            evidence: vec![
                "extremely terse for orchestration; one-liners are token-cheap",
                "word-splitting/quoting pitfalls fail silently — the classic agent foot-gun",
                "environment-dependent (PATH, locale, shell flavor): poor reproducibility",
                "every command is an arbitrary side effect; `rm -rf` distance from any typo",
            ],
        },
        Language::C => LanguageProfile {
            language: lang,
            token_efficiency: 0.6,
            determinism: 0.6,
            reliability: 0.3,
            safety: 0.15,
            evidence: vec![
                "UB (buffer overflows, use-after-free) turns agent mistakes into silent corruption rather than diagnostics",
                "compiler errors catch syntax/type issues; memory errors escape to runtime or worse",
                "build reproducibility varies wildly with toolchain/platform macros",
                "no memory safety, no sandbox: highest blast radius per generated line",
            ],
        },
        Language::Cpp => LanguageProfile {
            language: lang,
            token_efficiency: 0.45,
            determinism: 0.55,
            reliability: 0.35,
            safety: 0.2,
            evidence: vec![
                "template-error diagnostics are notoriously unactionable (poor self-correction signal)",
                "huge surface + UB inherited from C; modern subsets help but agents mix eras",
                "build systems (CMake et al.) add heavy standing context",
                "same unmanaged blast radius as C",
            ],
        },
        Language::Java => LanguageProfile {
            language: lang,
            token_efficiency: 0.4,
            determinism: 0.75,
            reliability: 0.7,
            safety: 0.6,
            evidence: vec![
                "boilerplate-heavy (class ceremony, getters): worst token economy of the mainstream set",
                "static types + managed runtime catch most agent mistakes; stack traces are structured",
                "Maven/Gradle reproducibility is decent with lockfiles/BOMs",
                "memory-safe JVM; SecurityManager deprecated, so containment is external",
            ],
        },
        Language::MechGen => LanguageProfile {
            language: lang,
            token_efficiency: 0.9,
            determinism: 0.9,
            reliability: 0.8,
            safety: 0.75,
            evidence: vec![
                "designed token-budgeted: Agent-mode symbols + elision keep programs compact (measured in MechGen's token-bench)",
                "RMIL binary IR target + deterministic formatter: byte-stable artifacts for caching/diffing",
                "self-healing compiler proposes ranked fixes (structured recovery, measured in reliability-bench); young toolchain is the main risk",
                "effect-typed (net/fs/exec surfaced in types) so blast radius is visible/gateable at compile time",
            ],
        },
    }
}

/// Profiles for all languages, in [`Language::all`] order (deterministic).
pub fn profiles() -> Vec<LanguageProfile> {
    Language::all().iter().map(|&l| profile(l)).collect()
}

/// All profiles ranked best-first by [`LanguageProfile::fitness`] (ties broken
/// by the fixed `Language::all` order, so output is deterministic).
pub fn rank_languages() -> Vec<LanguageProfile> {
    let mut v = profiles();
    v.sort_by(|a, b| {
        b.fitness()
            .partial_cmp(&a.fitness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

/// Compare two languages: positive means `a` fits agentic use better.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct LanguageComparison {
    /// First language (the subject).
    pub a: LanguageProfile,
    /// Second language (the baseline).
    pub b: LanguageProfile,
    /// `a.fitness() - b.fitness()`.
    pub fitness_delta: f64,
    /// Axis name → delta (a − b), in fixed axis order.
    pub axis_deltas: Vec<(&'static str, f64)>,
}

/// Compare language `a` against baseline `b` across all four axes.
pub fn compare_languages(a: Language, b: Language) -> LanguageComparison {
    let pa = profile(a);
    let pb = profile(b);
    let axis_deltas = vec![
        ("tokens", pa.token_efficiency - pb.token_efficiency),
        ("determinism", pa.determinism - pb.determinism),
        ("reliability", pa.reliability - pb.reliability),
        ("safety", pa.safety - pb.safety),
    ];
    LanguageComparison {
        fitness_delta: pa.fitness() - pb.fitness(),
        a: pa,
        b: pb,
        axis_deltas,
    }
}

impl std::fmt::Display for LanguageComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} vs {}: fitness delta {:+.2}",
            self.a.language.name(),
            self.b.language.name(),
            self.fitness_delta
        )?;
        for (axis, d) in &self.axis_deltas {
            writeln!(f, "  {axis}: {d:+.2}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_profiles_with_evidence() {
        for l in Language::all() {
            let p = profile(l);
            assert!(
                p.evidence.len() >= 3,
                "{} needs ≥3 evidence lines",
                l.name()
            );
            for s in [p.token_efficiency, p.determinism, p.reliability, p.safety] {
                assert!((0.0..=1.0).contains(&s), "{} score out of range", l.name());
            }
        }
    }

    #[test]
    fn from_name_roundtrip_and_aliases() {
        for l in Language::all() {
            assert_eq!(Language::from_name(l.name()), Some(l));
        }
        assert_eq!(Language::from_name("c++"), Some(Language::Cpp));
        assert_eq!(Language::from_name("JS"), Some(Language::JavaScript));
        assert_eq!(Language::from_name("klingon"), None);
    }

    #[test]
    fn ranking_is_deterministic_and_sorted() {
        let r1 = rank_languages();
        let r2 = rank_languages();
        let names1: Vec<_> = r1.iter().map(|p| p.language.name()).collect();
        let names2: Vec<_> = r2.iter().map(|p| p.language.name()).collect();
        assert_eq!(names1, names2);
        for w in r1.windows(2) {
            assert!(w[0].fitness() >= w[1].fitness());
        }
    }

    #[test]
    fn axis_judgments_hold_directionally() {
        // Encoded domain knowledge sanity: the *relative* judgments the
        // profiles exist to capture.
        let rust = profile(Language::Rust);
        let python = profile(Language::Python);
        let bash = profile(Language::Bash);
        let c = profile(Language::C);
        assert!(
            rust.reliability > python.reliability,
            "static > dynamic for catching agent mistakes"
        );
        assert!(
            python.token_efficiency > rust.token_efficiency,
            "python is terser than rust"
        );
        assert!(
            bash.safety < 0.4 && c.safety < 0.4,
            "bash/C are the high-blast-radius pair"
        );
        assert!(
            rust.determinism > bash.determinism,
            "cargo lockstep > shell env drift"
        );
    }

    #[test]
    fn comparison_deltas_are_consistent() {
        let cmp = compare_languages(Language::Rust, Language::Bash);
        assert!(cmp.fitness_delta > 0.0);
        let sum: f64 = cmp.axis_deltas.iter().map(|(_, d)| d).sum();
        assert!(
            (sum / 4.0 - cmp.fitness_delta).abs() < 1e-9,
            "fitness delta = mean of axis deltas"
        );
        let disp = format!("{cmp}");
        assert!(disp.contains("rust vs bash"));
    }
}
