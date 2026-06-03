# Changelog — agentic-eval

All notable changes to the `agentic-eval` crate. Follows
[Keep a Changelog](https://keepachangelog.com/) and [SemVer](https://semver.org/).

## [0.4.0] - 2026-06-02

### Added
- **`commands` module — heuristic CLI effect classification.** A curated table of
  ~200 common POSIX/Unix/dev tools mapped to their [`safety::Effect`] class, so the
  safety axis works on a **wide variety of real CLI programs** without a hand-written
  classifier: `classify_command` (name → effect), `classify_invocation` (one command
  line — strips `VAR=val` and path prefixes; unknown program → `Exec`; `sudo`/`doas`
  → `Privileged`), `classify_script` (split a script on `\n ; | & && ||` and classify
  each), and `assess_safety_script` (one call from a script to a `SafetyReport`).
  Fail-safe by design: an unrecognized program is treated as arbitrary execution, and
  multi-mode tools map to their most security-salient common effect (`git` → network,
  `docker`/`npm`/`make` → exec, `apt`/`mount` → privileged). All re-exported at the
  crate root.

## [0.3.0] - 2026-06-02

### Added
- **Pluggable tokenizer**: `tokens::evaluate_with` and `rank_with` accept any
  `Fn(&str) -> usize`, so a host can flow its own exact tokenizer through the cost
  model instead of the built-in heuristic/BPEs.
- `AgentCost::total_standing_per_turn` — the no-prompt-caching cost model (standing
  context re-sent every turn), complementing the caching-amortized `total_over`.
- `safety::assess_safety_named` — score safety from operation names plus a classifier
  closure (unknowns skipped).
- Release hardening: `#![forbid(unsafe_code)]`, `#![deny(missing_docs)]` (every public
  item is documented), full `Cargo.toml` metadata (`readme`, `documentation`,
  `rust-version`, docs.rs all-features), and a crate CHANGELOG.

## [0.2.0] - 2026-06-02

### Added
- Crate-root re-exports of the most-used types (`Model`, `Program`, `AgentCost`,
  `Effect`, `Mode`, `assess_*`, …).
- `Display` for every report type (`AgentCost`, `Comparison`, `DeterminismReport`,
  `ReliabilityReport`, `SafetyReport`, `Evaluation`).
- Optional `serde` feature deriving `Serialize` on all report/config types.
- `Model::from_name` (CLI/config parsing), `tokens::rank` (N-way comparison),
  `Evaluation` `with_*` builders, `safety::Effect::from_name`.

### Changed
- The heuristic tokenizer splits `snake_case` subwords (`file_read` ≈ 2 tokens).
- Corrected the `AnthropicClaude` model docs to state it is a heuristic approximation
  (no public offline Claude tokenizer), not a "calibrated" estimate.

## [0.1.0] - 2026-06-01

### Added
- Initial release. Four-axis evaluation of programs for agentic AI use:
  **token efficiency** (`tokens`), **determinism** (`determinism`), **reliability**
  (`reliability`), and **safety** (`safety`), plus a combined `Evaluation`.
- Token counting under OpenAI GPT-4 (`cl100k`) and GPT-4o (`o200k`) — exact with
  `--features real-tokens`, a documented Anthropic-Claude approximation, and a
  labeled heuristic otherwise. Execution-agnostic; zero heavy deps by default.
