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
//! The library is execution-agnostic: it can't run arbitrary languages, so the
//! axes that need behavior (determinism, reliability) take a caller-provided
//! closure, and safety takes the program's declared [`safety::Effect`]s. Token
//! efficiency works directly on text. Everything is dependency-light (a labeled
//! heuristic tokenizer by default; enable `--features real-tokens` for exact
//! OpenAI BPE counts via `tiktoken-rs`).
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

pub mod determinism;
pub mod reliability;
pub mod safety;
pub mod tokens;

/// A combined, all-axes evaluation of a single program. Construct with
/// [`Evaluation::new`] then fill in whichever axes you can measure; unset axes
/// stay `None`. A convenience for reporting a program's overall agentic fitness.
#[derive(Debug, Clone, Default)]
pub struct Evaluation {
    pub name: String,
    pub tokens: Option<tokens::AgentCost>,
    pub determinism: Option<determinism::DeterminismReport>,
    pub reliability: Option<reliability::ReliabilityReport>,
    pub safety: Option<safety::SafetyReport>,
}

impl Evaluation {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
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
