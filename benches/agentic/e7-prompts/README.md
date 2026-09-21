# E7 prompts — frozen

These are the exact prompts each arm receives. Per `../PREREGISTERED_E7.md`:

> The prompts are fixed in this file's companion `e7-prompts/` directory before
> the first run and are not edited afterwards; a change to a prompt starts a new
> run with a new name.

`e7.mjs` hashes this directory and records the hash in every result file. A run
whose prompt hash differs from an earlier run's is a different experiment and
must be reported separately — the runner refuses to merge them.

## Files

| File | Arm | Reference material |
| --- | --- | --- |
| `common.md` | all | The task framing and output contract, identical across arms |
| `sql.md` | SQL | the table schema only |
| `jq.md` | jq | the JSON schema only |
| `aethershell.md` | AetherShell (default) | schema + generated ontology |
| `agentic.md` | AetherShell (agentic) | schema + generated ontology + cheatsheet |
| `agentic-nocheat.md` | AetherShell (agentic, control) | schema + generated ontology |

`{{ONTOLOGY}}` is substituted at run time from the *live* `ontology_describe`
output for the corpus working set, not from a copy pasted here. That is
deliberate: the arm must be judged on the discovery surface the shell actually
ships, and pasting it here would let the two drift.
