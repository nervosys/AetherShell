# Agents

AI Agents in AetherShell are autonomous entities that can understand context, use tools, and accomplish complex tasks.

## Creating an Agent

```aethershell
# Simple agent with just a system prompt
let assistant = agent("You are a helpful coding assistant")

# Ask the agent
assistant("How do I reverse a string in Rust?")
```

## Agent with Tools

Agents become powerful when they can use tools:

```aethershell
let devops = agent("You are a DevOps expert who helps manage systems", {
    tools: ["ls", "cat", "grep", "ps", "http_get"]
})

# The agent can now use these commands
devops("Find all .log files larger than 1MB")
# Agent will:
# 1. Use ls to find files
# 2. Filter by extension and size
# 3. Return the results
```

## Available Tools

Agents can use any builtin command as a tool:

| Category        | Tools                                       |
| --------------- | ------------------------------------------- |
| **File System** | `ls`, `cat`, `read`, `write`, `mkdir`, `rm` |
| **Search**      | `grep`, `find`, `which`                     |
| **System**      | `ps`, `env`, `pwd`, `cd`                    |
| **Network**     | `http_get`, `http_post`, `curl`             |
| **Data**        | `json_parse`, `json_stringify`, `csv_parse` |

## Tool Whitelist

For security, configure which tools agents can use:

```bash
# Environment variable
export AGENT_ALLOW_CMDS="ls,cat,grep,http_get"
```

```aethershell
# Or in code
env_set("AGENT_ALLOW_CMDS", "ls,cat,grep,find")
```

## Agent Options

```aethershell
let agent = agent("System prompt", {
    # Model selection
    model: "gpt-4o",
    
    # Available tools
    tools: ["ls", "cat", "grep"],
    
    # Max iterations for complex tasks
    max_iterations: 10,
    
    # Temperature for creativity
    temperature: 0.3,
    
    # Context window
    context_length: 4096,
    
    # Timeout in seconds
    timeout: 300
})
```

## Conversation Memory

Agents maintain context across calls:

```aethershell
let coder = agent("You are a Python expert")

coder("Write a function to calculate factorial")
# Returns factorial implementation

coder("Now add memoization to it")
# Remembers the previous function and modifies it

coder("Add type hints")
# Still remembers context
```

## Clearing Memory

There is no reset builtin. An agent's memory is bound to the agent, so a fresh
one is the way to start over:

```aethershell
let new_coder = agent("You are a Python expert")
```

## Multi-Step Tasks

Agents can break down complex tasks:

```aethershell
let project_helper = agent("You help set up projects", {
    tools: ["ls", "mkdir", "write", "cat"]
})

project_helper("Create a new Rust project structure with src/main.rs, Cargo.toml, and README.md")
# Agent will:
# 1. Create directories
# 2. Generate file contents
# 3. Write files
# 4. Verify the structure
```

## Error Handling

```aethershell
let result = try {
    agent("helper")("Do something risky")
} catch err {
    print("Agent error: " + err.message)
    null
}
```

## Agent Patterns

### Research Agent

```aethershell
let researcher = agent("You research topics and summarize findings", {
    tools: ["http_get", "grep"]
})

researcher("Find information about WebAssembly and summarize the key benefits")
```

### Code Review Agent

```aethershell
let reviewer = agent("You review code for bugs and improvements. Be concise.", {
    tools: ["cat", "grep"]
})

reviewer("Review the file src/main.rs for potential issues")
```

### Data Analysis Agent

```aethershell
let analyst = agent("You analyze data and provide insights", {
    tools: ["cat", "grep", "ls"]
})

analyst("Analyze the CSV files in data/ and summarize the trends")
```

## Agent Swarms

For complex tasks, use multiple coordinated agents:

```aethershell
# Create specialized agents
let planner = agent("You break down tasks into steps")
let coder = agent("You write code", { tools: ["write", "cat"] })
let tester = agent("You test code", { tools: ["cat", "grep"] })

# Coordinate them
let plan = planner("Create a plan to build a CLI todo app")
let code = coder("Implement: " + plan)
let review = tester("Test this implementation: " + code)
```

## Best Practices

1. **Clear system prompts** - Be specific about the agent's role and capabilities
2. **Minimal tools** - Only give agents the tools they need
3. **Set timeouts** - Prevent runaway agents
4. **Handle errors** - Agents can fail, always have fallbacks
5. **Clear memory** - Reset agents when switching contexts

## Security Considerations

- Agents can execute commands on your system
- Always use the tool whitelist (`AGENT_ALLOW_CMDS`)
- Review agent actions in sensitive environments
- Consider sandboxing for untrusted inputs
