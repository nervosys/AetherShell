# TUI Guide

AetherShell's Terminal User Interface (TUI) provides a rich, interactive environment for working with AI, viewing multimodal content, and managing agents.

## Starting TUI Mode

```bash
# Start in TUI mode
ae --tui

# Or from the REPL
tui()
```

## Interface Overview

```
┌─────────────────────────────────────────────────────────────┐
│  AetherShell TUI                                    [Agents]│
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  User: Explain the concept of monads                        │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ AI: A monad is a design pattern used in functional      ││
│  │ programming to handle computations with context...       ││
│  │                                                          ││
│  │ Think of it as a wrapper that:                          ││
│  │ 1. Contains a value                                      ││
│  │ 2. Has a way to wrap values (return/unit)               ││
│  │ 3. Has a way to chain operations (bind/flatMap)         ││
│  └─────────────────────────────────────────────────────────┘│
│                                                             │
│  User: Show me an example in Rust                           │
│  ...                                                        │
│                                                             │
├─────────────────────────────────────────────────────────────┤
│ > Enter message...                                     [?]  │
└─────────────────────────────────────────────────────────────┘
```

## Key Bindings

| Key      | Action                |
| -------- | --------------------- |
| `Enter`  | Send message          |
| `Ctrl+C` | Cancel/Exit           |
| `Ctrl+L` | Clear screen          |
| `Tab`    | Switch panels         |
| `↑/↓`    | Scroll history        |
| `Ctrl+N` | New conversation      |
| `Ctrl+S` | Save conversation     |
| `Ctrl+O` | Open file             |
| `Ctrl+A` | Toggle agent panel    |
| `Ctrl+M` | Toggle model selector |
| `?`      | Help                  |

## Chat Commands

Within the TUI, you can use these commands:

```
/model <name>       - Switch AI model
/clear              - Clear conversation
/save <file>        - Save conversation
/load <file>        - Load conversation
/agent <name>       - Switch to agent
/system <prompt>    - Set system prompt
/image <path>       - Send image
/help               - Show help
```

## Multimodal Content

### Viewing Images

> **Not implemented.** Inline image rendering is intended, not present. The
> source contains no kitty, iterm or sixel support and reads no
> `AETHER_TUI_IMAGE_PROTOCOL` — exporting it does nothing.

```aethershell
# In TUI, send an image
/image screenshot.png

# Or via AI vision
ai("What's in this image?", { images: ["photo.jpg"] })
```

Supported terminals:
- **Kitty** - Full image support
- **iTerm2** - macOS with inline images
- **WezTerm** - Cross-platform
- **Sixel** - Many terminals

### Code Blocks

Code responses are syntax highlighted:

```
AI: Here's the implementation:

┌─rust─────────────────────────────────────────────┐
│ fn fibonacci(n: u64) -> u64 {                    │
│     match n {                                    │
│         0 => 0,                                  │
│         1 => 1,                                  │
│         n => fibonacci(n - 1) + fibonacci(n - 2)│
│     }                                            │
│ }                                                │
└──────────────────────────────────────────────────┘
```

### Tables

Data is rendered as formatted tables:

```
AI: Here are the results:

┌──────────┬───────┬──────────────┐
│ Name     │ Size  │ Modified     │
├──────────┼───────┼──────────────┤
│ main.rs  │ 2.4KB │ 2 hours ago  │
│ lib.rs   │ 1.8KB │ 3 hours ago  │
│ test.rs  │ 892B  │ 1 day ago    │
└──────────┴───────┴──────────────┘
```

## Agent Panel

Press `Ctrl+A` to toggle the agent panel:

```
┌─ Agents ──────────────────────────┐
│                                   │
│ ● coder (idle)                    │
│   "You are a Python expert"       │
│   Tools: [cat, write, grep]       │
│                                   │
│ ○ devops (idle)                   │
│   "You help with infrastructure"  │
│   Tools: [ls, ps, curl]           │
│                                   │
│ [+ New Agent]                     │
└───────────────────────────────────┘
```

## Model Selector

Press `Ctrl+M` to select a model:

```
┌─ Select Model ─────────────────────┐
│                                    │
│ OpenAI                             │
│   ● gpt-4o                         │
│   ○ gpt-4o-mini                    │
│   ○ gpt-4-turbo                    │
│                                    │
│ Anthropic                          │
│   ○ claude-3-opus                  │
│   ○ claude-3-sonnet                │
│                                    │
│ Local (Ollama)                     │
│   ○ llama3                         │
│   ○ codellama                      │
│                                    │
└────────────────────────────────────┘
```

## Reasoning Display

When using reasoning models (o1, R1), the TUI shows the reasoning process:

```
┌─ Thinking... ──────────────────────────────────┐
│ Let me break down this problem:                │
│ 1. First, I need to understand the constraint  │
│ 2. The input array could be empty              │
│ 3. Edge case: negative numbers                 │
│ ...                                            │
└────────────────────────────────────────────────┘

Final Answer:
Here's the optimized solution...
```

## Streaming Responses

Responses stream in real-time:

```
AI: The quick brown fox |  ← Cursor shows typing
```

## Configuration

TUI settings in `~/.config/aethershell/config.toml`:

```toml
[tui]
# Theme
theme = "catppuccin-mocha"

# Image display
show_images = true
image_protocol = "kitty"  # kitty, iterm, sixel
max_image_width = 80

# Chat
show_timestamps = true
show_token_count = true

# Colors (Catppuccin Mocha)
background = "#1e1e2e"
foreground = "#cdd6f4"
accent = "#cba6f7"
```

## Themes

Built-in themes:
- `catppuccin-mocha` (default)
- `catppuccin-latte`
- `dracula`
- `nord`
- `solarized-dark`
- `solarized-light`

## Keyboard Shortcuts Reference

### General
- `Ctrl+C` - Exit/Cancel
- `Ctrl+L` - Clear screen
- `Ctrl+Q` - Quit TUI
- `?` - Help overlay

### Navigation
- `Tab` - Next panel
- `Shift+Tab` - Previous panel
- `↑/↓` - Scroll/History
- `PgUp/PgDn` - Page scroll

### Actions
- `Enter` - Send/Confirm
- `Ctrl+N` - New conversation
- `Ctrl+S` - Save
- `Ctrl+O` - Open

### Panels
- `Ctrl+A` - Agents panel
- `Ctrl+M` - Model selector
- `Ctrl+H` - History
