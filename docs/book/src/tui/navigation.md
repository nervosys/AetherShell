# Navigation

The TUI uses a modal key binding system with **Normal** and **Editing** modes.

## Global Keys (Normal Mode)

These work from any tab:

| Key | Action |
|-----|--------|
| `q` / `Esc` / `Ctrl+C` / `Ctrl+Q` | Quit |
| `Tab` | Next tab (cycles through all modes) |
| `Shift+Tab` | Previous tab |
| `1`-`6` | Jump to specific tab |
| `↑` / `k` | Move selection up (wraps around) |
| `↓` / `j` | Move selection down (wraps around) |

## Chat Mode

### Normal Mode

| Key | Action |
|-----|--------|
| `Enter` / `i` | Enter Editing mode |
| `c` | Clear conversation |
| `m` | Switch to Media tab |
| `a` | Switch to Agents tab |
| `Ctrl+E` | Export conversation to Markdown |
| `Ctrl+J` | Export conversation to JSON |
| `Ctrl+L` | Clear conversation |
| `Ctrl+F` | Open Search |

### Editing Mode

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Esc` | Cancel, return to Normal mode |
| Arrow keys | Navigate within text input |
| Any key | Types into the input field |

## Agent Swarm Mode

### Normal Mode

| Key | Action |
|-----|--------|
| `n` | Create new agent |
| `d` / `Delete` | Remove selected agent |
| `Enter` / `s` | Enter Editing mode (type task) |
| `m` | View agent metrics |
| `r` | Restart selected agent |
| `c` | Switch to Chat |

## Media Browser Mode

### Normal Mode

| Key | Action |
|-----|--------|
| `Space` / `Enter` | Toggle file selection |
| `o` | Open/add file |
| `c` | Clear all selections |
| `d` / `Delete` | Remove file from library |
| `b` | Return to Chat with selected media |

## Search Mode

### Normal Mode

| Key | Action |
|-----|--------|
| `i` / `/` | Enter search Editing mode |
| `↓` / `j` | Next search result |
| `↑` / `k` | Previous search result |
| `Esc` | Clear search, return to Chat |
| `Ctrl+C` | Copy selected result |

## Distributed Agents Mode

| Key | Action |
|-----|--------|
| `s` | Start distributed swarm |
| `d` | Stop distributed swarm |
| `r` | Refresh network status |
| `t` | Test connection |

## Advanced Reasoning Mode

| Key | Action |
|-----|--------|
| `n` | New reasoning session |
| `p` | View planning goals |
| `k` | Browse knowledge base |
| `e` | Export reasoning chains |
| `i` | Import knowledge |

## Navigation Tips

- Use `j`/`k` (vim-style) or arrow keys to scroll through lists
- List selection wraps around — pressing `↑` at the top jumps to the bottom
- Number keys (`1`-`6`) provide the fastest way to switch tabs
- `Esc` always returns to Normal mode from Editing mode
- In Chat mode, `Ctrl+F` enters Search for finding messages in history
