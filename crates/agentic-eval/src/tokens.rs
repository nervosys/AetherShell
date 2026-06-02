//! Token efficiency: count tokens under popular agentic tokenizers and model the
//! four cost terms an agent pays per task.
//!
//! An agent's total cost is not just the characters it types. It is:
//! `standing_context` (the schema/cheatsheet it must carry to use the program,
//! re-sent each turn) + `input` (the program it writes) + `output` (what it reads
//! back) + `retries` (re-dos from ambiguity/failure). A representation that golfs
//! `input` while inflating `standing_context` can be a net loss — so this module
//! counts all four and amortizes over a session.

/// A popular agentic AI system, identified by its tokenizer family.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Model {
    /// OpenAI GPT-4 / GPT-3.5-turbo family — `cl100k_base` BPE.
    OpenAiGpt4,
    /// OpenAI GPT-4o / o-series family — `o200k_base` BPE.
    OpenAiGpt4o,
    /// Anthropic Claude. **Approximation:** Anthropic publishes no offline
    /// tokenizer crate, so this uses the shared [`heuristic_tokens`] estimate (the
    /// same as [`Model::Heuristic`]) and must be read as an estimate, not an exact
    /// count. [`Model::is_exact`] returns `false` for it.
    AnthropicClaude,
    /// A tokenizer-agnostic labeled heuristic (no model-specific BPE).
    Heuristic,
}

impl Model {
    /// A human-readable label for the model/tokenizer (e.g. for report output).
    pub fn name(self) -> &'static str {
        match self {
            Model::OpenAiGpt4 => "openai-gpt4 (cl100k_base)",
            Model::OpenAiGpt4o => "openai-gpt4o (o200k_base)",
            Model::AnthropicClaude => "anthropic-claude (approx)",
            Model::Heuristic => "heuristic",
        }
    }

    /// Every model this build can count for (exact or approximate).
    pub fn all() -> [Model; 4] {
        [
            Model::OpenAiGpt4,
            Model::OpenAiGpt4o,
            Model::AnthropicClaude,
            Model::Heuristic,
        ]
    }

    /// Parse a model from a short identifier (case-insensitive), for CLI/config
    /// use. Accepts common aliases: `gpt4`/`gpt-4`/`cl100k`; `gpt4o`/`gpt-4o`/
    /// `o200k`; `claude`/`anthropic`; `heuristic`/`heur`. Returns `None` otherwise.
    pub fn from_name(name: &str) -> Option<Model> {
        match name.trim().to_ascii_lowercase().as_str() {
            "gpt4" | "gpt-4" | "openai-gpt4" | "cl100k" | "cl100k_base" => Some(Model::OpenAiGpt4),
            "gpt4o" | "gpt-4o" | "openai-gpt4o" | "o200k" | "o200k_base" => {
                Some(Model::OpenAiGpt4o)
            }
            "claude" | "anthropic" | "anthropic-claude" => Some(Model::AnthropicClaude),
            "heuristic" | "heur" => Some(Model::Heuristic),
            _ => None,
        }
    }

    /// Whether this model's count is exact (a real BPE) in this build, vs. an
    /// estimate. OpenAI families are exact only with `--features real-tokens`.
    pub fn is_exact(self) -> bool {
        match self {
            Model::OpenAiGpt4 | Model::OpenAiGpt4o => cfg!(feature = "real-tokens"),
            Model::AnthropicClaude | Model::Heuristic => false,
        }
    }

    /// Count the tokens in `text` under this model.
    pub fn count(self, text: &str) -> usize {
        match self {
            Model::OpenAiGpt4 => count_openai(text, false),
            Model::OpenAiGpt4o => count_openai(text, true),
            // Claude: no public offline tokenizer, so fall back to the shared
            // heuristic — a documented approximation, not an exact count.
            Model::AnthropicClaude => heuristic_tokens(text),
            Model::Heuristic => heuristic_tokens(text),
        }
    }
}

#[cfg(feature = "real-tokens")]
fn count_openai(text: &str, o200k: bool) -> usize {
    use std::sync::OnceLock;
    static CL100K: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    static O200K: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    let bpe = if o200k {
        O200K.get_or_init(|| tiktoken_rs::o200k_base().expect("load o200k_base"))
    } else {
        CL100K.get_or_init(|| tiktoken_rs::cl100k_base().expect("load cl100k_base"))
    };
    bpe.encode_with_special_tokens(text).len()
}

#[cfg(not(feature = "real-tokens"))]
fn count_openai(text: &str, _o200k: bool) -> usize {
    heuristic_tokens(text)
}

/// A labeled, deterministic token heuristic that tracks real BPE counts within
/// ~10–20% for code-like text. Used when a real BPE isn't available (no
/// `real-tokens` feature, or Claude). Rules: each run of letters/digits is one
/// token; an underscore separates `snake_case` subwords (each its own ~token, as
/// real tokenizers usually split, e.g. `file_read` ≈ 2) but is not itself counted;
/// every other non-whitespace punctuation/symbol char counts ~1.
pub fn heuristic_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !c.is_whitespace() && c != '_' {
                tokens += 1; // punctuation/symbols tokenize ~1 each; `_` just splits
            }
        }
    }
    tokens
}

/// The four token-cost terms an agent pays per task. All in tokens.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentCost {
    /// Schema/cheatsheet the model must carry to use the program (re-sent/turn).
    pub standing_context: usize,
    /// What the agent writes — the program text itself.
    pub input: usize,
    /// What the agent reads back — a representative output sample.
    pub output: usize,
    /// Estimated re-do cost from ambiguity/parse failure (caller-supplied; 0 if
    /// the program is unambiguous).
    pub retries: usize,
}

impl AgentCost {
    /// Total tokens over `turns`, the §4 criterion: the standing context is paid
    /// once (amortized), input+output are paid each turn, and retries are added.
    /// `turns = 1` gives the single-shot cost.
    pub fn total_over(&self, turns: usize) -> usize {
        self.standing_context + (self.input + self.output) * turns.max(1) + self.retries
    }

    /// Total tokens over `turns` in the **no-prompt-caching** model: the standing
    /// context is re-sent *every* turn (the worst case for a representation with a
    /// heavy schema/cheatsheet). [`total_over`](Self::total_over) is the
    /// caching-aware default (standing context paid once); this is the upper bound.
    pub fn total_standing_per_turn(&self, turns: usize) -> usize {
        let t = turns.max(1);
        (self.standing_context + self.input + self.output) * t + self.retries
    }
}

impl std::fmt::Display for AgentCost {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "input={} output={} standing={} retries={}",
            self.input, self.output, self.standing_context, self.retries
        )
    }
}

/// A program representation to evaluate for token efficiency.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Program {
    /// Identifier for the program (used in comparisons/reports).
    pub name: String,
    /// The program text the agent writes.
    pub source: String,
    /// A representative output the agent reads back (empty if none).
    pub output_sample: String,
    /// The schema/docs the model must carry to use it (empty if none).
    pub standing_context: String,
    /// Estimated retry tokens for this representation (0 = unambiguous).
    pub retries: usize,
}

impl Program {
    /// A program with just a name and source (no output/standing-context/retries).
    pub fn new(name: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            source: source.into(),
            output_sample: String::new(),
            standing_context: String::new(),
            retries: 0,
        }
    }
    /// Builder: set the representative output sample.
    pub fn with_output(mut self, sample: impl Into<String>) -> Self {
        self.output_sample = sample.into();
        self
    }
    /// Builder: set the standing-context (schema/cheatsheet) text.
    pub fn with_standing_context(mut self, ctx: impl Into<String>) -> Self {
        self.standing_context = ctx.into();
        self
    }
    /// Builder: set the estimated retry-token cost.
    pub fn with_retries(mut self, retries: usize) -> Self {
        self.retries = retries;
        self
    }
}

/// Evaluate one program's cost terms under `model`.
pub fn evaluate(program: &Program, model: Model) -> AgentCost {
    AgentCost {
        standing_context: model.count(&program.standing_context),
        input: model.count(&program.source),
        output: model.count(&program.output_sample),
        retries: program.retries,
    }
}

/// Evaluate a program across every supported model.
pub fn evaluate_all(program: &Program) -> Vec<(Model, AgentCost)> {
    Model::all()
        .into_iter()
        .map(|m| (m, evaluate(program, m)))
        .collect()
}

/// Evaluate a program with a **custom token counter** — any `Fn(&str) -> usize`,
/// such as a host application's exact tokenizer or a model not in [`Model`]. This
/// lets the crate's cost model work with any tokenizer, not just the built-in set.
///
/// ```
/// use agentic_eval::tokens::{evaluate_with, Program};
/// // A trivial whitespace counter standing in for a real tokenizer.
/// let words = |s: &str| s.split_whitespace().count();
/// let cost = evaluate_with(&Program::new("p", "read a file"), words);
/// assert_eq!(cost.input, 3);
/// ```
pub fn evaluate_with<F: Fn(&str) -> usize>(program: &Program, count: F) -> AgentCost {
    AgentCost {
        standing_context: count(&program.standing_context),
        input: count(&program.source),
        output: count(&program.output_sample),
        retries: program.retries,
    }
}

/// The result of comparing two programs (e.g. two encodings of the same task).
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct Comparison {
    /// The model/tokenizer used.
    pub model: Model,
    /// Session length the totals are amortized over.
    pub turns: usize,
    /// Program A's cost terms.
    pub a: AgentCost,
    /// Program B's cost terms.
    pub b: AgentCost,
    /// Program A's amortized session total ([`AgentCost::total_over`]).
    pub a_total: usize,
    /// Program B's amortized session total.
    pub b_total: usize,
    /// True if `a` costs fewer total tokens over `turns` than `b`.
    pub winner_is_a: bool,
    /// cheaper / dearer ratio (≥ 1.0); how many times more the loser costs.
    pub ratio: f64,
}

/// Compare two programs under `model`, amortized over `turns`.
pub fn compare(a: &Program, b: &Program, model: Model, turns: usize) -> Comparison {
    let (ca, cb) = (evaluate(a, model), evaluate(b, model));
    let (at, bt) = (ca.total_over(turns), cb.total_over(turns));
    let winner_is_a = at <= bt;
    let (lo, hi) = if at <= bt { (at, bt) } else { (bt, at) };
    let ratio = if lo == 0 { 1.0 } else { hi as f64 / lo as f64 };
    Comparison {
        model,
        turns,
        a: ca,
        b: cb,
        a_total: at,
        b_total: bt,
        winner_is_a,
        ratio,
    }
}

impl std::fmt::Display for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let winner = if self.winner_is_a { "A" } else { "B" };
        write!(
            f,
            "{}: A={} B={} over {} turns → {} wins ({:.2}x){}",
            self.model.name(),
            self.a_total,
            self.b_total,
            self.turns,
            winner,
            self.ratio,
            if self.model.is_exact() { "" } else { " [est]" },
        )
    }
}

/// Rank N programs by their amortized session cost under `model` (cheapest first).
/// Returns `(index_into_programs, total_tokens)` pairs sorted ascending by total —
/// the N-way generalization of [`compare`]. Ties keep input order (stable sort).
pub fn rank(programs: &[Program], model: Model, turns: usize) -> Vec<(usize, usize)> {
    rank_with(programs, |s| model.count(s), turns)
}

/// Like [`rank`], but with a custom token counter (see [`evaluate_with`]). Returns
/// `(index, total_tokens)` pairs sorted cheapest-first; ties keep input order.
pub fn rank_with<F: Fn(&str) -> usize>(
    programs: &[Program],
    count: F,
    turns: usize,
) -> Vec<(usize, usize)> {
    let mut ranked: Vec<(usize, usize)> = programs
        .iter()
        .enumerate()
        .map(|(i, p)| (i, evaluate_with(p, &count).total_over(turns)))
        .collect();
    ranked.sort_by_key(|&(_, total)| total);
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_is_deterministic_and_sane() {
        let s = "file.read(\"README.md\")";
        assert_eq!(heuristic_tokens(s), heuristic_tokens(s)); // deterministic
        assert!(heuristic_tokens(s) > 0);
        // Empty text → 0 tokens.
        assert_eq!(heuristic_tokens(""), 0);
        // More text → at least as many tokens.
        assert!(heuristic_tokens("a b c") >= heuristic_tokens("a b"));
    }

    #[test]
    fn agent_cost_total_amortizes_standing_context_once() {
        let c = AgentCost {
            standing_context: 1000,
            input: 10,
            output: 20,
            retries: 5,
        };
        // 1 turn: 1000 + 30 + 5
        assert_eq!(c.total_over(1), 1035);
        // 10 turns: standing once, input+output ×10, retries once
        assert_eq!(c.total_over(10), 1000 + 300 + 5);
        // turns=0 is clamped to 1.
        assert_eq!(c.total_over(0), c.total_over(1));
    }

    #[test]
    fn standing_context_can_dominate_a_small_input_win() {
        // A terse "cipher" with a tiny input edge but a big standing-context tax
        // loses to a legible form over a session — the core §4 finding.
        let cipher = Program::new("t", "F.r x")
            .with_standing_context("<a multi-kilobyte cipher cheatsheet ".repeat(120).as_str());
        let legible = Program::new("t", "file.read x").with_standing_context("short index");
        let cmp = compare(&legible, &cipher, Model::Heuristic, 30);
        assert!(cmp.winner_is_a, "legible wins once standing context counts");
        assert!(cmp.ratio > 1.0);
    }

    #[test]
    fn evaluate_all_covers_every_model() {
        let p = Program::new("t", "len([1,2,3])");
        let all = evaluate_all(&p);
        assert_eq!(all.len(), 4);
        for (_m, c) in all {
            assert!(c.input > 0);
        }
    }

    #[test]
    fn heuristic_splits_snake_case_subwords() {
        // `_` separates subwords (each ~a token) but is not itself counted.
        assert_eq!(heuristic_tokens("file_read"), 2); // file + read
        assert_eq!(heuristic_tokens("a_b_c"), 3);
        // A dot is real punctuation → counts.
        assert_eq!(heuristic_tokens("file.read"), 3); // file + . + read
                                                      // A lone identifier is one token.
        assert_eq!(heuristic_tokens("len"), 1);
    }

    #[test]
    fn model_from_name_parses_aliases() {
        assert_eq!(Model::from_name("gpt-4"), Some(Model::OpenAiGpt4));
        assert_eq!(Model::from_name("o200k"), Some(Model::OpenAiGpt4o));
        assert_eq!(Model::from_name("CLAUDE"), Some(Model::AnthropicClaude));
        assert_eq!(Model::from_name("heur"), Some(Model::Heuristic));
        assert_eq!(Model::from_name("nope"), None);
    }

    #[test]
    fn rank_orders_programs_cheapest_first() {
        // Identical input; the heavier standing context ranks last over a session.
        let cheap = Program::new("cheap", "file.read x").with_standing_context("short");
        let dear = Program::new("dear", "file.read x")
            .with_standing_context("a much longer cheatsheet ".repeat(50).as_str());
        let progs = [dear, cheap];
        let ranked = rank(&progs, Model::Heuristic, 30);
        assert_eq!(ranked.len(), 2);
        // index 1 ("cheap") should come first (lowest total).
        assert_eq!(ranked[0].0, 1);
        assert!(ranked[0].1 <= ranked[1].1);
    }

    #[test]
    fn displays_are_non_empty() {
        let c = AgentCost {
            standing_context: 10,
            input: 5,
            output: 2,
            retries: 0,
        };
        assert!(c.to_string().contains("input=5"));
        let cmp = compare(
            &Program::new("a", "x"),
            &Program::new("b", "yy"),
            Model::Heuristic,
            10,
        );
        assert!(cmp.to_string().contains("wins"));
    }

    #[test]
    fn evaluate_with_uses_a_custom_counter() {
        // A counter that returns a fixed value lets us check wiring exactly.
        let p = Program::new("p", "abc")
            .with_output("de")
            .with_standing_context("fghi")
            .with_retries(7);
        let cost = evaluate_with(&p, |s| s.chars().count());
        assert_eq!(cost.input, 3);
        assert_eq!(cost.output, 2);
        assert_eq!(cost.standing_context, 4);
        assert_eq!(cost.retries, 7); // carried from the program, not the counter
    }

    #[test]
    fn standing_per_turn_is_the_no_caching_upper_bound() {
        let c = AgentCost {
            standing_context: 100,
            input: 10,
            output: 5,
            retries: 0,
        };
        // Cached default pays standing once; per-turn pays it every turn → larger.
        assert_eq!(c.total_over(10), 100 + 150);
        assert_eq!(c.total_standing_per_turn(10), (100 + 15) * 10);
        assert!(c.total_standing_per_turn(10) > c.total_over(10));
    }

    #[test]
    fn rank_with_custom_counter_orders_cheapest_first() {
        let progs = [
            Program::new("long", "a much longer program body here"),
            Program::new("short", "x"),
        ];
        let ranked = rank_with(&progs, |s| s.split_whitespace().count(), 1);
        assert_eq!(ranked[0].0, 1); // "short" is cheapest
    }
}
