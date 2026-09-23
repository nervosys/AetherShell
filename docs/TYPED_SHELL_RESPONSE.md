# A typed shell is also a shell

**A response to *Is Bash All You Need?* (Mak et al., Microsoft / CMU,
arXiv:2609.11999) and *Testing if bash is all you need* (Vercel, 2026), with
measurements from AetherShell 12.0.2.**

Nervosys builds AetherShell. Everything below is re-runnable from
`benches/agentic/` in this repository, including the results that go against
us, of which there are several, including a headline in §3 that we had to
retract. Read the
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
replication of the Vercel setup, and it is where we did worst.

### How this was measured

Five experiments, all executing real processes on a 24-core Debian (WSL2)
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

## 3. Replicating the Vercel result

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

| Engine | Command tokens | Output tokens | **Total** | Total ms |
| --- | ---: | ---: | ---: | ---: |
| sqlite | 202 | 56 | **258** | 52 |
| **AetherShell `-a`** | 275 | 67 | **342** | 438 |
| bash + jq | 287 | 58 | **345** | 140 |
| nushell | 297 | 56 | **353** | 361 |
| **AetherShell + `sqlite_query`** | 329 | 57 | **386** | 80 |
| AetherShell (default) | 381 | 67 | **448** | 308 |
| PowerShell | 474 | 57 | **531** | 33,995 |
| bash + coreutils | 647 | 57 | **704** | 3,686 |

**As first published, this section reported only the default row, and said
there was no reading of the table in which the typed shell won it.** That was
true of what had been measured and false about the shell: two routes were never
run. `ae -a` is the token-minimised syntax, which could not express six of these
ten queries until its implicit-parameter desugaring was fixed to bind more than
one reference. `sqlite_query` is SQL from inside the shell, which had simply
never been tried here.

With both measured, AetherShell is **second of eight on tokens** (342 against
jq's 345) and **second on latency** (80 ms against bare SQLite's 52). SQLite
still holds both columns outright. The axis is no longer lost; it is not won.

The default row is left in because it is what you get without flags, and
because deleting the number this section was originally wrong about would be
the wrong kind of tidying.

The reason is worth more than the ranking, and it survives the ranking changing.
**Look at the output column.** Every engine lands between 56 and 67 tokens — because every one of these ten
questions has a scalar answer. `10`. `293`. `4.97`. There is no structure in
the result for a typed representation to be efficient *about*. The entire
spread in the total column is command verbosity — which is exactly why a
terser syntax moved us five places and a typed one would not have.
AetherShell's default commands are the most verbose here:

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
| AetherShell | 10/10 | **10/10** | **10/10** | **7/10** | 141 |
| nushell | 10/10 | **10/10** | 7/10 | 0/10 | 294 |
| bash | 10/10 | 0/10 | 1/10 | **5/10** | **42** |
| PowerShell | 8/10 | 0/10 | 1/10 | 0/10 | 114 |

**Correction to this harness, found while re-running it.** `benches/agentic/errors.mjs`
scored an engine that was not installed. `spawnSync` on a missing binary returns
`status: null`, and both derived columns read that as good news — `failed` is
`null !== 0`, and so is `distinct_exit`. Run on a host without PowerShell or
nushell, the table awarded **both of them 10/10 for exit-status granularity**
on a mean of 0 bytes. The row above was measured on a host that had all four,
so it stands; but the harness now probes each engine and skips the absent ones
rather than scoring them. It is worth being plain about the direction of that
bug: it flattered the competition, not us, which is exactly why it survived a
reading of the output. A benchmark is not trustworthy because its numbers
favour the party who wrote it — it is trustworthy because its failure modes
have been looked for in both directions.

(AetherShell's own row moved over this work: **distinct exit statuses went
7/10 to 9/10**, because `validate_safe_path` stopped returning bare prose — a
traversal now exits `EX_NOPERM` and a missing file `EX_NOINPUT`, rather than
both exiting a generic 1. Mean bytes went 141 → 146. That is the free
pre-parse signal this document argues for, earned rather than asserted, and
it came out of fixing errors rather than out of trying to move the score.)

> **The 10/10 did not generalise when we first checked, and that is the part
> worth reading.** Ten failures chosen to be representative prove something
> about ten failures. A benchmark of hand-picked cases flattering the party
> who picked them is the criticism this document makes of other people's
> benchmarks, so we pointed it at ourselves: all 1,052 builtins, each handed
> an argument none of them can accept, every call through `--agent --policy
> strict` in a jail so the effect gate refuses anything dangerous before it
> runs.
>
> **The first sweep found 157 uncoded failures** — 15% of the catalogue
> answering `E_UNKNOWN`, the one code the taxonomy tells an agent *not* to
> reason about. The 10/10 was real and local. Eight passes later:
>
> | Response to a nonsense argument | Builtins |
> | --- | ---: |
> | accepted it and answered | 461 |
> | `E_BAD_ARG` | 285 |
> | `E_NEEDS_APPROVAL` (the gate, correctly) | 142 |
> | `E_BUDGET_EXCEEDED` (`AETHER_MAX_NET=0`, correctly) | 96 |
> | `E_TOOL_MISSING` | 31 |
> | other coded (7 kinds) | 37 |
> | **`E_UNKNOWN`** | **0** |
>
> **The first two findings, and the second corrected the first.** The sweep began at
> 157 uncoded. Of those, 57 turned out not to be argument-handling defects at
> all: the external tool the builtin shells out to was absent on the measuring
> machine, so the probe never reached argument handling. Counting those as
> shell defects would have inflated a claim about AetherShell with a fact about
> one laptop — the same error this document criticises elsewhere, pointed the
> other way.
>
> They were still a defect, just a different one: "the tool is not installed"
> is among the most *identifiable* failures a shell has, and it was being
> reported with the code that means unidentifiable. `E_TOOL_MISSING` now says
> it, exits **127** (bash's "command not found", one level out), reports
> `retryable: false` because no correction to the call will help, and names the
> tool rather than the builtin:
>
> ```
> error[E_TOOL_MISSING]: black: `black` is not installed (No such file or directory)
>   hint: install `black` and run this again; no change to the call will help
> ```
>
> That left sixteen, and **the label on them was wrong**. Five passes of
> mechanical conversion had reduced 157 to 16, and the residue was still being
> described as "the shell's own argument and type errors" because that is what
> the previous 141 had been. Read one at a time — which is the only thing a
> sweep is ultimately for — they were six different conditions:
>
> | builtin | reported | actually |
> | --- | --- | --- |
> | `crypto.verify_signature` | `E_UNKNOWN` | the shell does not implement it |
> | `tx_commit` | `E_UNKNOWN` | no transaction is open |
> | `finetune_status` | `E_UNKNOWN` | that job does not exist |
> | `docker_ps` | `E_UNKNOWN` | docker ran and exited non-zero |
> | `eza` | `E_UNKNOWN` | absent and failed, conflated |
> | `head`, `tail`, `wc` | `E_UNKNOWN` | a genuine argument error |
>
> One of the six was the argument error the label claimed. Two were **worse
> than uncoded**: `crypto.cert_parse` and `crypto.verify_cert` reported
> `E_BAD_ARG`, which tells an agent to retry with different arguments when no
> arguments will ever work — a wrong code is a wrong instruction, where a
> missing one is merely silence.
>
> So the taxonomy grew from ten codes to fourteen: `E_UNIMPLEMENTED`,
> `E_BAD_STATE`, `E_NOT_FOUND`, `E_TOOL_FAILED`. Not from a design review —
> from the sweep repeatedly naming conditions the shell could identify
> perfectly well and was declining to. `E_BAD_STATE` is the interesting one:
> it is the only *retryable* code whose repair is a **different builtin**
> (`tx_begin`, `sso.init`), so its hint names the prerequisite rather than a
> corrected argument.
>
> **And then the largest class turned out to be invisible to all of it.** The
> thirteen that remained were reported as "an external tool absent on this
> host" — a fact about the laptop, not the shell, and therefore excused. Two
> of the thirteen were not that at all: `make_targets` could not find a
> `Makefile` and `ssh_config` could not find `~/.ssh/config`. The classifier
> matched `No such file or directory`, which is a missing *file* as much as a
> missing *program*, and had been quietly moving real defects into the bucket
> it does not score. **A classifier that files defects under its own unscored
> category is the most flattering bug a benchmark can have**, and it is the
> third one this harness has had pointed at itself.
>
> The other eleven shared one shape: `Command::new(prog).output()?`. The `?`
> propagates a bare `io::Error`, so what reached an agent was `No such file or
> directory (os error 2)` — no code, no builtin, **not even the name of the
> program that was missing**. There were **346 such sites**, and thirteen
> builtins were still answering that way *after* `E_TOOL_MISSING` shipped,
> because that code had been added only where the tool was already named. No
> grep for error text could have found them: they have no error text.
>
> `spawn_error` now classifies every one — `NotFound` is `E_TOOL_MISSING`,
> anything else (a permission denial, a broken interpreter line) is
> `E_TOOL_FAILED`, because "install it" is advice that cannot work for a tool
> that is present but unusable.
>
> **Every failure in the catalogue is now coded.**
>
> ```
> uncoded: 0 of 1052
>       0  the shell's own: argument and type errors (0.0%)
>       0  external tool absent on this host
> ```
>
> `E_UNKNOWN` still exists — it is the boundary's guarantee that nothing
> escapes as bare prose — but nothing in the catalogue reaches it.
>
> **And that claim is narrower than it sounds, which we found by testing it.**
> One probe per builtin measures one thing: whether a builtin's *first*
> failure is coded. A builtin that rejects a nonsense argument on type never
> reaches its path handling at all.
>
> `cat("")` found the gap. The sweep probes `cat({unexpected: true})`, which
> fails on the type; an empty string gets past that and into
> `validate_safe_path` — which returned bare prose for **every** refusal,
> including path traversal. The containment boundary, the single most
> important thing for an agent to be able to branch on, was the least legible
> thing the shell said.
>
> So `benches/agentic/uncoded-paths.mjs` is the same question one layer down:
> every builtin handed a well-formed path that does not exist, which is the
> commonest failure any shell has.
>
> | | first failure | second failure |
> | --- | ---: | ---: |
> | uncoded, before | 157 (14.9%) | 54 (5.1%) |
> | uncoded, now | **0** | **0** |
>
> The second number came down 54 → 27 → 15 → 5 → 0, and it took four passes
> for a reason worth stating: each one fixed *the site that produced the
> message*, and the same message was produced from three or four other places.
> `Cannot read file` was three sites, `Plugin not found` four, `no input
> provided` four. Every pass looked complete from the diff and the sweep
> disagreed.
>
> `E_NOT_FOUND` now covers 45 conditions that used to be unknown,
> `E_OUTSIDE_WORKSPACE` finally has a constructor — it was in the taxonomy
> from the start and the one place that detects the condition never used it —
> and `E_IO` names the filesystem failures that are neither absence nor
> refusal.
>
> **Both probes report zero.** That is two independent questions answered, not
> one answered twice — and a third probe did find something, which is the
> point of saying so.
>
> **The failures that exit 0.** `errors.mjs` reports, for every engine,
> "failures that exited 0 — an agent checking status alone sees success". We
> apply that test to other shells and had never applied it to ourselves. Both
> sweeps report a number they pass over without comment: **461 builtins
> accepted the nonsense argument and answered.**
>
> `benches/agentic/silent-success.mjs` asks what they answered *with*. 148 of
> them returned a value carrying no information — `null`, `false`, `[]`, `0`
> — which is where a silent failure hides. Reading them:
>
> ```
> fn bi_zip_create(args, _) -> Result<Value> {
>     let archive = match args.first() {
>         Some(Value::Str(s)) => s.clone(),
>         _ => return Ok(Value::Bool(false)),   // exit 0. No archive.
> ```
>
> That is the same shape as `db_json_to_sqlite("x") -> false, exit 0`, one of
> the six defects that motivated `src/signature.rs` in the first place — still
> present in 173 argument arms. They now refuse.
>
> **And nine builtins were not implemented at all.** `user_lock`,
> `cron_enable`, `acl_set`, `session_undo` and five others were
> `fn(_args, _input) -> Ok(Bool(false))`: they ignored their arguments and
> always answered "no", while the ontology listed them as callable. An agent
> told `false` may retry, route around, or conclude the account was already
> locked. The truth was that the shell does not do it. They now answer
> `E_UNIMPLEMENTED` and say NOTHING WAS CHANGED.
>
> Suspects: **148 → 79**, and the `false` group **65 → 13** — of which five
> (`is_windows`, `is_macos`, `is_bsd`, `is_error`, `env_container`) are
> predicates correctly answering "no" on this host. This probe reports
> suspects, not defects: there is no oracle for "should this have failed?",
> and every hit needs reading. A count of suspects is not a count of bugs,
> and saying otherwise would be the same error as quoting the edit count.
>
> **And a fourth probe, with a real oracle this time.** The table at the top of
> `src/signature.rs` opens with `round(4.966, 2) -> 5` — the digits argument
> accepted and discarded. Declaring a signature fixes that, because `validate`
> checks arity before the body runs. **53 of 1,052 builtins are declared.**
> Nothing had ever checked the other 999.
>
> `benches/agentic/discarded-args.mjs` is differential, so it reports defects
> rather than suspects: if `f(x)` and `f(x, junk, junk, junk)` return the same
> bytes, the extra arguments changed nothing. Nondeterministic builtins are
> excluded by running each twice with identical arguments first, and the
> exclusions are counted — a sweep that silently dropped half the catalogue
> would otherwise look like a clean result.
>
> **369 of 393 comparable builtins (93.9%) returned the same value with three
> extra arguments as without them.** That is the founding defect of this whole
> effort, measured catalogue-wide for the first time, and it is not 53
> builtins' worth of work to fix — it is the migration.
>
> We are reporting that number without having fixed it. The declaration
> mechanism exists and is enforced; extending it to 999 builtins is a body of
> work, and **generating those declarations from this probe would be exactly
> the wrong move** — a builtin may use its third argument only on a branch the
> probe did not take, and declaring it away would delete working calls at
> scale. That failure mode has already happened twice here by hand.
>
> **The probe found something it was not looking for.** Excluding
> nondeterministic builtins meant listing them, and `tools()` was on the list:
> it collected from a `HashMap`, whose iteration order Rust randomises per
> process, so the agent-facing tool catalogue came back in a different order
> every call. It could not be cached, diffed or hashed. Determinism is one of
> the four axes this document argues on, and E1 measured it over *data
> queries* — nobody had pointed it at the discovery surface itself. It is
> sorted now, with a test that checks five rounds and that the order is
> actually sorted, because two unsorted runs can agree by chance.
>
> A further finding came from the sweep's own conduct rather than its results.
> One run was made against an `ae` binary five days older than `src/`, and it
> dutifully reported `head` and `uniq` — both long since fixed — as still
> broken. Nothing in the harness had objected. It now refuses to run against a
> binary older than the sources, because a number measured from the wrong
> build is worse than no number: it is the same error as quoting the edit count
> instead of the sweep, one level further out.
>
> The count came down 157 -> 156 -> 140 -> 98 -> 56 -> 29 -> 13 -> 2 -> 0
> across eight passes. Two lines on method, both unflattering to the method:
>
> **~750 edits moved ~157 builtins.** The last large pass is the clearest case:
> 346 call sites rewritten, **eleven builtins** moved out of the uncoded
> column. Most converted sites sit behind an earlier failure path the probe
> never reaches. Quote what the sweep reports, not the diff.
>
> And every regex pass was quietly incomplete, in a way invisible from the edit
> side: a dot in `a2a.register`, a missing article in `must be integer`, and a
> multi-line `anyhow!` that rustfmt had wrapped — that last one hid 74 sites,
> nearly as many as the pass that found it. Each surfaced only by asking the
> running shell about a builtin that was supposed to be fixed and was not. A
> regex over source is itself the name-based reasoning this work exists to
> remove.
>
> So the honest claim is narrower than the row above: on ten representative
> failures AetherShell codes all ten; across the whole catalogue of 1,052 it
> codes **all of them**. That claim needed eight passes, four new codes and
> three corrections to the measuring instrument to become true, and it was
> false in a flattering direction at every intermediate step — which is the
> only reason worth writing any of this down.
>
> Both numbers are ours and both are reproducible
> (`tests/uncoded_failure_census.rs` holds the line for the core;
> `benches/agentic/` has the sweep). A benchmark of ten hand-picked cases is
> exactly the kind of thing that flatters the party running it, which is the
> criticism this document makes of others.

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

**bash used to win the exit-status column 5/10 to our 0/10**, and that was the
one axis where it offered signal nobody else did: `command not found` exits
127, a syntax error 2. We exited 1 for everything. The taxonomy to do better
already existed in `ErrorCode`; it simply never reached the process boundary.
It does now — 127 and 2 borrowed from bash deliberately, the rest from
`sysexits.h` — and the re-measured score is **7/10**, ahead of bash. That is
the only number in this document that moved because of the document.

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

| Probe | Effect | AetherShell (human) | **`ae agent serve`, no flags** | **AetherShell (agent + jail)** | bash | PowerShell |
| --- | --- | --- | --- | --- | --- | --- |
| write inside workspace | WriteLocal | executed ✓ | executed ✓ | executed ✓ | executed ✓ | executed ✓ |
| write outside workspace | WriteLocal | executed | **contained** | **contained** | executed | executed |
| delete outside workspace | Destructive | executed | **contained** | **contained** | executed | executed |
| truncate file outside | WriteLocal | executed | **contained** | **contained** | executed | executed |
| arbitrary process spawn | Exec | contained | **contained** | **contained** | executed | executed |
| privilege self-grant | Privileged | executed | **contained** | **contained** | — | — |
| network egress past quota | Network | executed | executed | **contained** | executed | executed |
| | | **1/6 contained** | **5/6 contained** | **6/6 contained** | **0/5** | **0/5** |

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
prompt is not treated as an adversary. A shell that refused to write outside its
working directory would not be a shell, and `ae deploy.sh` is a documented
migration path.

*The middle column is the one we got wrong.* "A deployment that forgets the
flags has a shell exactly as unbounded as bash" is what an earlier draft of this
section said, and it was truer than intended: `ae agent serve` — the documented
way for an agent to drive this shell over HTTP — ran the **human** profile
unless the operator also passed `--agent`. The default posture of the
agent-facing surface was the permissive one.

It stayed invisible because a second defect hid it. `POST /api/v1/eval` built
its environment without the module namespaces bound, so `file.write(…)` failed
as a field access on `Null` and every probe came back *contained* — the call had
never run. Fixing the namespaces turned the same probe into a successful write
to an arbitrary path. **Containment that rests on a second bug reads exactly
like containment**, right up until someone fixes the second bug, and the only
reason we know is that E4 scores by consequence: it goes and looks at the file.

Serving an agent now implies agent mode. `ae agent serve` and `ae mcp serve`
default to default-deny with the jail rooted at the server's working directory,
and print which profile they are in at startup. `ae -c` and the REPL are
unchanged. The middle column is that default, measured with no flags at all:
**5/6**, not 6/6 — because network egress is governed by `AETHER_MAX_NET`, which
agent mode does not set, which is the next caveat.

*Network is metered, not denied.* The policy table
(`src/safety.rs`) has `Network → allow` in agent mode; what stopped the egress
probe was the `AETHER_MAX_NET` request-count governor set to zero. A quota
bounds how *many* requests leave, not where they go. For an exfiltration threat
model that is a real limitation, and destination policy is not implemented.

> **And until 2026-09-22 the quota bounded far less than this paragraph
> implied.** `Network` was not in `centrally_enforced`, so the governor charged
> only builtins that called `guard_network` themselves. Of 57 classified
> `Network`, **53 were never charged**. Under `--agent --policy strict` with
> `AETHER_MAX_NET=0`, `scp_upload` invoked `scp` — failing only because the
> local file was absent — while `git_fetch` and `host_lookup` ran. The list
> also held `git_push`, `rsync_sync`, `ssh_tunnel`, `socat_relay` and the whole
> `k8s_*` family: the exfiltration-shaped ones.
>
> The hole was invisible for a reason worth naming: the builtin anyone reaches
> for first is `http_get`, and `http_get` self-guards. The single case a
> reviewer would test was the one that worked. It surfaced only because a
> catalogue-wide sweep flagged `marketplace_search` as never returning, and
> chasing that led here.
>
> `Network` is now centrally enforced — one line — which is safe because
> `Network` decides `Allow` in agent mode, so it adds metering without new
> refusals, and all 19 self-guarding builtins sit in `SELF_GUARDED`, which
> `guard_dispatch` consults first. Verified that it meters rather than
> blanket-denies: at `AETHER_MAX_NET=1` the first call passes and the second is
> refused; at 3, the second passes.
>
> E4's 6/6 stands — its egress probe used `http_get`, which was always charged
> — but it would have missed all 53.

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
make no claim about them.** These five experiments characterise the substrate —
per-turn cost, determinism under a changed environment, failure legibility,
containment — and a substrate
result does not license an accuracy result. A shell can be cheap per turn and
still lead a model astray.

**`ATTEMPTS` is not a fair fight.** `corpus.mjs` records how many tries each
engine's command took to become correct (AetherShell 18 for ten queries; SQL, jq and
PowerShell 10 each; nushell 16; bash+coreutils 17). The author had written far more
bash and SQL than nushell and wrote AetherShell's standard library. That number
measures one author's fluency crossed with each language, not the languages. It
is published because the *reasons* for our retries were informative — one of
them, q8, traces directly to defect #3 in §7 — not because the count means
anything.

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
   and a model that skims tables well, the 4.2× output difference might not move
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
> the output is worth 2.9× in tokens, and gating the effects removes the
> precondition on which the paper's own recommendation rests.

And one methodological point, which we hold more firmly than the product claim:
a benchmark whose every answer is a scalar cannot see the difference between
these shells, and most of them are. §3 and §4 differ only in whether the result
has shape, and the ranking inverts completely between them.

---

## 10. Where this sits against the published benchmarks

Two questions follow from the above, and they have different answers. *Which
agentic benchmarks run from a shell?* Several. *Can they be pointed at
AetherShell today?* No — and the measurement of why is the useful part.

### The benchmark you probably mean is not the one at that address

`assistantbenchmark.com` is a consumer-assistant review site: 115 assistants
scored 1–10 by hand across 15 dimensions after real use, 215 tasks, a running
mean, and a leaderboard led by Muse (9.1), Instinct (8.4) and szn (8.0). It
publishes no dataset and no harness, the scoring is human judgement rather than
a programmatic checker, and nothing about it runs from a shell. It cannot be
used to evaluate a shell, by us or anyone.

**AssistantBench** — no relation — is the academic benchmark of that name: 214
realistic web tasks over 525+ pages from 258 websites. It has a harness, but the
action space is a browser, so a shell is not the thing under test there either.

The distinction matters because the two are a search result apart, and one of
them looks like a leaderboard we could enter.

### Which of the compendium's benchmarks are shell-native

Of the ~50 benchmarks in [philschmid's
compendium](https://github.com/philschmid/ai-agent-benchmark-compendium), most
drive an agent through a function-calling API (BFCL, ToolBench, τ-Bench,
API-Bank, ComplexFuncBench, the MCP suites) or a browser or GUI (WebArena,
Mind2Web, OSWorld, AndroidWorld, WorldGUI, macOSWorld). Neither family
exercises a shell.

The ones where the agent's action space *is* shell commands:

| Benchmark | Shell surface | Retargetable to AetherShell? |
| --- | --- | --- |
| **AgentBench** (OS environment) | commands executed in an Ubuntu Docker container | not today — see below |
| **SWE-bench** / Verified / Pro / PolyBench | agent edits a repo and runs tests through a shell in a container | not today |
| **SWE-agent** | action space "governed by a single `yaml` file" | the most promising route; untested |
| **OSWorld** (terminal tasks) | a real OS, terminal among other surfaces | partial at best |
| **Aider** benchmarks, **LiveCodeBench** | edit-and-run loops, shell-mediated | partial |

We have run none of them. Each needs model inference across hundreds of tasks —
SWE-bench Verified alone is 500 — and that is a budget question, not a
harness question. Section 8 states what the decisive experiment would cost.

### Why the cheap route is closed: 2 of 32

Retargeting a shell-native harness looks like it should be free, because the
repository advertises "Bash compatibility (via transpiler)" and every one of
those harnesses drives its agent with a string of shell. So we measured it
(`benches/agentic/bashcompat.mjs`): 32 ordinary commands of the kind an agent
emits on a SWE-bench-shaped task — orient, search, read, edit, test, inspect
git — through `ae -b`.

| Outcome | Count | |
| --- | ---: | --- |
| **native** | **2** | AetherShell ran it and matched bash |
| delegated | 10 | handed to `bash -lc`; bash did the work |
| refused | 3 | blocked by the `sh()` gate in the default posture |
| failed | 17 | did not run, or gave a different answer |

The delegated column is the one to read twice. Those ten did not demonstrate
compatibility; they demonstrated that the transpiler's fallback is
`sh(["bash","-lc", …])`. bash ran them. And because the effect gate sees one
`sh` call rather than the fifty things the script did, **every containment
property in section 6 is bypassed on exactly that path** — which is why the
default posture refuses it, and why turning `AETHER_ALLOW_SH=true` on to make
the compatibility work would trade away the argument section 6 makes.

So the feature claim and the safety claim are in direct tension, and the
tension is structural: the compatibility is implemented *by delegating to the
shell we are arguing against*. A benchmark retargeted onto that path would be
measuring bash in a costume.

Two defects surfaced while measuring this, both now fixed. `echo $HOME`
returned `null` at exit 0 — every environment-variable expansion did, because
the transpiler emitted a bare identifier and an unbound identifier evaluates to
null rather than raising. Underneath it, `env(name, default)` accepted a
default and discarded it, the same shape as defect 1 in section 7. A confident
wrong answer for the most common expansion in shell.

**What this means for the claim in section 9.** It narrows it. A typed shell is
a better substrate per turn, on the evidence above; but "you can have all four"
is a statement about a shell an agent is *prompted for*, not a drop-in for one
it already knows. Retargeting SWE-agent by rewriting its YAML action space and
its prompt is a real route and we have not walked it. Claiming bash
compatibility as though it were one is not, and the README has been corrected.

### Shells ranked, with the sources of every score

`crates/agentic-eval` now carries a `shells` module scoring six shells on the
crate's four axes, with each axis pinned to one of the runs above rather than
to a judgement:

```
cargo run -p agentic-eval --example shell_benchmark
```

| Shell | Fitness | Tokens | Determinism | Reliability | Safety | Basis |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| aethershell `--agent` | **0.82** | 0.85 | 0.90 | 0.75 | 0.80 | executed |
| aethershell (default) | 0.57 | 0.50 | 0.90 | 0.75 | 0.15 | executed |
| powershell | 0.45 | 0.60 | 0.90 | 0.25 | 0.05 | executed |
| nushell | 0.41 | 0.40 | 0.50 | 0.70 | 0.05 | executed |
| fish | 0.34 | 0.35 | 0.55 | 0.40 | 0.05 | **analogy to bash** |
| zsh | 0.33 | 0.35 | 0.55 | 0.38 | 0.05 | **analogy to bash** |
| bash | 0.33 | 0.35 | 0.55 | 0.35 | 0.05 | executed |

Read this table with three things in hand. **We build two of these rows**, and a
curated composite is exactly where that shows; the axes are anchored, the
weighting into a single number is not. **zsh and fish were never executed** —
their profiles are inherited from bash, which is an assumption the module marks
in its output and a test enforces, and which is weaker for fish, since fish
deliberately breaks POSIX compatibility. And **the composite hides the
inversion that section 4 is about**: on scalar answers AetherShell places fourth
of six, and no single fitness number will tell you that.

The row that most deserves attention is nushell's. It reached a complete error
taxonomy — 10/10 machine-readable codes — before AetherShell did, and we only
matched it by fixing our own gap after measuring. It loses the composite on
tokens and determinism, not on the axis it leads.

---

### Reproducing

```bash
node benches/agentic/prepare.mjs   /tmp/aebench      # 500 issues via gh
node benches/agentic/run.mjs       /tmp/aebench 11   # E1
node benches/agentic/run-shellops.mjs . /tmp/aebench 11   # E2
node benches/agentic/errors.mjs    /tmp/aebench      # E3
node benches/agentic/safety.mjs    /tmp/aebench      # E4
node benches/agentic/environment.mjs .               # E5, against this repo
node benches/agentic/report.mjs    /tmp/aebench      # exact BPE
```

Engines whose binary is absent are skipped and named, never silently dropped.
If the `real-tokens` build is unavailable the report says the token counts are
missing rather than substituting a heuristic.

*AetherShell 12.0.2 · AGPL-3.0-or-later · [github.com/nervosys/AetherShell](https://github.com/nervosys/AetherShell)*
