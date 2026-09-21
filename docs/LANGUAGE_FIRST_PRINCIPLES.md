# The language, rethought from the benchmarks

The previous proposal (`WINNING_ACROSS_THE_BOARD.md`) worked inside the current
design and moved three last-place finishes. This one asks a different question:
if the six experiments in `benches/agentic/` were the specification, what
language would they specify?

The answer disagrees with several things we have built, including one we have
been building for a year. That is the useful part.

---

## 1. The objective function, written down

An agent's cost for a *successful* task is:

```
total  =  standing_context                       paid once per session
        + Σ turns (command_tokens + output_tokens)
        + retries × full_turn_cost               a retry is a whole turn
subject to bounded blast radius
```

Four terms. The six experiments measure all four, and the design follows from
which of them dominate.

| Term | Measured by | Where we stand |
| --- | --- | --- |
| standing context | agentic cheatsheet: **1,196 tokens** | unpriced until now |
| command tokens | E1: `-a` 275, SQLite 202 | 2nd of 8 |
| output tokens | E2: `--agent` 334, bash 1,395 | **1st, by 4.2×** |
| retries | E3 codes/hints, `ATTEMPTS` | 1st on repair signal |
| blast radius | E4: 6/6 contained | **1st** |

The term that dominates a long session is not the one we have been optimising.

---

## 2. The central finding: novelty is taxed twice

**A retry costs a whole turn. Standing context is paid by every session. Both
are driven by the same variable — how unfamiliar the syntax is — and that
variable is the one our design has been spending freely.**

The evidence is in our own corpus. `ATTEMPTS` in `benches/agentic/corpus.mjs`
records tries-to-correct per engine over the same ten queries:

| Engine | Attempts for 10 queries |
| --- | ---: |
| SQL | 10 |
| jq | 10 |
| PowerShell | 10 |
| nushell | 16 |
| bash + coreutils | 17 |
| **AetherShell** | **18** |

That number carries a confound already stated in the benchmark README — the
author had written far more bash and SQL than nushell, and wrote AetherShell's
standard library. But the confound runs the *wrong way* for us: the person most
fluent in AetherShell needed the most attempts in it. And for an LLM the
mechanism is stronger and simpler than fluency: SQL and jq appear in
pretraining in volumes AetherShell never will.

Now price the other half. The agentic syntax's quick reference is **1,196
tokens**, and it saves **~9.3 tokens per query** — break-even at ~129 queries,
*before* counting a single retry. The full mapping in `AGENTS.md` is larger
still: all 26 letters a–z assigned to builtins, 92 module sigils, and roughly
200 per-module function abbreviations. `l`, `w`, `m`, `F.r`, `DK.p`, `H.g`.

That is a private language. Every symbol in it is a thing the model must be
told, gets wrong, and is charged for twice.

**And the headline claim is not supported.** `AGENTS.md` advertises agentic
syntax as "reduces LLM token consumption by ~60-70%". Measured on E1: **27.5%
on commands**, 0% on output, and net negative on any session shorter than ~129
queries. The 60–70% figure appears to count characters saved on hand-picked
constructs rather than tokens over a task.

---

## 3. The inversion

> **Spend novelty only where the language is *read*. Never where it is
> *written*.**

An agent must *write* the input, so every unfamiliar token there costs a retry
risk and a line of cheatsheet. It only *reads* the output, so a dense but
regular encoding costs almost nothing to learn — one header line and the shape
is apparent.

The current design has this exactly backwards: a maximally novel input syntax
(`.aeg`) and an output encoding whose advantage we only measured last week.

E2 shows what the read side is worth when it is taken seriously: **334 output
tokens against bash's 1,395**, from an encoding the agent never has to author.
`@suffix name: .rs` is self-describing; no cheatsheet entry was needed for any
agent to read it.

So:

- **Input surfaces should be borrowed, not invented.** SQL for set and
  aggregate work. jq-shaped path expressions for reshaping JSON. POSIX-shaped
  verbs for the ordinary shell nouns. Each is free: zero standing context, high
  first-try accuracy, and an enormous corpus of worked examples the model has
  already read.
- **The output encoding and the error contract are ours, and are where the
  investment belongs.** They are the parts no borrowed front-end provides, and
  the parts E2–E5 say we already win.

What is actually valuable in AetherShell is not its syntax. It is the **core**:
typed values, deterministic rendering, structured errors, the effect taxonomy,
the ontology. The syntax is the part we lose on (E1, E6) and the part with the
least defensible moat.

### What this makes AetherShell

**A typed, effect-gated evaluator with plural borrowed front-ends and one
output contract.** `sqlite_query` already demonstrates the shape and measures
well — 386 tokens at 80 ms, second on latency, with the typed result, the
structured errors and the jail still around it. It is the model for the rest,
not an exception to it.

---

## 4. The second structural finding: the builtin ABI permits wrong answers

Six defects surfaced in one week. They are not six bugs; they are one design
property observed six times.

| Defect | What it returned |
| --- | --- |
| `round(4.966, 2)` | `5` — digits accepted, discarded |
| `mean \| round(2)` | `2` — took the digit count as the subject |
| `env(name, default)` | default accepted, discarded |
| `echo $HOME` (compat) | `null`, exit 0 |
| `1 / 0`, `1 % 0` | `inf` at exit 0; a panic |
| `db_json_to_sqlite("x")` | `false`, exit 0 |

Every one is a call the builtin could not honour, answered with a value instead
of a refusal. The root cause is the ABI:

```rust
fn(Vec<Value>, Option<Value>) -> Result<Value>
```

There is no schema. Nothing can check arity or types before the body runs, and
the body is free to ignore what it was given. `ontology_describe` carries a
*separate*, hand-written signature, which is why `max()` could advertise
`Aggregation / "Get maximum value" / max() -> Number` while refusing arrays,
and why `sqlite_query` is absent from the ontology entirely while working
perfectly.

**First-principles fix: make the declaration the only source of truth.**

```rust
builtin! {
    name: "round",
    subject: Numeric,                 // what the pipe supplies
    params: [digits: Int in 0..=17 = 0],
    returns: Numeric,
    effect: Pure,
    examples: ["4.966 | round(2)  # 4.97"],
}
```

The dispatcher validates against this before the body runs. Three things follow
at once, and they are the four objective-function terms again:

1. **The defect class becomes unrepresentable.** A body cannot silently ignore
   an argument it did not declare, and cannot receive one it cannot honour.
2. **The ontology becomes true by construction.** It is generated from the same
   declaration, so a builtin cannot advertise a signature it does not implement,
   and cannot be missing from its own catalogue. Discovery is part of the
   language, and ours is currently part fiction.
3. **Repair hints get an argument level.** "expected `digits: Int in 0..=17`,
   got String" is a one-retry error; "check this builtin's required argument
   count and types" is a guess.

This is the highest-leverage change available and it is mechanical: the
information mostly exists, scattered between the dispatch table, the ontology
and the doc comments. It needs to exist once.

**Status: a vertical slice has landed** (`src/signature.rs`). Nine builtins are
declared: the three from the table above that are builtins with arguments
(`round`, `env`, `db_json_to_sqlite`), `max` and `min` whose ontology entries
contradicted their behaviour, and the everyday verbs the benchmark corpus
actually reaches for (`where`, `map`, `sum`, `len`).
`builtins::call_with_input` validates against the declaration before the body
runs. Undeclared builtins dispatch exactly as before, so the
migration is incremental and nothing regressed. The measured effect on the
discovery surface:

| `ontology_describe` | Before | After |
| --- | --- | --- |
| `db_json_to_sqlite` | "Database: json to sqlite", 0 parameters | `db_json_to_sqlite(db: String, json: String, table?: String) -> Bool`, 3 parameters, 1 worked example |
| `max` | `max() -> Number`, refusing the array it implies | `Array \| max(values?: Any) -> Number`, 2 worked examples |

And on refusals: `round(1.0, 99)` now names `0..=17`; `round(1.0, "two")` names
`digits: Int`; `where(5)` names `predicate`; `db_json_to_sqlite("x")` prints the
full signature with the argument order, which is the part that was actually
wrong. `tests/declared_signatures.rs` asserts each of these, that every
declaration names a builtin that is really dispatched, and — as non-vacuity —
that validation both accepts a valid call and refuses an invalid one.

What has *not* landed is the `builtin!` macro or the remaining 1,271 builtins.
The mechanism does not depend on the list's size; growing it is the migration.

Landing it also found a seventh instance of the same defect, which is the
argument for the approach more than any of the first six. `map` is one of the
nineteen builtins with a *hand-written* catalogue entry, and that entry said
`map(array: Array, fn: Lambda) -> Array` while the dispatcher enforced
something else. A declaration that sits beside a hand-written description is
still two descriptions, so declarations now outrank them.

And `tests/self_healing.rs` caught the cost of doing it carelessly: `diagnose`
is contractually cheaper than a full `ontology_describe`, and a refusal that
names its whole signature inside `expected` — then again inside `hint`, which
is literally `"pass an argument matching: {expected}"` — broke that bound
(144 tokens against a 107 ceiling). The signature belongs in the field named
`signature`, once. `expected` is now dropped when it equals it, the hint is
dropped when it only restates it, and `got` carries `"0 of 1 required"`. The
same repair context costs **82 tokens against `ontology_describe`'s 212** —
measured with the same exact BPE path as every other number here.

---

## 5. The third finding: 1,280 builtins is a discovery cost, not a feature

`AGENTS.md` advertises 1,280+ builtins across 108 modules. E1 and E2 together —
eighteen realistic tasks — needed about **twenty**. The other 1,260 are carried
in every ontology dump, every schema export, and every drift check, and we found
three of them mis-described in a single week of looking.

Not a proposal to delete anything. A proposal to **stratify**:

- **Core (~60):** the composable verbs — `ls`, `cat`, `where`, `map`, `sum`,
  `sort_by`, `group_by`, `from_json`, `sqlite_query`. Enforced signatures,
  worked examples, ontology coverage asserted by a test.
- **Extended (the rest):** reachable by name, lower documentation contract,
  labelled as such so an agent knows the difference between "documented" and
  "present".

An agent cannot hold 1,280 names. It can hold 60 and look up the rest. The
ontology's progressive disclosure is the right mechanism; the missing piece is
an honest tier marking so "not in the ontology" stops meaning "does not exist".

---

## 6. The fourth finding: effects should be checkable before execution

E4's 6/6 containment is a **runtime** property: the gate fires when the call is
made. But `safety::effect_of` computes a builtin's effect class statically, so
the effect set of a whole pipeline is computable without running it.

```
$ ae --explain 'ls("src") | where(…) | rm'
effects: ReadLocal, Destructive
jail:    /home/me/project
decision: Destructive requires approval in agent mode
would touch: src/**            (3 paths)
```

This turns containment from something an agent discovers by being refused into
something it can ask about first — and it is nearly free, because the taxonomy,
the policy table and the approval descriptor all exist. It also makes the
paper's conditional ("where arbitrary execution can be isolated") answerable
statically rather than by trusting a runtime.

---

## 7. What to stop

**Stop growing the sigil map.** It is the novelty tax made concrete, and §2
prices it. Keep the implicit parameter and `|.field` — they are small, regular
and were measured — and stop assigning letters.

**Stop advertising bash compatibility.** Measured 2 of 32 (E6). Already removed
from the README; it should come out of `AGENTS.md` too, which still lists
"Option 4: Migrate Existing Shell Scripts" as though it worked.

**Stop chasing SQLite on latency by optimising the evaluator.** SQLite reads an
index; we parse a document. Not re-parsing is the answer, and it exists.

---

## 8. Sequencing

| | Change | Effort | Term it moves |
| --- | --- | --- | --- |
| 1 | Declarative builtin signatures, enforced at dispatch | large, mechanical | retries, discovery |
| 2 | Generate the ontology from those declarations | follows from 1 | retries |
| 3 | Core/extended stratification | medium | standing context |
| 4 | `--explain`: static effect set | small | blast radius |
| 5 | Borrowed front-ends as a stated strategy (SQL first) | medium | command tokens, retries |
| 6 | Freeze the sigil map | free | standing context |

Items 1–2 are one project and should be done together; doing 1 without 2 leaves
the ontology hand-written and therefore still capable of lying.

---

## 9. What this does not claim

**None of this is evidence about task score.** Every number above is substrate
cost. The experiment that would test whether a typed, effect-gated shell keeps
bash's composition advantage is the 1,700-run study in §8 of the response
document, and nothing here substitutes for running it.

**The novelty argument is reasoned, not measured, for LLMs.** `ATTEMPTS` is one
author, and the benchmark README says so. The clean experiment is cheap and we
should run it before acting on §3 at scale: give a model the ontology and the
cheatsheet, ask for the same ten queries in each surface, and measure first-try
correctness and total tokens to a correct answer. If the borrowed surfaces do
not win that, §3 is wrong.

**And "decisively across the board" is the wrong target on one axis.** We will
not beat SQL at SQL, and a shell that becomes a query language to win a query
benchmark has conceded the argument. What is available is to be the only engine
that is near-best on every axis and best on the ones that compose — errors,
containment, determinism — which is what §4–§6 of the response already show and
what §4 and §6 here would make structural rather than incidental.
