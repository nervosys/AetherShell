# IronShell Quick Comparison Chart

## 🎯 One-Minute Summary

**Question**: *"What makes IronShell different?"*

**Answer**: IronShell is the **first shell with programmable AI agents** built into the language itself, calling the shell's own builtins as tools rather than bolting an assistant on the side — behind a default-deny effect gate and a keyed audit chain.

---

## ⚡ Quick Feature Grid

| What You Want      | Bash/Zsh | PowerShell | Warp          | **IronShell** |
| ------------------ | -------- | ---------- | ------------- | --------------- |
| Run scripts        | ✅        | ✅          | ✅             | ✅               |
| Structured data    | ❌        | ✅          | ❌             | ✅               |
| Type safety        | ❌        | ⚠️ Manual   | ❌             | ✅ Auto          |
| AI assistance      | ❌        | ❌          | ✅ Suggestions | ✅ Full          |
| AI agents          | ❌        | ❌          | ❌             | ✅               |
| MCP server         | ❌        | ❌          | ❌             | ✅               |
| Effect gate + jail | ❌        | ❌          | ❌             | ✅               |
| Audit chain        | ❌        | ❌          | ❌             | ✅               |
| Lambda functions   | ❌        | ❌          | ❌             | ✅               |

---

## 💡 Real-World Examples

### **Task**: Analyze log files for errors

**Bash**:
```bash
cat app.log | grep ERROR | awk '{print $3}' | sort | uniq -c
# Text parsing, manual formatting, error-prone
```

**PowerShell**:
```powershell
Get-Content app.log | Where-Object {$_ -match "ERROR"} | 
  Select-Object @{Name='Error';Expression={$_.Split()[2]}} | 
  Group-Object | Sort-Object Count
# Verbose, complex syntax
```

**IronShell**:
```ish
read_text "app.log" 
  | parse_logs 
  | where(fn(log) => log.level == "ERROR")
  | group("error_type")
  | ai "Explain root causes and suggest fixes"
# Type-safe, AI-powered analysis
```

---

### **Task**: Process images in a folder

**Bash**:
```bash
for img in *.jpg; do
  convert "$img" -resize 800x600 "thumb_$img"
done
# External tools, no AI analysis
```

**IronShell**:
```ish
ls "images/" 
  | where(fn(f) => f.ext == "jpg")
  | map(fn(f) => {
      image: f.path,
      size: f.size,
      modified: f.modified
    })
  | to_json
# Structured output, no text parsing
```

---

### **Task**: Create a research report

**Bash**: ❌ Not possible without external tools

**Warp**: ❌ Can suggest commands but can't execute multi-step research

**IronShell**:
```ish
swarm "Research quantum computing applications" [
  "searcher:find recent papers",
  "analyzer:identify key trends",
  "summarizer:create executive summary",
  "fact_checker:verify claims"
] --strategy=specialized
# Autonomous agent swarm handles entire workflow
```

---

## 🏆 Unique Capabilities Matrix

| Capability             | Description                       | Available In                |
| ---------------------- | --------------------------------- | --------------------------- |
| **Effect Gate + Jail** | Default-deny destructive classes   | **IronShell ONLY**        |
| **Keyed Audit Chain**  | Tamper-evident record of every op  | **IronShell ONLY**        |
| **MCP Server**         | Every builtin as an MCP tool       | **IronShell ONLY**        |
| **HM Type Inference**  | Automatic type detection          | **IronShell ONLY**        |
| **Typed Lambdas**      | `fn(x) => x * 2`                  | **IronShell ONLY**        |
| Structured Pipelines   | Tables/records not text           | PowerShell, **IronShell** |
| AI Suggestions         | Command help                      | Warp, **IronShell**       |
| POSIX Compatible       | Run bash scripts                  | Bash, Zsh, **IronShell**  |

---

## 📊 When To Use What

### ✅ Use **IronShell** for:
- 🤖 AI-powered automation
- 🔒 Automation that must be confined and auditable
- 🧠 Complex decision-making tasks
- 🔌 Driving or serving MCP tools
- 📊 Type-safe data pipelines
- 🔄 Modern functional scripting

### ✅ Use **Bash** for:
- Legacy script compatibility
- Simple one-liners
- CI/CD with existing tooling
- Universal Unix/Linux compatibility

### ✅ Use **PowerShell** for:
- Windows administration
- .NET integration
- Enterprise Windows environments

### ✅ Use **Warp** for:
- Modern terminal UI
- Basic AI command help
- Collaborative sessions

---

## 🚀 Migration Path

### From Bash → IronShell

**Phase 1**: Run existing scripts via compatibility layer
```bash
ish --bash your_script.sh
```

**Phase 2**: Add AI features to existing workflows
```ish
# Keep bash logic, add AI
source "legacy.sh"
result | ai "optimize this workflow"
```

**Phase 3**: Rewrite with type-safe pipelines
```ish
# Full IronShell syntax
ls "." 
  | where(fn(f) => f.size > 1000000)
  | ai_analyze
```

---

## 💰 Cost Comparison

| Shell           | License                | AI Cost           | Total Cost         |
| --------------- | ---------------------- | ----------------- | ------------------ |
| Bash            | Free                   | N/A               | $0                 |
| PowerShell      | Free                   | N/A               | $0                 |
| Warp            | Free/$20/mo            | Included          | $0-240/year        |
| **IronShell** | **Free (Open Source)** | **Your own keys** | **$0 + API costs** |

**IronShell Advantage**: 
- Open source forever
- Use any AI provider (OpenAI, Anthropic, Ollama, local models)
- No vendor lock-in

---

## 🎓 Learning Difficulty

```
Easy ──────────────────────────────────────► Hard
 │
 ├── Bash (widely known) ──────────────── 1 week
 ├── IronShell (consistent syntax) ─── 2 weeks
 ├── PowerShell (verbose) ──────────────  1 month
 └── Learning AI agents (new concept) ── 1 month
```

**IronShell is easier than PowerShell** thanks to:
- Type inference (no manual annotations)
- Functional programming (consistent patterns)
- Interactive TUI with examples
- AI assistance for learning

---

## 🔮 Future-Proofing

| Shell           | Last Major Update | Future                  | AI Integration            |
| --------------- | ----------------- | ----------------------- | ------------------------- |
| Bash            | 2019              | Maintenance mode        | External only             |
| Zsh             | 2023              | Slow evolution          | External only             |
| PowerShell      | 2024              | Active but conservative | External only             |
| Warp            | 2024              | Active, AI focus        | Assistive only            |
| **IronShell** | **2025**          | **Rapid development**   | **Native & Programmable** |

---

## ✨ The Bottom Line

**Choose IronShell if you answer YES to any:**

1. ❓ Do you process images, audio, or video in your workflows?
2. ❓ Do you want AI to make decisions, not just suggest commands?
3. ❓ Do you need to coordinate multiple AI agents?
4. ❓ Do you want type safety and functional programming in your shell?
5. ❓ Do you automate complex multi-step processes?
6. ❓ Do you need distributed task execution?
7. ❓ Do you want a modern language with bash compatibility?

**If you answered NO to all:** Stick with Bash or your current shell.

**If you answered YES to 1+:** IronShell will revolutionize your workflow.

---

## 📈 Adoption Strategy

**Week 1**: Install and run demos
```bash
cargo install --path . --bin ish
ish tui  # Explore the interface
ish demos/showcase.ish
```

**Week 2**: Use alongside existing shell
```bash
# Use bash for familiar tasks
cd /projects && git pull

# Use IronShell for AI tasks
ish -c 'ls | where(fn(f) => f.size > 1000000) | to_json'
```

**Week 3**: Build your first AI agent
```ish
agent "Analyze error logs daily" ["read_text", "parse_logs", "ai"] 5 false
```

**Month 2+**: Migrate critical workflows
```ish
# Replace bash scripts with type-safe IronShell
swarm "Daily DevOps tasks" [...]
```

---

## 🎯 Key Takeaway

> **IronShell is not just "a shell with AI"**—it's a **complete rethinking of shell design for the AI era**, combining type safety, functional programming, agents that call builtins as tools, and a default-deny effect gate over a keyed audit chain.

**The future of automation is programmable AI agents. IronShell brings that future to your terminal today.**

---

**Questions?** See the [full comparison](SHELL_COMPARISON.md) for detailed analysis.
