# A typed shell is also a shell

**A response to *Is Bash All You Need?* (Mak et al., Microsoft / CMU,
arXiv:2609.11999) and *Testing if bash is all you need* (Vercel, 2026), with
measurements from AetherShell 12.0.2.**

Nervosys builds AetherShell. Everything below is re-runnable from
`benches/agentic/` in this repository, including the results that go against
us, of which there are several — one of them is the headline of §3. Read the
[conflict-of-interest note](#8-conflict-of-interest-and-what-would-change-our-mind)
first if you would rather know the bias before the numbers.

---

## 1. Two results that look opposed and are not

Within weeks of each other, two careful empirical studies reached what reads
like opposite conclusions.

**Microsoft and CMU** put frontier models on TheAgentCompany (174 tasks) and
APEX-Agents (480 tasks) behind five different tool interfaces, and found the
plain shell won:

> Bash outscores both shell-free interfaces while remaining among the cheapest
> options.

Against a typed tool catalogue, bash scored **+24.6 pp** on TheAgentCompany
(69.4% vs 44.9%, Opus-4.8), used **19–72% fewer tokens**, and cost **$0.37/task
against $1.20**. Adding a typed catalogue back on top of bash moved the pooled
score by **−0.6 pp**. Letting the agent synthesise and persist its own tools
produced no detectable gain. The recommendation:

> these results favor bash alone when arbitrary execution can be isolated and
> PTC when security or compliance policies require a fixed tool catalog.

**Vercel and Braintrust** pointed a Sonnet-4.5 agent at a corpus of GitHub
issues and pull requests, and found the plain shell lost badly:

| Agent | Accuracy | Avg tokens | Cost | Duration |
| --- | ---: | ---: | ---: | ---: |
| SQL | 100% | 155,531 | $0.51 | 45 s |
| Bash | 52.7% | 1,062,031 | $3.34 | 401 s |
| Filesystem | 63.0% | 1,275,871 | $3.89 | 126 s |

Seven times the tokens for half the accuracy. Their conclusion: *"For
structured data with clear schemas, SQL remains the most direct path."*

These findings are not in tension, and the second study's own diagnosis says
why. The bash agent did not fail because it had a shell. It failed for two
stated reasons:

> The bash agent didn't know the structure of the JSON files it was querying.

> Commands that should run in milliseconds were timing out at 10 seconds.
> `stat()` calls across 68,000 files were the culprit.

The first is a **discovery** failure — the agent could not ask what shape the
data was. The second is a **substrate** failure — the primitive it reached for
had the wrong cost curve. Neither is a fact about shells. Both are facts about
*that* shell.

So the two studies bracket a variable neither one varied. Microsoft compared a
shell against tool catalogues and found composition wins. Vercel compared a
shell against a typed engine and found types win. Nobody ran the cell where
**the shell is the typed engine**.

That cell is what AetherShell is, and this document is what we measured in it.

---

## 2. What neither study examines

Read for what it *doesn't* say, the Microsoft paper is as interesting as its
headline. Across the text there is no treatment of typed output schemas, output
determinism, or how an agent parses what comes back; the interfaces are
compared on task score and token count alone. Safety appears once, as a
precondition rather than a subject:

> Practitioners could use shell-only agents where arbitrary execution can be
> isolated.

That conditional is load-bearing, and it is usually discharged by a container
around the shell. It is worth asking how much of it the shell could discharge
itself — because the alternative offered for policy-constrained environments
(PTC, a restricted program over a fixed catalogue) *gives up 16.5 pp of task
score* to get that containment. The paper measures the price of containment
under one architecture. It does not test whether that price is intrinsic.

Four axes are unmeasured by both works, and an agent pays for all four every
turn:

1. **Output token cost of the substrate**, before any task-specific reasoning.
2. **Determinism** — same command twice, same bytes? This decides whether an
   agent can cache, diff or snapshot anything.
3. **Failure legibility** — can an error be branched on, or must it be read,
   guessed at, and retried?
4. **Containment by the shell** — is "isolate arbitrary execution" something
   the surrounding infrastructure must provide, or something the interpreter
   already does?

Sections 3–6 report measurements on all four, in that order. §3 is the
replication of the Vercel setup, and it is where we lose.

### How this was measured

Four experiments, all executing real processes on a 24-core Debian (WSL2)
host, all re-runnable from `benches/agentic/`. Token counts are **exact
cl100k_base BPE** via tiktoken, computed by this repository's own `tokens_of`
example over the captured bytes — never estimated. Latency is the median of 11
runs after a discarded warm-up.

Engines are given their best form: bash is measured **with `jq` available**,
because an agent with a shell has `jq`. `bash+coreutils` is a *separate* entry
representing the filesystem-only case the Vercel post measured, not a
handicapped bash. Versions: bash 5.2.37, jq 1.7.1, SQLite 3.50.4, nushell
0.115.1, PowerShell 7.6.6, AetherShell 12.0.2.

**A note on wall clock.** This host's timings varied by up to 2.8× between
otherwise identical runs, and the Windows side was worse — a factor of five.
Latency is therefore reported as a range across two clean runs, and the
argument never rests on a difference smaller than an order of magnitude. Token
counts, correctness and determinism do not vary at all, and those carry the
weight.

---

## 3. Replicating the Vercel result: we lose

500 real issues and pull requests from `github.com/cli/cli`, fetched with `gh`,
materialised in the three representations the blog post compared: one JSON
array, 500 individual files, and a SQLite database. Ten questions of increasing
difficulty, from *"how many open items mention security"* to *"how many authors
have opened both a PR and an issue"*. Every answer is checked against an oracle
computed independently in JavaScript.

**Every engine answered all ten correctly, and every engine was byte-stable
across all 11 runs.** That is worth stating plainly: on this corpus the
determinism axis separates nobody, and correctness separates nobody. What
separates them is cost. (That determinism result is also weaker than it looks,
because nothing varied except the clock — §4 puts it under real pressure.)

| Engine | Command tokens | Output tokens | **Total** | Total ms (two runs) |
| --- | ---: | ---: | ---: | ---: |
| sqlite | 202 | 56 | **258** | 37 / 52 |
| bash + jq | 287 | 58 | **345** | 78 / 140 |
| nushell | 297 | 56 | **353** | 207 / 361 |
| **AetherShell** | 381 | 67 | **448** | 150 / 419 |
| PowerShell | 474 | 57 | **531** | 26,657 / 33,995 |
| bash + coreutils | 647 | 57 | **704** | 2,661 / 3,686 |

AetherShell is **fourth of six**. It costs 74% more tokens than SQLite and 30%
more than `bash + jq`, and it is slower than both. There is no reading of this
table in which the typed shell wins it.

The reason is worth more than the ranking. **Look at the output column.** Every
engine lands between 56 and 67 tokens — because every one of these ten
questions has a scalar answer. `10`. `293`. `4.97`. There is no structure in
the result for a typed representation to be efficient *about*. The entire
spread in the total column is command verbosity, and AetherShell's commands are
verbose:

```
sqlite       SELECT count(*) FROM issues WHERE is_pr=1;
jq           jq '[.[]|select(.is_pr)]|length' issues.json
AetherShell  cat("issues.json") | from_json | where(fn(r) => r.is_pr) | len
```

Thirteen, seventeen and twenty-two tokens for the whole exchange. The typed
shell pays for `from_json`, for a named lambda parameter, and for spelling out
the read. Against a query language
purpose-built for exactly this shape, on a benchmark whose every answer is one
number, that is simply a worse deal — and §4 is the case where the same design
decision pays.

Two further observations from this table:

**`bash + coreutils` is the Vercel finding, reproduced at the substrate level.**
704 tokens and 2.7–3.7 seconds against jq's 345 and 78–140 ms. The three
queries that dominate are the ones that must open each of 500 files: q1 (610
ms), q5 (945 ms), q7 (710 ms). This is the same shape as *"`stat()` calls
across 68,000 files were the culprit"* — a per-file process spawn is the wrong
primitive, and the cost is in the substrate, not the model. On Windows the same
three queries took 33 s, 24 s and 40 s — a 25–56× penalty per query, from
fork emulation alone.

**PowerShell is unusable for this workload on Linux**, at 2.3–3.6 s *per
invocation*, almost all of it .NET start-up. It is not slow at the work; it is
slow at existing. On Windows the same commands run in ~350 ms.

### An accident worth reporting

During the first run we started an unrelated compile on the same machine.
Under that contention, `bash + coreutils` q10 returned **5.02** where the answer
is **4.97** — one record silently dropped from a ~2,000-subprocess pipeline,
exit status 0, no warning, and the value is plausible. Re-run unloaded, it
returns 4.97 ten times out of ten.

We are not presenting this as a property of bash. The most likely cause is a
transient process-spawn failure under load, and the run that produced it was
contaminated by our own mistake. It is reported because the *shape* is worth
seeing: an approach that composes 2,000 processes to answer one question has
2,000 opportunities to lose a record, and the failure mode is a wrong number
rather than an error. The engines that did the same work in one process cannot
fail this way. Whether the probability is 10⁻⁵ or 10⁻³ is a question we have
not answered, and we would not assert either.

---

## 4. The same comparison where the answer is data: 2.9×

§3 asks ten questions whose answers are scalars. §4 asks eight questions whose
answers are *tables* — the things an agent actually does between decisions:
list the source files with their sizes, find the five largest, count the tests,
read the declared version, show the three most recently modified. Run against
this repository, five engines, same methodology.

> **This section was wrong the first time, in our favour, and the correction is
> larger than most of the differences it reports.** The first run measured
> AetherShell's default output at 340 total tokens and reported a 4.3× win. Its
> renderer was printing an array of records as `[{…}, {…}, …]`, so the
> "cheapest" output contained none of the requested data, and the harness —
> which then checked only that the exit status was zero and the output
> non-empty — scored it as correct. The renderer is fixed (§7, defect 9), the
> harness now requires each answer to contain the facts it asked for, and the
> numbers below are from the re-run. The real advantage is **2.9×**, not 4.3×.

| Engine | Command tokens | **Output tokens** | **Total** | Median ms |
| --- | ---: | ---: | ---: | ---: |
| **AetherShell `--agent`** | 163 | **334** | **497** | 9.7 |
| PowerShell | 142 | 593 | **735** | 3,285 |
| AetherShell (default) | 163 | 679 | **842** | 9.7 |
| nushell | 91 | 1,183 | **1,274** | 36.6 |
| bash | 68 | 1,395 | **1,463** | 5.3 |

Two AetherShell rows, because the shell has two renderers and it would be
sleight of hand to quote only the better one. `--agent` is the mode an agent
runs in: it emits AECON, which factors repeated structure out of a column. The
default is the human pretty-printer. The programs are identical; only the
rendering differs.

The command column has not changed its story: AetherShell is still the most
verbose to write, 2.4× bash. The output column inverts it. **Agent-mode output
costs 4.2× less than bash's and 3.5× less than nushell's**, and the total is
**2.9× cheaper than bash**, 2.6× cheaper than nushell, 1.5× cheaper than
PowerShell. In its default human rendering it is still 1.7× cheaper than bash
overall.

One task carries most of it. *List the `.rs` files in `src/` with their sizes*
— 45 files:

| | tokens | bytes | exact sizes? |
| --- | ---: | ---: | --- |
| AetherShell `--agent` | **285** | 657 | yes |
| PowerShell `Get-ChildItem src/*.rs \| Select-Object Name,Length` | 398 | 1,409 | yes |
| AetherShell default `ls("src") \| where(…) \| pick("name", "size")` | 582 | 1,485 | yes |
| nushell `ls src/*.rs \| select name size` | 913 | 3,023 | **no** |
| bash `ls -l src/*.rs` | 1,177 | 2,650 | yes |

Same 45 facts, four renderings of them. bash spends its tokens on permission
strings, owner, group, month-day-time and a repeated `src/` prefix on every
line — six fields nobody asked for. nushell spends them on box-drawing
characters and column padding, and is *more* expensive than bash, because its
table renderer is built for a human looking at a terminal.

nushell also loses information doing it. Its default table rounds to three
significant figures — `27.2 kB` where the file is 27,260 bytes — so an agent
that asks for sizes cannot sum, diff or compare them exactly. It is the only
engine here whose cheapest path to the answer is lossy, and it is also the most
expensive. That combination is worth more than the ranking: the cost is being
paid for presentation, and the presentation is destroying the data.

AetherShell's agent mode emits the two requested fields and factors the common
suffix into a header (`@suffix name: .rs`), which is reconstructible and exact.

This is the finding we would ask a reader to take away, because it explains the
disagreement in §1 rather than just adding a data point to it:

> **A typed shell's advantage is proportional to how much data comes back.
> Benchmarks whose every answer is a scalar cannot measure it.**

The Vercel benchmark is of the scalar kind — reasonably, since it was testing
query accuracy. But an agent's session is not ten scalar questions; it is
hundreds of turns of listing, reading, diffing and inspecting, where the result
*is* a table. That is the regime §4 measures, and the 2.9× applies to the term
that dominates a real transcript.

It also puts a number on the Microsoft finding from the other side. Their
Bash-vs-Tool-only token gap was 19–72%. The gap here between two *shells*, on
output alone, is 4.2×. Whether the shell's values are typed is a bigger lever
on token cost than whether the interface is a shell at all.

### The same output, on a different machine

§3's determinism result — every engine byte-stable across eleven runs — is true
and nearly worthless, because the only thing that varied was the clock. An
agent's output is not re-read in the environment that produced it. It is cached,
diffed against last week's run, compared across a fleet, or replayed in CI, and
those environments differ in locale, timezone and terminal width.

The same listing, run under six environments a real fleet spans:

| Engine | Distinct outputs | Varies with |
| --- | ---: | --- |
| **AetherShell** (both modes) | **1 of 6** | nothing |
| PowerShell | **1 of 6** | nothing |
| bash | 2 of 6 | **timezone** |
| nushell | 2 of 6 | **locale** |

bash's `ls -l` prints the modification time in local time, so the same unchanged
file reads `Sep 16 18:56` in UTC and `Sep 17 03:56` in `Asia/Tokyo`. Two
developers diffing the same directory listing get a diff.

nushell's is the more interesting one. Its size column is locale-formatted:

```
LC_ALL=C         │ src/agent.rs │  27.2 kB │
LC_ALL=de_DE     │ src/agent.rs │  27,2 kB │
```

A decimal comma. So that column is *rounded* (§4: 27,260 bytes rendered as three
significant figures), *locale-dependent* in its separator, and therefore both
lossy and unparseable by a consumer written against a different machine — three
compounding problems in one field, all in service of presentation. It happens
even on a host where the German locale is not generated, because nushell applies
the formatting itself rather than deferring to the system.

Two caveats, both against the strength of this result. The locale axis is
**understated for bash**: `de_DE.UTF-8` and `ja_JP.UTF-8` were not generated on
the test host, so `ls` could not localise its month names and scored stable on
an axis where it would otherwise vary. And a single stable engine proves less
than six environments suggest — these are the environments we thought to try,
and an axis nobody varies is an axis on which everything looks deterministic,
which is the mistake §3 made.

---

## 5. What a failure costs and what it tells you

Total token spend is dominated by turns the agent did not intend to take. A
failure it can branch on costs one retry; a failure it must interpret costs a
read, a guess, and often a second wrong guess. Ten failures an agent actually
causes — a misspelled name, a missing file, malformed JSON, a type error, a
missing field, a syntax error, wrong arity, division by zero, a permission
denial, an index past the end — expressed in each shell.

| Engine | Failed | Machine-readable code | Repair hint | Distinct exit status | Mean bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| AetherShell | 10/10 | **10/10** | **10/10** | 0/10 | 141 |
| nushell | 10/10 | **10/10** | 7/10 | 0/10 | 294 |
| bash | 10/10 | 0/10 | 1/10 | **5/10** | **42** |
| PowerShell | 8/10 | 0/10 | 1/10 | 0/10 | 114 |

**The first measurement put AetherShell at 9/10 and nushell at 10/10, and we
changed the shell rather than the benchmark.** The row above is after that fix;
the row before it was 9/10 codes, 9/10 hints, 129 bytes. §7 describes what was
wrong. We report the before as well as the after because a benchmark you edit
your product in response to is only honest if the edit is visible — and because
nushell is the other shell that took this problem seriously and got there
first.

What separates the two now is what the code costs. The same mistake, `lenght`
instead of `length`:

```
AetherShell   error[E_UNKNOWN_BUILTIN]: unknown builtin: lenght
              hint: did you mean: length

nushell       Error: nu::shell::external_command
                x External command failed
                 ,-[source:1:1]
               1 | lenght [1 2 3]
                 : ^^^|^^
                 :    `-- Command `lenght` not found
                 `----
              help: Did you mean `length`?

bash          bash: line 1: lenght: command not found
```

76 bytes, ~300 bytes, 38 bytes. All three identify the problem; two suggest the
fix; nushell spends 4× the tokens on source-span art that is valuable to a
human at a terminal and is pure cost to a model that already has the source. At
a mean of 294 bytes against our 141, nushell's failures cost 2.1× more to read.
bash's are the cheapest of all and carry neither a code nor, in 9 cases out of
10, a suggestion.

That is the trade an agent is actually making on this axis: bash's errors are
3.4× cheaper than ours and tell you nothing you can branch on, nushell's tell
you the same things ours do and cost 2.1× more. A code and a suggestion are
worth roughly 100 bytes; a rendered source span is not.

**bash wins the exit-status column, and we lose it 0/10.** `command not found`
exits 127, a permission denial exits 1, a syntax error exits 2. That is free,
pre-parse signal, and AetherShell throws it away by exiting 1 for everything.
Whether an agent that reads stderr anyway benefits from it is arguable; that
bash offers something here and we do not is not.

**The gap the first measurement found.** Parse errors were the 1 in 10 that
carried no code:

```
error: found 1 error(s): unexpected token Eof at line 1, column 27
```

`ErrorCode`'s own doc comment says "every uncoded failure lands here rather
than escaping as bare prose, so *every* failure is branchable on `.code`." For
parse errors that was simply untrue, and the obvious fix — build the error out
of `SafetyError` — would have made the contract differ between the native and
wasm builds, because `safety` is `#[cfg(feature = "native")]` and the parser is
not. The error type now lives in the parser, renders the identical JSON, and
the library still compiles clean for `wasm32-unknown-unknown`.

Worth recording: the first version of that fix wrapped the parser's
diagnostics and not the lexer's, so `let x = @` still escaped as bare prose.
The test written alongside it caught that within minutes. The measurement
found the class of defect; only a test found the second instance of it.

---

## 6. Blast radius: the axis the recommendation turns on

The Microsoft recommendation is conditional — *"bash alone when arbitrary
execution can be isolated"* — and the condition is normally met by putting a
container around the shell. This asks a different question: how much does the
shell isolate by itself?

Seven dangerous operations, attempted from inside a workspace, **scored by
consequence rather than by message**. After each attempt the harness looks at
the world, not the exit status: is the canary file outside the jail still
intact? A refusal that prints politely and performs the write anyway scores as
executed.

| Probe | Effect | AetherShell (human) | **AetherShell (agent + jail)** | bash | PowerShell |
| --- | --- | --- | --- | --- | --- |
| write inside workspace | WriteLocal | executed ✓ | executed ✓ | executed ✓ | executed ✓ |
| write outside workspace | WriteLocal | executed | **contained** | executed | executed |
| delete outside workspace | Destructive | executed | **contained** | executed | executed |
| truncate file outside | WriteLocal | executed | **contained** | executed | executed |
| arbitrary process spawn | Exec | contained | **contained** | executed | executed |
| privilege self-grant | Privileged | executed | **contained** | — | — |
| network egress past quota | Network | executed | **contained** | executed | executed |
| | | **1/6 contained** | **6/6 contained** | **0/5** | **0/5** |

`ae --agent --policy strict --workspace <dir>` contained every one. bash and
PowerShell contained none. The refusals are structured, not prose:

```json
{"error":{"builtin":"rm","code":"E_OUTSIDE_WORKSPACE",
  "message":"rm: '…/outside/canary.txt' is outside the workspace root",
  "hint":"operate on paths under …/jail or set AETHER_WORKSPACE",
  "retryable":true}}
```

`retryable: true` is the field that matters: the agent is told this is a
mistake it can correct, not a wall to stop at. The refusals carry
`E_OUTSIDE_WORKSPACE`, `E_POLICY_DENY` and `E_BUDGET_EXCEEDED`.

The `sh` refusal was the exception when we first ran this, and in the worst
possible place: the most security-relevant refusal the shell makes carried the
generic `E_UNKNOWN` with the hint "sh failed without a specific error code;
inspect the message rather than retrying the same call". `E_UNKNOWN` is the one
code the taxonomy tells an agent *not* to reason about, and it was attached to
a refusal whose cause was known exactly. It is now `E_POLICY_DENY`, not
retryable, with a hint naming the switch that changes the answer.

**Three caveats, all of which we would raise against ourselves.**

*Human mode contains almost nothing, by design.* The 1/6 column is not a bug
and it is not a safe default — it is the deliberate choice that a person at a
prompt is not treated as an adversary. The containment is opt-in, and a
deployment that forgets the flags has a shell exactly as unbounded as bash.

*Network is metered, not denied.* The policy table
(`src/safety.rs`) has `Network → allow` in agent mode; what stopped the egress
probe was the `AETHER_MAX_NET` request-count governor set to zero. A quota
bounds how *many* requests leave, not where they go. For an exfiltration threat
model that is a real limitation, and destination policy is not implemented.

*This is not a substitute for a sandbox.* These are language-level effect gates
in the interpreter's own process. They stop a shell *program* from exceeding
its declared blast radius; they do not stop a native binary the shell launched,
or a memory-safety bug in the interpreter. The honest claim is narrower than
"you don't need a container": it is that the price the Microsoft paper measured
for containment — 16.5 pp of task score to move from Bash to PTC — is a price
of *that* architecture. A shell that gates its own effects keeps shell
composition and gets a bounded blast radius, and the composition is what their
data says the score comes from.

---

## 7. What the benchmark found in our own shell

Writing ten ordinary queries and checking the answers against an oracle is not
a demanding test. It found nine defects that more than 2,200 passing tests had
not, and the pattern in them is the interesting part: **every one was invisible
to unit tests because unit tests use small inputs and check the cases the
author thought of.**

| # | Defect | Before | After |
| --- | --- | --- | --- |
| 1 | `round(x, n)` accepted `n` and discarded it | `round(4.966, 2)` → `5` | `4.97` |
| 2 | `mean \| round(2)` took the digit count as the subject | `2` | `4.97` |
| 3 | `max`/`min` refused the arrays their own ontology advertised | `E_BAD_ARG` | aggregates |
| 4 | `map`/`where` deep-copied the whole pipe per element | O(n²) | linear |
| 5 | Path denylist matched substrings | keys readable, `samples/` unreadable | anchored |
| 6 | `1 / 0` → `inf` exit 0; `1 % 0` panicked the evaluator | silent / crash | structured error |
| 7 | Parse and lexer failures carried no error code | bare prose | `E_PARSE`, retryable |
| 8 | The `sh()` refusal was filed under `E_UNKNOWN` | "no specific error code" | `E_POLICY_DENY` |
| 9 | An array of records rendered as `[{…}, {…}, …]` | no data at all | fields, bounded at depth two |

All nine are fixed, each with a regression test. Defect 9 is the one that
matters most, because it is the only one that made *this document* wrong: it is
described in §4 and below. The suite is **150 binaries, 2,289 tests, 0 failing** on Linux,
and the library still compiles clean for `wasm32-unknown-unknown`. Three
deserve description.

### The quadratic in every pipeline

`call_lambda` saved the environment's pipe input by **cloning** it before each
invocation. In `xs | map(fn(x) => …)` the pipe input *is* `xs`, so the whole
collection was deep-copied once per element.

The giveaway was a closure that reads nothing:

| measurement (Linux, best of 11) | before | after |
| --- | ---: | ---: |
| 500 GitHub records, closure *ignoring* its argument | 246 ms | **9 ms** |
| 500 records, `where` reading one field | 290 ms | **15 ms** |
| `map` over 10,000 integers | 515 ms | **9 ms** |
| `map` over 20,000 integers | 1,801 ms | **14 ms** |

A closure that never touches its argument cannot legitimately cost 237 ms over
500 elements; that is the price of copying something nobody was using. Doubling
N multiplied the time by 3.3–4.2× — the signature of an O(n²) term inside an
O(n) loop. It is now 1.3–1.6×. The fix is `env.take_input()` instead of
`env.input().cloned()`, plus asking the lambda its arity once instead of
calling it speculatively with two arguments and retrying with one.

Nothing in the unit suite could see this: every correctness test used
collections of three or four elements, where a quadratic term is invisible. It
took running the shell against 500 real records, next to `jq` doing the same
job in 1 ms, for the gap to become a number.

The ratchet that guards it (`tests/pipeline_scaling.rs`) was itself wrong on
the first attempt — a fixed threshold that failed inside `cargo test
--workspace --jobs 12` because 146 binaries competing for 24 cores turned a 10×
ratio into 41×. It now calibrates against a closure-free walk of the same two
sizes measured in the same process, so load inflates both sides together. We
verified it still bites by reverting the fix and re-running: **127.5× against
an 8.6× baseline**.

### A denylist wrong in both directions at once

`validate_safe_path` matched each blocked pattern with
`path.to_lowercase().contains(&pattern.to_lowercase())`. One line, two opposite
failures.

**It protected none of the files it named.** The entries `*.key`, `*.pem`,
`*.p12` and `*.pfx` were compared as literal text, and no filename contains the
characters `*.pem`. Measured before the fix:

```
cat server.pem   => 72        # the private key, returned
cat id.key       => 78
cat bundle.p12   => 5
```

Four of the thirteen patterns then in the list — the four aimed squarely at credential
exfiltration by an agent — had no effect whatsoever.

**It blocked ordinary source files.** `SAM`, `SYSTEM`, `SECURITY` and
`SOFTWARE` name Windows registry hives, but as substrings they match any path
containing those letters:

```
cat src/security.rs     => error: path validation failed     # "security"
cat filesystem.rs       => error: path validation failed     # "system"
cat samples/data.txt    => error: path validation failed     # "sam"
```

This repository has a `samples/` directory, so the shell could not read its own
examples. It surfaced because a line count over `src/` succeeded for 33 files
and failed on the 34th.

Patterns are now matched by shape: `*.ext` on the extension, anything with a
separator as a path suffix on component boundaries, a bare name as a whole
final component. `/etc/passwd` is still refused, `server.pem` is now refused,
`src/security.rs` is readable.

We report this at length for one reason. A denylist that is wrong in the
permissive direction is a security defect; one that is wrong in the restrictive
direction is a usability defect. The same line produced both, it shipped, and
what found it was not a security review — it was asking the shell to count some
lines.

### The one that made this document wrong

The other eight defects cost users something. This one cost *us* our headline,
which is a more useful story.

`ls("src") | pick("name", "size")` — the flagship example in the README and in
the book — printed this:

```
[{…}, {…}, {…}, {…}, … forty-five of them]
```

No filenames, no sizes. `pp_item` rendered every nested record as `{…}`,
one level too early. It was not a token budget, though it looked like one:
`[{a: 1}, {a: 2}]` did it too, with two tiny records and nothing to budget.
Volume is already handled by `budget_value` before the value reaches the
renderer, so eliding here bought nothing and cost the answer.

Then the part that matters. §4 originally measured that output at **316 bytes
against bash's 2,650** and reported a 7.9× output advantage — the single most
quotable number in this document. The 316 bytes were forty-five copies of
`{…}`. The harness marked it correct because E2's check was `exit == 0 &&
output is non-empty`, and `benches/agentic/README.md` had a paragraph
*explaining* why E2 needed no oracle. That paragraph was the defect: **an
output-size comparison is meaningless unless something asserts that both
outputs contain the answer.** A smaller output is only better if it is still an
answer, and "smaller" is exactly what a broken renderer produces.

E2 now carries a content oracle — each task names facts its answer must contain
— and that oracle carries a self-check, because the first version of it was
written through a script whose escaping ate every backslash and left patterns
like `/d{4,}/` that could never match anything. Rigour-shaped and useless, in
the code whose entire job was to catch output that was not an answer. The
self-check asserts the patterns accept all three engines' real encodings (raw
bytes, AECON with a factored suffix, nushell's rounded units) and reject an
elided placeholder, an empty string and a bare header row.

Re-run with the renderer fixed and the oracle in place, the advantage is 2.9×
rather than 4.3×. We would rather publish the smaller true number than the
larger one, and the way this was found — the benchmark's own output looked
*too good*, so we opened it — is the only method that reliably catches this
class of mistake.

---

## 8. Conflict of interest, and what would change our mind

**We are not disinterested.** Nervosys builds AetherShell. The same caveat
`crates/agentic-eval/README.md` carries applies here: treat this as a
documented argument to check, not as an independent evaluation. The corpus, the
oracle, the harness and the raw results are in `benches/agentic/`, including
§3, where we come fourth.

**What we did not measure, and what nothing above licenses.** No model was in
the loop. The Microsoft headline (+24.6 pp on TheAgentCompany) is an accuracy
number produced by 174 tasks × 2 frontier models; the Vercel accuracies are
likewise end-to-end. **Nothing here is evidence about either quantity, and we
make no claim about them.** These four experiments characterise the substrate —
per-turn cost, determinism, failure legibility, containment — and a substrate
result does not license an accuracy result. A shell can be cheap per turn and
still lead a model astray.

**`ATTEMPTS` is not a fair fight.** `corpus.mjs` records how many tries each
engine's command took to become correct (AetherShell 18 for ten queries; SQL, jq and
PowerShell 10 each; nushell 16; bash+coreutils 17). The author had written far more
bash and SQL than nushell and wrote AetherShell's standard library. That number
measures one author's fluency crossed with each language, not the languages. It
is published because the *reasons* for our retries were informative — one of them, q8, traces
directly to defect #3 below — not because the count means anything.

**The experiment that would settle this.** Run TheAgentCompany's 174 tasks
under five conditions — Tool-only, Bash, Bash+Tool, PTC, and **AetherShell
under `--agent --workspace`** — with the same two models and the same scoring.
The paper's Bash arm is the control; the question is whether a typed,
effect-gated shell keeps bash's composition advantage while removing the
isolation precondition, and at what cost in task score. That is roughly 1,700
model-task runs and is not something we can fund out of a benchmark harness. We
would help run it, and we would publish the result if AetherShell lost.

Three specific ways we could be shown wrong:

1. **If output tokens don't matter as much as §4 implies.** With prompt caching
   and a model that skims tables well, the 7.9× output difference might not move
   end-to-end cost much. Measurable, and we have not measured it.
2. **If the command-verbosity penalty compounds.** §3 shows AetherShell costs
   more to *write*. Over hundreds of turns, and with more retries from an
   unfamiliar syntax, that could swamp the output saving. Our own `ATTEMPTS`
   column is a weak hint that it might.
3. **If containment is cheaper elsewhere.** If a well-tuned container plus plain
   bash gives equal blast-radius control at no score cost, §6's argument
   evaporates — the gates only earn their place if the paper's 16.5 pp is real
   and architecture-specific.

---

## 9. The claim we will defend

Not "bash is not all you need". Bash is a great deal, and the Microsoft paper
makes the case for it better than we could: composition beats catalogues, and
the shell is the right interface.

The claim is narrower:

> **The shell is the right interface, and untyped output is the wrong default.**
> The composition that makes bash win is orthogonal to whether its values are
> typed, whether its failures carry codes, and whether its effects are gated.
> You can have all four. On the operations that dominate a real agent
> transcript — the ones whose answer is a table rather than a number — typing
> the output is worth 4.3× in tokens, and gating the effects removes the
> precondition on which the paper's own recommendation rests.

And one methodological point, which we hold more firmly than the product claim:
a benchmark whose every answer is a scalar cannot see the difference between
these shells, and most of them are. §3 and §4 differ only in whether the result
has shape, and the ranking inverts completely between them.

---

### Reproducing

```bash
node benches/agentic/prepare.mjs   /tmp/aebench      # 500 issues via gh
node benches/agentic/run.mjs       /tmp/aebench 11   # E1
node benches/agentic/run-shellops.mjs . /tmp/aebench 11   # E2
node benches/agentic/errors.mjs    /tmp/aebench      # E3
node benches/agentic/safety.mjs    /tmp/aebench      # E4
node benches/agentic/report.mjs    /tmp/aebench      # exact BPE
```

Engines whose binary is absent are skipped and named, never silently dropped.
If the `real-tokens` build is unavailable the report says the token counts are
missing rather than substituting a heuristic.

*AetherShell 12.0.2 · AGPL-3.0-or-later · [github.com/nervosys/AetherShell](https://github.com/nervosys/AetherShell)*
