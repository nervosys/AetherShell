# Agent Swarms

## What `swarm` currently does

`swarm` is a builtin, and it works, but not the way its name suggests: it takes
the same arguments as [`agent`](./agents.md) and delegates to the same
single-agent loop. `ai::agents::swarm::run_sync` is one line — it calls
`ai::agents::run_sync`.

```aethershell
swarm("Analyze this project thoroughly", ["ls", "cat", "grep"], 12)
```

That runs one agent with those three tools and a twelve-step limit. It does not
create multiple agents.

The multi-agent engine below **exists in the library and is not reachable from
the shell**. `Swarm`, its blackboard and its two coordinators are defined in
`src/ai.rs` and never constructed by any builtin. They are documented here so
the distinction is on the record, not because you can call them today.

- **Coordination policies.** `RoundRobin` takes agents in turn; `Router` sends
  each turn to the agent whose declared capabilities best match. Both
  coordinators are implemented; neither is selectable from the shell, because
  the `swarm` builtin reads only `goal`, `tools`, `max_steps` and `dry_run`.
- **Blackboard.** Agents would share a message list with kinds `note`,
  `thought` and `final`, and could delegate with
  `{"type": "delegate", "target": "...", "input": "..."}`.

If you want several agents today, chain calls and pass each result into the next
goal — see [Creating Agents](./agents.md#each-call-starts-fresh).

## Model override

Both builtins honour an environment variable for the model URI:

```aethershell
set_env("AETHER_AGENT_MODEL_URI", "openai:gpt-4o")
set_env("AETHER_SWARM_AGENT_MODEL_URI", "ollama:llama3")
```

There is no `model` key in the record form. `agent`/`swarm` read exactly four
keys — `goal`, `tools`, `max_steps`, `dry_run` — and silently ignore the rest,
so a `model:` entry has no effect.

## Choosing tools

Tools are the builtins the agent may call. Name them individually, or as an
array:

```aethershell
agent("Find large files", ["ls", "cat", "grep"])
```

An **empty array gives the agent no tools at all**, not every tool. Tool names
are resolved one by one against the registry, so an empty list resolves to an
empty toolset and the agent can only answer from the model.

```aethershell
agent("Analyze the project", [])   # no tools; the model answers unaided
```

## Agent security

### Command allowlist

`AGENT_ALLOW_CMDS` restricts which shell commands an agent may run, and it is
default-deny: unset, *nothing* is allowed, and the refusal says so.

```bash
export AGENT_ALLOW_CMDS=ls,cat,grep,wc
```

The list is read once, when the security configuration is first built. Setting
it from inside a running shell with `set_env` only takes effect if no command
has been validated yet, so prefer exporting it before starting `ae`.

### Measured limits

- **Prompt validation** — goals are capped at 4,000 characters and at most 50
  newlines, rejected if empty or containing a null byte, and screened for
  injection.
- **Argument validation** — shell metacharacters are rejected in tool
  arguments.
- **Rate limiting** — 10 agent calls per minute, and within the agent API, 10
  plans and 5 executions per minute.
- **Output cap** — 10 MB per execution.
- **Timeout** — 30 seconds per execution, on every platform.
- **Memory** — 512 MB, **on Linux and macOS only**. The Windows
  `configure_sandbox` is a documented no-op: Job Object sandboxing is a TODO,
  so on Windows you get the timeout and the output cap and nothing else.
- **No shell escape** — the `sh` builtin is gated behind `AETHER_ALLOW_SH=true`
  and is unavailable by default.

## Agents with MCP tools

`agent_with_mcp` takes a goal, an array of tool names, and optionally one
endpoint or an array of endpoints:

```aethershell
agent_with_mcp("Analyze repository", ["read_file", "list_dir"], "http://localhost:9090")
```

The agent discovers the available tools from the server and can call them
during its loop. Note that `mcp_server_start` takes a **configuration record**,
not a URL string.

## Practical examples

### Code review

```aethershell
agent({
  goal: "Review src/main.rs for potential bugs, style issues, and missing error handling",
  tools: ["cat", "grep", "wc"],
  max_steps: 8
})
```

### Project analysis

```aethershell
agent({
  goal: "Describe this Rust project: structure, dependencies, and test coverage",
  tools: ["ls", "cat", "grep", "find", "wc", "fs_tree"],
  max_steps: 25
})
```

### Git summary

```bash
export AGENT_ALLOW_CMDS=ls,cat,grep,git
```

```aethershell
agent({
  goal: "Summarize all changes since the last release tag",
  tools: ["git_log", "git_diff", "cat"],
  max_steps: 10
})
```
