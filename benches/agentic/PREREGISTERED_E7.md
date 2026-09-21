# E7 — does a borrowed syntax beat an invented one? (pre-registered)

`docs/LANGUAGE_FIRST_PRINCIPLES.md` §3 argues that an agentic language should
borrow its input syntax rather than invent one, because novelty is charged twice
— once as standing context, once as retries. The evidence offered is our own
`ATTEMPTS` column, which is one author and is confounded.

This is the experiment that would test it. **It is written down before it is
run, and it is not run yet.**

## Why pre-register

Five results this session moved after being measured, and one of them —
§4 of the response — moved because the harness had no oracle and scored a
placeholder as a win. The pattern is consistent: a benchmark run by the party
it flatters tends to flatter it, and the protection is to fix the protocol
before seeing the numbers.

That applies here more than anywhere else, because this experiment can
invalidate a design direction the project has invested a year in. If it is
specified afterwards, it will be specified to survive.

## Hypothesis

**H1.** A model writing the same ten queries will be correct on the first
attempt more often in a syntax it has seen in pretraining (SQL, jq) than in one
it has not (AetherShell default, AetherShell agentic), given equal
documentation.

**H2.** Counting standing context, total tokens to a *correct* answer will be
lower for the borrowed syntaxes over a session of fewer than ~129 queries — the
break-even implied by the 1,196-token agentic cheatsheet against its measured
9.3 tokens/query saving.

**If H1 fails, §3 of the first-principles document is wrong** and the agentic
syntax direction is vindicated. That outcome is publishable and this file exists
so it cannot be quietly dropped.

## Protocol

**Corpus.** The ten questions already in `corpus.mjs`, over the same 500
`cli/cli` records, checked against the same oracle. Nothing new to validate.

**Arms.** Five, one per surface (the fifth added by Amendment 1):

| Arm | Reference material supplied |
| --- | --- |
| SQL | the table schema only |
| jq | the JSON schema only |
| AetherShell (default) | the schema + `ontology_describe` output for the ~20 relevant builtins |
| AetherShell (agentic) | the schema + the same ontology + the 1,196-token cheatsheet |
| AetherShell (agentic, no cheatsheet) | the schema + the same ontology — the control |

The asymmetry in reference material *is the hypothesis*: the borrowed surfaces
need none. Supplying AetherShell's ontology and cheatsheet is the steel-manned
version of our own side, not a handicap on theirs.

**Model.** One frontier model, one attempt per question per arm (temperature per
Amendment 1), no retries, no tool access, no repository access. The prompt asks only for the
command, in a fenced block, with no prose.

**Blinding.** Arm order randomised per question. The prompts are fixed in this
file's companion `e7-prompts/` directory before the first run and are not edited
afterwards; a change to a prompt starts a new run with a new name.

**Scoring.** The emitted command is executed against the corpus and compared to
the oracle with the existing `normalise`. First-try correct or not — no partial
credit, no "close enough". Tokens counted with the same exact cl100k path as
every other experiment.

**Sample size.** ~~10 questions × 4 arms × 5 seeds = 200 generations. Small, and
stated as small: this detects a large effect and will not resolve a subtle
one.~~ — superseded by Amendment 1 below; the original wording is kept struck
through because it was internally inconsistent and that is worth seeing.

## What would falsify each claim

| Claim | Falsified if |
| --- | --- |
| H1: borrowed beats invented on first-try | AetherShell's arms match or beat SQL/jq |
| H2: borrowed wins on total tokens | agentic wins at session lengths under ~129 queries |
| "the cheatsheet is a real cost" | the agentic arm scores as well *without* the cheatsheet |

That last row is the cheapest and most informative control, and it should be
run as a fifth arm.

## Known limits, stated now

- **One model family.** Pretraining mix differs between vendors; a result here
  is about one model, not about models.
- **Ten questions, one domain.** They are query-shaped and scalar-answered,
  which §4 of the response shows is the regime least favourable to a typed
  shell. A tabular corpus might answer differently and should be run second.
- **We wrote the AetherShell reference material.** If the ontology entries are
  poor, the arm loses for a reason that is fixable rather than fundamental —
  which is itself worth knowing, and is why the ontology work in
  `src/signature.rs` should land before this runs.
- **Not an accuracy benchmark.** This measures whether a model can express a
  known query in a syntax. It says nothing about whether the shell helps it
  solve a task.

## Cost

Headline tier: **50 generations** of a few hundred tokens each (see Amendment
1). Small enough that the reason it has not been run is a budget decision rather
than a technical one, and it should be made explicitly rather than by default.
The conditional variance tier would add 200 more.

## Amendment 1 — 2026-09-21, before the first generation

**The protocol as first written could not be executed.** It specified
*temperature 0* and *5 seeds* in the same breath. At temperature 0 the five
seeds are five identical generations, so the stated sample of 200 was really a
sample of 40, and the four duplicates would have inflated any significance test
run over them.

This is recorded rather than quietly corrected because the point of the file is
that the protocol is fixed before the numbers exist. It is being amended before
the first generation, with no results in hand, and the original text is struck
through above rather than deleted.

**Resolved as two tiers:**

| Tier | Setting | Generations | Purpose |
| --- | --- | --- | --- |
| Headline | temperature 0, 1 generation per (question, arm) | 10 × 5 = **50** | What the model deterministically does with each syntax |
| Variance | temperature 0.7, 4 further seeds per (question, arm) | 10 × 5 × 4 = **200** | Run only if the headline arms are within 2 of each other |

The headline is the pre-registered result. The variance tier exists so that a
close headline is not over-read, and it is explicitly *conditional* — running it
after seeing a decisive headline would be choosing the analysis that suits the
answer.

**A fifth arm** is added, as the original "known limits" section already
recommended: AetherShell agentic **without** the cheatsheet. It is the cheapest
control and the one that tests whether the cheatsheet is a real cost or a real
benefit.

**What has not changed:** the corpus, the oracle, `normalise`, the arm-order
randomisation, the no-retries rule, the scoring (first-try correct, executed
against the corpus, no partial credit), and the exact cl100k token path.
