# Winning across the board: what it would actually take

A proposal, grounded in the six experiments in `benches/agentic/`. Every
"before" number is measured. Every "after" number is marked **measured**,
**projected** or **unknown**, and the projections say what they assume.

Two of the seven items are recommendations *not* to do something.

---

## The scoreboard as it stands

| Axis | AetherShell | Best other | Standing |
| --- | --- | --- | --- |
| E1 scalar-query tokens | 448 | sqlite 258, jq 345 | **4th of 6** |
| E1 latency (Linux) | 13–24 ms/query | sqlite 3–4, jq 6 | **3rd** |
| E2 tabular tokens | **497** (`--agent`) | pwsh 735, bash 1,463 | **1st** |
| E3 error codes / hints | **10/10 · 10/10** | nushell 10/10 · 7/10 | **1st (tied on codes)** |
| E3 error byte cost | 141 | bash 42 | **3rd** |
| E3 exit-status granularity | 0/10 | bash 5/10 | **last** |
| E4 containment (agent) | **6/6** | bash 0/5 | **1st** |
| E4 containment (human) | 1/6 | — | **last, by design** |
| E5 environment stability | **1 of 6 distinct** | pwsh also stable | **1st (tied)** |
| E6 bash compatibility | 2/32 native | — | **the claim does not hold** |

Four firsts, three lasts. The lasts are what this document is about.

---

## 1. Fix implicit-lambda desugaring — the largest single win

**Measured prize: −27.5% on command tokens. Cost: one function.**

E1 is lost entirely on the command side: our output is 67 tokens against
SQLite's 56, but our commands are 381 against 202. AetherShell already ships a
token-minimised syntax for exactly this (`ae -a`), and it was never benchmarked.
It should have been; that is a gap in the measurement, not only in the product.

It was never benchmarked because it does not work on the queries that matter.
`~.field` desugars to `fn(__) => __.field`, and the desugaring fires at the
**first** `~`, swallowing the rest of the expression:

```
w(~.is_pr)                          → where(fn(__) => __.is_pr)        ✓
w(~.state=="closed")                → where(fn(__) => __.state=="closed")  ✓
w(!~.is_pr && ~.state=="open")      → where(!fn(__) => __.is_pr && …)  ✗
                                      error: where: expected a lambda, got Bool
lower(~.title + " " + ~.body)       → lower(fn(__) => …)               ✗
                                      error: lower: expected a string, got Lambda
```

So a single top-level reference works and **two do not**, which rules out six of
the ten E1 queries — the six with compound predicates, which is where the tokens
are.

**The change.** `consume_lambda` (`src/transpile/agentic.rs:1745`) builds the
lambda at the `~` and runs to the end of the body. It should instead hoist to
the enclosing expression boundary and substitute *every* `~` in it:
`expr` containing one or more `~` becomes `fn(__) => expr[~ ↦ __]`. The single-
reference case is unchanged, so nothing that works today breaks.

**The prize, measured.** Nine E1 queries, exact cl100k:

| Form | Tokens | vs SQLite |
| --- | ---: | ---: |
| AetherShell, current verbose | 305 | +74% |
| AetherShell, agentic syntax | **221** | +26% |
| SQLite | 175 | — |

Extrapolated to the full ten-query E1 that would be roughly 276 command tokens
against today's 381, putting the total near **343 against jq's 345 and SQLite's
258** — fourth place to a tie for second. *Projected*, because two of those nine
agentic forms do not run until this is fixed; the token count of the text is
exact, the claim that it will run is not.

**The cost nobody should skip.** `crates/agentic-eval/README.md` warns that a
syntax can golf its input while inflating the standing context it carries every
turn. Priced: the agentic quick reference is **1,196 tokens**, and the saving is
about 9.3 tokens per query. Break-even is **~129 queries** in a session, once,
amortised under prompt caching. For a long autonomous run that is clearly worth
it. For a ten-turn task it is a net loss, and the docs should say so rather than
recommending agentic mode everywhere.

---

## 2. Ship the hybrid — it is already built and undocumented

**Cost: documentation and one convenience builtin. No engine work.**

The Vercel post's own conclusion was not that SQL wins; it was that *the hybrid
wins*: "the hybrid approach consistently hits 100% accuracy, while pure SQL
occasionally gets things wrong", at roughly twice SQL's tokens.

AetherShell already has `sqlite_query`, `db_json_to_sqlite`, `db_csv_to_sqlite`
and eleven other `db_sqlite_*` builtins. This runs today:

```aethershell
sqlite_query("issues.db", "SELECT count(*) FROM issues WHERE is_pr=1")
# [{count(*): 293}]
```

So the configuration the blog found best — shell for exploration, SQL for the
query — is one shell, with typed results, structured errors and the effect gate
around it. We argued in §3 that we lose to SQLite, and the more useful response
is that **you do not have to choose**; we simply never said so, and never
measured it.

**Do:** document the ingest-then-query path; add a `to_sqlite` pipeline sink so
`cat(f) | from_json | to_sqlite("x.db","issues")` is one line; benchmark the
hybrid as a seventh E1 engine. **Do not** claim it beats SQLite until that
benchmark exists.

---

## 3. Map error codes onto exit statuses

**Cost: a match arm. Prize: the one axis where bash beats us on signal.**

bash exits 127 for command-not-found, 2 for a syntax error, 1 for a permission
denial. We exit 1 for everything, scoring 0/10 where bash scores 5/10. That is
free pre-parse signal thrown away — an agent, or a `set -e` wrapper, or a CI
step can branch on it without reading stderr at all.

`safety::ErrorCode` is already the exhaustive taxonomy. Give it an
`exit_code()`:

| Code | Exit | Why |
| --- | ---: | --- |
| `E_UNKNOWN_BUILTIN` | 127 | matches bash's command-not-found, deliberately |
| `E_PARSE` | 2 | matches bash's syntax-error convention |
| `E_BAD_ARG`, `E_UNKNOWN_FIELD` | 64 | `EX_USAGE` from sysexits.h |
| `E_POLICY_DENY`, `E_OUTSIDE_WORKSPACE` | 77 | `EX_NOPERM` |
| `E_NEEDS_APPROVAL` | 75 | `EX_TEMPFAIL` — retry after approval |
| `E_TOOL_MISSING` | 127 | command-not-found, one level out |
| `E_BUDGET_EXCEEDED`, `E_NO_UI`, `E_TOOL_FAILED` | 69 | `EX_UNAVAILABLE` |
| `E_UNIMPLEMENTED` | 70 | `EX_SOFTWARE` |
| `E_NOT_FOUND` | 66 | `EX_NOINPUT` |
| `E_BAD_STATE` | 76 | `EX_PROTOCOL` -- issued out of sequence |
| `E_UNKNOWN` | 1 | unchanged |

The last six were not in the original design. Each was found the same way, by
sweeping the whole catalogue rather than by reasoning about it: every builtin
handed one nonsense argument, and every `E_UNKNOWN` read individually. The
taxonomy grew from eight codes to fourteen because six conditions the shell
could name perfectly well were being reported as unidentifiable -- and two of
them (`crypto.cert_parse`, `crypto.verify_cert`) were reporting `E_BAD_ARG`,
which is worse than uncoded: it tells an agent to retry with different
arguments when no arguments will ever work.

Borrowing bash's 127 and 2 is the point: an agent that already knows shell
conventions gets them for free. The rest follow `sysexits.h` rather than being
invented.

**Risk, audited rather than estimated.** Ten places across `tests/` read a
process exit status and **none asserts it equals 1** — they check `is_err()`, or
zero against non-zero. Nothing in `scripts/` or the CI workflows branches on an
AetherShell exit code either; the `exit 1` lines there are those scripts failing
themselves. The blast radius is empty, which is worth establishing before
proposing the change rather than after.

---

## 4. Make containment the default, not the flag

**Cost: a CLI default and a migration note.**

E4 is our strongest result — 6/6 contained — and it is **opt-in**. Human mode
contains 1 of 6, and the one is `sh()` being off rather than a gate firing. A
deployment that forgets `--agent --workspace` has a shell exactly as unbounded
as bash, which means the headline result describes a configuration most users
will not be in.

Proposal: `--workspace` defaults to the current working directory when the shell
is non-interactive (`-c`, a script file, MCP, the agent API), and stays unset for
an interactive REPL, where treating a person as an adversary is the wrong
default. An explicit `--workspace /` restores today's behaviour for the scripts
that need it.

This is a breaking change for non-interactive callers that write outside the
cwd, which is why it wants a minor-version boundary and a release note, not a
quiet patch.

> **Resolved, narrower than proposed.** See "Item 4, resolved" at the end of
> this document: the blanket non-interactive default was rejected, and the two
> agent-serving subcommands imply agent mode instead.

---

## 5. Latency: index, do not micro-optimise

**Honest assessment: this is structural, and the gap is smaller than it looks.**

Per-query on Linux, AetherShell is 13–24 ms against SQLite's 3–4. The
decomposition says where it goes: **4 ms process start, 3 ms to read and parse
334 KB of JSON, the rest evaluation.** SQLite is not faster at the same work; it
is doing different work — reading a pre-built index instead of parsing a
document.

Any engine that parses the corpus per invocation pays that, including jq (6 ms).
So the route to SQLite-class latency is not a faster evaluator; it is not
re-parsing — which is proposal 2, and which already exists. Micro-optimising the
JSON path might buy 2–3 ms and will not change a ranking.

What *is* worth doing: the O(n²) fix in §7 of the response landed a 57×
improvement from one line, and nobody had looked. A profile of the evaluator's
hot path is likely to find more of that kind. That is a different activity from
chasing SQLite.

---

## 6. Do NOT chase bash compatibility

**Recommendation: drop the claim, take the other route.**

E6 measured `ae -b` at 2 of 32 commands run natively, 10 delegated to
`bash -lc`. Closing that properly means a real bash parser — word splitting,
globbing, parameter expansion, job control, `trap`, arrays, arithmetic — which
is a multi-quarter project with a well-populated graveyard.

And succeeding would undermine §6 of the response. The delegated path works by
handing the script to bash, at which point the effect gate sees one `sh` call
rather than the fifty things the script did. A compatibility layer good enough
to run SWE-bench would be a compatibility layer good enough to bypass every
containment property we argue for.

**Instead:** retarget the harness, not the shell. SWE-agent's action space is
"governed by a single `yaml` file"; rewriting that file and the system prompt to
emit AetherShell is a bounded piece of work, and it is the honest experiment —
it measures a typed shell rather than a bash emulator. Meanwhile the README
should stop advertising bash compatibility as a feature, which it has now
stopped doing.

---

## 7. Do NOT optimise error byte cost

**Recommendation: leave it. bash's 42 bytes is not a target.**

We are third on mean error size: bash 42, PowerShell 114, AetherShell 141,
nushell 294. The instinct is to trim. Do not.

bash's 42 bytes buy 0/10 machine-readable codes and 1/10 repair hints. Our extra
99 bytes buy 10/10 and 10/10. The measured trade is that a code and a suggestion
cost about 100 bytes, which is roughly one retry avoided — and a retry is a
whole turn, not a hundred bytes. The axis to watch is nushell's 294, which buys
nothing ours does not, and which we should not drift toward.

---

## Sequencing

| | Item | Effort | Prize | Risk |
| --- | --- | --- | --- | --- |
| 1 | Implicit-lambda desugaring | one function | −27.5% command tokens, 4th → ~2nd on E1 | low; single-reference case unchanged |
| 2 | Document + benchmark the hybrid | docs + one sink | answers §3 without a rewrite | low |
| 3 | `ErrorCode::exit_code()` | a match arm | last → competitive on E3's remaining axis | low; audit `exit == 1` assertions |
| 4 | Workspace-by-default | CLI default | 1/6 → 6/6 for the common case | **breaking**; minor-version boundary |
| 5 | Profile the evaluator | open-ended | unknown; the last look found 57× | low |
| 6 | Bash compatibility | quarters | undermines §6 if it succeeds | — |
| 7 | Error byte trimming | — | negative | — |

Items 1–3 are small, independently landable, and together move three of the
three last-place finishes. Item 4 is the one that needs a decision rather than a
patch.

## What this does not buy

"Decisively across the board" is the wrong target on one axis and an
overstatement on another.

On E1, the way to stop losing to SQLite is not to out-golf it — it is to have
it, which we do. A shell that becomes a query language to win a query benchmark
has conceded the argument the rest of the document makes.

And none of this is evidence about task score. Every number here is substrate
cost; the experiment that would settle whether a typed, effect-gated shell keeps
bash's composition advantage is the 1,700-run study in §8 of the response, and
nothing proposed above is a substitute for running it.

---

## Outcome: items 1–3 implemented, and what they measured

Landed 2026-09-17. The projections above are replaced by measurements; where
the two differ, the measurement stands.

### E1, re-run with every engine

| Engine | Cmd tokens | Output | **Total** | Total ms |
| --- | ---: | ---: | ---: | ---: |
| sqlite | 202 | 56 | **258** | 52 |
| **AetherShell `-a`** | 275 | 67 | **342** | 438 |
| bash + jq | 287 | 58 | **345** | 140 |
| nushell | 297 | 56 | **353** | 361 |
| **AetherShell + `sqlite_query`** | 329 | 57 | **386** | 80 |
| AetherShell (default) | 381 | 67 | **448** | 308 |
| PowerShell | 474 | 57 | 531 | 33,995 |
| bash + coreutils | 647 | 57 | 704 | 3,686 |

**Fourth of six to second of eight on tokens.** Item 1 projected ~343 total
against jq's 345; the measurement is **342**. All ten queries correct, all
byte-stable.

**And second on latency, by a different route.** `sqlite_query` from inside
AetherShell answers all ten in 80 ms against bare `sqlite3`'s 52 and the JSON
pipeline's 308 — the hybrid the Vercel post found best, inside one shell, with
the typed output, the structured errors and the effect gate still around it.

Neither route takes first place: SQLite holds both columns. The honest summary
is that the axis we were losing is no longer lost, not that it is won.

One result went the wrong way and is worth recording: **agentic mode is slower**
(438 ms against the default renderer's 308), because every invocation pays to
transpile. That is a real cost of the token saving, it was not predicted above,
and it argues for caching the transpile rather than for pretending it is free.

### E3 exit statuses

| Failure | Before | After | Matches |
| --- | ---: | ---: | --- |
| unknown name | 1 | **127** | bash |
| syntax error | 1 | **2** | bash |
| malformed call | 1 | **64** | `EX_USAGE` |
| refusal (`sh()`, jail) | 1 | **77** | `EX_NOPERM` |
| budget exhausted | 1 | **69** | `EX_UNAVAILABLE` |
| unidentified | 1 | 1 | unchanged |

0/10 to **7/10** on distinct exit statuses, past bash's 5/10 on the one axis
where bash was the only shell scoring at all. The proposal predicted parity;
the measurement came out ahead of it, because the taxonomy is finer than
bash's conventions are.

### What is still open

Items 5–7 stand as written — profile the evaluator, do not chase bash
compatibility, do not trim error bytes.

### Item 4, resolved — narrower than proposed, and for a reason found by testing

The proposal was `--workspace` by default for **all** non-interactive
invocations. That was too broad. `ae -c` and `ae deploy.sh` are how people and
shell scripts use the shell, and Option 4 of `AGENTS.md` sells running existing
bash scripts unchanged; a shell that refuses to write outside its working
directory would break that and would not really be a shell. Scaling a safety
default up until it breaks the product is not a safety win.

What the proposal was right about was that the headline described a
configuration most users would not be in. Probing the actual surface showed
where: **`ae agent serve` and `ae mcp serve` ran the human profile** unless the
operator also passed `--agent`. Those two subcommands exist for no purpose other
than to serve an AI agent, so they now imply agent mode — default-deny, jail
rooted at the server's working directory, profile printed at startup, and
`AETHER_MODE` honoured as an explicit opt-out. Nothing about `ae -c`, `ae
script.ae` or the REPL changes, so this is not the breaking change the proposal
worried about and does not need a version boundary.

Two things worth keeping from how this was found:

**The hole was masked by a second bug.** `POST /api/v1/eval` built its
environment without the module namespaces bound (`Env::default()` where two
other call sites looped over `modules::all_modules()`), so every
module-qualified call — all 108 namespaces — failed as a field access on
`Null`. A containment probe therefore came back *contained* because the call had
never run. Fixing the namespaces turned the same probe into a successful write
to an arbitrary path. Containment that rests on a second defect is
indistinguishable from containment until the day someone fixes the second
defect.

**The measured number went down, and that is the honest one.** E4 now carries a
third AetherShell arm — a live `ae agent serve` with no flags — and it scores
**5/6, not 6/6**. Network egress still executes, because that axis is governed
by `AETHER_MAX_NET`, which agent mode does not set. Making agent mode imply a
zero network quota would make the table read 6/6 and would break every
legitimate `http.get` an agent makes, so the gap is reported rather than
closed.
