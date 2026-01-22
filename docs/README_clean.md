# Aether Shell (ae) 🚀

*The next-generation shell that combines the power of modern programming languages with cutting-edge AI capabilities. Built in Rust for safety and performance, featuring a stunning Terminal UI (TUI) for multimodal AI interactions.*

> **"What if your shell could think, see, hear, and coordinate teams of AI agents?"**

---

## ✨ Revolutionary Features

### 🧠 **Multimodal AI Integration**
* **Vision AI**: Analyze images, screenshots, and visual content directly in your terminal
* **Audio Processing**: Transcribe speech, analyze audio files, and voice commands
* **Video Analysis**: Process video content with AI-powered insights
* **Smart Agents**: Deploy specialized AI agents for different tasks
* **Agent Swarms**: Coordinate multiple AI agents working together

### 🎨 **Beautiful Terminal UI (TUI)**
* **Interactive Interface**: Modern, responsive terminal GUI with real-time updates
* **Media Viewer**: Display images, play audio, and preview videos in terminal
* **Chat Interface**: Conversational AI with context-aware responses
* **Agent Dashboard**: Monitor and control your AI agent swarms
* **Multimodal Sessions**: Seamlessly mix text, images, audio in conversations

### 💪 **Advanced Programming Features**
* **Typed Pipelines**: Pass structured records/tables, not just raw text
* **Rust-Grade Safety**: Memory-safe runtime with zero-cost abstractions
* **Strong Type System**: Hindley–Milner inference with algebraic data types
* **Metaprogramming**: Hygienic macros and AST manipulation
* **Async/Await**: Built-in structured concurrency and cancellation
* **POSIX Compatibility**: Run existing tools seamlessly

### 🔄 **Seamless Interoperability**
* **Bash Compatibility**: Transpile and run existing `.sh` scripts
* **Command Integration**: Auto-wrap unknown commands in safe shells
* **Multi-Backend AI**: Support for OpenAI, Ollama, and custom providers
* **OS Tools Database**: Cross-platform native command integration

---

## 🚀 Quick Start Guide

### Installation

```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo install --path . --bin ae
```

### Launch Options

**Classic REPL Mode:**
```bash
ae
```

**🎨 Interactive TUI Mode (Recommended):**
```bash
ae tui
```

**Run Scripts:**
```bash
ae script.ae          # Run Aether script
ae --bash script.sh   # Run Bash script in compatibility mode
```

---

## 🎯 Experience the Magic

### 🖼️ **Multimodal AI in Action**

**Analyze images with AI:**
```bash
ae tui  # Launch TUI, then:
# 1. Switch to Media tab (Tab key)
# 2. Select your image files (Space to select)
# 3. Switch to Chat tab
# 4. Ask: "What do you see in these images?"
```

**Voice-to-text transcription:**
```ae
# Load audio file and get transcription
audio_file := media("recording.mp3")
audio_file | ai("transcribe this audio")
```

**Video content analysis:**
```ae
# Analyze video content
video_file := media("presentation.mp4") 
video_file | ai("summarize the key points from this video")
```

### 🤖 **AI Agent Swarms**

**Deploy a research team:**
```ae
swarm "research quantum computing" [
  "researcher:gather recent papers",
  "analyst:identify key trends", 
  "writer:create summary report"
] --strategy=specialized
```

**Content creation swarm:**
```ae
swarm "create blog post about AI" [
  "planner:outline structure",
  "writer:draft content",
  "editor:refine and polish",
  "fact_checker:verify claims"
]
```

### 💬 **Smart Chat Sessions**

**Context-aware conversations:**
```ae
# In TUI mode, chat sessions remember:
# • Previous messages and context
# • Attached media files  
# • Active agent capabilities
# • User preferences and settings
```

---

## 🔥 Powerful Examples

### Data Processing Pipeline
```ae
# Load CSV, transform, and analyze
load_csv("sales.csv") 
  | where fn(r) => r.amount > 1000
  | group_by fn(r) => r.region
  | map fn(g) => {region: g.key, total: sum(g.values)}
  | sort_by fn(r) => r.total desc
  | ai("analyze these sales trends")
```

### Smart File Management
```ae
# Find and organize photos using AI
ls("~/Pictures") 
  | where fn(f) => f.ext in [".jpg", ".png"]
  | take(10)
  | each fn(img) => {
      path: img.path,
      description: img | ai("describe this image briefly")
    }
  | save_json("photo_catalog.json")
```

---

## 🧠 Core Language Features

### Basic Syntax

**Hello World:**
```ae
print("Hello, Aether!")
```

**Structured Pipelines:**
```ae
[1,2,3,4] | map fn(x) => x*x | reduce fn(a,b) => a+b 0
# → 30
```

**Pattern Matching:**
```ae
let msg = Some(42)
match msg {
  None => print("no value"),
  Some(x) if x > 40 => print("big: ${x}"),
  Some(x) => print("small: ${x}")
}
```

**Typed HTTP:**
```ae
resp := http_get "https://api.github.com"
print(resp.status)
print(resp.headers."content-type")
```

---

## 🎮 TUI Interface Guide

### Navigation
- **Tab**: Switch between Chat, Agents, Media, Help tabs
- **Arrow Keys**: Navigate lists and selections
- **Space**: Select/deselect media files
- **Enter**: Send messages, activate agents
- **Esc**: Return to normal mode
- **Ctrl+C / q**: Quit application

### Media Tab Features
- **Image Viewer**: Display images directly in terminal using advanced algorithms
- **Audio Player**: Play audio files with waveform visualization  
- **Video Preview**: Extract frames and metadata from video files
- **Format Support**: 20+ media formats including PNG, JPG, MP3, MP4, WEBM, GIF
- **Batch Selection**: Select multiple files for multimodal AI analysis

### Agent Management
- **Create Agents**: Spawn AI agents with specific capabilities
- **Monitor Status**: Real-time agent status and task progress
- **Swarm Coordination**: Deploy coordinated teams of agents
- **Task Assignment**: Distribute work across agent networks
- **Strategy Selection**: Choose from Round-Robin, Load-Balanced, or Specialized coordination

### Chat Interface  
- **Multimodal Messages**: Include text, images, audio, and video in conversations
- **Context Awareness**: AI remembers conversation history and attached media
- **Export Options**: Save conversations as Markdown or JSON
- **Session Management**: Multiple chat sessions with persistent history
- **Auto-Summarization**: Intelligent conversation summarization

---

## 🛠️ Advanced Features

### Bash Compatibility

**Run old scripts seamlessly:**

```bash
ae --bash script.sh
```

**Or pipe Bash from stdin:**

```bash
echo 'echo hello | wc -l' | ae -b
```

**Transpiler magic - turns this:**

```bash
echo hello | wc -l
```

**into Aether:**

```ae
echo("hello") | sh(["wc","-l"])
```

---

## 🎨 AI & Media Configuration

### Supported AI Backends
- **OpenAI**: GPT-4V for vision, GPT-4 for text, Whisper for audio
- **Ollama**: Local LLaVA for vision, Llama for text, local models
- **Custom**: Implement your own `MultiModalLlmBackend` trait

### Media Format Support
- **Images**: PNG, JPG, JPEG, WEBP, GIF, BMP, TIFF, ICO, SVG
- **Audio**: MP3, WAV, FLAC, OGG, M4A, AAC, WMA  
- **Video**: MP4, AVI, MOV, MKV, WEBM, FLV, WMV

### Environment Setup
```bash
# For OpenAI integration
export OPENAI_API_KEY="your-api-key"

# For agent command permissions  
export AGENT_ALLOW_CMDS="ls,git,curl,python"

# For custom AI backends
export AI_BACKEND="ollama"  # or "openai" or "custom"
```

---

## 🚀 Example Workflows

### 1. **Document Analysis Pipeline**
```bash
ae tui
# 1. Load PDFs, images, audio recordings in Media tab
# 2. Select multiple files (Space key)
# 3. Chat: "Analyze these documents and create a summary report"
# 4. AI processes all media types and generates comprehensive analysis
```

### 2. **Content Creation Swarm**
```ae
# Deploy specialized agents for blog creation
swarm "create tech blog post" [
  "researcher:gather latest AI trends",
  "writer:draft engaging content", 
  "editor:polish and optimize SEO",
  "designer:suggest visual elements"
] --strategy=specialized --max_iterations=3
```

### 3. **Interactive Media Analysis**
```ae
# Batch process images with AI descriptions
ls("./photos") 
  | where fn(f) => f.ext in [".jpg", ".png"]
  | map fn(img) => {
      file: img.path,
      analysis: img | ai("describe this image in detail"),
      tags: img | ai("generate 5 relevant tags")
    }
  | save_json("photo_analysis.json")
```

### 4. **Voice-Controlled Automation**
```ae
# Record voice command and execute
audio_input := record_audio(5) # 5 seconds
command := audio_input | ai("transcribe and extract shell command")
result := command | sh([]) 
print("Executed: ${command}")
print("Result: ${result}")
```

---

## 🧪 Developer Features

### Type System
- **Hindley-Milner inference**: Automatic type deduction
- **Algebraic data types**: `Option<T>`, `Result<T,E>`, custom enums
- **Strong safety**: Compile-time error prevention
- **Generic programming**: Parametric polymorphism

### Metaprogramming
- **Hygienic macros**: Safe code generation
- **AST manipulation**: Runtime code transformation  
- **Quoting/splicing**: Embed code as data

### Concurrency
- **Async/await**: Built-in structured concurrency
- **Cancellation**: Graceful task termination
- **Pipelines**: Parallel data processing

### OS Tools Integration
- **Cross-platform database**: 25+ native OS tools (Linux/Windows/macOS)
- **Safety levels**: Safe, Moderate, RequiresAdmin classification
- **Command recommendations**: AI-powered tool suggestions
- **Platform filtering**: OS-specific tool availability

---

## 📊 Performance & Testing

### Benchmarks
- **Memory safe**: Zero buffer overflows or memory leaks
- **Fast execution**: Rust-powered performance
- **Concurrent pipelines**: Multi-core utilization
- **Efficient AI calls**: Batched multimodal requests

### Test Coverage
- **173+ tests**: Comprehensive test suite
- **Unit tests**: Individual component validation
- **Integration tests**: End-to-end workflow testing
- **TUI tests**: Interactive interface validation
- **AI tests**: Multimodal backend testing
- **OS Tools tests**: Cross-platform command database validation

---

## 📚 Documentation & Examples

### Example Scripts
- [`examples/00_hello.ae`](examples/00_hello.ae): Basic syntax introduction
- [`examples/05_ai.ae`](examples/05_ai.ae): AI integration examples
- [`examples/06_agent.ae`](examples/06_agent.ae): Agent deployment
- [`examples/09_tui_basic.ae`](examples/09_tui_basic.ae): TUI usage guide
- [`examples/10_multimodal.ae`](examples/10_multimodal.ae): Multimodal AI workflows
- [`examples/11_agent_swarm.ae`](examples/11_agent_swarm.ae): Advanced swarm coordination

### Learning Resources
- **Type system**: See `tests/typecheck.rs` for comprehensive examples
- **Bash compatibility**: Check `tests/transpile_bash.rs` for transpilation rules
- **AI integration**: Explore `tests/multimodal_ai.rs` for backend implementation
- **TUI features**: Review `tests/tui_*.rs` for interface testing
- **OS Tools**: Examine `tests/os_tools.rs` for cross-platform tool usage

---

## 🛣️ Roadmap

### Near-term (Q4 2025)
- **Streaming AI responses**: Real-time token streaming in TUI
- **Plugin system**: Extensible architecture for custom backends  
- **Advanced media**: Video streaming and real-time audio processing
- **Mobile TUI**: Touch-friendly interface adaptations

### Long-term (2026+)
- **Module system**: Package management and imports
- **Distributed agents**: Network-connected agent swarms
- **Advanced AI strategies**: Multi-modal reasoning and planning
- **IDE integration**: VS Code extension and language server

---

## 🤝 Contributing

We welcome contributions! Here's how to get started:

1. **Fork the repository**
2. **Check the test suite**: `cargo test --tests`
3. **Add your feature** with corresponding tests
4. **Ensure TUI compatibility** if UI changes are involved
5. **Submit a pull request** with clear description

### Development Setup
```bash
git clone https://github.com/nervosys/AetherShell
cd AetherShell
cargo build --release
cargo test --tests --all-features
```

---

## 📜 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

**Ready to experience the future of shell interaction? Start with `ae tui` and prepare to be amazed! 🚀**
