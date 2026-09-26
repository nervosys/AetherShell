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

## Status — 2026-09-21: harness ready, not run

`e7.mjs` implements this protocol and `e7-prompts/` holds the frozen prompts.
What has been done, and what has not:

**Done.** The scoring path is exercised end to end by `--replay`, which feeds
the known-good corpus commands through it: **all five arms score 10/10**, and a
per-run self-check proves the scorer marks a wrong-but-valid command, an
erroring command, an empty answer and a plausible placeholder as incorrect.
That guard was itself checked by replacing the scorer with one that returns
true for everything: the run aborts at exit 3 rather than printing five perfect
arms.

Getting there took fixing something that would have quietly halved the
experiment. The first replay reported `sql 0/10` on commands known to be
correct, because this host had no `sqlite3` — a missing tool wearing the costume
of a model that cannot write SQL. Since H1 is *specifically* the claim that
borrowed syntaxes (SQL, jq) beat invented ones, an E7 run with the SQL arm
silently scoring zero would have been evidence for H1 manufactured by a missing
package. An arm whose interpreter is absent is now named and skipped, never
scored; `sqlite3` has been installed here, so all five arms are live.

**Also done, and it was a prerequisite this file asked for.** "We wrote the
AetherShell reference material" above warned that a poor ontology would make
that arm lose for a fixable reason. It was worse than poor: four of the
builtins the answers require (`from_json`, `group_by`, `mean`, `to_string`)
were not in the ontology at all, because the catalogue walked only one of the
dispatcher's two halves. `e7.mjs` now refuses to run if any builtin in the
working set is missing from the ontology, rather than quietly handing an arm
a hobbled reference.

**Not done: no model has been called.** This environment has no provider
credentials and no local endpoint. The headline tier is 50 generations and
wants an explicit budget decision, which is what the Cost section above says
it should get. Nothing in this file has been revised in the light of a result,
because there is no result.

## Amendment 2 — 2026-09-25, before the first generation

**The oracle's implementation changed; its definition did not.** `corpus.mjs`
carried the answers as a pasted table, attributed to an `oracle.mjs` that was
never committed. The table was right only for the 2026-09-17 fetch. On a fresh
fetch, all eight E1 engines agreed on every answer and were all scored wrong.
`oracle.mjs` now exists: it answers the same ten questions in plain JavaScript
from the `issues.json` on disk, at run time, sharing no code with any arm. On
the 2026-09-25 fetch it agrees with all eight engines on all ten questions, and
`--replay` scores every arm 10/10 against it. The prompt hash is unaffected,
since prompts do not contain answers.

**Consequence for the protocol.** "The corpus" above means whatever fetch the
run is scored against. That fetch is not committed, so the run's result file
must record its record count, PR count and open count (as `prepare.mjs`
prints them), and the report must quote them. The questions, `normalise`, the
arms, the scoring and the token path are unchanged. q4 now refuses to score a
corpus on which the top PR author is tied, instead of silently rewarding one
engine's tie-break.
