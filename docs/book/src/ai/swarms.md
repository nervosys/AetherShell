# Agent Swarms

AetherShell supports autonomous AI agents that can use shell builtins as tools, and multi-agent swarms that coordinate to solve complex tasks.

## Single Agent

An agent is an AI model in a ReAct (Reason + Act) loop, emitting tool calls and observations until it reaches a final answer.

### Basic Usage

```aethershell
agent "Find the 3 largest files in src/"
```

The agent will:
1. Plan its approach
2. Call tools like `ls`, `sort_by`, `take`
3. Observe results
4. Return a final answer

### Specifying Tools

Control which builtins the agent can use:

```aethershell
# Allow specific tools
agent "Find large files" ["ls", "cat", "grep"]

# Allow all tools (use with caution)
agent "Analyze the project" []
```

### Configuration Options

```aethershell
# Positional: goal, tools, max_steps
agent "Analyze logs" ["cat", "grep", "wc"] 6

# Record syntax for full control
agent {
  goal: "Find and fix TODO comments",
  tools: ["grep", "cat", "file_replace"],
  max_steps: 10,
  dry_run: true,          # Show plan without executing
  model: "openai:gpt-4o"  # Override model
}
```

### Max Steps

Limit how many tool calls an agent can make (default: 8):

```aethershell
agent "Quick check" ["ls"] 3        # At most 3 tool calls
agent "Deep analysis" ["ls", "cat", "grep"] 20   # Up to 20 steps
```

### Dry Run

Preview what the agent would do without executing:

```aethershell
agent { goal: "Delete temp files", tools: ["ls", "rm"], dry_run: true }
# Shows the plan without actually deleting anything
```

## Agent Security

Agents operate in a sandboxed environment with multiple safety layers.

### Command Allowlist

The `AGENT_ALLOW_CMDS` environment variable restricts which commands agents can call:

```aethershell
set_env "AGENT_ALLOW_CMDS" "ls,cat,grep,wc"

# The agent can only use these 4 commands
agent "Analyze source code" ["ls", "cat", "grep"]
```

### Safety Measures

- **Command validation**: Shell metacharacters (`;`, `|`, `&&`, `||`, `` ` ``) are blocked in tool arguments
- **Prompt validation**: Agent goals are limited to 4,000 characters with injection prevention
- **Rate limiting**: 10 plans/minute, 5 executions/minute
- **Execution sandbox**: 30-second timeout, 10MB output limit, 512MB memory limit
- **No escalation**: Agents cannot call `sh`, `shell`, or `exit` by default

### Model Override

Set the model for all agent calls:

```aethershell
set_env "AETHER_AGENT_MODEL_URI" "openai:gpt-4o"
# All agents now use GPT-4o regardless of default provider

# Or per-call:
agent { goal: "...", model: "ollama:codellama" }
```

## Multi-Agent Swarms

Swarms coordinate multiple agents with different specializations.

### Basic Swarm

```aethershell
swarm "Analyze this project thoroughly" ["ls", "cat", "grep"] 12
```

A swarm creates multiple agents that share a **blackboard** — a shared communication space where agents post notes, thoughts, and intermediate results.

### Swarm Configuration

```aethershell
swarm {
  goal: "Review code quality and security",
  tools: ["ls", "cat", "grep", "find"],
  max_steps: 20,
  dry_run: false
}
```

### Coordination Policies

Swarms use a coordination policy to decide which agent acts next:

| Policy | Description |
|--------|-------------|
| `RoundRobin` | Agents take turns in order |
| `Router` | A coordinator selects the best agent for each step |

### Blackboard Communication

Agents communicate through the shared blackboard using structured messages:

| Message Kind | Purpose |
|-------------|---------|
| `note` | Share observations or intermediate findings |
| `thought` | Internal reasoning visible to other agents |
| `final` | Propose a final answer |

Agents can also delegate tasks to specific peers:

```
# Agent A delegates to Agent B
{"type": "delegate", "target": "agent-b", "input": "check security of auth.rs"}
```

### Swarm Model Override

```aethershell
set_env "AETHER_SWARM_AGENT_MODEL_URI" "ollama:llama3"
# All swarm agents use this model
```

## Agent with MCP Tools

Connect agents to external tool servers via the Model Context Protocol:

```aethershell
# Start an MCP server
mcp_server_start "http://localhost:9090"

# Create an agent that uses MCP tools
agent_with_mcp "Analyze repository" ["mcp:read_file", "mcp:list_dir"] "http://localhost:9090"
```

The agent discovers available tools from the MCP server and can call them during its execution loop.

## Practical Examples

### Code Review Agent

```aethershell
agent {
  goal: "Review src/main.rs for potential bugs, style issues, and missing error handling",
  tools: ["cat", "grep", "wc"],
  max_steps: 8
}
```

### Project Analysis Swarm

```aethershell
swarm {
  goal: "Provide a comprehensive analysis of this Rust project: structure, dependencies, code quality, and test coverage",
  tools: ["ls", "cat", "grep", "find", "wc", "fs_tree"],
  max_steps: 25
}
```

### Automated Git Workflow

```aethershell
set_env "AGENT_ALLOW_CMDS" "ls,cat,grep,git_status,git_diff,git_log"

agent {
  goal: "Summarize all changes since the last release tag",
  tools: ["git_log", "git_diff", "cat"],
  max_steps: 10
}
```

### Research Agent

```aethershell
agent {
  goal: "Find all TODO and FIXME comments in the project and create a prioritized list",
  tools: ["grep", "cat", "ls"],
  max_steps: 12
}
```
