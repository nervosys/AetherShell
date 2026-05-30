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

---

## 5. Shared core (serves both surfaces)

> **STATUS:** the `canonical(value)` builtin (dispatch 1113) implements this
> contract — sorted keys, shortest round-trip locale-independent floats, explicit
> `null` for non-finite/non-serializable values, correct escaping. Tests assert
> byte-stability and insertion-order independence (`tests/reliability.rs`). The
> lossless counterpart to `aecon` (§6.2). Remaining: make it the default agent-mode
> render path behind `--deterministic`.

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
paged/truncated to fit (✅ implemented). Remaining: per-session aggregation and
wiring `ai.usage()`/`ai.cost()` (stubs, `builtins.rs:3708`) to the same counters.

> **STATUS:** implemented as builtins — `aecon` (compact render), `tokens`
> (estimate), and `budget(value, max_tokens, cursor?)` (row paging with
> `next_cursor`/`elided`/`page_tokens` + lossless string truncation with an
> explicit elision marker; dispatch 1109/1110/1118; see `tests/output_economy.rs`).
> Remaining: a CLI `--budget`/`AE_TOKEN_BUDGET` flag that applies `budget`
> automatically to results, and per-call `tokens_in/out` in `AgentResponse`.

### 6.2 Output budgeting + compact format (the biggest real win)

- **`--budget N` / `AE_TOKEN_BUDGET`.** Every result renders to ≤ N tokens.
  Over budget → head+tail rows, an explicit `…(987 more rows)` elision marker, and
  a stable **`cursor`** the agent passes to page. *Never silently lossy* — always
  states what was dropped (a safety property too).
- **AECON — Aether Compact Object Notation.** For a homogeneous `Table`/array of
  records, emit field names *once* as a header, then positional rows. Typed,
  deterministic, ~CSV-of-records density with JSON fidelity. This is where the
  honest 50–70% *output*-token savings live — the dominant cost term. ✅
  **Constant-column factoring**: columns whose value is identical across all rows
  are emitted once in a `@const` line and omitted from each row (big saving for
  constant status/type/owner fields; backward-compatible — no constants → the
  prior format).
- ✅ **Source-side projection is first-class.** `pick(fields…)` (dispatch 1128)
  keeps only the named fields of records / array-of-records / tables *before*
  rendering, so the agent never pays output tokens for discarded fields. Composes
  with `aecon`: `data | pick("name","size") | aecon` (`tests/output_economy.rs`).
- ✅ **`digest(value)`** (dispatch 1127): a constant-token structural summary —
  kind, length, element/field shape, a 2-row sample, and the full-vs-compact
  token cost — so an agent grasps a large result's shape + size cheaply, then
  decides to `budget` (page), `aecon` (compact), or skip. Maximal information per
  token (`tests/output_economy.rs`).

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
  in a single event. *Remaining:* true stage-by-stage streaming evaluation (the
  evaluator still computes the full value before chunking the output).

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

- **Governors** per call/session: max output bytes/tokens, max files touched, max
  processes, wall-clock timeout, network-egress cap. Agent runs execute inside a
  budget envelope; breach → `E_BUDGET_EXCEEDED`.
- **Secret hygiene:** redact known secret shapes (API keys, tokens) from results
  *and* audit entries; `sys.env` of a known-secret name returns a handle, not the
  value, unless policy permits. Closes the credential-in-env exposure.

---

## 8. Human surface — reliability

The `.ae` surface keeps readable, unambiguous syntax and gains:

- **Grammar-level everything** — no transpiler passes; the same parser the agent
  surface uses, so the two can never semantically diverge.
- **Boundary type-checking** — 🟡 the structured `E_BAD_ARG` vehicle is landed:
  `safety::bad_arg(builtin, expected, got)` produces a catchable
  `{error:{code:"E_BAD_ARG", message, hint, retryable}}` (used by `rm`, `aecon`,
  `tokens`, `ontology_describe`; see `tests/reliability.rs`). *Remaining:*
  drive it from the HM signatures (`src/typecheck.rs`) so every builtin validates
  args at the boundary automatically rather than per-builtin.
- **Great errors with `hint`** — humans benefit from the same structured errors;
  the REPL renders them richly, agents read them as data.
- **Determinism on demand** — `--deterministic` available to humans too (for
  reproducible scripts / diffs / CI).

---

## 9. Features that no shell has (agentic-native)

- ✅ **Transactions / checkpoints.** `tx_begin` / `tx_commit` / `tx_rollback` /
  `tx_status` (dispatch 1114-1117) over a backup journal (`src/tx.rs`): while a
  transaction is active, `rm`/`file_write` record their pre-modification state
  (`crate::tx::snapshot`), and rollback restores the pre-transaction state —
  overwrites reverted, deletions undone, created files removed
  (`tests/transactions.rs`). v1 scope: single (non-nested) transaction, files.
  Nothing in Bash/PowerShell offers this. Plan/Apply ops: `write`/`append`/`rm`/
  `mkdir`. *Remaining:* extend `snapshot` to more effecting builtins (rmdir trees,
  db) and add nesting/savepoints.
- ✅ **Plan / Apply** (Terraform-style) for a destructive batch: `plan(ops)`
  returns a typed, reviewable summary + a content-bound approval token (executes
  nothing); `apply(ops)` runs the batch atomically inside a transaction — agent
  mode gates it on the plan token, paths are workspace-jailed, any failure rolls
  the whole batch back, and the outcome is audited (dispatch 1119/1120,
  `tests/transactions.rs`). Ties together approval + transactions + structured
  output. Ops: `write`/`rm`/`mkdir`. *Remaining:* more op kinds; a plan diff view.
- ✅ **Be an MCP server, not just a client.** `McpServer::list_builtin_tools`
  exposes every AetherShell builtin as an MCP tool annotated with its `x-effect`
  class; `McpServer::call_builtin` routes calls through `builtins::call` so the
  same policy/approval/jail/audit applies — any MCP-speaking agent gets the full
  typed surface *and* the safety model with zero bespoke integration
  (`agent_api::builtin_tool_specs`, `tests/mcp_tools.rs`). On the wire: a
  `/mcp/v1/builtins` route lists them (kept separate from `/tools` so the OS-tool
  list stays small — progressive disclosure), and `/mcp/v1/tools/:name/execute`
  falls back to `call_builtin` for builtin names, so they're callable over the
  server with full gating. *Remaining:* a strict stdio JSON-RPC MCP transport
  (current server is HTTP) if a spec-exact `tools/list`/`tools/call` is required.
- **Self-correcting loop** falls out of §5.2 + §7.3: structured `code`+`hint`
  errors let an agent repair calls without a human.

---

## 10. Phased roadmap (with file-level touch points)

| Phase | Theme | Key work | Primary files |
| --- | --- | --- | --- |
| **1** | **Measure** | ✅ Token-benchmark harness + **real cl100k tokenizer** (`--features bench-tokenizer`); verdict = legible-first, real-tokenizer-confirmed (§4.0). *Remaining:* broaden the corpus; add a Claude tokenizer for cross-provider check. | `examples/token_bench.rs`, `Cargo.toml` |
| **2** | **Core: errors + effects + determinism** | Structured-error `Value` (§5.2); effect taxonomy macro/table (§5.3); float/serialize determinism (§5.1) | `src/value.rs`, `src/builtins.rs`, `src/eval.rs` |
| **3** | **Safety core (headline)** | Policy engine + workspace jail (§7.1/7.4); approval protocol (§7.2); hash-chained audit (§7.5); wire RBAC | `src/security.rs`, `src/auth.rs`, `src/builtins.rs` (rm/kill/sh/db/docker), `src/agent_api.rs` |
| **4** | **Token economy** | 🟡 First slice: `aecon(value)` compact rendering + `tokens(value)` estimate builtins (§6.2, tests prove AECON < JSON on homogeneous records) + `budget()` paging/truncation (§6.2) + `ontology_manifest`/`ontology_describe` progressive disclosure (§5.4, >4× cheaper than full dump) + CLI `--budget N` flag (REPL applies `budget_value`) + per-call `token_accounting` in `AgentResponse.metadata`. **Core complete.** *Remaining:* per-session token aggregation | `src/builtins.rs`, `src/agent_api.rs`, `tests/output_economy.rs`, `src/value.rs`, `src/metrics.rs`, `src/main.rs` |
| **5** | **Grammar unification** | 🟡 Started: `|.field` projection (with `.a.b` chains) now parsed by the real grammar (`parser::parse_pipe`) + SI suffixes in the lexer + `~x: body` lambda + `?match` prefix + `if`-expression — all additive (`tests/grammar.rs`). **Transpiler retirement in progress** (2 passes retired): `expand_si_suffixes` and `expand_match` (`?`) are removed from the pipeline (grammar covers both); their golden tests migrated to behavior assertions (`eval_aeg` harness) and ONTOLOGY examples updated to pass-through. Recipe: remove the call → `#[allow(dead_code)]` the fn → behavior-migrate the affected text tests → update ONTOLOGY examples. (Lesson: text-coupled golden tests make each retirement cost real migration — SI touched 4 test sites + 2 ontology examples.) *Remaining:* `expand_lambdas`/`expand_pipelines` can only be *partially* retired (they also handle cipher forms the grammar lacks: `\x:`/`~.field`/`>`-pipe); `!try`/`^cond` ciphers overload `Bang`/`Caret`, so they stay transpiler-only; retire the 10-pass transpiler to a shim (§4.3); boundary type-checking (§8) | `src/parser.rs`, `src/typecheck.rs`, `src/transpile/agentic.rs` |
| **6** | **Agentic features** | 🟡 Transactions/checkpoints (`tx_*`) + Plan/Apply (`plan`/`apply`) + builtins-as-MCP-tools (`McpServer::list_builtin_tools`/`call_builtin`) landed. *Remaining:* stateful sessions + streaming execute (§6.3); MCP stdio wire path for builtin tools | `src/tx.rs`, `src/builtins.rs`, `src/agent_api.rs`, `src/mcp.rs`, `src/ai_api/server.rs` |

Phases 2–3 are independent of the §4 benchmark and deliver the safety + reliability
headline immediately; Phase 5 depends on the §4 outcome.

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
- **Self-correction:** ≥X% of failed agent calls repaired without human input,
  attributable to structured `code`+`hint`.

---

## 12. Open questions / risks

- **Tokenizer dependence (§4):** the cipher-vs-legible answer may differ by
  provider; the design accommodates per-provider syntax in schema export if so.
- **Effect-tagging 1,100+ builtins** is labor; mitigate with conservative
  defaults (unknown → most-restrictive class that still runs) + a lint that fails
  CI on untagged builtins.
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

CLI flags set these directly (no env export needed): `--agent` → `AETHER_MODE=agent`,
`--workspace <DIR>` → `AETHER_WORKSPACE`, `--policy <p>` → `AETHER_POLICY`,
`--budget N` → `AE_TOKEN_BUDGET`. E.g. `ae --agent --workspace . script.ae`.
Verified end-to-end (`ae --agent -c 'safety_status()'` reports `mode: "agent"`).

### Immediate next steps (remaining Phase 3)

- ✅ Extended `guard()` to the destructive db/docker/file builtins.
  Remaining candidates to assess: `k8s_delete`, `svc_delete`, `platform_db_delete`.
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
  - *Remaining:* load a manager at startup from config / a login flow, and
    optionally bridge the older `rbac_*` `RBAC_ROLES` role registry to
    `RbacManager` (two RBAC stores exist today).

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
