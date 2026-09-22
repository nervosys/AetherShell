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
| **E2** | `shellops.mjs` | Eight ordinary repository operations — the per-turn cost of the shell itself, in two AetherShell render modes. |
| **E3** | `errors.mjs` | Ten induced failures per shell: exit status, machine-readable code, repair hint, byte cost. |
| **E3b** | `uncoded.mjs` | The same question as E3, asked of all 1,052 catalogued builtins instead of ten chosen ones: how many report a failure an agent can branch on? |
| **E4** | `safety.mjs` | Seven dangerous operations, scored by whether the file outside the jail actually changed. One arm is a live `ae agent serve` with no flags, because that is the surface an agent really drives. |
| **E5** | `environment.mjs` | The same listing under six locales, timezones and terminal widths: do the bytes change? |
| **E6** | `bashcompat.mjs` | Thirty-two everyday bash one-liners through the compatibility transpiler: how many run unchanged? |
| **E7** | `e7.mjs`, `e7-prompts/`, `PREREGISTERED_E7.md` | **Harness ready; not yet run against a model.** Does a model write a borrowed syntax (SQL, jq) correctly on the first attempt more often than an invented one? Pre-registered before the numbers exist, because it can falsify a direction this project is invested in. |

## Running them

```bash
node benches/agentic/prepare.mjs   /tmp/aebench     # fetches 500 issues via gh
node benches/agentic/run.mjs       /tmp/aebench 7   # E1, median of 7
node benches/agentic/report.mjs    /tmp/aebench     # exact BPE, writes report.md
node benches/agentic/run-shellops.mjs . /tmp/aebench 7   # E2, median of 7
node benches/agentic/errors.mjs    /tmp/aebench     # E3
node benches/agentic/uncoded.mjs                    # E3b, sweeps the catalogue
node benches/agentic/safety.mjs    /tmp/aebench     # E4
node benches/agentic/environment.mjs .              # E5, against this repo
node benches/agentic/bashcompat.mjs .               # E6, against this repo
node benches/agentic/e7.mjs /tmp/aebench --replay      # E7 harness check, no model, no spend
node benches/agentic/e7.mjs /tmp/aebench --provider anthropic   # E7 for real
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

## E2 has an oracle, and the oracle has a self-check

An earlier version of this file argued that E2 needed no oracle, because its
tasks have no single canonical rendering and the question is what a turn
*costs*. That argument was wrong and it produced a wrong published number.

AetherShell's renderer was printing an array of records as `[{…}, {…}, …]`.
E2's check was `exit == 0 && output non-empty`, so that scored as correct — at
316 bytes against bash's 2,650, and it was reported as a 7.9× advantage. The
316 bytes contained none of the requested data. **An output-size comparison is
meaningless unless something asserts that both outputs contain the answer**, and
the engine most likely to produce a suspiciously small output is the one that
has stopped answering.

`EXPECT` in `shellops.mjs` now names, per task, the facts an answer must
contain. It is a content check rather than an equality check, because the
engines legitimately encode the same facts differently:

| | bash | AetherShell `--agent` | nushell |
| --- | --- | --- | --- |
| `src/agent.rs`, 27260 bytes | `27260` | `27260` under `@suffix name: .rs` | `27.2 kB` |

`selfCheck()` asserts the patterns accept all three of those and reject an
elided placeholder, an empty string and a bare header row; `run-shellops.mjs`
refuses to report at all if it fails. That guard exists because the first
version of `EXPECT` was written through a script whose escaping ate every
backslash, leaving patterns like `/d{4,}/` — four literal letter d's — in the
table whose one job was to catch output that is not an answer. A broken oracle
is worse than no oracle, because it reads as rigour.

## E7 will not run without being able to fail

E7 calls a model, so its harness cannot be checked by reading it. `--replay`
pushes the *known-good* commands from `corpus.mjs` through the identical
prompt-hash, extraction, execution, oracle and scoring path. Before any of
that, a self-check asserts the scorer marks a wrong-but-valid command, an
erroring command, an empty answer and a plausible placeholder as **incorrect**,
and that extraction rejects prose with no fenced block. If any of those pass
when they should fail, the runner exits 3 and reports nothing — a scorer that
cannot tell right from wrong produces numbers that mean nothing, which is
exactly how E2 once scored `[{…}, {…}, …]` as the cheapest correct answer in
the benchmark.

The first replay run reported `sql 0/10` on commands known to be correct,
because the host had no `sqlite3`. An arm whose interpreter is absent is now
named and **skipped, never scored**: absence and failure must not look alike,
or every comparative number here is worthless. It mattered more than usual
here — E7's hypothesis is that borrowed syntaxes (SQL, jq) beat invented ones,
so a silently-zeroed SQL arm would have produced evidence *for* the hypothesis
out of a missing package. With `sqlite3` installed, all five arms replay
10/10.

The prompts in `e7-prompts/` are hashed into every result file. A run whose
hash differs from an earlier run is a different experiment and is reported
separately. The ontology the AetherShell arms receive is generated at run time
from the shell under test, not pasted into the prompts, so the reference
material and the shell cannot drift apart.
