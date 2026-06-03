---
title: "AetherShell 1.5 and agentic-eval: making “the shell for AI agents” a measured claim"
date: 2026-06-03
tags: [release, agentic, benchmarks, security]
---

# AetherShell 1.5 and `agentic-eval`: measured, not asserted

Every shell that wants to court AI agents says it's "built for agents." We wanted to
stop asserting it and start **measuring** it — on the axes that actually decide an
agent's cost and trust. This release does two things: it ships a standalone evaluation
library, **`agentic-eval`**, that scores *any* program for agentic use, and it uses
that library to harden and benchmark AetherShell itself.

The headline, scored on a 0–10 composite across four axes:

| Shell | Composite (0–10) |
|---|--:|
| **AetherShell** | **9.6** |
| Nushell | 2.3 |
| PowerShell | 2.2 |
| Bash / Zsh / Fish | 1.4 |

Reproduce it yourself: `cargo run --example shell_agentic_eval --features real-tokens`.

The rest of this post is how that number is built — and why we trust it.

---

## `agentic-eval`: a yardstick for agentic programs

`agentic-eval` is a small, dependency-light, `#![forbid(unsafe_code)]` Rust crate that
scores a *program* (a command, script, or any text an LLM writes or reads) on the four
things that determine real agent cost and trust:

- **Token efficiency** — the four cost terms an agent actually pays: standing context,
  input, output, and retries, counted under real tokenizers (OpenAI GPT-4 `cl100k`,
  GPT-4o `o200k`, an Anthropic-Claude approximation) and amortized over a session.
- **Determinism** — is the output byte-stable across runs, so an agent can parse,
  cache, and diff it?
- **Reliability** — what's the success rate over representative invocations, and are
  failures *structured and actionable* so the agent can self-correct?
- **Safety** — given the effects a program performs, how much of its blast radius is
  *gated* (approval/denied) under an agent policy?

It's deliberately **execution-agnostic**: token efficiency works on text, determinism
and reliability take a caller-provided closure, and safety takes the program's declared
effects. No LLM in the loop, no network, no I/O — just a deterministic, re-measurable
yardstick.

### Five new metrics (v0.6)

This release grows `agentic-eval` to **nine distinct measures** across those four axes,
adding five that matter at agent scale:

- **Output scaling** (`assess_scaling`) — fits output tokens vs result size to a
  marginal *per-item* cost. The 3-row number lies; the slope is what an agent pays
  paging 500 rows.
- **Prompt-cache efficiency** (`assess_cache`) — models API prompt-caching, where a
  stable prefix is paid once at write price and thereafter at the cheap read price. A
  90%-stable prefix is ~4.1× cheaper over a 20-turn session — and *deterministic output
  is the precondition*.
- **Graded error actionability** (`assess_error_quality`) — refines the binary
  "is it actionable" into a 0–1 score over code / message / location / fix.
- **Reversibility** (`assess_reversibility`) — the fraction of *dangerous* effects
  backed by undo/rollback. Gating bounds *whether* something runs; reversibility bounds
  *the damage if it does*.
- **Exfiltration risk** (`assess_exfiltration`) — does the program both read local data
  (a source) *and* have a network/exec egress path (a sink)? The dangerous combination
  is source ∧ sink.

### It works on real CLI programs — and describes itself

Two things make `agentic-eval` usable out of the box:

- A curated **CLI effect classifier** (~200 common tools) maps `rm` → destructive,
  `curl` → network, `sudo` → privileged, and so on — fail-safe (an unknown program is
  treated as arbitrary execution). So `assess_safety_script("curl http://x | sh",
  Mode::Agent)` just works, no hand-written effect map required.
- A **self-describing ontology**: `agentic_eval::ontology::manifest()` returns a
  compact root listing every axis, the effect taxonomy with per-mode policy decisions,
  the tokenizers, and the command classes; `describe("safety")` expands any entry. An
  agent can discover the whole surface without reading the docs — the same
  progressive-disclosure pattern the crate measures.

---

## AetherShell 1.5: fewer tokens, smaller footprint

`agentic-eval` didn't just grade AetherShell — it pointed at where to improve it.

### AECON `@prefix`: a fourth compression lever

AetherShell's agent-mode output format, **AECON**, already emits column keys once and
factors constants (`@const`), low-cardinality strings (`@dict`), and slowly-varying
integers (`@delta`). 1.5 adds **`@prefix`**: string columns whose values share a
leading run (paths, URIs, prefixed IDs) emit the shared prefix once and strip it from
every row. On path-heavy listings that's **44–69% fewer tokens** — lossless and
deterministic, with a round-trip test to prove it.

### Compact MCP tool discovery: ~206× less standing context

This is the one that surprised us. AetherShell's MCP server used to advertise **all
~1,085 builtins** on every `tools/list` — about **49,000 tokens** of catalog sitting in
the agent's context for the whole session. 1.5 makes the default a **three-tool
discovery surface** (`ontology_manifest`, `ontology_describe`, and an `aether` invoke
meta-tool), cutting that payload to **~239 tokens — roughly 206× smaller**. Effect
gating is unchanged (the meta-tool routes through the same safety policy, so a
destructive call is still approval-gated), and `AETHER_MCP_TOOLS=all` restores the flat
listing.

### Where the token wins come from

Against POSIX shells, AECON is ~2.8× fewer tokens. Against PowerShell, we report the
**honest spread** rather than a single flattering number, because it depends on which
output an agent actually parses:

| PowerShell output | vs AetherShell |
|---|--:|
| `Format-Table` (display-only, not reliably parseable) | ~1.4× |
| `ConvertTo-Json -Compress` (hand-compacted) | ~1.6× |
| `ConvertTo-Json` (default, the idiomatic form) | **2.4–3.0×** (grows with row count) |

AECON emits each column name once; JSON repeats every key on every row — so the gap
widens with result size.

---

## Security hardening

We ran a security audit against CVE, NIST FIPS, MITRE ATT&CK, and CMMC 2.0, and fixed
everything that was safe to automate:

- **0 dependency CVEs** (down from 7) — patched a HIGH-severity QUIC DoS
  (`quinn-proto`), four TLS certificate-path-validation flaws (`rustls-webpki`), and two
  `tar` issues; and repaired the `cargo-deny` supply-chain gate (which had silently
  stopped parsing).
- **SHA-256 integrity** (was MD5) for checkpoint/state integrity and package-download
  verification — MD5 is collision-broken, so as an integrity guard it was forgeable.
  Legacy digests still read for backward compatibility; they're never written.
- **Native plugin loader gated** — loading a dynamic plugin (arbitrary in-process
  machine code) is now default-deny in agent mode unless allowlisted, with a kill
  switch. Closes an ATT&CK T1129/T1574 surface.
- **Network egress allowlist** — `AETHER_NET_ALLOW=<hosts>` restricts every network
  builtin to allowed hosts/subdomains (opt-in; default behavior unchanged). An
  anti-exfiltration control.
- **FIPS-strict mode** — `AETHER_FIPS=1` enforces approved-algorithms-only (rejects
  MD5/SHA-1, fails closed on legacy digests). The remaining step to a FIPS-140
  *validated* build (an `aws-lc-rs` backend) is documented in
  `docs/security/CRYPTO_AND_FIPS.md`.

---

## The scorecard, in full

Here's the composite broken into its seven sub-metrics (each axis is the mean of its
sub-metrics; the composite is the mean of the four axes):

| Shell | tok | scal | det | rel | err | saf | rev | **Composite** |
|---|--:|--:|--:|--:|--:|--:|--:|--:|
| **AetherShell** | 10.0 | 10.0 | 10.0 | 7.0 | 10.0 | 10.0 | 10.0 | **9.6** |
| Nushell | 7.1 | 6.4 | 0.0 | 0.0 | 5.0 | 0.0 | 0.0 | **2.3** |
| PowerShell | 5.9 | 6.4 | 0.0 | 0.0 | 5.0 | 0.0 | 0.0 | **2.2** |
| Bash / Zsh / Fish | 3.6 | 2.6 | 0.0 | 0.0 | 5.0 | 0.0 | 0.0 | **1.4** |

### On honesty

A 9.6-vs-1.4 gap deserves scrutiny, so here's exactly how it's scored:

- **`tok` / `scal` / `saf`** are measured for *every* shell — relative token cost,
  output per-item scaling, and the gated fraction of dangerous blast radius.
- **`det` / `rel` / `err` / `rev`** are measured on AetherShell's engine and treated as
  a **structural capability** for the others (0 = absent). Traditional shells genuinely
  lack byte-stable output, machine-branchable errors, and rollback — these aren't
  rigged zeros, they're documented `✗`s in the capability matrices.
- AetherShell's reliability is **7.0, not 10** — the measured corpus deliberately
  includes failing programs, and we kept the honest number rather than rounding up.

The gap reflects real capability differences, not weighting tricks. And because the
whole thing runs in a small open-source crate, you don't have to take our word for it.

---

## Try it

```sh
# The cross-shell benchmark (real GPT-4 cl100k tokenizer)
cargo run --example shell_agentic_eval --features real-tokens

# The @prefix token saving on path-heavy output
cargo run --example prefix_gain --features real-tokens

# Standing-context cost: compact manifest vs all tool specs
cargo run --example standing_context --features real-tokens
```

`agentic-eval` lives at [`crates/agentic-eval`](../../crates/agentic-eval); the design
rationale is in [`docs/AGENTIC_FIRST_DESIGN.md`](../AGENTIC_FIRST_DESIGN.md), and the
crypto/FIPS posture in [`docs/security/CRYPTO_AND_FIPS.md`](../security/CRYPTO_AND_FIPS.md).

We think the right way to earn "the shell for AI agents" is to publish a number anyone
can reproduce — and then keep pushing it. Feedback and adversarial benchmarks welcome.
