# E7 answered by a fresh Claude Haiku 4.5 subagent

2026-09-29. The frozen prompts (hash `dd1eb51326fc5d9c`) were handed to a
Claude Haiku 4.5 subagent that started with no context from the session
that maintains this repository. Its instructions allowed reading only the
prompt directory and writing one answers file; its transcript confirms 50
reads, all in that directory, and one write -- nothing run, nothing tested,
no other file read. The file was frozen and scored once with
`e7.mjs --provider file`. Windows, all five interpreters present.

| Arm | First-try correct | Correct fact, wrong form |
| --- | ---: | ---: |
| sql | **9/10** | +1 (q5) |
| jq | **9/10** | +1 (q4) |
| aethershell | **7/10** | +1 (q4) |
| agentic, with cheatsheet | **2/10** | +1 (q4) |
| agentic, no cheatsheet | **7/10** | +1 (q4) |

The first column is the pre-registered score; the second counts answers
whose fact was right but whose form the oracle rejects, reported only so
the failures below can be read, not as a rescoring.

## Against the hypotheses

**H1 -- a borrowed syntax beats an invented one on first-try correctness.**
Supported for this model: SQL and jq 9/10 against AetherShell's 7/10, and
2/10 for the cipher. Ten questions, one model, one generation each; the
pre-registration's frontier-model run would carry more weight.

**The cheatsheet costs correctness, not only tokens.** The same prompt with
the 1,196-token cipher reference scored 2/10; without it, Haiku fell back
to the standard syntax and scored 7/10. `LANGUAGE_FIRST_PRINCIPLES.md`
priced the cheatsheet as standing context; this run prices it as errors.

## The failures

- **Right fact, rejected form (4):** SQL q5 printed the numbers one per
  line rather than joined; jq q4 printed a JSON string (no `-r`);
  AetherShell q4 in both arms returned the whole group record
  `{Count: 66, Group: …, Name: williammartin}` rather than `williammartin 66`.
- **Pipe precedence in the standard syntax (q1, q9):**
  `x.title | lower() | contains("security") || x.body | …` and
  `g.Group | any(…) && g.Group | any(…)`. `|` binds looser than `||`/`&&`,
  as it does in jq, so these do not parse as intended. A model writing
  predicates inline reaches for this shape.
- **The cipher, from its own cheatsheet (7 of 8 failures):**
  single-letter builtins called with parentheses (`n()`, `b()`, `u()`) do
  not parse, though bare `n` does; a second field reference in one
  predicate (`w~.state=="open"&&!.is_pr`) needs its own `~`, which no
  cheatsheet example shows; and `u` is `uniq`, adjacent-only, so
  `m~.labels|b|u|n` answers 565 where the question's answer is 42 -- a
  wrong number with no error.

## The same answers, rescored after the fixes they motivated

The frozen answers file, unchanged (byte-compared), scored again on the
build that fixed `n()`, `u` and the unbound second `.field`, and that names
the `"${g.Name} ${g.Count}"` idiom in `group`'s ontology entry:

| Arm | Before | After |
| --- | ---: | ---: |
| sql | 9/10 | 9/10 |
| jq | 9/10 | 9/10 |
| aethershell | 7/10 | 7/10 |
| agentic, with cheatsheet | 2/10 | **6/10** |
| agentic, no cheatsheet | 7/10 | 7/10 |

This is not a new measurement of the model -- it is the same text through a
changed interpreter -- and it is not a reason to keep the cipher: its best
score is still below the standard syntax's, and it still costs a 1,196-token
reference. The ontology change cannot show here, because the answers were
written against the old entry; it is for the next run.
