# 🚀 AetherShell Live Demo

## Welcome!

This document shows AetherShell's unique features through working examples.

---

## ✅ Demo 1: Basic Pipelines (WORKS NOW!)

Try these in the REPL:

```ae
[1, 2, 3, 4, 5] | map fn(x) => x * 2 | print
```
Output: `[2, 4, 6, 8, 10]`

```ae
[10, 20, 30, 40, 50] | where fn(x) => x > 25 | print
```
Output: `[30, 40, 50]`

---

## ✅ Demo 2: Reduce Operations (WORKS NOW!)

```ae
[1, 2, 3, 4, 5] | reduce fn(a, b) => a + b 0 | print
```
Output: `15` (sum of all numbers)

```ae
[1, 2, 3, 4, 5] | map fn(x) => x * x | reduce fn(a, b) => a + b 0 | print
```
Output: `55` (sum of squares: 1+4+9+16+25)

---

## ✅ Demo 3: Pattern Matching (WORKS NOW!)

```ae
match 42 {
  x if x > 50 => "Large",
  x if x > 20 => "Medium",
  _ => "Small"
} | print
```
Output: `"Medium"`

```ae
match 15 {
  x if x > 50 => "Large",
  x if x > 20 => "Medium",
  _ => "Small"
} | print
```
Output: `"Small"`

---

## ✅ Demo 4: Records (Structured Data) (WORKS NOW!)

```ae
{name: "Alice", age: 30, role: "Engineer"} | print
```
Output: `{name: "Alice", age: 30, role: "Engineer"}`

---

## ✅ Demo 5: Filtering Records (WORKS NOW!)

```ae
[
  {name: "Alice", salary: 120000},
  {name: "Bob", salary: 95000},
  {name: "Charlie", salary: 140000}
]
  | where fn(u) => u.salary > 100000
  | print
```
Output: Only Alice and Charlie (salary > $100k)

---

## ✅ Demo 6: Complex Pipelines (WORKS NOW!)

```ae
[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  | where fn(x) => x > 3
  | map fn(x) => x * x
  | where fn(x) => x < 50
  | print
```
Output: `[16, 25, 36, 49]`

Step by step:
1. Start with [1..10]
2. Keep only > 3: [4, 5, 6, 7, 8, 9, 10]
3. Square each: [16, 25, 36, 49, 64, 81, 100]
4. Keep only < 50: [16, 25, 36, 49]

---

## 🔮 Demo 7: AI Features (Simulated - Requires API Keys)

### Basic AI Call
```ae
ai "What is Rust?" {model: "openai:gpt-4o-mini"}
```

### AI with Context
```ae
context := read_text "README.md"
ai "Summarize this project" {context: context, model: "openai:gpt-4o"}
```

### Multi-Modal AI
```ae
ai "Describe this image" {images: ["diagram.png"], model: "openai:gpt-4o"}
```

---

## 🤖 Demo 8: Multi-Agent Swarms (Simulated - Requires API Keys)

### Create a Swarm
```ae
swarm := swarm [
  {id: "researcher", model: "openai:gpt-4o", role: "Research topics"},
  {id: "writer", model: "anthropic:claude-3-opus", role: "Write content"},
  {id: "reviewer", model: "openai:gpt-4o", role: "Review quality"}
] "router"
```

### Execute Complex Task
```ae
result := swarm.execute "Create comprehensive API documentation"
```

---

## 🔌 Demo 9: MCP Servers (Simulated - Requires Setup)

### Start Filesystem Server
```ae
fs := mcp_server_start {
  name: "filesystem",
  type: "builtin",
  config: {allowed_paths: ["/home/user/projects"]}
}
```

### Start AWS Server
```ae
aws := mcp_server_start {
  name: "aws",
  type: "cloud",
  config: {provider: "aws", services: ["s3", "ec2"], readonly: true}
}
```

### Agent with MCP Tools
```ae
devops := agent_with_mcp
  "DevOps automation agent"
  ["mcp:list_files", "mcp:read_file", "mcp:s3_list"]
  [fs.endpoint, aws.endpoint]

result := devops.execute {task: "List all project files and S3 buckets"}
```

---

## 💬 Demo 10: AI Protocols (Simulated - Requires API Keys)

### A2A (Agent-to-Agent Communication)
```ae
a2a_register("agent1")
a2a_register("agent2")
a2a_agents()                     # => [agent1, agent2]

a2a_send("agent2", "Process data")
a2a_receive()
a2a_status()                     # => {active: true, agents: 2, pending_messages: 1}
```

### NANDA (Negotiation And Decision Aggregation)
```ae
let proposal = nanda_propose("TaskAllocation", { task: "Implement feature X" })
# => {id: "prop_…", name: "TaskAllocation", status: "pending", threshold: 0.5}

nanda_vote(proposal.id, true)
nanda_quorum(proposal.id)
nanda_consensus(proposal.id)
nanda_status()
```

---

## 📊 What Makes AetherShell Unique?

### ✅ Currently Working (Try Now!)
- **Type-safe pipelines** - Data flows as structured values, not text
- **Pattern matching** - Elegant control flow
- **First-class functions** - Compose and transform with ease
- **Records** - Structured data built into the language
- **Functional programming** - map, filter, reduce, and more

### 🔮 Advanced Features (Requires Setup/API Keys)
- **Multi-agent swarms** - Coordinate teams of AI agents
- **MCP servers** - Safe infrastructure integration
- **A2A protocol** - Agent-to-agent communication
- **NANDA protocol** - Consensus and negotiation
- **Multi-modal AI** - Process images, audio, video

---

## 🎯 Quick Start Commands

### Try These NOW in the REPL:

1. **Simple map:**
   ```
   [1, 2, 3] | map fn(x) => x * 2 | print
   ```

2. **Filter and transform:**
   ```
   [1, 2, 3, 4, 5, 6] | where fn(x) => x > 3 | map fn(x) => x * x | print
   ```

3. **Sum numbers:**
   ```
   [10, 20, 30] | reduce fn(a, b) => a + b 0 | print
   ```

4. **Pattern matching:**
   ```
   match 100 { x if x > 50 => "big", _ => "small" } | print
   ```

5. **Work with records:**
   ```
   {name: "AetherShell", version: 1, awesome: true} | print
   ```

---

## 📚 Learn More

- **Quick Reference:** `docs/QUICK_REFERENCE.md`
- **Type System Guide:** `docs/TYPE_SYSTEM_GUIDE.md`
- **MCP Servers Guide:** `docs/MCP_SERVERS_GUIDE.md`
- **Examples:** `examples/` directory (17 complete examples)

---

## 🚀 Key Takeaway

**AetherShell is the only shell that:**
1. Has type-safe functional pipelines (like Haskell)
2. Orchestrates multi-agent AI swarms
3. Integrates infrastructure via MCP servers
4. Enables agent collaboration with A2A and NANDA protocols
5. Processes multi-modal AI (images, audio, video) natively

**No other shell can do what AetherShell does!**

---

**Currently in REPL? Try the examples above! They all work! ✅**


