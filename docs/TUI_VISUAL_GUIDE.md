# AetherShell TUI Visual Guide

**Launch TUI Mode**: `ae --tui`

## 🎨 TUI Interface Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🚀 AetherShell TUI v0.1.0                      [Chat] [Agents] [MCP]│
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 💬 Conversation                                              │   │
│  │                                                              │   │
│  │ 👤 You (10:45 AM):                                          │   │
│  │ What are the key features of Rust?                          │   │
│  │                                                              │   │
│  │ 🤖 GPT-4o-mini (10:45 AM):                                  │   │
│  │ Rust's key features include:                                │   │
│  │ 1. **Memory Safety** - No null pointers, no data races     │   │
│  │ 2. **Zero-cost Abstractions** - High-level without overhead│   │
│  │ 3. **Ownership System** - Compile-time memory management   │   │
│  │ 4. **Pattern Matching** - Powerful match expressions       │   │
│  │ 5. **Concurrency** - Safe multi-threading                  │   │
│  │                                                              │   │
│  │ 👤 You (10:46 AM):                                          │   │
│  │ Show me an example                                          │   │
│  │                                                              │   │
│  │ 🤖 GPT-4o-mini (10:46 AM):                                  │   │
│  │ ```rust                                                     │   │
│  │ fn main() {                                                 │   │
│  │     let numbers = vec![1, 2, 3, 4, 5];                     │   │
│  │     let sum: i32 = numbers.iter().sum();                   │   │
│  │     println!("Sum: {}", sum);                              │   │
│  │ }                                                           │   │
│  │ ```                                                          │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
├─────────────────────────────────────────────────────────────────────┤
│ 📝 Type your message...                                              │
├─────────────────────────────────────────────────────────────────────┤
│ ⌨️  Tab: Switch Mode │ ↑↓: Scroll │ Ctrl+C: Copy │ Ctrl+Q: Quit    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 🎯 TUI Modes

### 1. **Chat Mode** (Default)
Interactive conversation with AI models.

**Features:**
- Real-time message streaming
- Syntax highlighting in code blocks
- Message history (scrollable)
- Multiple AI model support
- Copy/paste functionality
- Markdown rendering

**Keyboard Shortcuts:**
- `Enter` - Send message
- `↑/↓` - Scroll history
- `Ctrl+C` - Copy selected text
- `Ctrl+V` - Paste
- `Tab` - Switch to Agents mode
- `Ctrl+Q` - Quit

---

### 2. **Agents Mode**
Multi-agent coordination and management.

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🤖 Agent Swarm Dashboard                                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─────────────────────┐  ┌─────────────────────┐  ┌──────────────┐│
│  │ 👤 Researcher       │  │ 📝 Writer           │  │ ✅ Reviewer  ││
│  │ Status: Active      │  │ Status: Idle        │  │ Status: Done ││
│  │ Progress: ▓▓▓▓▓░░░  │  │ Progress: ▓▓▓▓▓▓▓▓  │  │ Progress: ✓  ││
│  │ Step: 5/10          │  │ Step: 8/8           │  │ Step: 5/5    ││
│  │ Model: GPT-4o-mini  │  │ Model: GPT-4o-mini  │  │ Model: Claude││
│  └─────────────────────┘  └─────────────────────┘  └──────────────┘│
│                                                                       │
│  📊 Blackboard (Shared Messages):                                   │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ [10:45:30] Researcher → Writer:                             │   │
│  │   "Research complete: Found 5 key papers on functional..."  │   │
│  │                                                              │   │
│  │ [10:45:45] Writer → Reviewer:                               │   │
│  │   "Draft complete: 1200 words covering all key concepts..." │   │
│  │                                                              │   │
│  │ [10:46:00] Reviewer → All:                                  │   │
│  │   "Review complete: Approved with minor suggestions..."     │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  🎯 Swarm Policy: Router │ Iterations: 15/20 │ Elapsed: 2m 15s     │
├─────────────────────────────────────────────────────────────────────┤
│ ⌨️  Space: Pause/Resume │ R: Restart │ Tab: Switch Mode │ Q: Quit   │
└─────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Real-time agent status
- Swarm coordination visualization
- Message blackboard
- Execution trace
- Performance metrics
- Pause/resume controls

**Keyboard Shortcuts:**
- `Space` - Pause/Resume swarm
- `R` - Restart swarm
- `↑/↓` - Navigate agents
- `Enter` - Inspect selected agent
- `Tab` - Switch to MCP mode
- `Q` - Quit

---

### 3. **MCP Mode**
Model Context Protocol server management.

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🔌 MCP Server Registry                                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  🟢 filesystem     http://localhost:3001     ⚡ 12ms    ✓ Healthy   │
│     Tools: read_file, write_file, list_dir, search (4 tools)        │
│                                                                       │
│  🟢 aws            http://localhost:3002     ⚡ 45ms    ✓ Healthy   │
│     Tools: s3_list, s3_upload, ec2_status, lambda_invoke (4 tools)  │
│                                                                       │
│  🟢 database       http://localhost:3003     ⚡ 8ms     ✓ Healthy   │
│     Tools: query, insert, update, delete, schema (5 tools)           │
│                                                                       │
│  🔴 web-scraper    http://localhost:3004     ⚡ timeout ✗ Error     │
│     Tools: fetch, parse, extract (3 tools)                           │
│                                                                       │
├─────────────────────────────────────────────────────────────────────┤
│  📋 Tool Catalog (13 tools available):                              │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ ▸ read_file (filesystem)                                    │   │
│  │   Read contents of a file from the filesystem               │   │
│  │   Args: path (string, required)                             │   │
│  │                                                              │   │
│  │ ▸ s3_list (aws)                                             │   │
│  │   List objects in an S3 bucket                              │   │
│  │   Args: bucket (string, required), prefix (string, optional)│   │
│  │                                                              │   │
│  │ ▸ query (database)                                          │   │
│  │   Execute SQL query on the database                         │   │
│  │   Args: sql (string, required), limit (int, optional)      │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  📊 Usage Stats (Last hour):                                        │
│    read_file: 24 calls │ s3_list: 8 calls │ query: 15 calls        │
├─────────────────────────────────────────────────────────────────────┤
│ ⌨️  Enter: Test Tool │ H: Health Check │ Tab: Switch Mode │ Q: Quit │
└─────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Server health monitoring (green/yellow/red)
- Real-time latency metrics
- Tool catalog browser
- Usage statistics
- Connection status
- Auto-reconnect handling

**Keyboard Shortcuts:**
- `Enter` - Test selected tool
- `H` - Run health check
- `R` - Refresh servers
- `↑/↓` - Navigate tools
- `Tab` - Switch to Chat mode
- `Q` - Quit

---

### 4. **Multimodal Mode**
Media attachment and preview.

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🖼️  Multimodal Chat                                                 │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─────────────────┐  ┌─────────────────────────────────────────┐  │
│  │ 📎 Media        │  │ 💬 Conversation                         │  │
│  │                 │  │                                          │  │
│  │ 🖼️  image1.jpg  │  │ 👤 You:                                 │  │
│  │   [Preview]     │  │ What's in this image?                   │  │
│  │   640x480       │  │ [📷 image1.jpg attached]               │  │
│  │   45KB          │  │                                          │  │
│  │                 │  │ 🤖 GPT-4o:                              │  │
│  │ 🎵 audio.mp3    │  │ The image shows a sunset over the       │  │
│  │   [Waveform]    │  │ ocean with vibrant orange and pink      │  │
│  │   3:24          │  │ colors reflecting on the water...       │  │
│  │   2.1MB         │  │                                          │  │
│  │                 │  │                                          │  │
│  │ 🎬 video.mp4    │  │ 👤 You:                                 │  │
│  │   [Thumbnail]   │  │ Now analyze this audio clip             │  │
│  │   1920x1080     │  │ [🎵 audio.mp3 attached]                │  │
│  │   15MB          │  │                                          │  │
│  │                 │  │ 🤖 GPT-4o:                              │  │
│  │ [+ Add Media]   │  │ The audio contains a piano melody       │  │
│  │                 │  │ in C major, with a tempo of about...    │  │
│  └─────────────────┘  └─────────────────────────────────────────┘  │
├─────────────────────────────────────────────────────────────────────┤
│ ⌨️  A: Add Media │ D: Delete │ Space: Preview │ Tab: Mode │ Q: Quit│
└─────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Drag-and-drop media
- Image/video previews
- Audio waveform display
- File size/dimensions
- Format validation
- Base64 encoding for APIs

**Keyboard Shortcuts:**
- `A` - Add media file
- `D` - Delete selected media
- `Space` - Preview/play media
- `↑/↓` - Navigate media list
- `Tab` - Switch mode
- `Q` - Quit

---

### 5. **A2A Protocol Mode**
Agent-to-Agent messaging visualization.

```
┌─────────────────────────────────────────────────────────────────────┐
│ 📡 A2A Message Bus                                                   │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Network Topology:                                                   │
│                                                                       │
│            ┌──────────────┐                                          │
│            │ Coordinator  │                                          │
│            │   (Active)   │                                          │
│            └──────┬───────┘                                          │
│                   │                                                  │
│        ┌──────────┼──────────┐                                      │
│        │          │          │                                      │
│   ┌────▼───┐ ┌───▼────┐ ┌──▼──────┐                               │
│   │Research│ │ Writer │ │Reviewer │                               │
│   │(Recv)  │ │(Idle)  │ │(Send)  │                               │
│   └────────┘ └────────┘ └─────────┘                               │
│                                                                       │
│  Message Flow (Last 10):                                             │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 10:45:12 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │ Coordinator ──[Task]──> Researcher                          │   │
│  │ "Research the latest AI trends"                             │   │
│  │                                                              │   │
│  │ 10:45:15 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │ Coordinator ──[Broadcast]──> ALL                            │   │
│  │ "Project kickoff meeting in 5 minutes"                      │   │
│  │                                                              │   │
│  │ 10:45:18 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │ Researcher ──[Response]──> Coordinator                      │   │
│  │ "Research complete. Found 5 relevant papers"                │   │
│  │                                                              │   │
│  │ 10:45:20 ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  │   │
│  │ Coordinator ──[Delegate]──> Writer                          │   │
│  │ "Write summary based on researcher's findings"              │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  Queue Status: Researcher(0) Writer(1) Reviewer(0) Coordinator(2)   │
├─────────────────────────────────────────────────────────────────────┤
│ ⌨️  Enter: Inspect Message │ F: Filter │ Tab: Mode │ Q: Quit        │
└─────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Network topology graph
- Real-time message flow animation
- Agent status (active/idle/sending/receiving)
- Message queue visualization
- Delivery tracking
- Message filtering

---

### 6. **NANDA Protocol Mode**
Consensus and voting visualization.

```
┌─────────────────────────────────────────────────────────────────────┐
│ 🗳️  NANDA Consensus Dashboard                                       │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Active Negotiations (3):                                            │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 📋 NEG-001: Coordination Strategy                           │   │
│  │ Proposed by: agent1 │ Threshold: 75% │ Votes: 3/4          │   │
│  │                                                              │   │
│  │ Progress: ▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░ 75% (CONSENSUS REACHED ✓)   │   │
│  │                                                              │   │
│  │ Votes:                                                       │   │
│  │   ✅ agent1: Accept                                         │   │
│  │   ✅ agent2: Accept                                         │   │
│  │   🔄 agent3: Counter-proposal (router policy instead)      │   │
│  │   ✅ agent4: Accept                                         │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 📋 NEG-002: Task Allocation                                 │   │
│  │ Proposed by: coordinator │ Threshold: 75% │ Votes: 3/4     │   │
│  │                                                              │   │
│  │ Progress: ▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░ 75% (CONSENSUS REACHED ✓)   │   │
│  │                                                              │   │
│  │ Votes:                                                       │   │
│  │   ✅ agent1: Accept                                         │   │
│  │   ✅ agent2: Accept                                         │   │
│  │   ✅ agent3: Accept                                         │   │
│  │   ⚪ agent4: Abstain                                        │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────┐   │
│  │ 📋 NEG-003: Resource Allocation                             │   │
│  │ Proposed by: agent2 │ Threshold: 75% │ Votes: 1/4          │   │
│  │                                                              │   │
│  │ Progress: ▓▓▓▓▓░░░░░░░░░░░░░░░ 25% (IN PROGRESS...)        │   │
│  │                                                              │   │
│  │ Votes:                                                       │   │
│  │   🔄 agent1: Counter-proposal (centralized compute)        │   │
│  │   ⏳ agent2: Pending...                                     │   │
│  │   ⏳ agent3: Pending...                                     │   │
│  │   ⏳ agent4: Pending...                                     │   │
│  │                                                              │   │
│  │ ⚠️  Deadline: 45 seconds remaining                          │   │
│  └─────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  Consensus Threshold: 75% │ Quorum: 4 agents │ Timeout: 30s        │
├─────────────────────────────────────────────────────────────────────┤
│ ⌨️  Enter: Vote │ C: Counter-propose │ Tab: Mode │ Q: Quit          │
└─────────────────────────────────────────────────────────────────────┘
```

**Features:**
- Real-time vote tracking
- Consensus progress bars
- Counter-proposal comparison
- Deadline countdowns
- Quorum status
- Voting history
- Audit trail

---

## 🎮 Global Keyboard Shortcuts

| Key             | Action                                                                       |
| --------------- | ---------------------------------------------------------------------------- |
| `Tab`           | Switch between modes (Chat → Agents → MCP → Multimodal → A2A → NANDA → Chat) |
| `Ctrl+Q` / `Q`  | Quit TUI                                                                     |
| `Ctrl+C`        | Copy selected text                                                           |
| `Ctrl+V`        | Paste                                                                        |
| `Ctrl+L`        | Clear screen                                                                 |
| `Ctrl+R`        | Refresh view                                                                 |
| `↑` / `↓`       | Scroll up/down or navigate lists                                             |
| `←` / `→`       | Navigate tabs or panels                                                      |
| `PgUp` / `PgDn` | Page up/down                                                                 |
| `Home` / `End`  | Jump to start/end                                                            |
| `Enter`         | Select/submit                                                                |
| `Esc`           | Cancel/back                                                                  |
| `?`             | Show help                                                                    |

---

## 🎨 Color Scheme

The TUI uses semantic colors:
- **Green** 🟢 - Active, healthy, success
- **Yellow** 🟡 - Warning, pending, in-progress
- **Red** 🔴 - Error, failed, critical
- **Blue** 🔵 - Info, selected, focus
- **Gray** ⚪ - Inactive, disabled, idle
- **Cyan** - Timestamps, metadata
- **Magenta** - Highlights, important

---

## 📊 Performance Indicators

**Status Icons:**
- ⚡ Latency (milliseconds)
- ▓ Progress bars
- ✓ Completed
- ✗ Failed
- ⏳ Pending
- 🔄 In progress
- 📊 Statistics
- 🎯 Target/goal
- 📈 Trending up
- 📉 Trending down

---

## 🚀 Launch Examples

```bash
# Basic TUI
ae --tui

# TUI with specific script
ae --tui demos/tui_chat_demo.ae

# TUI with agent swarm
ae --tui demos/tui_agent_swarm_demo.ae

# TUI with MCP servers
ae --tui demos/tui_mcp_demo.ae

# TUI with multimodal
ae --tui demos/tui_multimodal_demo.ae
```

---

## 💡 Tips

1. **Resize Friendly**: TUI adapts to terminal size. Recommended minimum: 100x30 characters
2. **Mouse Support**: Click to select, scroll to navigate (if terminal supports it)
3. **Copy/Paste**: Works with terminal clipboard integration
4. **Syntax Highlighting**: Code blocks in chat are syntax-highlighted
5. **Export**: Press `E` to export conversation to markdown
6. **Search**: Press `/` to search in chat history
7. **Themes**: TUI respects terminal color scheme

---

**Ready to explore?** Try: `ae --tui demos/tui_chat_demo.ae` 🚀
