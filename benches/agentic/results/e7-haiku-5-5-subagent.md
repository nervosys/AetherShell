# E7 answered by a fresh Claude Haiku 5.5 subagent

2026-10-29. The frozen prompts (hash `dd1eb51326fc5d9c`, dumped with
`e7.mjs --dump-prompts`) were handed to a Claude Haiku 5.5 subagent
(`claude-haiku-5-5`) that started with no context from the session that
maintains this repository. Its instructions allowed reading only the prompt
directory and writing one answers file. Its transcript shows 50 distinct
reads, all in that directory, and one write: nothing run, nothing tested, no
other file read. The file was frozen (SHA-256 `c1f6ff14c4f38177…`) and scored
once with `e7.mjs --provider file`. Windows, all five interpreters present,
`ae` built from master at 82f5a7f, under a 4 GiB job-object memory cap.

The corpus was fetched fresh with `prepare.mjs`: 500 records from `cli/cli`,
207 PRs, 63 open (numbers 14315..14870). The earlier E7 runs used an older
fetch (266 PRs, 91 open), so the answers differ while the questions do not.

| Arm | First-try correct (scored) | Relabelled q7–q10 (diagnostic) |
| --- | ---: | ---: |
| sql | **5/10** | 9/10 |
| jq | **6/10** | 10/10 |
| aethershell | **6/10** | 10/10 |
| agentic, with cheatsheet | **4/10** | 5/10 |
| agentic, no cheatsheet | **6/10** | 10/10 |

The first column is the pre-registered score. The second is not a rescoring;
it is reported so the first can be read.

## What went wrong in the first column

In every arm, the answers filed under q7, q8, q9 and q10 answer the *next*
question: the "q7" answer computes the largest comment count (q8), "q8" the
authors with both a PR and an issue (q9), "q9" the mean comment count (q10),
and "q10" the open `bug` count (q7). Each returns exactly the oracle's value
for the question it answers. The subagent filed four answers under the wrong
keys, identically in all five arms. It did not write them wrongly. Because the
rotation is the same in every arm, it costs each arm the same 4 points, and
the comparison between arms survives in both columns.

Relabelled, the remaining failures are:

- **sql q5**: the right five numbers, printed one per line where the oracle
  wants them comma-separated (correct fact, wrong form).
- **agentic, with cheatsheet**: q1, q5, q8, q9 and q10 do not parse. Each
  writes `w~(...)`, which the cheatsheet's `w~` lambda shorthand does not
  accept (`expected parameter name after '~'`). The same model, given the same
  prompt without the cheatsheet, wrote standard syntax and got all ten.

## Against the hypotheses

**H1: a borrowed syntax beats an invented one on first-try correctness.**
Not supported for this model. SQL and jq do no better than AetherShell's
standard syntax in either column (scored 5 and 6 against 6; relabelled 9 and
10 against 10). The invented *agentic* syntax is the one that loses.

**The cheatsheet costs correctness.** The cheatsheet arm is the only one below
the others in both columns. The no-cheatsheet control, which has the same
ontology, scores the same as the standard arm. That is the same direction as
the Haiku 4.5 run (`e7-haiku-4-5-subagent.md`: 2/10 with the cheatsheet,
7/10 without it).

## Why this is not the pre-registered result

The pre-registration names one frontier model called through its API at
temperature 0. This is a subagent answering frozen prompts. That is the same
method as the Haiku 4.5 run, not the specified one. The API run was attempted
the same day and stopped before its first generation: the configured
`ANTHROPIC_API_KEY` was rejected (`invalid x-api-key`). No tokens were bought
and no result was written. It remains the experiment.

Files: `e7-haiku-5-5-subagent-answers.json` (the frozen answers) and
`e7-haiku-5-5-subagent.json` (the scored rows, as `e7.mjs` wrote them).
