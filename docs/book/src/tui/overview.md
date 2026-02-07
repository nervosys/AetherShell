# TUI Overview

AetherShell's Terminal User Interface (TUI) provides a rich, multi-pane interface for AI chat, agent management, media browsing, and more — all within your terminal.

## Launching the TUI

```bash
ae tui              # standard launch
ae --tui            # alternative flag
RUST_LOG=debug ae tui   # with debug logging
```

## Interface Layout

The TUI has three main areas:

```
┌──────────────────────────────────────────────────────┐
│  Chat │ Agents │ Media │ Settings │ Distributed │ …  │  ← Tabs
├──────────────────────────────────────────────────────┤
│                                                      │
│                   Main Content                       │  ← Mode-specific
│                                                      │
├──────────────────────────────────────────────────────┤
│  > Type a message...                    │ Help: ?    │  ← Input + Help
└──────────────────────────────────────────────────────┘
```

1. **Header** — Tab bar showing the current mode
2. **Main Content** — Changes based on the active tab
3. **Footer** — Text input (70%) and help hints (30%)

## Tabs / Modes

Switch between modes using `Tab` / `Shift+Tab` or number keys `1`-`6`:

| # | Tab | Description |
|---|-----|-------------|
| 1 | **Chat** | AI conversation with multimodal support |
| 2 | **Agents** | Create and manage AI agent swarms |
| 3 | **Media** | Browse and select images, audio, video |
| 4 | **Settings** | Configure model, preferences |
| 5 | **Distributed** | Manage distributed agent networks |
| 6 | **Reasoning** | Advanced reasoning chains and knowledge |

## Input Modes

The TUI operates in two input modes:

### Normal Mode
Key presses are interpreted as **navigation commands**. Use arrow keys, `j`/`k` for movement, `Tab` to switch tabs, `q` to quit.

### Editing Mode
Key presses go to the **text input field**. Press `Enter` or `i` to enter Editing mode, `Esc` to return to Normal mode.

The current mode is indicated in the input box border style.

## Configuration

The TUI reads configuration from environment variables and defaults:

| Setting | Default | Description |
|---------|---------|-------------|
| Model | `$AETHER_AI` | Default AI model for chat |
| Max messages | 1,000 | Message history limit |
| Auto-scroll | On | Scroll to latest message |
| Timestamps | On | Show message timestamps |
| Media preview | On | Enable in-terminal image preview |
| Agent update interval | 1,000ms | Agent status refresh rate |

## Quick Start

1. Set your AI provider:
   ```bash
   export AETHER_AI=openai
   export OPENAI_API_KEY=sk-...
   ```

2. Launch the TUI:
   ```bash
   ae tui
   ```

3. Press `Enter` to start typing, write your message, press `Enter` to send

4. Press `Tab` to explore other modes (Agents, Media, etc.)

5. Press `q` or `Ctrl+C` to exit
