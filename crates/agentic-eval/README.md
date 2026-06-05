# agentic-eval

A small, standalone Rust library for evaluating how well a **program** (a command,
script, snippet, or any text an LLM writes or reads) serves an **agentic AI
system** — across the four axes that actually determine an agent's cost and trust:

| Axis                 | Module                              | Question it answers                                                                                                                |
| -------------------- | ----------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| **Token efficiency** | [`tokens`](src/tokens.rs)           | How many tokens does it cost — standing context + input + output + retries — under popular tokenizers, amortized over a session?   |
| **Determinism**      | [`determinism`](src/determinism.rs) | Is the output byte-stable across runs, so an agent can parse / cache / diff it?                                                    |
| **Reliability**      | [`reliability`](src/reliability.rs) | What's the success rate over representative invocations, and are failures *structured/actionable* (so the agent can self-correct)? |
| **Safety**           | [`safety`](src/safety.rs)           | Given the effects it performs, how much of its blast radius is gated (approval/denied) under an agent policy?                      |

It is **execution-agnostic**: token efficiency works on text directly; determinism
and reliability take a caller-provided closure (the library can't run arbitrary
languages); safety takes the program's declared effects.

## Benchmark: VM / sandbox systems for agentic AI use

Curated benchmark of the VM/sandbox systems an agent runtime spawns (one isolated
environment per tool call), scored on five **agent-native** axes. Reproduce with
`cargo run -p agentic-eval --example vm_benchmark`; ranked best-first by composite
fitness:

| System           |  Fitness | start-latency |  density | isolation | snapshotting | agent-control |
| ---------------- | -------: | ------------: | -------: | --------: | -----------: | ------------: |
| **AetherVM**     | **0.86** |          0.80 |     0.85 |      0.80 |     **0.90** |      **0.95** |
| Firecracker      |     0.79 |      **0.90** | **0.90** |      0.85 |         0.80 |          0.50 |
| Cloud Hypervisor |     0.76 |          0.85 |     0.80 |      0.85 |         0.80 |          0.50 |
| gVisor           |     0.65 |          0.85 |     0.85 |      0.60 |         0.40 |          0.55 |
| Docker           |     0.65 |      **0.95** | **0.95** |      0.35 |         0.40 |          0.60 |
| Kata Containers  |     0.62 |          0.65 |     0.60 |      0.85 |         0.40 |          0.60 |
| QEMU/KVM         |     0.61 |          0.40 |     0.45 |  **0.90** |         0.85 |          0.45 |

**Head-to-head — AetherVM vs Firecracker** (+ = AetherVM fits agentic use better):
fitness `+0.07`; agent-control `+0.45`, snapshotting `+0.10`; start-latency `−0.10`,
density `−0.05`, isolation `−0.05`.

**Reading.** AetherVM leads on the axes it was *designed* for — instant CoW
branching (fork a primed context per call) and an MCP-native control plane — while
the microVMs (Firecracker, Cloud Hypervisor) lead on raw cold-start and
battle-tested isolation, and shared-kernel containers (Docker) top speed/density
but rank low on isolation for untrusted, agent-generated code. Scores are honest
curated judgments with evidence (AetherVM's isolation carries an explicit "younger,
less battle-tested at scale" caveat); see [`vms`](src/vms.rs) and
`describe("aethervm")`.

## Beyond programs: languages, AI frameworks & VM systems

Three further modules profile what agents *build with* and *run on*:

| Subject                   | Module                            | What it scores                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ------------------------- | --------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Programming languages** | [`languages`](src/languages.rs)   | 10 languages (Python, Rust, JS, TS, Go, Bash, C, C++, Java, MechGen): code token economy, toolchain reproducibility, whether the compiler catches agent mistakes with actionable diagnostics, and default blast radius.                                                                                                                                                                                                                                                    |
| **AI frameworks**         | [`frameworks`](src/frameworks.rs) | 9 frameworks (PyTorch, TensorFlow, JAX, HF Transformers, ONNX Runtime, scikit-learn, Candle, Burn, RecursiveMachineIntelligence-RMI): the four axes **plus discoverability** — can an agent learn the surface from the framework itself (schemas/ontology/introspection) instead of prose? Includes artifact-safety facts (pickle ≈ arbitrary code on load, `trust_remote_code`, safetensors).                                                                             |
| **VM / sandbox systems**  | [`vms`](src/vms.rs)               | 7 systems (AetherVM, Firecracker, Cloud Hypervisor, gVisor, Kata, QEMU/KVM, Docker) on **agent-native axes** for the *ephemeral sandbox* workload an agent runtime drives: **start-latency** (cold-start per tool call), **density** (sandboxes per host), **isolation** (boundary strength for untrusted agent-generated code), **snapshotting** (CoW fork / warm-pool branching), and **agent-control** (is the control plane tool/MCP-native, or bring-your-own glue?). |

These are **curated static profiles** (deterministic, serializable, each score
backed by evidence strings), not measurements of your codebase — use the
program-level axes for that. `rank_languages()` / `rank_frameworks()` /
`rank_vms()` order by composite fitness; `compare_languages(a, b)` /
`compare_frameworks(a, b)` / `compare_vms(a, b)` give per-axis deltas;
everything is reachable from the ontology (`describe("vms")`,
`describe("firecracker")`, `describe("aethervm")`).

> The VM axes are deliberately *workload-specific*: a great long-lived
> datacenter VM (QEMU/KVM) can rank low for the spawn-and-tear-down agent
> sandbox, and a shared-kernel container (Docker) ranks high on speed/density
> but low on isolation for untrusted code — exactly the trade-offs that matter
> when an agent runs code it just wrote.

## Tokenizers

- **OpenAI GPT-4** (`cl100k_base`) and **GPT-4o** (`o200k_base`) — *exact* with
  `--features real-tokens` (via `tiktoken-rs`), heuristic otherwise.
- **Anthropic Claude** — a heuristic *approximation*; Anthropic ships no offline
  tokenizer crate, so this is labeled an estimate, not an exact count.
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

## Pluggable tokenizer

The cost model isn't locked to the built-in `Model` set. `tokens::evaluate_with`
(and `rank_with`) take **any `Fn(&str) -> usize`**, so a host can flow its own exact
tokenizer through the library:

```rust
use agentic_eval::tokens::{evaluate_with, Program};
// e.g. pass a host's tokenizer (here, a stand-in word counter)
let cost = evaluate_with(&Program::new("p", "read a file"), |s| s.split_whitespace().count());
assert_eq!(cost.input, 3);
```

`AgentCost::total_over` amortizes the standing context once (the prompt-caching
default); `total_standing_per_turn` is the no-caching upper bound. `safety::
assess_safety_named` scores directly from operation names plus a classifier closure.

## CLI programs & a self-describing ontology

- The [`commands`](src/commands.rs) module ships a curated heuristic classifier for
  ~200 common CLI tools (`rm` → destructive, `curl` → network, `sudo` → privileged,
  …), so the safety axis works on real shell programs out of the box —
  `assess_safety_script("curl http://x | sh", Mode::Agent)` in one call.
  Unrecognized programs are treated as arbitrary execution (fail-safe).
- The crate is **self-describing**: [`ontology`](src/ontology.rs) exposes a compact,
  deterministic `manifest()` (axes, the effect taxonomy with per-mode policy
  decisions, models, command count) and `describe("<name>")` to expand any entry —
  the same progressive-disclosure pattern the library measures, so an agent can
  discover the whole surface without reading these docs. `ontology()` returns the
  full structured catalog (serde-serializable).

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
