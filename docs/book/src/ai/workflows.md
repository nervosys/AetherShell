# Workflows

A workflow is a template of steps plus an instance that runs it. Steps are
records, a step's `run` names a builtin, and the engine passes data between them
through named variables.

This chapter was removed from the book once, because the sixteen `workflow_*`
names existed in the source and none of them were registered: `workflow_create`
at the prompt answered `unknown builtin`. Everything below was run against the
shell before being written down.

## A first workflow

```aethershell
let t = workflow_pipeline("demo", [
  {id: "up",    run: "upper", args: ["hello"], output: "$.shouted"},
  {id: "count", run: "len",   input: "$.shouted"}
])

let w = workflow_create(t, {input: null})
workflow_execute(w)
# 5
```

`workflow_pipeline` returns a template id; `workflow_create` returns a workflow
id; `workflow_execute` runs it and returns the last step's value. The pipeline
template declares `input` as a required parameter, which is why the record is
passed even when the value is unused.

## Steps

A step is a record. Exactly one key decides what kind of step it is:

| Key | Step | Example |
| --- | --- | --- |
| `run` | call a builtin | `{run: "grep", args: ["TODO", "src"]}` |
| `agent` | run an agent | `{agent: "reviewer", prompt: "$.task"}` |
| `url` | make an HTTP request | `{url: "https://example.com", method: "POST", body: {...}}` |
| `delay_ms` | wait | `{delay_ms: 500}` |
| `emit` | emit an event | `{emit: "ready", payload: 7}` |
| `wait` | wait for an event | `{wait: "ready", timeout_ms: 2000}` |
| `workflow` | run another template | `{workflow: "<template id>"}` |

Any step also takes:

| Key | Meaning |
| --- | --- |
| `id` | its name in results and events (defaults to `step-N`) |
| `input` | the variable to feed it, as `$.name` |
| `output` | where to store its result, as `$.name` |
| `when` | a condition; the step is skipped unless it holds |
| `timeout_ms` | how long it may run before failing |
| `retries` | how many times to retry, with exponential backoff |
| `compensate` | the step to run if a later saga step fails |

`input`/`output` are how stages connect. Above, `up` stores `"HELLO"` in
`shouted` and `count` reads it.

## Conditions

`when` is AetherShell source, evaluated against the workflow's variables:

```aethershell
let t = workflow_pipeline("guarded", [
  {id: "big", run: "upper", args: ["over"], when: "threshold > 5"}
])
```

A condition that does not hold skips the step; one that fails to parse is
treated as *not* holding, because a guard you cannot read must not count as
satisfied.

## Patterns

```aethershell
workflow_pipeline(name, steps)                    # sequential
workflow_map_reduce(name, mapper, reducer)        # parallel map, then reduce
workflow_fan_out(name, workers, aggregator)       # parallel workers, then fan-in
workflow_scatter_gather(name, targets, strategy)  # scatter to agents, gather
workflow_saga(name, transactions)                 # with compensation on failure
workflow_register({name: ..., steps: [...]})      # a plain sequence
```

Fan-out runs its workers concurrently and hands their results to the
aggregator:

```aethershell
let t = workflow_fan_out("scan",
  [{id: "a", run: "upper", args: ["a"]},
   {id: "b", run: "upper", args: ["b"]}],
  {id: "agg", run: "len"})

workflow_execute(workflow_create(t, {input: null}))
# 2
```

`workflow_scatter_gather`'s third argument is a strategy, not a step: `"all"`,
`"first"`, `{first: 3}`, `{timeout_ms: 5000}` or `{consensus: 0.75}`.

### Sagas

A saga takes `[action, compensation]` pairs. If a later action fails, the
compensations for the actions that already succeeded run in reverse order:

```aethershell
let t = workflow_saga("order", [
  [{id: "charge", run: "upper", args: ["charged"]},
   {id: "refund", run: "upper", args: ["refunded"], output: "$.undone"}],
  [{id: "ship",   run: "nope_not_real"},
   {id: "unship", run: "upper", args: ["unshipped"]}]
])

workflow_execute(workflow_create(t, {input: null}))
# error[E_UNKNOWN]: Saga failed at step saga-step-1: unknown builtin: nope_not_real
```

The refund ran; `$.undone` holds `"REFUNDED"`.

## Inspecting a run

```aethershell
workflow_status(w)
# {id: …, template: …, status: completed, steps_completed: 1, variables: {…}}

workflow_status(w).variables.out
workflow_list()
workflow_templates()
```

`workflow_cancel`, `workflow_pause` and `workflow_resume` take a workflow id.

## Circuit breakers

```aethershell
circuit_breaker_create("payments", {failure_threshold: 3})
circuit_breaker_status("payments")
# closed
```

States are `closed`, `open` and `half-open`. The optional record also takes
`success_threshold` and `reset_timeout_ms`.

## Safety

A workflow is not a way around the shell's gates.

- Every `run` step goes through the same dispatcher as a call typed at the
  prompt, so the effect gate, the workspace jail and the audit chain all apply
  to it.
- An `agent` step goes through the `agent` builtin, so `AGENT_ALLOW_CMDS` and
  the rate limit still apply.
- A `url` step goes through the same egress allowlist and SSRF validation as
  `http_get`.
- In agent mode, `workflow_execute` and `workflow_create` require approval —
  running a workflow is the same capability as running the commands inside it.
  `workflow_templates` and the other read-only calls do not.
- A `workflow` step that reaches its own template is refused after 8 levels
  rather than recursing until the stack ends.

## Limits

- Templates and instances live in the shell process. They are not persisted, so
  they are gone when it exits.
- Workflow builtins cannot be called from inside a single-threaded async
  context; blocking it would deadlock, so they report instead. Use a `workflow`
  step to nest one workflow inside another.
- `Choreography` runs its steps sequentially and emits events; there is no
  event-driven scheduler behind it.
