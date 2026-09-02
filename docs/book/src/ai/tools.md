# Tool Use

An agent is only as useful as the things it can actually run. AetherShell ships
a catalogue of external OS tools — `grep`, `curl`, `git`, and so on — each with a
description, a parameter list, the platforms it runs on, and a safety level. The
same catalogue backs both the agent loop and the `tool_*` builtins below, so
what an agent can reach is exactly what you can inspect from the prompt.

## Listing the catalogue

```aethershell
tool_list() | len
# 198

tool_list() | first
# {category: TextProcessing, command: grep, description: Search text patterns in files,
#  name: grep, requires_admin: false, safety: Safe, supported_os: [len=5]}
```

## Finding a tool

`tool_search` takes a query and returns the tools recommended for it,
falling back to a plain name and description match when nothing is recommended:

```aethershell
tool_search("http") | len
# 7
```

`tool_info` returns the full record for one tool, including its parameters and
worked examples:

```aethershell
tool_info("ls")
# {category: FileSystem, command: ls, common_args: [len=3],
#  description: List directory contents, examples: [len=1], name: ls,
#  parameters: [len=3], requires_admin: false, safety_level: Safe,
#  supported_os: [len=5]}
```

## Schemas for model tool-calling

`tool_schema` renders the catalogue as OpenAI-style function schemas, ready to
hand to a model that supports tool calling:

```aethershell
tool_schema() | first
# {function: {…}, type: function}
```

## Running a tool

```aethershell
tool_exec(<name>, [args], [allow_dangerous])
```

`tool_execute` is an alias for the same builtin. Only the name is required;
`args` may be an array of strings or a single string.

```aethershell
tool_exec("git", ["status", "--short"])
```

Two things can stop a call, and both report rather than guess:

- **Platform.** A tool is only run where it is supported. On Windows,
  `tool_exec("ls", ["docs/book"])` returns
  `error[E_UNKNOWN]: Tool execution failed: Tool 'ls' is not supported on Windows`
  instead of falling back to something that merely looks similar.
- **Safety.** Every tool carries a level — `Safe`, `Caution`, `Dangerous`, or
  `Critical`. Of the 198 catalogued tools, 131 are `Safe`, 49 are `Caution`, 14
  are `Dangerous` and 4 are `Critical`:

  ```aethershell
  tool_list() | where(fn(t) => t.safety == "Dangerous") | len
  # 14
  ```

  `Safe` and `Caution` run normally. `Dangerous` and `Critical` are refused
  outright unless the third argument is `true`, and the refusal names the level
  rather than failing vaguely:

  ```aethershell
  tool_exec("iptables", ["-L"])
  # error[E_UNKNOWN]: Tool execution failed: Tool 'iptables' has safety level
  # Critical. Set allow_dangerous=true to execute.
  ```

  A tool marked `requires_admin` is additionally refused unless the process is
  actually privileged.

## Over MCP

`ae mcp stdio` serves the shell as an MCP server. `tools/list` returns **three**
tools, not one per builtin:

| Tool | Purpose |
| --- | --- |
| `ontology_manifest` | the categories, with counts and effect classes |
| `ontology_describe` | expand a category into its builtins, or one builtin into full detail |
| `aether` | invoke a builtin by name |

That is deliberate. Advertising several hundred tool schemas would spend an
agent's context before it had read a single result; the manifest is the compact
index and detail is fetched for the slice actually needed. Every builtin is
reachable through `aether`, and every call goes through the same safety model
as one typed at the prompt.

`tool_exec` also passes through the ordinary execution guard, so agent mode's
effect gate and the workspace jail apply to it exactly as they do to any other
process-spawning builtin. See [Security & Auth](../advanced/security.md).
