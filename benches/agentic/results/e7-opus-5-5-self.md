# E7 answered by Claude Opus 5.5 itself -- an informed subject, not the E7 result

2026-09-29. Asked to "run it yourself", the agent maintaining this repository
answered the 50 frozen prompts (hash `dd1eb51326fc5d9c`, dumped with
`e7.mjs --dump-prompts`), wrote all 50 responses in one pass into
`e7-opus-5-5-self-answers.json`, froze that file, and scored it once with
`e7.mjs --provider file --answers …`: the same extraction, execution and
oracle as any provider. No answer was run before the file was frozen, and
there were no retries. Windows, all five interpreters present. Corpus: 500
records, 266 PRs, 91 open.

| Arm | First-try correct |
| --- | ---: |
| sql | 10/10 |
| jq | 10/10 |
| aethershell | 10/10 |
| agentic (with cheatsheet) | 10/10 |
| agentic, no cheatsheet | 10/10 |

## Why this is not the pre-registered result

**The subject is contaminated, and in AetherShell's favour.** In the same
session, before answering, it had read `corpus.mjs` -- the known-good
command for every question in every arm -- and had built or changed several
of the builtins the AetherShell arms use. A model meeting AetherShell cold
has neither. To limit that, the answers used only what each prompt
documents: the default arm uses the pipe-subject signatures and `fn`
lambdas the ontology shows, not `open` or the `.field` row argument added
later that day; the no-cheatsheet arm, whose prompt documents no agentic
syntax, was answered in the standard syntax, as a careful model would.

**Read the table as a ceiling, not a comparison.** Every arm is at 10/10, so
it cannot separate the syntaxes and neither supports nor refutes H1 (a
borrowed syntax beats an invented one on first-try correctness). What it
does show: with the frozen reference material, a frontier model *can* write
all five correctly -- the prompts are sufficient, and the 0/10 AetherShell
arms from `llama3.2:3b` (`e7-local-llama3.2-3b.md`) are the model's limit,
not a defect in the material. The uncontaminated frontier run
`PREREGISTERED_E7.md` specifies is still the experiment.
