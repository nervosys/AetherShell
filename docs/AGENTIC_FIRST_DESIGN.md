# AetherShell — Agentic-First Design

> **Goal:** make AetherShell the shell with the best token-efficiency, reliability,
> features, and safety for AI agents in the world — without sacrificing the
> human experience.
>
> **Status:** design + roadmap. **Phase 3 (safety core) — first vertical slice landed**
> (see §13). **Date:** 2026-05-28. **Baseline:** v1.2.0.

---

## 0. Thesis: two surfaces, two objective functions, one core

AetherShell is consumed by two very different operators with *different cost
functions*, and the current design tries to serve both with one knob (the `.aeg`
transpiler). That is the root mistake to correct.

| Surface | Operator | Optimize for | Tolerates |
| --- | --- | --- | --- |
| **`.ae` (human surface)** | A person at a REPL / writing scripts | **Reliability & legibility** — readable, unambiguous, great errors, deterministic | More keystrokes |
| **Agent surface** | An LLM over the API / `.aeg` / MCP | **Token-efficiency & safety** — minimal total tokens per *successful* task, provably bounded blast radius | Terseness that's hard for humans to author by hand |

Both surfaces compile to **one shared core**: lexer → AST → evaluator → typed
`Value` → ontology. The core owns the things that must be identical for both —
deterministic typed output, structured errors, the effect/safety model, and the
introspection ontology. The surfaces differ only in *syntax* and *defaults*.

```
        ┌────────────── human .ae syntax ──────────────┐   objective: reliability
        │   readable names · grammar-level parsing      │
        └───────────────────────┬───────────────────────┘
                                 │
   ┌─────────────────────────────┴─────────────────────────────┐
   │                       SHARED CORE                           │
   │  AST · Evaluator · typed Value · deterministic output       │
   │  structured errors · effect taxonomy · policy/approval      │
   │  audit · ontology / schema export                           │
   └─────────────────────────────┬─────────────────────────────┘
                                 │
        ┌───────────────────────┴───────────────────────┐   objective: tokens + safety
        │  agent surface: terse syntax (TBD by benchmark) │
        │  + token-budgeted output + default-deny policy  │
        └─────────────────────────────────────────────────┘
```

The current `.aeg` transpiler (`src/transpile/agentic.rs`) is the *seed* of the
agent surface — but its compression strategy is unproven and its implementation
is a reliability liability (see §2.2). We will **measure before we commit** (§4).

---

## 1. What we keep (current strengths)

These are genuine assets and the design builds on them, not over them:

- **Typed `Value` system** (`src/value.rs:98`) — 15 variants incl. `Record`
  (BTreeMap, sorted), `Table`, `Error(String)`, plus `to_json`/`from_json`
  (`:255`). Deterministic-by-construction key ordering already exists.
- **Ontology & multi-provider schema export** (`src/agent_api.rs`) —
  `LanguageOntology` (`:189`), `BuiltinDefinition` (`:242`) with signature /
  params / return-type / examples / json_schema, and 26 provider formats incl. a
  compact `Ontology` format. This is the discovery substrate agents need.
- **1,100+ builtins, O(1) dispatch** — `BUILTIN_LOOKUP` (`src/builtins.rs:41`) →
  `BUILTIN_DISPATCH` function-pointer table.
- **HM type inference** (`src/typecheck.rs`, `src/types.rs:10`) — static analysis
  ready to be promoted to a boundary validator.
- **Security primitives that work** — prompt-injection defense, path-traversal
  prevention (`validate_write_path`, `src/security.rs:389`), command allowlisting
  (`validate_command`, `:480`), structured `SecurityAuditEvent` (`:24`), error
  sanitization (`sanitize_error_message`, `:1209`), and a complete-but-dormant
  **RBAC** module (`src/auth.rs`).
- **Protocol breadth** — MCP client, A2A, A2UI (`a2ui.confirm` already exists),
  NANDA; SSE streaming for AI chat (`src/ai_api/server.rs`).

---

## 2. What's wrong today (gap analysis)

### 2.1 Token efficiency optimizes the wrong axis

The `.aeg` syntax compresses **input characters** via a memorized cipher: 26
single-letter builtins (`a`=all, `b`=flatten, `c`=cat…), 92 module sigils
(`F`=file, `DK`=docker…), 152 function abbreviations (`file.r`→`file.read`).

But an agent's cost per task is:

```
total_tokens = standing_context        (teach the model the surface — paid every context)
             + input_tokens            (what the agent writes)      ← .aeg targets only this
             + output_tokens           (what the agent reads back)  ← usually the largest
             + retry_tokens            (re-do failed cycles)        ← ambiguity amplifies this
```

Three problems:

1. **Input is the smallest lever.** Modern BPE tokenizers often encode `map`,
   `read`, `get`, `where`, `file` as *single tokens*. `m`, `F.r`, `w~` save
   *characters*, not necessarily *tokens*. The advertised "60–70%" is a character
   ratio, not a token ratio.
2. **The cipher inflates standing context.** To emit valid `.aeg` the model must
   carry the BUILTIN_SHORT / MODULE_MAP / FUNC_ABBREV tables in context (the
   `AGENTS.md` dump is multiple KB). That fixed tax is re-sent every turn.
3. **The cipher inflates retries.** `b`→flatten is not a mapping the model has a
   prior for; it guesses, mis-emits, gets an unstructured error, and retries.
   Ambiguous, order-dependent expansion (§2.2) makes failures worse.

Meanwhile the dominant cost — **output** — is untouched: no token accounting, no
output budget, full table serialization, no elision-with-summary
(explore confirms: no `tokens_in/out`, no cost tracking, `ai_cost()` is a stub).

> **We will not assume legible beats cipher — we will measure it (§4).** The point
> is only that the *current* design optimizes the provably-smallest term and
> worsens the two largest.

### 2.2 Reliability: the transpiler is fragile by construction

`transpile_agentic_to_ae` runs **~10 ordered text passes** over raw strings, with
21 documented "conflict rules" (R01–R21) and ~18 overloaded reserved characters
(`T N ~ ^ ? ! $ % ; ' \`` …). Each pass re-implements string-literal skipping by
hand. Correctness depends on *pass ordering* and hand-rolled char scanning — a
class of bug that is invisible until a specific input hits it. This is the
opposite of the "deterministic" promise.

Other reliability gaps:

- **Types not enforced at runtime.** HM inference is static-only; a wrong-typed
  arg fails deep inside a builtin with an ad-hoc message, not at the boundary.
- **Float/serialization determinism not guaranteed.** Column widths sampled from
  50 rows (`src/value.rs:375`); float formatting uses default `to_string`.
- **Errors are strings.** `Value::Error(String)` and `anyhow!` give prose, not a
  stable `code` + machine-actionable `hint` — so agents can't self-correct.

### 2.3 Safety: the largest gap to "safest shell in the world"

| Operation | Path check | Confirm | Audit | Dry-run | Policy |
| --- | --- | --- | --- | --- | --- |
| `sh()` exec | — | — | eprintln only | — | `AETHER_ALLOW_SH` (binary) |
| `rm`/`rmdir` (`builtins.rs:34651`) | **none** | none | none | none | none |
| `proc.kill` (`:15880`) | n/a | none | none | none | none |
| `db.delete`, `docker.rm` | none | none | none | none | none |
| `file.write` | yes | none | none | none | none |

- **No effect model.** Nothing in dispatch knows that `rm` is destructive and
  `len` is pure. Safety can't be reasoned about because the property isn't
  represented.
- **RBAC is dormant** — `src/auth.rs` exists but is not wired into execution.
- **Approval is ad-hoc** — `input.confirm`/`a2ui.confirm` exist but are never
  *required*; nothing forces a destructive op through them.
- **Audit is partial & in-memory** — lost on exit, not tamper-evident, doesn't
  cover most effecting builtins.
- **No workspace jail, no resource governors** on the general path (only agent
  planning is rate-limited).

---

## 3. Design principles

1. **Two surfaces, one core.** Syntax and defaults diverge; semantics, output,
   errors, and the safety model are shared and identical.
2. **Measure compression; don't fold it.** The agent surface's terseness strategy
   is an empirical question with a benchmark gate (§4).
3. **Optimize total tokens per *successful* task**, not characters per call.
   Spend on output economy and retry-elimination before input golf.
4. **Safety is a property of the *value*, not the syntax.** Effects are typed and
   travel with the builtin, so any surface (REPL, API, MCP, `.aeg`) is equally
   safe.
5. **Default-deny for the dangerous classes in agent mode; default-allow with
   great errors for humans.** Same engine, different policy.
6. **Every refusal is actionable.** A blocked or failed call returns a structured
   error with a `hint` and (if applicable) an approval path — never a dead end.
7. **Determinism is a contract**, byte-for-byte, across OS/locale.

---

## 4. The measurement-first gate (resolves the syntax question)

Before committing the agent surface's compression strategy, build a benchmark and
let it decide. **This is Phase 1 and blocks Phase 2 syntax work.**

> **STATUS: measured & finalized with a real tokenizer.** `examples/token_bench.rs`
> counts tokens with a labeled heuristic by default, or the **real GPT-4 cl100k BPE**
> (embedded `tiktoken-rs`) under `--features bench-tokenizer`. The real-tokenizer
> run (§4.0) authoritatively confirms **legible-first**.

### 4.0 Measured results (10-task corpus)

Token numbers below are from a **real GPT-4 tokenizer** (cl100k_base via
`tiktoken-rs`, vocab embedded in-crate; run `cargo run --example token_bench
--features bench-tokenizer`). Char / standing-context / reliability are exact.

| Metric | Cipher (`.aeg`) | Legible | Note |
| --- | --- | --- | --- |
| Input, chars | 272 | 492 | 45% char savings (not the advertised 60–70%) |
| Input, **real cl100k tokens** | 135 | 184 | **only 27% token savings** — and *0%* on several tasks (`read file` 6/6, `sys host echo` 9/9): legible names tokenize as cheaply as the cipher |
| **Standing context, real tokens** | **4 695** | ~400 | real cl100k of `describe_ontology()` vs. a module index — **~11×** |
| §4 total over 30 turns | **8 880** | **5 920** | standing + input·turns + retry proxy |
| Reliability failures | 1/10 | 1/10 | the `json.parse(_)` pipe idiom fails to parse in *both* forms — symmetric, doesn't bias the verdict |

**Verdict (now real-tokenizer authoritative): LEGIBLE wins.** The cipher's ~11×
standing-context tax dwarfs its per-line input savings, and the "60–70%" figure is
a *character* ratio — the *token* saving is only 27% (zero on several tasks,
because BPE encodes `file.read`/`sys.hostname` as cheaply as `F.r`/`S.h()`) and
goes net-negative once standing context counts. Input is the smallest of the three
live cost terms.
→ **Commit the agent surface to legible-first; keep `.aeg` cipher as opt-in
legacy; invest the token budget in output economy (§6.2) + progressive ontology
disclosure (§5.4).** ✅ Done — exactly the work carried out in §5–§9.

### 4.0b Cross-shell comparison (`examples/shell_bench.rs`)

An agent pays tokens for the **command** it writes *and* the **output it reads
back**. Traditional shells return verbose, non-deterministic text; AetherShell
returns compact typed AECON. Measured with the real cl100k tokenizer
(`cargo run --example shell_bench --features real-tokens`) over 4 representative
tasks (list files, processes, JSON field, disk usage), counting command + output:

| Shell | cmd tok | output tok | total | vs AetherShell |
| --- | --- | --- | --- | --- |
| **aethershell** | 48 | 70 | **118** | 1.00× |
| bash / zsh / fish | 28 | 303 | 331 | 2.81× |
| nushell | 34 | 341 | 375 | 3.18× |
| powershell | 55 | 105 | 160 | 1.36× |

After tightening AECON to a bare TSV header (dropping the `@aecon rows=N cols=`
prefix), the small-result totals are **2.8× cheaper than the POSIX shells, 3.2×
vs Nushell, and 1.36× vs PowerShell** (scalar results at parity).

**At realistic scale** (`scale_comparison`, a 50-row listing rendered with the
*real* `aecon`): `name` (shared path prefix factored to one `@prefix` line) and
variable-width `size`, constant `owner`/`group` (factored to one `@const` line), and
a low-cardinality `perm` column with 3 distinct values as real listings have
(dictionary-encoded to one `@dict` line, referenced per-row by a 1-token index):

| Output format (50 rows) | tokens | vs AECON |
| --- | --- | --- |
| **aethershell (aecon)** | 470 | 1.00× |
| powershell `Format-Table` | 821 | 1.75× |
| powershell `ConvertTo-Json` | 1447 | **3.08×** |
| nushell (boxed table) | 1402 | 2.98× |

Honest reading (not rigged): the ratio vs PowerShell depends entirely on which
output an agent parses, so we report the full spread rather than a single headline.
`Format-Table` is display-only (variable widths, truncation, culture-dependent) and
**not reliably parseable** — so a parsing agent uses `ConvertTo-Json`. Against its
**default** (pretty) `ConvertTo-Json` — the idiomatic form an agent gets without
flags — AECON is **~3.1×** cheaper on this constant-heavy listing (2.4–3× on a plain
`name,size` listing as rows grow). Against the hand-compacted `ConvertTo-Json
-Compress` the edge narrows to **~1.6×**, and against the (unparseable) `Format-Table`
it's only **~1.4×**. The ≥2× claim is true and measured, but specifically against the
default JSON serialization — which is the honest comparison since AECON is also
AetherShell's *default* output. Four structural levers, all deterministic and all
applied only where they're a measured *token* win:
**constant-factoring** (cardinality-1 columns → one `@const` line);
**dictionary encoding** (low-cardinality, multi-token string columns → one
`@dict` line + 1-token indices); **delta encoding** (large-valued,
slowly-varying integer columns such as timestamps/sequential ids → a `@delta:`
line + per-row differences, where a 10-digit absolute ≈4 tokens collapses to a
~1-token step); and **common-prefix factoring** (string columns whose values share
a leading run — paths, URIs, prefixed ids → one `@prefix col: …` line, the prefix
stripped from every row; 44–69% fewer tokens on path-heavy listings). Each is gated
so a cheap form can only ever *replace* an expensive
one — delta, for instance, fires only when raws average ≥4 digits (genuinely
multi-token) and the deltas are under half the raw width, so small or oscillating
integers are left literal. The levers compound on this harder listing: dropping the
dict lever alone (the literal `perm` string ~5 tok × 50 rows ≈ +185 tokens) would
lift AECON from 470 to ~655 and narrow the default-`ConvertTo-Json` ratio from ~3.1×
to ~2.2× — still a clear win. In every case AECON emits each column name once where
JSON repeats every key on every row,
and AECON is deterministic where the shell text formats are not.

### 4.0c Four-axis evaluation, applied to the real engine (`agentic-eval`)

The measure-first principle is now generalized into a **standalone, reusable
library** — `crates/agentic-eval` — that scores any program for agentic use across
all four axes this design optimizes, and it is **applied to AetherShell's shipped
engine** (`examples/agentic_eval.rs` + `tests/agentic_eval_integration.rs`):

- **Token efficiency** — AetherShell's own `est_token_count` (exact cl100k under
  `--features real-tokens`) fills the library's `AgentCost` for the legible `.ae`
  surface vs. the `.aeg` cipher, charging the cipher its real `describe_ontology`
  standing-context tax. Over a 30-turn session the legible form wins
  (~1.1k vs ~6.1k tokens) — the cipher's standing tax dwarfs its per-line input edge.
- **Determinism** — the library runs AetherShell's `render_canonical` repeatedly and
  confirms byte-stable output (sorted keys, shortest-round-trip floats):
  `{ b: 2.0, a: 1, items: [3,1,2] }` → `{"a":1,"b":2,"items":[3,1,2]}`, identical
  across runs.
- **Reliability** — representative programs are run through the real parser+evaluator
  and classified; a wrong-typed arg (`env(123)`) surfaces as a structured,
  branchable `E_BAD_ARG` (an *actionable* failure, not a dead end).
- **Safety** — AetherShell's `safety::effect_of(builtin)` is mapped into the library's
  effect taxonomy and scored under the agent policy: every dangerous builtin
  (`rm`/`sh`/`proc_kill`) is approval-gated, so the blast radius is **bounded (grade
  A)**.

The library is execution-agnostic (token efficiency on text; determinism/reliability
via a caller closure; safety via declared effects), depends on nothing heavy by
default, and counts tokens for OpenAI GPT-4/GPT-4o exactly (and a documented Claude
approximation). So the agentic-first claims are not just asserted — they are
re-measurable against the real engine by an independent, reusable tool.

### 4.1 Harness

A new crate-internal tool `ae bench tokens` (or `cargo run -p ae-bench`) that, for
a fixed corpus of **real agent task transcripts** (10–20 multi-step tasks: file
edits, pipelines, http+json, container ops, error-and-retry loops), renders each
task three ways and counts tokens with a real tokenizer:

- **Variant A — cipher:** current `.aeg`.
- **Variant B — legible:** readable names + the genuinely-useful terse forms
  (native `|` pipe, `|.field` projection, `~`/`fn` lambdas) but no
  cipher tables.
- **Variant C — hybrid:** legible names + a *small, derivable* set of aliases
  (e.g. only the ~12 pipeline verbs agents use constantly), no module sigils.

Tokenize with `tiktoken` (cl100k/o200k) **and** a Claude tokenizer, since the
answer may be tokenizer-dependent. Count all four cost terms:

- `standing_context`: size of the minimum schema/cheatsheet the model needs to
  emit each variant correctly (the cipher's tables vs. legible's near-zero).
- `input_tokens`: the commands themselves.
- `output_tokens`: held constant (same results) — *control*, not a variable.
- `retry_tokens`: estimated by replaying each variant through the actual
  transpiler/parser and counting parse/expansion failures on a held-out set of
  model-generated samples.

### 4.2 Decision criterion

Commit to whichever variant minimizes **`standing_context_amortized +
input_tokens + retry_tokens`** across the corpus, where `standing_context` is
amortized over a representative session length (e.g. 30 turns). Publish the table
in `docs/` so the choice is auditable and re-runnable as tokenizers evolve.

> Hypothesis to test (not assume): **B or C wins** because the cipher's
> standing-context and retry terms dominate its small input win. If A wins on some
> tokenizer, we keep it for that provider's schema export. The harness makes this
> a fact, not an argument.

> **STATUS: started.** Promoted from the transpiler's text passes into the real
> grammar (`tests/grammar.rs`): the `|.field` projection (`parser::parse_pipe`),
> SI numeric suffixes `1k`/`1M`/`1G` (lexer `read_number`), the `~x: body`
> lambda (`Tok::Tilde` + `parse_tilde_lambda`, body bounded at logic-or), and the
> `?scrutinee { arms }` match prefix (`Tok::Question` → `parse_match`), and a real
> `if cond { then } else { else }` expression (`parse_if_expr`, desugars to a
> boolean match — the legible conditional, chosen over the `^` cipher since `^` is
> already `Pow`). The implicit `~.field` cipher stays in the `.aeg` transpiler —
> the legible grammar uses an explicit parameter.

### 4.3 Regardless of winner: move terseness into the grammar

Whatever syntax wins, the agent surface must be parsed by the **real
lexer/parser** as grammar productions — *not* by string-substitution passes. Kill
the 10-pass pipeline (§2.2). Terse tokens the parser understands cannot be
mis-expanded by pass ordering. The `.aeg` transpiler becomes, at most, a thin
legacy shim that emits canonical AST.

> **STATUS (Phase 5 — Stage 1 landed).** The transpiler's integration tests
> (`tests/transpile_agentic.rs`, 121 tests) are now **behavior assertions** — they
> eval the transpiled `.ae` (for self-contained forms) or assert the
> spacing-tolerant semantic mapping (for external/IO forms), instead of checking
> the transpiler's exact internal text. This decouples the tests from the
> transpiler's representation so the pipeline can be replaced without rewriting
> every test.
>
> **Re-assessment of Stage 2 (single-pass rewrite).** Reading the 10 passes in
> full shows they are **essentially order-dependent**, not accidentally so:
> `for_each` must precede lambda expansion (it consumes the `~x:body`); module
> sigils → func abbreviations → auto-parens form a dependent chain; builtin
> shorthands precede pipeline normalization; etc. A single-pass rewrite must encode
> the *same* ordering, so it is a reorganization rather than a simplification — the
> hoped-for "no pass-ordering bugs" benefit is muted. Each pass also *already*
> skips string literals, so the main "rewrite-fired-inside-a-string" hazard is
> handled today, and the 121-test suite shows no ordering bug currently manifests.
> Combined with the **measure-first verdict** (legible wins; the cipher is opt-in
> legacy), the full from-scratch rewrite is **high-risk, large-effort, low-value on
> a deprecated surface**. Recommendation: treat Stage 1 as the Phase 5 increment
> that lands; defer the from-scratch single-pass rewrite unless the transpiler's
> maintenance cost actually bites.
>
> **PHASE 5 COMPLETE (done anyway, per explicit decision).** The 10-pass pipeline
> is retired. `transpile_line` is now a **single left-to-right `scan`** (plus two
> statement-prefix handlers, `try_for_each`/`try_assignment`) that protects
> string/backtick literals inline and maps each cipher form directly to legible
> `.ae`, recursing into nested constructs — a terse token can no longer be
> mis-expanded by pass ordering. All 14 `expand_*`/`preprocess_ultra` functions
> (~2,000 lines) were deleted and their internals-testing unit tests replaced with
> a slim end-to-end + ontology-integrity suite. Two historical ordering quirks are
> matched deliberately: a zero-arg bare builtin fires only right after an explicit
> `|` (`data|b`→`flatten()`, but `a > b`→`a | b`), and a bare builtin letter that
> is a variable inside a recursively-scanned body stays literal (`^cond{x}` keeps
> `x`, not the `sh` shorthand). Verified: agentic integration 121/121, lib 342/0,
> production build warning-clean.

---

## 5. Shared core (serves both surfaces)

> **STATUS:** the `canonical(value)` builtin (dispatch 1113) implements this
> contract — sorted keys, shortest round-trip locale-independent floats, explicit
> `null` for non-finite/non-serializable values, correct escaping. Tests assert
> byte-stability and insertion-order independence (`tests/reliability.rs`). The
> lossless counterpart to `aecon` (§6.2). ✅ **Wired as the default render under
> `--deterministic` / `AE_DETERMINISTIC`** (`repl::run_one` → `render_canonical`),
> taking precedence over both the agent AECON renderer and the human pretty-printer
> — the whole value is emitted as canonical JSON (budget intentionally not applied,
> since reproducibility wants the full result). `deterministic_mode_renders_canonical_json`
> covers it (`tests/output_economy.rs`).

### 5.1 Deterministic output contract

Guarantee byte-identical output for identical values across OS/locale:

- Records/Tables already sort keys (BTreeMap) — keep.
- **Floats:** shortest round-trip formatting (Ryū / Grisu), locale-independent,
  fixed `NaN`/`inf` spellings.
- **Tables:** column widths computed from the *full* dataset in deterministic
  mode, or — preferred for agents — emitted in the compact format (§6.2) where
  width is irrelevant.
- A `--deterministic` flag (default **on** in agent mode) disables all
  color/locale/terminal-width influence.

### 5.2 Structured errors (kills retry-token blowup)

Upgrade `Value::Error(String)` to a structured record that is itself a first-class
`Value`:

```jsonc
{ "error": {
    "code": "E_NEEDS_APPROVAL",      // stable enum, see §7.3
    "message": "rm would delete 412 files",
    "builtin": "rm",
    "arg": "path",
    "hint": "re-call with approval token, or narrow the path",
    "retryable": true,
    "approval": { "token": "apv_…", "descriptor": { … } }  // when applicable
} }
```

Stable `code` values let the agent branch without parsing prose; `hint` tells it
*how* to fix the call. This is the single highest-leverage reliability +
token-efficiency change after output budgeting.

✅ **Structured by default, not opt-in.** The two structured-error helpers
(`safety::bad_arg`, `safety::arg_err`) cover ~520 call sites in `builtins.rs`, but
~879 `anyhow!`/`bail!` sites remained, and *those* failures reached the agent as
prose with nothing to branch on. Adoption per-call-site had stalled, so the
guarantee is now enforced at the boundary instead: `builtins::call_with_input`
passes every error through `safety::ensure_structured`, which

- leaves an error that already carries a specific code **untouched** (a real code
  is strictly better information than a generic one), and
- wraps anything else as `E_UNKNOWN` with the original message preserved verbatim
  and `retryable: false` — an agent that cannot identify the fault should stop, not
  re-run the same call and spend its envelope discovering the same thing.

The same boundary fills in the `builtin` field when a helper left it empty.
`arg_err` — the most-used helper by an order of magnitude — takes only a message,
so its `builtin` was blank at ~490 sites; the call site is the one place that knows
the name. Without this, `diagnose` could not look up a signature for the *majority*
of `E_BAD_ARG` failures, which is exactly the population it exists to serve. Found
by writing the test first and watching it fail.

Codes: `E_POLICY_DENY`, `E_NEEDS_APPROVAL`, `E_OUTSIDE_WORKSPACE`, `E_BAD_ARG`,
`E_BUDGET_EXCEEDED`, `E_UNKNOWN_BUILTIN`, `E_UNKNOWN`. `ErrorCode::retryable()`
is the single definition of which of those a repair loop should act on.

✅ **`did_you_mean` on an unknown name.** `E_UNKNOWN_BUILTIN` carries up to three
nearest real names (bounded Levenshtein over the live `BUILTIN_LOOKUP` table, edit
budget scaled by name length, ties broken by name so the ordering is stable across
runs). Two properties matter more than the matching itself: the candidates are
drawn from the live table, so they cannot drift as builtins are added; and when
nothing is within budget the field is **omitted entirely**. The previous
suggester (`shell_features::suggest_similar_commands`) searched a hardcoded list of
16 names out of 1,100+ and fell back to `"ls, cat, grep"` — a confident wrong answer,
which costs a retrying agent a whole round trip to learn nothing.

### 5.3 Effect taxonomy on every builtin

Tag each of the 1,100+ builtins (via a `#[builtin(effect = …)]` attribute macro or
a side table keyed by `BUILTIN_LOOKUP` index) with one effect class:

`Pure` · `ReadLocal` · `WriteLocal` · `Destructive` · `Process` · `Network` ·
`Exec` (shell passthrough) · `Privileged`

This metadata is consumed by (a) the policy engine (§7), (b) the ontology (so
agents *see* which calls are dangerous before calling them), and (c) the audit
log. It is the representational change that makes safety *reasoned about* rather
than bolted on per-builtin.

> **STATUS: implemented.** `ontology_manifest()` returns the cheap root (one
> entry per category: count + effect classes + legend + hint);
> `ontology_describe("<category>"|"<builtin>")` expands one slice on demand
> (dispatch 1111/1112). A test asserts the manifest is >4× cheaper in estimated
> tokens than the full `ontology_json()` dump.

### 5.4 Progressive ontology disclosure (standing-context win for both)

The full ontology is large; agents need a slice. Serve it in layers:

- **Root manifest** (~hundreds of tokens): 106 module names + one-line each + the
  effect-class legend + "call `describe(module)` to expand."
- `describe(module)` → that module's builtins with signatures + effects.
- `describe(builtin)` → params, examples, json_schema.

The pieces exist (`Describe`, `ListBuiltins`, compact `Ontology` format); this
makes lazy disclosure the default so the fixed schema tax shrinks dramatically.

---

## 6. Agent surface — token efficiency

(Syntax direction decided by §4; everything below is syntax-independent.)

### 6.1 Token accounting (close the stub gap)

All token-economy builtins (`tokens`/`budget`/`digest`) and the Agent API
accounting share one `builtins::est_token_count`: the **real GPT-4 cl100k BPE**
(embedded `tiktoken-rs`) under `--features real-tokens`, or a labeled heuristic
otherwise — so token counts are *exact*, not estimated, when the feature is on.

Per call the Agent API records `tokens_in` / `tokens_out` /
`tokens_total` and exposes them under `AgentResponse.metadata.token_accounting`
(injected in `process_request`; ✅ implemented). The CLI `--budget N` flag sets
`AE_TOKEN_BUDGET`, which the REPL applies via `budget_value` so every result is
paged/truncated to fit (✅ implemented). ✅ Per-session aggregation lands via
`sess_usage(id)` (dispatch 1125) — running `tokens_in`/`tokens_out`/`tokens_total`/
`evals` for a stateful session. ✅ `ai_usage()`/`ai_cost()` are wired to the live
`ai::COST_TRACKER` (not stubs) and report cumulative provider cost/usage.

> **STATUS: ✅ complete.** Implemented as builtins — `aecon` (compact render),
> `tokens` (estimate), and `budget(value, max_tokens, cursor?)` (row paging with
> `next_cursor`/`elided`/`page_tokens` + lossless string truncation with an
> explicit elision marker; dispatch 1109/1110/1118; see `tests/output_economy.rs`).
> The CLI `--budget`/`AE_TOKEN_BUDGET` flag applies `budget` automatically to every
> result, and `AgentResponse.metadata.token_accounting` carries per-call
> `tokens_in/out/total`.

### 6.2 Output budgeting + compact format (the biggest real win)

- **`--budget N` / `AE_TOKEN_BUDGET`.** Every result renders to ≤ N tokens.
  Over budget → head+tail rows, an explicit `…(987 more rows)` elision marker, and
  a stable **`cursor`** the agent passes to page. *Never silently lossy* — always
  states what was dropped (a safety property too).
- **AECON — Aether Compact Object Notation.** For a homogeneous `Table`/array of
  records, emit field names *once* as a header, then positional rows. Typed,
  deterministic, ~CSV-of-records density with JSON fidelity. This is where the
  honest 50–70% *output*-token savings live — the dominant cost term. ✅
  The header is followed by optional metadata lines, then positional rows. Four
  deterministic, self-describing compression levers, each gated to fire only where
  it's a measured token win:
    - **`@const k=v …`** — columns identical across all rows, emitted once and
      omitted from each row (status/type/owner fields).
    - **`@dict col: v0\tv1\t…`** — low-cardinality, multi-token *string* columns;
      rows reference the distinct values by a 1-token integer index.
    - **`@delta: col …`** — large-valued, slowly-varying *integer* columns
      (timestamps, sequential ids); row 0 holds the absolute value, each later row
      holds the difference from the previous (reconstruct by running sum).
    - **`@prefix col: <prefix>`** — *string* columns whose values share a leading
      run (paths, URIs, prefixed ids); the common prefix is emitted once and
      stripped from every row (reconstruct by re-prepending). 44–69% fewer tokens
      on path-heavy listings; gated to a real char win, never on `@dict` columns.
    - **`@type col:s|f …`** — type tags for *lossless* decode, emitted **only**
      where the compact form is ambiguous: a string that looks like a
      number/bool/null (`s`), or a float with an integral value that renders
      without a `.` (`f`). Dict columns (always strings) and delta columns (always
      integers) never need one, so this costs tokens only on genuine ambiguity.
  All backward-compatible (no eligible columns → the prior bare-header form), and
  none ever inflate a result — a cheaper encoding can only *replace* a costlier one.
  ✅ **Reversible — losslessly.** `aecon_decode(text)` (dispatch 1129) is the exact
  inverse for tabular AECON: it restores `@const` columns to every row, resolves
  `@dict` indices, reconstructs `@delta` columns by running sum, re-prepends
  `@prefix` columns, and honors `@type` tags so numeric-looking strings and integral
  floats decode to their exact type. Round-trip property tests assert
  `decode(aecon(v)) == v` — one with the `@const`/`@dict`/`@delta` levers active, one
  with the ambiguous string/float values the `@type` line resolves, and one with
  shared-prefix columns — so the compression is a genuine, lossless encoding rather
  than lossy display. (`canonical` remains the JSON-fidelity form for tooling that
  needs JSON specifically.)
- ✅ **Source-side projection is first-class.** `pick(fields…)` (dispatch 1128)
  keeps only the named fields of records / array-of-records / tables *before*
  rendering, so the agent never pays output tokens for discarded fields. Composes
  with `aecon`: `data | pick("name","size") | aecon` (`tests/output_economy.rs`).
- ✅ **`digest(value)`** (dispatch 1127): a constant-token structural summary —
  kind, length, element/field shape, a 2-row sample, and the full-vs-compact
  token cost — so an agent grasps a large result's shape + size cheaply, then
  decides to `budget` (page), `aecon` (compact), or skip. Maximal information per
  token (`tests/output_economy.rs`).
- ✅ **AECON is the default render in agent mode — on every surface.** Under
  `AETHER_MODE=agent` (or `AETHER_AGENT=1`) the output-token savings happen
  *automatically* instead of requiring an explicit `| aecon`:
    - **CLI / REPL** (`repl::run_one`) renders every result through
      `builtins::render_agent` — compact, deterministic AECON, no ANSI. A bare
      string is returned raw; `Null` prints nothing; with a budget set the value
      is paged and emitted as the AECON page plus one compact `@page …` line
      (`shown`/`total`/`elided`/`next_cursor`).
    - **HTTP Agent API** (`agent_api::execute_eval`) returns tabular results
      (`Array`/`Table`) as AECON text with `result_type: "aecon"`; scalars and
      single records stay native JSON (a bare number must not become a quoted
      string). `token_accounting` then reflects the smaller payload.
    - **MCP** (`McpServer::call_builtin`) returns tabular results as AECON text
      content (no JSON re-escaping cost — MCP content is already plain text).
  The human REPL keeps its colorized pretty-printer; non-agent API/MCP consumers
  keep full JSON — only the agent path diverts (`tests/output_economy.rs`,
  `tests/mcp_tools.rs`).

### 6.3 Stateful sessions + streaming

- ✅ **Session handles**: `sess_open()` / `sess_eval(id, code)` / `sess_close(id)`
  (dispatch 1122-1124) back a persistent `Env` keyed by id, so `let`-bindings and
  mutations persist across calls and multi-step agent work doesn't re-send prior
  context. `Value`/`Env` are `Send` (no `Rc`/trait objects), so the store is a
  global `Mutex<HashMap<String, Env>>`; `sess_eval` takes the env out, evals, and
  puts it back (no lock held across eval). Sessions are isolated, and — because
  the store is global — they are **stateful across separate Agent API requests**
  via the `Call` action (no new API surface needed: `Call{builtin:"sess_eval",…}`
  routes to the builtin; verified in `tests/sessions.rs`). Plan/Apply are likewise
  reachable via `Call`. `sess_usage(id)` (dispatch 1125) reports the session's
  running estimated `tokens_in`/`tokens_out`/`tokens_total`/`evals` so an agent
  watches its budget burn down. Session environments register all module
  namespaces (`modules::all_modules`), so `sess_eval` runs module-qualified calls
  like `file.read(…)`/`str.upper(…)`, not just bare builtins.
- ✅ **Streaming execute**: the `/api/v1/stream/execute` SSE route now does *real*
  chunking — a large array result is split into ordered `chunk` events
  (`StreamEvent::chunk`, default 50 rows) by `stream_events_from_response`, so a
  client consumes rows incrementally and can early-stop instead of receiving one
  atomic `complete` (`tests/streaming.rs`). Previously it emitted the whole result
  in a single event.
- ✅ **Streaming evaluation** (`eval::eval_stream`): results are produced
  *incrementally* rather than materializing the whole value first. When the final
  expression is a pipeline `source | stage…` whose source is an `Array` and whose
  stages are all element-independent (`map`/`where`/`filter`), each element is pushed
  through the stage chain one at a time and emitted as soon as it survives — true
  stage-by-stage streaming, computed lazily per element. Each streamed stage is
  driven through the *same* pipe mechanism (`env.set_input` + `eval_expr`) the eager
  evaluator uses, so element semantics are identical — there is no second
  `map`/`where` implementation to diverge, and `eval_expr`/`eval_program` are
  untouched (additive). Whole-collection stages (`sort`/`take`/`reduce`/`uniq`) or a
  non-array source fall back to eager evaluation, still emitting elements after — so
  correctness is preserved for every input. The `/api/v1/stream/eval` SSE route uses
  it to emit `chunk` events as items are produced. Verified by
  `eval_stream_streams_array_pipeline_element_wise` (+ fallback and scalar cases) in
  `tests/streaming.rs`. *Remaining:* fully lazy iterators end-to-end (the fallback
  path and whole-collection stages still materialize).

---

## 7. Agent surface — safety (the headline)

A coherent **capability → policy → approval → audit** model replacing today's
one-off env-var gates.

### 7.1 Policy engine (default-deny for dangerous classes in agent mode)

A declarative policy (`.ae/policy.toml`, env, or API-supplied), evaluated against
each call's effect class (§5.3):

```toml
[agent]                       # defaults when running under the agent surface
ReadLocal   = "allow"
Network     = { allow = ["api.github.com", "*.internal"] }
WriteLocal  = { allow_under = ["${workspace}"] }   # workspace jail
Destructive = "approve"       # never auto-runs; requires an approval token
Exec        = "approve"
Privileged  = "deny"
```

Humans default to `allow` with great errors; agents default as above. This
subsumes `AGENT_ALLOW_CMDS` and `AETHER_ALLOW_SH` into one model and **activates
the dormant RBAC** (`src/auth.rs`) as the principal/role source for policy.

### 7.2 Approval protocol (human- or supervisor-in-the-loop)

When a call's effect exceeds policy, it **does not execute**. It returns
`E_NEEDS_APPROVAL` (§5.2) with a structured **action descriptor**:

```jsonc
{ "what": "delete", "builtin": "rm",
  "targets": ["/proj/build/**"], "blast_radius": { "files": 412, "bytes": 1.2e9 },
  "reversible": false, "approval_token": "apv_8f2…" }
```

Approval can arrive from: an interactive TTY prompt, the **A2UI** channel
(`a2ui.confirm` already exists), or a **signed token** from a supervising agent /
RBAC principal. Re-calling with the token (bound to the exact descriptor hash, so
it can't be replayed for a different action) executes it. This is "dry-run by
default for destructive ops," made mandatory and machine-mediated.

### 7.3 Stable error/refusal codes

`E_NOT_FOUND · E_PERM_DENIED · E_BAD_ARG · E_NEEDS_APPROVAL · E_POLICY_DENY ·
E_BUDGET_EXCEEDED · E_TIMEOUT · E_RATE_LIMITED · E_OUTSIDE_WORKSPACE` (+ more).
Agents branch on these; humans read the `message`.

### 7.4 Workspace jail + path safety everywhere

A configurable workspace root. All `WriteLocal`/`Destructive` paths are
canonicalized and must resolve inside it. Extend the existing
`validate_write_path` (`src/security.rs:389`) to the builtins that currently
bypass it — `rm`, `rmdir`, `proc.kill` (by ownership), `db.delete`, `docker.rm`.

### 7.5 Tamper-evident audit (turns logs into proof)

Every effecting call emits a structured event (reuse `SecurityAuditEvent`,
`src/security.rs:24`) to an **append-only, hash-chained** log: each entry includes
the SHA-256 of the previous entry, so any tampering is detectable. Complete
coverage (all effect classes), persistent across runs, and the canonical record of
"what did the agent actually do." This is the differentiator that earns "safest
shell."

### 7.6 Resource governors + secret hygiene

- ✅ **Resource governors.** A per-run blast-radius envelope enforced at the
  `guard()` chokepoint (so it covers every effecting builtin uniformly) and only in
  agent mode. All limits are opt-in via env — unset = unlimited, so existing runs
  are unaffected until configured — and a breach returns the structured
  `E_BUDGET_EXCEEDED` (non-retryable) so an agent stops rather than loops:
  `AETHER_MAX_OPS` (total guarded ops), `AETHER_MAX_FILES` (WriteLocal +
  Destructive), `AETHER_MAX_PROCS` (Process + Exec), `AETHER_MAX_NET` (Network —
  egress request count), and `AETHER_TIMEOUT_MS` (wall-clock since the first guarded
  op, checked at each guard boundary). Counters tally *attempts* at the boundary (an
  op later denied by jail/policy still counts — the envelope bounds what the agent
  may try, the strictly-safe reading). The network cap is enforced by routing the
  **every egress builtin** (`http_get`, `curl_exec`, `wget_download`, and the full
  `web_*` fetch family — `web_fetch`/`web_get`/`web_post`/`web_json_get`/`web_json_post`/
  `web_scrape`/`web_download`/`web_headers`/`web_cookies`/`web_form_submit`/
  `web_upload_file`/`web_rest_api`/`web_graphql`/`web_check_url`, with
  `web_robots_txt`/`web_sitemap` inheriting it by delegating to `web_fetch`) through
  `guard()` with the `Network` effect via the `guard_network` helper — which
  also brings them under the audit log (`Network` is policy-`allow`, so this meters +
  audits without prompting). `governor_status()` reports counts/limits/elapsed (also
  folded into `safety_status()`); `governor_reset()` starts a fresh envelope. Verified
  by 6 unit tests + an end-to-end test (a 2nd `rm` under `AETHER_MAX_FILES=1` returns
  `E_BUDGET_EXCEEDED`, file untouched). (Output-bytes is covered separately by
  `--budget`/`AE_TOKEN_BUDGET` paging.)
- ✅ **Secret hygiene.** Two deterministic defenses in `src/safety.rs`, opt-out via
  `AETHER_REDACT=off`:
    - **Shape redaction** (`redact_str`/`redact_json`/`builtins::redact_value`) scrubs
      known secret *shapes* — provider key prefixes (`sk-`/`sk-ant-`, `ghp_`/`gho_`…,
      `xox*-`, `AIza…`, Stripe `sk_live`/`rk_test`), AWS access-key ids (`AKIA…`),
      JWTs, PEM `PRIVATE KEY` blocks, `scheme://user:password@host` URL credentials
      (password only), and `key=secret`/`key: secret` assignment forms — replacing
      each with `[REDACTED]`. Applied on the **agent render path**
      (`render_agent`, so secrets never enter the model's context) **and to every
      audit entry** before it is hashed and persisted (a leaked credential in the
      durable, hash-chained log would otherwise outlive the run). Ordinary prose is
      returned byte-for-byte unchanged; the chain still verifies over redacted
      content.
    - **Name gating** (`env_secret_gated`): in agent mode, reading a secret-*named*
      env var (`*_KEY`, `*TOKEN*`, `*SECRET*`, `*PASSWORD*`, … — `KEY` alone excluded
      to avoid `KEYBOARD`/`MONKEY`) returns an opaque `[REDACTED:NAME]` handle from
      `env`/`sys.env`/`env.var`/`env.vars` *before the value enters the program's
      value space* — unless the operator permits clear reads with
      `AETHER_SECRETS=allow`. Human mode is never gated (a person reading their own
      env wants the value — legibility). Closes the credential-in-env / secret-in-
      output exposure. Verified by 6 unit tests + a 4-test end-to-end suite
      (`tests/secret_hygiene.rs`): render, env-gating, and audit-log surfaces.

---

## 8. Human surface — reliability

The `.ae` surface keeps readable, unambiguous syntax and gains:

- **Grammar-level everything** — no transpiler passes; the same parser the agent
  surface uses, so the two can never semantically diverge.
- ✅ **Boundary type-checking.** `safety::bad_arg(builtin, expected, got)` produces
  a catchable `{error:{code:"E_BAD_ARG", message, hint, retryable}}`. Rather than a
  per-builtin signature table (the 1,100+ builtins have no structured HM signatures —
  `type_builtin_call` covers only a handful and infers `Any` otherwise), the structured
  error is driven from the **shared argument-extraction helpers** that are the de-facto
  boundary for most builtins: `expect_string` / `expect_int` / `expect_array` /
  `need_lambda` (~90 call sites) now emit `bad_arg(builtin, expected, value.type_name())`
  instead of ad-hoc `anyhow!` prose. So a wrong-typed argument to any builtin that
  uses them surfaces a branchable `E_BAD_ARG` naming both the expected and the actual
  type — caught by try/catch as a structured record for agent self-correction
  (`tests/reliability.rs`). The arity (missing-arg) counterpart `arg(builtin, args,
  idx, expected)` gives the same structured `E_BAD_ARG`; the core agent-facing verbs
  (`map`/`where`/`reduce`/`take`/`call`/`agent`/`swarm`/`mcp_call`) now use it.
  `type_builtin_call` static inference also gained the shape-preserving array
  transforms (`sort`/`uniq`/`take`/`head`/`tail`) and `len`/`wc` → `Int`.
  ✅ **The peripheral long tail is now converted too:** all ~490 prose arity/usage
  errors across the builtins (and the evaluator's lambda-arity checks) emit the
  structured `E_BAD_ARG` via `safety::arg_err(message)` — the message text is
  preserved, only the error *type* is upgraded, so every argument/arity failure is
  now a branchable, catchable record (and renders as legible prose for humans).
  Errors that are *not* about caller arguments — API-response parsing, feature-flag
  gating, URL/URI validation, workflow-definition checks — were deliberately left as
  plain errors, since coding them `E_BAD_ARG` would be wrong.
- ✅ **Great errors with `hint`** — humans benefit from the same structured errors,
  rendered richly. The human REPL unpacks an uncaught `SafetyError` into legible prose
  — `error[CODE]: message`, an indented `hint:` line, and (for an approvable action)
  the exact `AETHER_APPROVE=…` re-run incantation — instead of dumping the JSON form
  (`repl::print_eval_error`). **Agent mode keeps the raw JSON** so `code`/`hint`/
  `approval` survive for programmatic branching. One structured error, two renderings.
- **Determinism on demand** — `--deterministic` available to humans too (for
  reproducible scripts / diffs / CI).

---

## 9. Features that no shell has (agentic-native)

- ✅ **Transactions / checkpoints.** `tx_begin` / `tx_commit` / `tx_rollback` /
  `tx_status` (dispatch 1114-1117) over a backup journal (`src/tx.rs`): while a
  transaction is active, `rm` / `file_write` / `file_append` / `rmdir` and every
  **mutating sqlite builtin** (insert/update/delete/create_table/drop_table, which
  all route through `db_sqlite_exec` — the single snapshot chokepoint) record their
  pre-modification state (`crate::tx::snapshot`), and rollback restores the
  pre-transaction state — overwrites reverted, appends truncated, deletions undone
  (including **whole directory trees**, recursively backed up and restored), an
  edited **database file** restored byte-for-byte, created files removed
  (`tests/transactions.rs`). v1 scope: single (non-nested) transaction; files,
  directory trees, and sqlite db files. Nothing in Bash/PowerShell offers this.
  Plan/Apply ops: `write`/`append`/`rm`/`mkdir`. The sqlite-backed key-value
  builtins (`db_kv_get`/`set`/`delete`/`keys`/`store`, dispatch 1130-1134) route
  through `db_sqlite_exec`, so they inherit transactionality automatically *via the
  same chokepoint* — verified by `rollback_restores_a_key_value_store_mutation`.
  ✅ **Savepoints** (`tx_savepoint(name)` / `tx_rollback_to(name)`, dispatch
  1135-1136): SQL-style partial rollback — revert only the operations recorded
  after a named savepoint while leaving the transaction open (and the savepoint
  re-usable). Verified by `savepoint_enables_partial_rollback_then_commit`.
  ✅ **Full nesting.** `tx_begin` while a transaction is active pushes a child frame
  (SQL nested-transaction semantics): a child `commit` folds its changes into the
  parent (nothing durable until the **outermost** commit), a child `rollback` reverts
  only the child's ops and leaves the parent open. Each frame keeps its own `seen`
  set and captures its own pre-image, so an inner rollback restores a path to its
  pre-*inner* state even when an outer frame also touched it, and replaying
  inner→outer undos in reverse restores the pre-*outer* state. All frames share one
  journal dir (removed when the outermost frame ends); `tx_begin`/`tx_status` report
  `depth`. Verified by `nested_rollback_isolates_inner_then_outer` and
  `nested_commit_folds_into_parent_then_outer_rollback_undoes_all` (`tests/transactions.rs`).
- ✅ **Plan / Apply** (Terraform-style) for a destructive batch: `plan(ops)`
  returns a typed, reviewable summary + a content-bound approval token (executes
  nothing); `apply(ops)` runs the batch atomically inside a transaction — agent
  mode gates it on the plan token, paths are workspace-jailed, any failure rolls
  the whole batch back, and the outcome is audited (dispatch 1119/1120,
  `tests/transactions.rs`). Ties together approval + transactions + structured
  output. Ops: `write`/`append`/`rm`/`mkdir`/`copy`/`move` (the last two take a
  `dest` path; both endpoints are jailed and snapshotted so a copy/move rolls back
  cleanly). *Remaining:* a textual plan diff view.
- ✅ **Be an MCP server, not just a client.** `McpServer::list_builtin_tools`
  exposes every AetherShell builtin as an MCP tool annotated with its `x-effect`
  class; `McpServer::call_builtin` routes calls through `builtins::call` so the
  same policy/approval/jail/audit applies — any MCP-speaking agent gets the full
  typed surface *and* the safety model with zero bespoke integration
  (`agent_api::builtin_tool_specs`, `tests/mcp_tools.rs`). On the wire: a
  `/mcp/v1/builtins` route lists them (kept separate from `/tools` so the OS-tool
  list stays small — progressive disclosure), and `/mcp/v1/tools/:name/execute`
  falls back to `call_builtin` for builtin names, so they're callable over the
  server with full gating. ✅ **Strict stdio JSON-RPC transport** (the canonical MCP
  transport): `McpServer::serve_stdio` runs a JSON-RPC 2.0 loop over stdin/stdout
  (`ae [--agent] mcp stdio`) handling `initialize`/`tools/list`/`tools/call`/`ping`,
  routing every call through `call_builtin` (so policy/jail/approval/audit apply) and
  echoing JSON-RPC ids; notifications get no reply. The per-request dispatch
  (`McpServer::handle_rpc`) is unit-tested for all methods + the error/notification
  cases (`tests/mcp_tools.rs::test_mcp_stdio_jsonrpc_dispatch`).
- ✅ **Self-correcting loop.** This used to read *"falls out of §5.2 + §7.3"* — an
  inference, never measured. It does not simply fall out: the inference holds only
  if failures actually carry codes (they mostly did not — see §5.2), if suggestions
  are real, and if a failed attempt leaves no debris for the next one. Three pieces
  close it, and `tests/self_healing.rs` (12 tests) asserts each:
    - **`diagnose(error)`** (dispatch 1139) — progressive disclosure applied to
      failure. Given the record `catch` bound, it returns *only* what repairing
      **this** call needs: code, `retryable`, hint, `did_you_mean`, and the offending
      builtin's signature and effect class — no prose description, no category
      listing, at most one example, and no expansion of parameters/return type that
      the signature already spells out. It never costs more than the full
      `ontology_describe`, and on a richly-documented builtin it is roughly half:
      **map 82 vs 206 tokens (2.5×), http_get 91 vs 170 (1.9×)**. On a builtin whose
      definition is already thin the saving is small (**grep 54 vs 67, sort 64 vs 68**)
      — the win tracks how much prose is being avoided, which is the honest shape of
      a disclosure optimisation. Note `explain` was already taken by the AI helper.
    - **`try_repair(code)`** (dispatch 1140) — makes retrying *safe*. It does not
      invent a fix; nothing in the shell can. It brackets the evaluation in a unique
      named savepoint and, on failure, rolls back to it, so attempt N+1 starts from
      exactly the state attempt N did instead of from the debris of a half-applied
      batch. Returns `{ok, value}` or `{ok:false, restored, retryable, error}`, the
      `error` being the same structured record `catch` binds, so it feeds straight
      into `diagnose`. An enclosing transaction keeps its own earlier work.
    - **A measured repair rate** — see §11.

---

## 10. Phased roadmap (with file-level touch points)

| Phase | Theme | Key work | Primary files |
| --- | --- | --- | --- |
| **1** | **Measure** | ✅ Token-benchmark harness + **real cl100k tokenizer** (`--features real-tokens`); verdict = legible-first, real-tokenizer-confirmed (§4.0). ✅ Corpus broadened to 13 tasks; ✅ **cross-tokenizer check** added — the §4 criterion is re-run under a second real BPE (GPT-4o `o200k_base`) and **the legible-first verdict holds under both** (standing-context ~11× under each). Anthropic ships no offline Claude tokenizer crate, so `o200k_base` serves as the cross-provider proxy. | `examples/token_bench.rs`, `Cargo.toml` |
| **2** | **Core: errors + effects + determinism** | Structured-error `Value` (§5.2); effect taxonomy macro/table (§5.3); float/serialize determinism (§5.1) | `src/value.rs`, `src/builtins.rs`, `src/eval.rs` |
| **3** | **Safety core (headline)** | Policy engine + workspace jail (§7.1/7.4); approval protocol (§7.2); hash-chained audit (§7.5); wire RBAC | `src/security.rs`, `src/auth.rs`, `src/builtins.rs` (rm/kill/sh/db/docker), `src/agent_api.rs` |
| **4** | **Token economy** | 🟡 First slice: `aecon(value)` compact rendering + `tokens(value)` estimate builtins (§6.2, tests prove AECON < JSON on homogeneous records) + `budget()` paging/truncation (§6.2) + `ontology_manifest`/`ontology_describe` progressive disclosure (§5.4, >4× cheaper than full dump) + CLI `--budget N` flag (REPL applies `budget_value`) + per-call `token_accounting` in `AgentResponse.metadata`. **Core complete.** *Remaining:* per-session token aggregation | `src/builtins.rs`, `src/agent_api.rs`, `tests/output_economy.rs`, `src/value.rs`, `src/metrics.rs`, `src/main.rs` |
| **5** | **Grammar unification** | 🟡 Started: `|.field` projection (with `.a.b` chains) now parsed by the real grammar (`parser::parse_pipe`) + SI suffixes in the lexer + `~x: body` lambda + `?match` prefix + `if`-expression — all additive (`tests/grammar.rs`). **Transpiler retirement in progress** (2 passes retired): `expand_si_suffixes` and `expand_match` (`?`) are removed from the pipeline (grammar covers both); their golden tests migrated to behavior assertions (`eval_aeg` harness) and ONTOLOGY examples updated to pass-through. Recipe: remove the call → `#[allow(dead_code)]` the fn → behavior-migrate the affected text tests → update ONTOLOGY examples. (Lesson: text-coupled golden tests make each retirement cost real migration — SI touched 4 test sites + 2 ontology examples.) *Remaining:* `expand_lambdas`/`expand_pipelines` can only be *partially* retired (they also handle cipher forms the grammar lacks: `\x:`/`~.field`/`>`-pipe); `!try`/`^cond` ciphers overload `Bang`/`Caret`, so they stay transpiler-only; retire the 10-pass transpiler to a shim (§4.3); boundary type-checking (§8) | `src/parser.rs`, `src/typecheck.rs`, `src/transpile/agentic.rs` |
| **6** | **Agentic features** | ✅ Transactions/checkpoints (`tx_*` + savepoints) + Plan/Apply (`plan`/`apply`, ops `write`/`append`/`rm`/`mkdir`/`copy`/`move`) + builtins-as-MCP-tools (`McpServer::list_builtin_tools`/`call_builtin`) + **stateful sessions** (`sess_*`) + chunked **streaming execute** landed. *Remaining:* true stage-by-stage streaming *evaluation*; a strict stdio JSON-RPC MCP transport; full transaction nesting | `src/tx.rs`, `src/builtins.rs`, `src/agent_api.rs`, `src/mcp.rs`, `src/ai_api/server.rs` |

| **7** | **Self-healing** | ✅ Structured-by-default at the builtin boundary (`safety::ensure_structured`, new codes `E_UNKNOWN_BUILTIN`/`E_UNKNOWN`, `ErrorCode::retryable()`); `did_you_mean` over the live builtin table; `diagnose(error)` minimal repair context (1139); `try_repair(code)` rollback-bracketed attempts (1140); `agentic_eval::repair` harness + a **measured** repair rate filling §11's placeholder. *Remaining:* a model-driven repair strategy scored against the same corpus (the mechanical 100% is a floor, not a ceiling); extend `did_you_mean` to module paths (`file.raed`), which resolve outside `BUILTIN_LOOKUP` | `src/safety.rs`, `src/builtins.rs`, `crates/agentic-eval/src/repair.rs`, `tests/self_healing.rs`, `tests/repair_rate.rs` |

Phases 2–3 are independent of the §4 benchmark and deliver the safety + reliability
headline immediately; Phase 5 depends on the §4 outcome. Phase 7 depends on nothing
but §5.2, and exists because §9 originally asserted self-correction as a corollary
of structured errors rather than building or measuring it.

---

## 11. Success metrics

- **Tokens:** ≥40% reduction in total tokens per *successful* benchmark task vs.
  v1.2.0 (driven by output budgeting + progressive disclosure, validated by §4).
- **Reliability:** 0 pass-ordering bugs (grammar-level parsing); 100% of arg-type
  errors surfaced as `E_BAD_ARG` at the boundary; byte-identical output across
  Linux/macOS/Windows for the determinism test corpus.
- **Safety:** 100% of `Destructive`/`Exec`/`Privileged` calls gated by
  policy+approval; 100% effecting-call audit coverage; tamper-evident chain
  verified by a `ae audit verify` check.
- **Self-correction:** ✅ **measured, not assumed** (`tests/repair_rate.rs`, built on
  the reusable `agentic_eval::repair` harness). The placeholder here read `≥X%` for
  as long as this document existed, because the pillar was inferred from the presence
  of structured errors rather than measured. The harness replays each failed call
  using **nothing but the error record** and re-runs it:

  | corpus | failures | repaired mechanically | uncoded |
  | --- | --- | --- | --- |
  | plausible misspellings (8) | 8 | **8 (100%)** | 0 |
  | wrong-argument calls (4) | 4 | 0 *(strategy declines)* | 0 |
  | mixed, incl. real runtime failures (13) | 13 | 8 | **0** |

  Three things about that 100% keep it honest. The strategy is **model-free** — it
  substitutes the first `did_you_mean` candidate and nothing else — so the figure is a
  *floor*: what the error surface alone is worth before any intelligence is applied.
  The corpus **isolates the name as the only defect**; an early version supplied
  arguments that were invalid for the target too, so a correct suggestion still failed
  and scored 88%, measuring the corpus rather than the product. And it covers exactly
  the population that *is* mechanically repairable: a wrong-typed argument is
  diagnosable but not fixable without deciding what the value should have been, which
  is a model's job — `wrong_argument_failures_are_actionable_but_not_mechanically_repairable`
  pins that down so the headline can never be read as "all failures are repairable".

  The harness scores a **`MisleadingError`** distinctly from a dead end: an error that
  carries a stable code, a confident hint, and a suggestion that does not work looks
  actionable at every structural layer and still sends the agent the wrong way. Only
  replaying the repair separates the two — which is the whole reason this is measured
  by re-running calls rather than by classifying errors.

---

## 12. Open questions / risks

- **Tokenizer dependence (§4):** ✅ *largely resolved* — the benchmark now runs
  under two genuinely different real BPEs (cl100k_base and o200k_base) and the
  legible-first verdict holds under both, so the result is not cl100k-specific. The
  one residual unknown is Anthropic's own tokenizer (no public offline crate), for
  which o200k_base is the closest available proxy; the design still accommodates
  per-provider syntax in schema export should a provider's tokenizer ever invert it.
- **Effect-tagging 1,100+ builtins** is labor. 🟡 The proposed lint now exists
  (`tests/effect_coverage.rs`) and it found the risk was not hypothetical: **28
  builtins named a side effect and classified as `Pure`**, among them `ssh_exec`,
  `sudo_exec`, `remote_exec`, `docker_exec`, `k8s_exec`/`kubectl_exec`,
  `kubectl_delete`, `terraform_destroy`, `cloud_instance_destroy` and
  `db_sqlite_drop_table`. Because `Pure` is the fall-through, each was advertised
  to agents as side-effect-free via `x-effect` and would have been allowed
  outright by `guard()`. All 28 are now classified (2 were genuine false
  positives — `platform_has_sudo` is a `which` lookup, `platform_shell_type` an
  env read — and are allow-listed with that reason recorded).

  ✅ **The classification is now backed by guards.** A tag only changes what is
  *advertised*; `guard()` has to be called at the site for anything to be gated.
  Guards were wired into the eight that genuinely act — `ssh_exec`, `docker_exec`,
  `podman_exec`, `k8s_exec`, `tool_exec`, `rlm_spawn` (Exec) and
  `terraform_destroy`, `cloud_instance_destroy`, `db_sqlite_drop_table`
  (Destructive) — so in agent mode they refuse with `E_NEEDS_APPROVAL` and land in
  the audit log. `terraform destroy -auto-approve` is the sharpest case: it will
  not prompt on its own, so if approval does not happen at the guard it does not
  happen at all. Human mode is unchanged. Asserted by three tests in
  `tests/safety.rs`; `kubectl_delete`/`kubectl_exec` needed nothing, being aliases
  of the already-guarded `k8s_*`.

  ⚠️ **Reading each body before wiring a guard corrected four of the lint's own
  results.** `sudo_exec` returns the advice "use sudo directly in terminal";
  `watchexec_run` returns a suggested invocation; `env_shell` reads `$SHELL`;
  `remote_exec`/`exec_remote` is a stub whose comment says *"Simulate remote
  execution (in real impl would use SSH/RPC)"*. All five execute nothing, and all
  had been tagged `Exec` **from the name alone** — the exact error the lint exists
  to catch, committed while fixing it. A name-based lint can only ever nominate
  suspects; the allow-list in `tests/effect_coverage.rs` records the verified
  reason for each. Separately, `remote_exec` returns `status: "executed"` to its
  caller while running nothing — an honesty problem in the *return value* rather
  than the effect tag, and left alone here because changing it would break callers.

  ✅ **The lint was then broadened past exec/delete names** to egress
  (`upload`/`download`/`publish`/`post`/`webhook`) and persistence
  (`write`/`save`/`install`/`mount`), which surfaced 29 more and two findings worth
  naming separately:

  - **Package installers were `Pure`.** `npm_install`, `yarn_install`,
    `pnpm_install`, `bun_install`, `pipx_install`, `poetry_install`, `pkg_install`,
    `asdf_install`, `helm_install`, `marketplace_install`, `pre_commit_install` each
    shell out to a package manager that fetches remote code and runs its install
    scripts. That is the supply-chain surface (CWE-494), and `effect_of` was telling
    every consumer that `npm_install("anything")` was side-effect-free. Now `Exec`
    (`helm_uninstall`/`marketplace_uninstall` are `Destructive`).
  - **The `web_*` family was gated but mislabelled.** Every `web_*` fetch already
    routes through `guard_network` with `Effect::Network` at the call site, yet
    `effect_of("web_post")` returned `Pure` because the Network arm only matched
    `http*`/`net_`/`nc_`. So the control was right and the *label* an agent reads
    was wrong — the ontology advertised `web_post` as pure. The label now agrees
    with the control.

  A third pass added privilege/service-control names and surfaced eight more —
  `svc_restart`/`k8s_rollout_restart` (`Process`), `chmod`/`fs_chmod`/`fs_chown`
  (`WriteLocal`), `k8s_deployments`/`k8s_services` (`Network`: reads, but remote
  ones that ship credentials to a cluster endpoint) — plus **a second fabricating
  stub**: `cloud_deploy` minted a UUID and returned `status: "deployed"` while
  containing no HTTP client and spawning no process. Corrected to `simulated`,
  like `remote_exec`.

  Coverage after three passes: **1,122 of 1,301 (86%) fall through to `Pure`**, down
  from 1,183; classified builtins went 118 → 179.

  One thing this still does **not** fix: the lint only catches names that
  *advertise* a side effect, so a dangerous builtin with an innocuous name is
  invisible to it. And `db_sqlite_exec` is classified `Exec` but
  deliberately left unguarded — gating it would put every sqlite *read*, including
  `db_kv_get`, behind approval; its mutating paths are separately classified and
  its snapshot chokepoint makes them reversible. Flipping the default to a
  restrictive class — §12's other proposed mitigation — would gate roughly a
  thousand builtins at once and is a product decision, not a bug fix.
- **Approval UX latency** for interactive humans — mitigate with policy presets
  ("trust this workspace") and session-scoped grants.
- **Backward compatibility:** existing `.aeg` scripts must keep working; the
  transpiler-to-shim path (Phase 5) must be behavior-preserving, gated by the
  existing `tests/transpile_agentic.rs` corpus plus golden tests.

---

## 13. Implementation status — Phase 3 safety core (first slice)

Landed in `src/safety.rs` (native-gated, registered in `src/lib.rs`), with
`guard()` wired into nine effecting builtins in `src/builtins.rs`:

- **filesystem** (jailed to the workspace): `rm`, `rmdir`, `file_delete_lines`
- **process**: `proc_kill`
- **exec**: `sh`
- **destructive non-path** (not jailed): `db_kv_delete`, `db_sqlite_delete`,
  `docker_rm`, `docker_compose_down`, `platform_db_delete`, `k8s_delete`
  (remote/cluster — the guard fires before `kubectl`/db work)

`GuardCtx.fs_paths` distinguishes filesystem-path targets (subject to the jail)
from non-path targets like database rows or container names, so the jail never
misfires on a table name or container id. Verified by 7 unit tests + 4 end-to-end
integration tests (`tests/safety.rs`); full lib suite green (424 passing, 0
regressions).

**What works now:**

- `Effect` taxonomy (8 classes) + best-effort `effect_of(name)` classifier (§5.3).
- `Mode` (Human / Agent) and the `decide(effect, mode) → Allow | Deny | Approve`
  policy table (§7.1).
- Structured `SafetyError` with stable codes (`E_NEEDS_APPROVAL`,
  `E_POLICY_DENY`, `E_OUTSIDE_WORKSPACE`), an actionable `hint`, and a JSON
  rendering for agents (§5.2/§7.3).
- Content-bound approval tokens (`apv_…` = first 16 hex of SHA-256 over the
  action descriptor) — a token cannot be replayed to approve a different action
  (§7.2).
- Workspace jail with cross-platform path resolution: canonicalizes both root
  and target (Windows `\\?\` verbatim prefixes, POSIX symlinks) and lexically
  collapses `..` so traversal can't escape even for not-yet-existing leaves
  (§7.4).
- Append-only, **hash-chained audit log** + `verify_audit()` that detects
  tampering, broken chain links, and non-monotonic sequence (§7.5). Chain state
  is path-aware (switching log files resets + reloads rather than continuing a
  stale chain). `audit_tail(n?)` (dispatch 1126) returns the most recent entries
  for in-shell review — so the log loop is write → `audit_verify` (integrity) →
  `audit_tail` (review).
- `safety_status()` introspection builtin (dispatch 1121): reports the live
  operating envelope — mode, principal, workspace, audit log, active transaction,
  and the resolved `allow`/`deny`/`approve` decision for every effect class — so
  an agent can check what it may do *before* acting.

**Behaviour by default:** human mode is unchanged (default-allow, no audit file —
zero regression). Agent mode (`AETHER_MODE=agent`) gates `Destructive`/`Exec`/
`Process` behind approval, denies `Privileged`, jails filesystem writes to the
workspace, and writes the audit chain.

### Environment reference (this slice)

| Var | Effect |
| --- | --- |
| `AETHER_MODE=agent` (or `AETHER_AGENT=1`) | Select the agent surface (default-deny dangerous classes). |
| `AETHER_POLICY=permissive` | Agent mode behaves like human mode (allow all). |
| `AETHER_WORKSPACE=<dir>` | Workspace jail root (also enables the jail in human mode). |
| `AETHER_APPROVE=<token[,token…]>` | Pre-approve the listed bound approval tokens. |
| `AETHER_APPROVE_ALL=1` | Blanket-approve all approvable actions (trusted automation). |
| `AETHER_AUDIT_LOG=<file>` | Audit log path (defaults to `<workspace>/.ae/audit.log` in agent mode). |
| `AETHER_AUDIT_REQUIRED=1` | Fail the guarded op if the audit write fails (default: best-effort + warn). |
| `AETHER_REDACT=off` | Disable secret-shape redaction of agent output and audit entries (default: on). |
| `AETHER_SECRETS=allow` | Permit clear reads of secret-named env vars in agent mode (default: return a `[REDACTED:NAME]` handle). |
| `AETHER_MAX_OPS=<n>` | Resource governor: max total guarded operations per run (agent mode; unset = unlimited). |
| `AETHER_MAX_FILES=<n>` | Resource governor: max filesystem ops (WriteLocal + Destructive). |
| `AETHER_MAX_PROCS=<n>` | Resource governor: max process/exec ops (Process + Exec). |
| `AETHER_MAX_NET=<n>` | Resource governor: max network egress operations (Network). |
| `AETHER_TIMEOUT_MS=<ms>` | Resource governor: wall-clock budget since the first guarded op (checked at each guard boundary). |

CLI flags set these directly (no env export needed): `--agent` → `AETHER_MODE=agent`,
`--workspace <DIR>` → `AETHER_WORKSPACE`, `--policy <p>` → `AETHER_POLICY`,
`--budget N` → `AE_TOKEN_BUDGET`. E.g. `ae --agent --workspace . script.ae`.
Verified end-to-end (`ae --agent -c 'safety_status()'` reports `mode: "agent"`).

### Immediate next steps (remaining Phase 3)

- ✅ Extended `guard()` to the destructive db/docker/file builtins. Assessed the
  remaining candidates: `k8s_delete` and `platform_db_delete` are guarded
  (`Destructive`); `svc_delete` is a no-op stub (returns a message, deletes nothing)
  so it needs no guard. Network egress (`http_get`/`curl_exec`/`wget_download`/the
  `web_*` fetch family) is now guarded too (`Network`).
- ✅ Exposed `approve(token)` (dispatch index 1104) and `audit_verify(path?)`
  (1105) as builtins, so the loop is usable in-shell (not just via env):
  a guarded op returns `E_NEEDS_APPROVAL` with a token → `approve(token)` →
  re-run succeeds. `audit_verify()` checks the active hash-chained log.
- ✅ Surfaced `Effect` in the ontology: `annotate_effects` (`src/agent_api.rs`)
  injects an `x-effect` key into every builtin's `json_schema` via
  `safety::effect_of`, applied in both `get_builtin_definitions` and
  `get_all_builtin_definitions`, so agents see an op's danger class before
  calling it (covers the ontology + Describe/List endpoints).
- ✅ Promoted `SafetyError` to a first-class `Value`: the `eval.rs` try/catch
  downcasts the caught error and, for a `SafetyError`, binds the catch variable
  to a structured `Value::Record` (`{error: {code, message, hint, …}}`) so agents
  branch on `e.error.code` instead of parsing a string. Non-safety errors stay
  as strings.
- ✅ Wired the RBAC manager (`src/auth.rs`) into the safety layer:
  `safety::set_rbac_manager` / `set_principal` / `current_principal`, consulted by
  `guard()` via `rbac_authorized`. A principal holding `effect:<class>`,
  `effect:*`, `*:*`, or `builtin:<name>` (resolved through `RbacManager` with role
  inheritance + wildcards) **bypasses the per-action approval** — but **not** the
  workspace jail (defense in depth: authorization grants capabilities, not an
  escape from the jail). RBAC is additive: lacking a grant defers to the normal
  policy, never a hard deny.
  - ✅ In-shell builtins (dispatch 1106-1108): `rbac_principal(user_id?)`
    (set/get the acting principal), `rbac_grant(user_id, permission)` (grant
    `effect:<class>` / `effect:*` / `builtin:<name>` / `*:*`), `rbac_can(permission)`
    (introspect). Backed by a process-global `RbacManager` that installs into the
    safety layer; `auth::invalidate_user_cache` busts the resolved-permission
    cache on direct grants. So an operator/agent configures principals + grants
    entirely from the shell.
  - ✅ **Startup config loading.** `RbacManager::from_config_str` parses a TOML
    [`RbacConfig`] (roles with permissions + inheritance, principals with role
    assignments + direct grants); `safety::init_rbac_from_env()` (called from
    `main`) loads it from `AETHER_RBAC_CONFIG` (or `<workspace>/.ae/rbac.toml` if
    present), installs the manager, and sets the acting principal (from
    `AETHER_PRINCIPAL`, else the config's `principal`). So the authorization model
    is configured from a file at boot, not just via in-shell `rbac_*` calls
    (`auth::tests::test_rbac_from_config_str`,
    `tests/safety.rs::rbac_config_loaded_at_startup_authorizes_principal`).
    *Remaining:* an interactive login flow; optionally bridge the older `rbac_*`
    `RBAC_ROLES` role registry into `RbacManager` (two stores still coexist).

> **Test-infra fix (done):** `plugins::tests::test_plugin_source` was flaky under
> parallel execution (it read the global plugin registry's `builtin.json` enabled
> flag while `test_plugin_enable_disable` toggled it). Now serialized behind a
> shared `REGISTRY_LOCK`; the parallel lib run is a clean 425/0.

> **Finding (needs a product decision):** `bi_rm`/`bi_rmdir` are **not registered**
> in `BUILTIN_LOOKUP`/`BUILTIN_DISPATCH` (the `// 950` comments are stale — 950 is
> `helm_status`), so `rm(...)`/`rmdir(...)` by name currently return
> "unknown builtin"; they're reachable only by direct Rust callers. The guards
> protect every caller regardless, but if file removal should be a first-class
> shell builtin it must be registered (and would then be gated automatically).
