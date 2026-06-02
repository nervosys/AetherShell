# agentic-eval

A small, standalone Rust library for evaluating how well a **program** (a command,
script, snippet, or any text an LLM writes or reads) serves an **agentic AI
system** — across the four axes that actually determine an agent's cost and trust:

| Axis | Module | Question it answers |
|---|---|---|
| **Token efficiency** | [`tokens`](src/tokens.rs) | How many tokens does it cost — standing context + input + output + retries — under popular tokenizers, amortized over a session? |
| **Determinism** | [`determinism`](src/determinism.rs) | Is the output byte-stable across runs, so an agent can parse / cache / diff it? |
| **Reliability** | [`reliability`](src/reliability.rs) | What's the success rate over representative invocations, and are failures *structured/actionable* (so the agent can self-correct)? |
| **Safety** | [`safety`](src/safety.rs) | Given the effects it performs, how much of its blast radius is gated (approval/denied) under an agent policy? |

It is **execution-agnostic**: token efficiency works on text directly; determinism
and reliability take a caller-provided closure (the library can't run arbitrary
languages); safety takes the program's declared effects.

## Tokenizers

- **OpenAI GPT-4** (`cl100k_base`) and **GPT-4o** (`o200k_base`) — *exact* with
  `--features real-tokens` (via `tiktoken-rs`), heuristic otherwise.
- **Anthropic Claude** — a calibrated heuristic *approximation*; Anthropic ships no
  offline tokenizer crate, so this is labeled an estimate, not an exact count.
- **Heuristic** — a labeled, dependency-free fallback.

By default the crate pulls **zero heavy dependencies** (heuristic counts). Enable
exact OpenAI counts with `--features real-tokens`. The heuristic splits
`snake_case` subwords (so `file_read` ≈ 2 tokens), tracking real BPE within
~10–20% for code-like text.

## Output & ergonomics

- The most-used types are **re-exported at the crate root** (`agentic_eval::Model`,
  `Program`, `AgentCost`, `Comparison`, `Effect`, `Mode`, `assess_*`, …).
- Every report (`AgentCost`, `Comparison`, `DeterminismReport`,
  `ReliabilityReport`, `SafetyReport`, `Evaluation`) implements **`Display`** for
  ready-to-print summaries.
- `--features serde` derives **`serde::Serialize`** on every report/config type for
  machine-readable (e.g. JSON) output.
- `Model::from_name` / `safety::Effect::from_name` parse identifiers for CLI/config
  use; `tokens::rank` is the N-way generalization of `compare`; `Evaluation` has
  `with_*` builders.

## Example

```sh
cargo run -p agentic-eval --example evaluate                    # heuristic
cargo run -p agentic-eval --example evaluate --features real-tokens   # exact OpenAI BPE
```

```rust
use agentic_eval::tokens::{compare, Model, Program};

let legible = Program::new("read", "file.read(\"README.md\")")
    .with_standing_context("file.read(path) -> String");
let cipher  = Program::new("read", "F.r\"README.md\"")
    .with_standing_context("<multi-KB single-letter+sigil cheatsheet>");

let cmp = compare(&legible, &cipher, Model::OpenAiGpt4, 30);
assert!(cmp.winner_is_a); // legible wins once standing context is counted
```

## Why these four axes

An agent's real cost is not the characters it types. A representation can golf
*input* while inflating the *standing context* it must carry every turn — a net
loss. And beyond cost, an agent needs output it can deterministically parse,
failures it can branch on, and a blast radius it can't accidentally exceed. This
library scores all four so a language/encoding/tool can be compared on the terms
that matter for autonomous use.

Licensed AGPL-3.0-or-later.
