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

/// A labeled, deterministic token heuristic: split on whitespace and treat each
/// run of letters/digits as one token plus one token per punctuation/symbol char,
/// which tracks real BPE token counts within ~10–20% for code-like text. Used when
/// a real BPE isn't available (no `real-tokens` feature, or Claude).
pub fn heuristic_tokens(text: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !c.is_whitespace() {
                tokens += 1; // punctuation/symbols tokenize ~1 each
            }
        }
    }
    tokens
}

/// The four token-cost terms an agent pays per task. All in tokens.
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
}

/// A program representation to evaluate for token efficiency.
#[derive(Debug, Clone)]
pub struct Program {
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
    pub fn with_output(mut self, sample: impl Into<String>) -> Self {
        self.output_sample = sample.into();
        self
    }
    pub fn with_standing_context(mut self, ctx: impl Into<String>) -> Self {
        self.standing_context = ctx.into();
        self
    }
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

/// The result of comparing two programs (e.g. two encodings of the same task).
#[derive(Debug, Clone)]
pub struct Comparison {
    pub model: Model,
    pub turns: usize,
    pub a: AgentCost,
    pub b: AgentCost,
    pub a_total: usize,
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
}
