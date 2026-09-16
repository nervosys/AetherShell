# Agentic shell benchmarks

Four experiments comparing AetherShell against bash, PowerShell, nushell, jq and
SQLite on the terms that decide an AI agent's bill and its blast radius. They
exist to answer two specific published claims:

- **"Is Bash All You Need?"** (Mak et al., Microsoft / CMU, arXiv:2609.11999) —
  bash beats typed tool catalogues on enterprise agent benchmarks, at a fraction
  of the tokens.
- **"Testing if bash is all you need"** (Vercel, 2026) — for querying structured
  data, a SQL agent reached 100% accuracy on 155k tokens where a bash agent
  reached 52.7% on 1.06M.

Both are right about what they measured. Neither measured a shell that is
*itself* typed, and that is the configuration these benchmarks fill in.

## What is measured, and what is not

**Measured here.** Command and output tokens under real cl100k_base BPE;
wall-clock latency; byte-stability of output across repeated runs; whether a
failure carries a machine-branchable code; and whether a dangerous operation
actually took effect. Every number comes from a process that ran.

**Not measured here.** End-to-end task score with a model in the loop. The
Microsoft paper's headline (+24.6 pp on TheAgentCompany) is an accuracy number
produced by 174 tasks × 2 frontier models; nothing below is evidence about that
quantity, and no claim is made about it. These benchmarks characterise the
*substrate* an agent acts through — the per-turn cost and the containment — and
a substrate result does not license an accuracy result. `docs/` states the
experiment that would close the gap.

## The experiments

| | File | What it does |
| --- | --- | --- |
| **E1** | `corpus.mjs`, `run.mjs` | Ten queries over 500 real GitHub issues, in six engines, against an oracle. Replicates the Vercel post's setup. |
| **E2** | `shellops.mjs` | Eight ordinary repository operations — the per-turn cost of the shell itself. |
| **E3** | `errors.mjs` | Ten induced failures per shell: exit status, machine-readable code, repair hint, byte cost. |
| **E4** | `safety.mjs` | Seven dangerous operations, scored by whether the file outside the jail actually changed. |

## Running them

```bash
node benches/agentic/prepare.mjs   /tmp/aebench     # fetches 500 issues via gh
node benches/agentic/run.mjs       /tmp/aebench 7   # E1, median of 7
node benches/agentic/report.mjs    /tmp/aebench     # exact BPE, writes report.md
node benches/agentic/errors.mjs    /tmp/aebench     # E3
node benches/agentic/safety.mjs    /tmp/aebench     # E4
```

Requires `ae` on PATH plus whichever comparators are installed; engines whose
binary is absent are skipped and named, never silently dropped. Token counts
come from this repository's `tokens_of` example built with
`--features real-tokens`; if that build is unavailable the report says the
counts are missing rather than substituting a heuristic.

## Three things to hold against these numbers

**The author is not disinterested.** Nervosys builds AetherShell. The same
caveat `crates/agentic-eval/README.md` carries applies here: treat this as a
documented argument to check, not as an independent evaluation. Everything
needed to re-run it is in this directory, including the losses.

**`ATTEMPTS` is not a fair fight.** `corpus.mjs` records how many tries each
engine's command took to become correct. The author had written far more bash
and SQL than nushell, and wrote AetherShell's standard library. That number
measures one author's fluency crossed with each language, not the languages.
It is reported because the *reasons* for the retries are informative — which
they are for AetherShell, in the wrong direction; see the findings section of
the response document.

**Every engine here is given its best form.** bash is measured with `jq`
available, not without, because an agent with a shell has `jq`. The
`bash+coreutils` engine is a second, separate entry representing the
filesystem-only case, not a handicapped bash.

## Results on the record

`results/` holds the output of the run reported in
[`docs/TYPED_SHELL_RESPONSE.md`](../../docs/TYPED_SHELL_RESPONSE.md): the two
generated markdown reports, the raw per-measurement JSON for all four
experiments, and the E2 transcript so a reader can check that every engine was
asked the same question and gave the same answer. Platform: Debian on WSL2,
24 cores, AetherShell 12.0.2, median of 11 runs.
