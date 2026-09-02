# Creating Agents

An agent is a model in a ReAct loop: it is given a goal and a set of tools,
emits a JSON tool call, sees the result, and repeats until it emits a final
answer or runs out of steps. `agent` runs that loop and returns the final
answer as a string.

## Prerequisites

An agent needs a provider. Without one, every call fails immediately and says
so:

```aethershell
agent("list the files here")
# error[E_UNKNOWN]: No AI provider configured.
# Set AETHER_AI environment variable to: irongate, openai, ollama, or compat
```

See [Configuring Providers](./providers.md).

## Running an agent

```aethershell
agent(<goal>, [tools...], [max_steps], [dry_run])
```

Only the goal is required. It is a task, not a persona — the system prompt is
supplied by the loop, and the goal is the user turn.

```aethershell
agent("Find the three largest files under src/")
```

Tools are named individually or as an array. They are ordinary builtins:

```aethershell
agent("Find every .log file over 1 MB", "ls", "find", "stat")

agent("Summarise what this project builds", ["cat", "ls", "grep"])
```

After the tools come two optional positional arguments: an integer step limit
(default **8**) and a boolean dry-run flag.

```aethershell
# Twenty steps, and do not actually execute the tool calls.
agent("Reorganise the test fixtures", ["ls", "mv"], 20, true)
```

## The record form

The same call can be written as a record, which is easier to build
programmatically. Exactly four keys are read — `goal`, `tools`, `max_steps` and
`dry_run` — and anything else is ignored:

```aethershell
agent({
    goal: "Summarise the open TODOs",
    tools: ["grep", "cat"],
    max_steps: 12,
    dry_run: false
})
```

A record with no `goal` is refused with `agent config requires {goal: String}`.

## Each call starts fresh

`agent` builds a new dialogue every time — a system prompt and your goal — and
returns a string. There is no session, no conversation history, and no reset
builtin, so a second call knows nothing about the first. To carry context
forward, put it in the next goal:

```aethershell
let plan = agent("Break down building a CLI todo app into steps")
let code = agent("Implement this plan: " + plan, ["write", "cat"])
```

This is also how multiple agents are composed; see
[Agent Swarms](./swarms.md) for the coordinated form.

## Which tools an agent may run

Naming a tool in the call does not by itself permit it. Shell-command execution
is default-deny: with `AGENT_ALLOW_CMDS` unset, *no* command is allowed, and the
refusal says exactly that.

```bash
export AGENT_ALLOW_CMDS=ls,cat,grep,git
```

The list is read once, when the security configuration is first built, so
export it before starting `ae`. Setting it from inside a running shell with
`env_set` or `set_env` takes effect only if no command has been validated yet.

Anything outside the list is refused by name, and both the allowed and the
refused attempts are written to the security audit log.

## MCP tools

`agent_with_mcp` takes a goal and an array of MCP tool names, for agents that
should reach tools served over the Model Context Protocol rather than builtins:

```aethershell
agent_with_mcp("Check the deployment status", ["k8s_get_pods", "k8s_logs"])
```

## Rate limiting

`agent` is capped at 10 calls per minute per process. Exceeding it fails with
`Agent rate limit exceeded` rather than queuing.

## Security

An agent runs real commands. Beyond `AGENT_ALLOW_CMDS`:

- `ae --agent` puts the shell in default-deny mode, gating destructive effect
  classes behind approval.
- `ae --workspace <dir>` confines writes and destructive operations to that
  directory.
- `dry_run` lets you watch the loop's intent without executing it.

See [Security & Auth](../advanced/security.md).
