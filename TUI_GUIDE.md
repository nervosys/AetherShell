# AetherShell Terminal User Interface (TUI)

## Overview

The AetherShell TUI provides a rich terminal-based interface for interacting with multi-modal Large Language Models (LLMs) and managing AI agent swarms. It offers an intuitive visual environment for complex AI workflows while maintaining the power and flexibility of the command line.

## Features

### 🎯 Core Capabilities

- **Multi-modal AI Chat**: Support for text, images, audio, and video inputs
- **Agent Swarm Management**: Create, monitor, and coordinate multiple AI agents
- **Media Processing**: Terminal-friendly display and processing of media files
- **Real-time Monitoring**: Live status updates for agents and conversations
- **Interactive Configuration**: Settings management through the UI

### 📺 Interface Modes

1. **Chat Mode** - Multi-modal conversation interface
2. **Agent Swarm Mode** - Agent management and monitoring
3. **Media Browser Mode** - File management and attachment system
4. **Settings Mode** - Configuration and preferences

## Quick Start

```bash
# Launch the TUI
ae --tui

# Navigate between modes
Tab / Shift+Tab    # Cycle through modes
1, 2, 3, 4         # Jump directly to Chat, Agent, Media, Settings

# Basic navigation
↑↓ or j/k          # Navigate lists
Enter or i         # Start editing
Esc                # Cancel/return to navigation
Ctrl+C or q        # Quit application
```

## Usage Guide

### Chat Mode

The chat interface supports multi-modal conversations with AI models:

```
Key Bindings:
- i/Enter: Start typing a message
- m: Switch to media browser to attach files
- a: Switch to agent management
- c: Clear chat history
```

**Multi-modal Workflow:**
1. Switch to Media mode (press `3` or `m`)
2. Select media files (press `Space` to toggle selection)
3. Return to Chat mode (press `1` or `b`)
4. Type your message and press Enter
5. The AI will analyze both text and attached media

### Agent Swarm Mode

Manage and monitor AI agents working on complex tasks:

```
Key Bindings:
- n: Create new agent
- d/Delete: Remove selected agent
- Enter/s: Assign task to agent
- c: Switch to chat mode
```

**Agent Workflow:**
1. Create agents with specialized capabilities
2. Assign tasks and monitor progress
3. Coordinate between agents through shared context
4. View real-time status updates and results

### Media Browser Mode

Handle various media types in the terminal:

```
Key Bindings:
- Space/Enter: Toggle file selection
- o: Add file to library (placeholder in demo)
- c: Clear all selections
- d/Delete: Remove from library
- b: Return to chat with selected files
```

**Supported Formats:**
- **Images**: JPG, PNG, GIF, WebP, BMP, TIFF, SVG
- **Videos**: MP4, AVI, MOV, MKV, WebM, FLV
- **Audio**: MP3, WAV, FLAC, AAC, OGG, M4A

## Multi-modal AI Integration

### Supported Models

The TUI integrates with various AI backends:

- **OpenAI**: GPT-4V for vision, GPT-4 for text
- **Ollama**: LLaVA for vision, Llama3 for text
- **OpenAI-compatible APIs**: Custom endpoints
- **Text Generation Inference (TGI)**: High-performance serving

### Configuration

Set environment variables to configure AI backends:

```bash
# OpenAI (with vision support)
export OPENAI_API_KEY="your-key"
export OPENAI_MODEL="gpt-4o"

# Ollama (with vision model)
export OLLAMA_URL="http://localhost:11434"
export OLLAMA_MODEL="llava"

# Custom endpoints
export AETHER_MODEL_URI="openai:gpt-4o"
```

## Agent Swarm Architecture

### Agent Types

- **Single Agents**: Individual AI workers with specific tools
- **Specialized Agents**: Domain-specific capabilities (file analysis, code generation, etc.)
- **Coordinator Agents**: Manage task distribution and orchestration

### Coordination Strategies

- **Round Robin**: Equal task distribution
- **Load Balanced**: Assignment based on agent capacity
- **Specialized**: Task routing based on agent capabilities

### Communication

- **Blackboard Pattern**: Shared memory for inter-agent communication
- **Task Queue**: Centralized task management
- **Real-time Updates**: Live status monitoring in the TUI

## Examples

### Basic Multi-modal Chat

```bash
# Launch TUI and try:
# 1. Go to Media tab
# 2. Select an image
# 3. Go to Chat tab
# 4. Type: "What do you see in this image?"
# 5. Press Enter
```

### Agent Swarm Coordination

```bash
# Create a swarm for code analysis:
# 1. Go to Agent tab
# 2. Create agent "FileAnalyzer" 
# 3. Create agent "DocumentGenerator"
# 4. Assign coordinated tasks
# 5. Monitor progress in real-time
```

## Advanced Features

### Session Management

- **Auto-save**: Conversations and agent states persist
- **Export**: Markdown export for chat sessions
- **Context Window**: Intelligent message summarization

### Performance

- **Streaming**: Real-time response display
- **Caching**: Media thumbnail generation
- **Async Processing**: Non-blocking agent operations

### Extensibility

- **Plugin Architecture**: Custom tool integration
- **Model Registry**: Easy backend switching
- **Custom Agents**: Specialized agent implementations

## Tips and Best Practices

1. **Media Optimization**: Use compressed formats for faster processing
2. **Agent Coordination**: Design complementary agent capabilities
3. **Context Management**: Use clear, descriptive prompts
4. **Resource Monitoring**: Watch agent status for bottlenecks
5. **Session Organization**: Regular exports for important conversations

## Troubleshooting

### Common Issues

- **No Response**: Check AI backend configuration and API keys
- **Media Not Loading**: Verify file format support and permissions
- **Agent Errors**: Review tool allowlists and environment setup
- **Performance Issues**: Monitor system resources and agent count

### Debug Mode

```bash
# Enable debug logging
export RUST_LOG=debug
ae --tui
```

## Future Enhancements

- Voice input/output support
- Video frame analysis
- Real-time collaboration
- Custom UI themes
- Performance profiling
- Cloud backend integration

---

For more examples and advanced usage, see the `examples/` directory:
- `09_tui_multimodal.ae` - Multi-modal chat examples
- `10_tui_agent_swarm.ae` - Agent coordination patterns
- `11_tui_showcase.ae` - Complete feature demonstration
