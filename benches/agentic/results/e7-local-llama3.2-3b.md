# E7 harness run against a local model -- not the pre-registered result

2026-09-29. `node benches/agentic/e7.mjs <datadir> --provider ollama --model
llama3.2:3b`, temperature 0, one generation per question per arm, frozen
prompts (hash `dd1eb51326fc5d9c`), on Windows because the Ollama server
listens on 127.0.0.1 only. `jq` was not installed there, so that arm was
skipped and named, not scored. Raw rows: `e7-local-llama3.2-3b.json`.

**This is not E7.** `PREREGISTERED_E7.md` specifies one frontier model; a
3.2B-parameter local model exercises the harness against a real generator
and says something about small models, nothing more. n = 10 per arm.

| Arm | First-try correct |
| --- | ---: |
| sql | 5/10 |
| jq | skipped (not installed) |
| aethershell | 0/10 |
| agentic (with cheatsheet) | 0/10 |
| agentic, no cheatsheet | 0/10 |

## What the failures were

- **sql**: five valid queries with the wrong meaning -- `AND`/`OR`
  precedence (q1, q7), an unrounded average (q10), a missing `ORDER BY`
  on the right column (q5), `COUNT(DISTINCT labels)` over JSON text (q6).
  The syntax was never the problem.
- **aethershell**: 8 of 10 produced a command; every one began
  `from_json("issues.json")` -- the parser handed a path -- and most used
  keyword booleans (`and`, `not`) and bare field names
  (`where(state == "closed")`, `map(user)`), which are Python's and
  nushell's habits. 2 produced no fenced command.
- **agentic, both**: 9 of 10 produced no fenced command at all; the model
  did not hold the output contract with the cipher in the prompt.

The direction agrees with H1 (a borrowed syntax beats an invented one on
first-try correctness) and the effect is large, but a 3B model and ten
questions cannot carry that claim. The frozen prompts predate `open` and
the `.field` row argument, which is correct for the protocol and means this
run does not measure them.

## Rerun after cd98133 (and/or/not, arrowless lambdas, keyword fields)

Same model, prompts and corpus. Scores unchanged: sql 5/10, every
AetherShell arm 0/10. What changed is where the AetherShell attempts fail:
five of eight now parse and stop at `from_json("issues.json")`, with an
E_BAD_ARG that names `open("issues.json")` as the repair, where most had
failed in the parser. E7 allows no retry, so this cannot show in its score;
it is the difference between a dead end and a one-step fix.

A caution for anyone repeating this: only 16 of the 40 generations were
identical to the first run's. Ollama at temperature 0 was not deterministic
on this GPU, so "temperature 0, one generation" does not give the
repeatability here that `PREREGISTERED_E7.md` assumes of a hosted model.

## Five arms, 2026-09-29 (jq installed)

With `jq` on PATH, all five arms ran (`e7-local-llama3.2-3b-5arm.json`):

| Arm | First-try correct |
| --- | ---: |
| sql | 5/10 |
| jq | 0/10 |
| aethershell | 0/10 |
| agentic (with cheatsheet) | 0/10 |
| agentic, no cheatsheet | 0/10 |

The jq zeros are the model's, not the harness's (`--replay` scores jq 10/10
on the same host): `select(.state == "open" and .title | contains(…))`
precedence, `jq 'sum'`, a dropped `is_pr` filter. A borrowed syntax does
not rescue a 3B model on one-liners of this length; SQL, the most familiar
and most declarative of the five, is the only one it wrote correctly at
all. Still not E7: that needs the pre-registered frontier model. A hosted
run was attempted and stopped by the harness before any generation --
first an unscoped key, then an account without credit -- and wrote nothing.
