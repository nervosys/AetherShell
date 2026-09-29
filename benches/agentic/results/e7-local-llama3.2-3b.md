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
