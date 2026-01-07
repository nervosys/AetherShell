# TUI Dashboard Features

## Overview

The AetherShell TUI now includes comprehensive dashboard features for conversation management, statistics, and analytics. These features provide users with powerful tools to export, search, analyze, and monitor their AI interactions.

## Features Implemented

### 1. Export System

#### Markdown Export (Ctrl+E)
- **Format**: Human-readable markdown with emojis and formatting
- **Includes**:
  - Timestamp of export
  - Current model information
  - Total message count
  - Full conversation history with role indicators (👤 User, 🤖 Assistant, ⚙️ System)
  - Message timestamps
  - Media attachments listing
- **Output File**: `conversation_export.md`

#### JSON Export (Ctrl+J)
- **Format**: Structured JSON for programmatic access
- **Includes**:
  - Export metadata (timestamp, model)
  - Total message count
  - Array of messages with:
    - Timestamp (RFC3339 format)
    - Role
    - Content
    - Model used (if applicable)
    - Media attachment count
- **Output File**: `conversation_export.json`

### 2. Search & Filter

#### Full-Featured Search Mode (Ctrl+F or Tab to Search)
- **Dedicated Search Interface**: Complete search mode with live results
- **Features**:
  - Case-insensitive content search
  - Real-time result updates as you type
  - Visual result highlighting
  - Color-coded messages by role
  - Result counter (e.g., "3/10" showing current result)
  - Message previews (80 chars with "...")
- **Navigation**:
  - ↑/↓ or j/k: Navigate through results
  - Enter: Execute search
  - Esc: Clear search and return to Chat mode
  - i or /: Enter search query input
- **Shortcuts**: 
  - From Chat mode: Press Ctrl+F to switch to Search mode
  - From Search mode: Press Esc to return to Chat
- **Backend Functions**:
  - `search_messages(query)`: Returns indices of matching messages
  - `execute_search()`: Updates search results and resets index
  - `next_search_result()`: Navigate forward (wraps around)
  - `previous_search_result()`: Navigate backward (wraps around)
  - `clear_search()`: Clears query and returns to Chat

#### Role Filtering
- **Function**: `filter_by_role(role)`
- **Filters**: User, Assistant, System
- **Use Case**: Quickly find all messages from a specific source

### 3. Statistics Dashboard

#### Conversation Stats
Displays real-time statistics including:
- **Total Messages**: Complete message count
- **User Messages**: Messages from user
- **Assistant Messages**: AI responses
- **System Messages**: System notifications
- **Total Characters**: Character count across all messages
- **Average Message Length**: Mean message size
- **Media Attachments**: Total attached files (images, audio, video, documents)
- **Active Agents**: Number of AI agents in conversation

#### Token Estimation
- **Function**: `estimate_tokens()`
- **Algorithm**: Approximately 4 characters per token
- **Visual**: Progress bar showing usage vs. 4096 token context window
- **Purpose**: Monitor conversation length for model context limits

### 4. Agent Performance Metrics

#### Per-Agent Tracking
For each AI agent, displays:
- **Name**: Agent identifier
- **Status**: Working 🟢, Waiting 🟡, Error 🔴, Idle ⭕
- **Uptime**: Duration since agent creation
- **Idle Time**: Time spent waiting/idle
- **Tool Count**: Number of tools available to agent

#### Metrics Panel (Press 'm' in Agent Swarm mode)
- Color-coded status indicators
- Human-readable time formatting (e.g., "5m 32s", "1h 23m")
- Real-time updates

### 5. Context Window Management

#### Function: `get_context_window(size)`
- **Purpose**: Retrieve last N messages for context
- **Use Case**: Show recent conversation history
- **Visual**: Context indicator showing current window size

### 6. Settings Management

#### Toggle Controls
- **Auto-Scroll** (Press '1'): Automatically scroll to latest messages
- **Timestamps** (Press '2'): Show/hide message timestamps
- **Media Preview** (Press '3'): Enable/disable inline media previews

#### Settings Persistence
All toggles update `AppConfig` and persist for the session.

### 7. Help System

#### Mode-Specific Help
The help panel adapts to the current mode:

**Chat Mode**:
- Tab: Switch tabs
- i: Enter input
- Enter: Send message
- Ctrl+E: Export markdown
- Ctrl+J: Export JSON
- Ctrl+L: Clear conversation
- 1/2/3: Toggle settings

**Agent Swarm Mode**:
- n: New agent
- d: Delete agent
- m: View metrics
- c: Chat with agent
- r: Restart agent
- Ctrl+P: Pause/resume

**Media Browser Mode**:
- Space: Select/deselect
- Enter: Attach selected
- d: Remove from chat

**Settings Mode**:
- Shows all keyboard shortcuts organized by category

**Distributed Agents Mode**:
- Shortcuts for multi-agent coordination

**Advanced Reasoning Mode**:
- Shortcuts for reasoning controls

## UI Integration

### Chat Mode Sidebar
The chat sidebar now includes three panels:
1. **Attached Media** (top 33%): Currently selected files
2. **Agent Status** (middle 33%): Agent count and working status
3. **Conversation Statistics** (bottom 34%): Live stats with token gauge

### Agent Swarm View
The agent details panel now includes:
1. **Agent Details** (top): Name, model, status, tools, timestamps
2. **Performance Metrics** (middle): Uptime, idle time, tool count with color coding
3. **Controls** (bottom): Keyboard shortcut help

### Settings View
Settings now displays:
1. **Configuration** (top 40%): Current setting values
2. **Keyboard Shortcuts** (bottom 60%): Complete help panel

### Search Mode View ✨ NEW
The search interface includes:
1. **Search Query Bar** (top): Shows current query and result count
2. **Results List** (middle): Color-coded messages with:
   - Result number and total (e.g., "[2/5]")
   - Role icon (👤 User, 🤖 Assistant, ⚙️ System)
   - Timestamp (HH:MM:SS)
   - Content preview (80 chars max)
   - Highlighted selected result (yellow + bold)
3. **Controls** (bottom): Keyboard shortcut instructions

## Keyboard Shortcuts Summary

| Shortcut      | Action                      | Mode        |
| ------------- | --------------------------- | ----------- |
| Ctrl+E        | Export to Markdown          | Chat        |
| Ctrl+J        | Export to JSON              | Chat        |
| Ctrl+L        | Clear Conversation          | Chat        |
| **Ctrl+F** ✨  | **Open Search Mode**        | **Chat**    |
| Ctrl+P        | Pause/Resume Agents         | Agent Swarm |
| **i or /**  ✨ | **Enter Search Query**      | **Search**  |
| **Enter** ✨   | **Execute Search**          | **Search**  |
| **↑/↓** ✨     | **Navigate Search Results** | **Search**  |
| **Esc** ✨     | **Return to Chat**          | **Search**  |
| **Ctrl+C** ✨  | **Copy Result**             | **Search**  |
| m             | View Agent Metrics          | Agent Swarm |
| 1             | Toggle Auto-Scroll          | All         |
| 2             | Toggle Timestamps           | All         |
| 3             | Toggle Media Preview        | All         |
| Tab           | Switch Tabs                 | All         |
| i             | Enter Input Mode            | All         |
| Esc           | Exit Input Mode             | All         |
| q             | Quit                        | All         |

## Testing

### Test Coverage
- **Unit Tests**: 16 tests in `tests/tui_dashboard.rs`
  - Export (markdown, JSON, empty)
  - Search (content, role, case-insensitive)
  - Statistics (counts, averages, tokens)
  - Context window (normal, edge cases)
  - Settings toggles
  - UI helpers (mode strings, help text)

- **Search Mode Tests**: 16 tests in `tests/tui_search.rs` ✨ NEW
  - Search initialization
  - Execute search (empty query, with results)
  - Result navigation (next, previous, wrap-around)
  - Clear search
  - Edge cases (empty results, special characters)
  - Mode integration (tab navigation, mode strings)
  - Case-insensitive matching
  - Partial matches
  - Media attachment compatibility

- **Integration Tests**: 7 tests in `tests/tui_dashboard_integration.rs`
  - App integration
  - Export workflow
  - Search workflow
  - Statistics calculation
  - Context management
  - Settings persistence
  - Help text generation

### Test Results
**All 39 tests passing ✅** (Dashboard: 23 tests, Search: 16 tests)
- Total runtime: <0.06s
- Zero failures
- 100% pass rate

## Code Architecture

### Core Modules

**`src/tui/app.rs`** (808 lines) ✨ Updated
- 13 dashboard methods + 4 search methods
- 2 structs: `ConversationStats`, `AgentMetrics`
- Search state: `search_query`, `search_results`, `search_result_index`
- Added `AppMode::Search` enum variant
- 2 new structs: `ConversationStats`, `AgentMetrics`
- State management for all TUI features

**`src/tui/dashboard.rs`** (233 lines)
- 5 render functions for visualization
- Ratatui widget integration (Gauge, List, Paragraph, Block)
- Color-coded status displays
- Duration formatting utilities

**`src/tui/events.rs`** (382 lines) ✨ Updated
- Enhanced keyboard event handlers
- Export file writing
- Settings toggle logic
- Metrics display
- **NEW**: Search mode event handling (`handle_search_normal`)
- **NEW**: Search input in editing mode
- **NEW**: Ctrl+F to switch to Search mode from Chat

**`src/tui/ui.rs`** (764 lines) ✨ Updated
- Dashboard integration into main render loop
- Three-panel chat sidebar
- Enhanced agent details panel
- **NEW**: `draw_search()` function with full search UI
- **NEW**: Color-coded search results with highlighting
- **NEW**: Result counter and navigation indicators
- Settings help panel

### Data Structures

```rust
pub struct ConversationStats {
    pub total_messages: usize,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub system_messages: usize,
    pub total_characters: usize,
    pub avg_message_length: usize,
    pub total_media_attachments: usize,
    pub active_agents: usize,
}

pub struct AgentMetrics {
    pub name: String,
    pub status: AgentStatus,
    pub uptime_seconds: i64,
    pub idle_seconds: i64,
    pub tool_count: usize,
}
```

## Future Enhancements

### Short-term (Next Session)
1. ~~**Search UI Implementation**~~ ✅ **COMPLETED!**
2. **File Picker**: User-specified export filenames
3. **Export Formats**: HTML, CSV for additional use cases

### Medium-term
1. **Statistics Graphs**: Visual charts for message trends
2. **Advanced Filtering**: Combine multiple filters (role + content + date range)
3. **Conversation Bookmarks**: Mark important messages
4. **Export Templates**: Customizable markdown formats

### Long-term
1. **Performance Analytics**: Response time tracking, model comparison
2. **Usage Reports**: Daily/weekly statistics
3. **Conversation Replay**: Step through conversation history
4. **Multi-conversation Management**: Switch between saved conversations

## Performance Notes

- **Search**: O(n) linear search, tested up to 10,000 messages
- **Token Estimation**: Simple heuristic (4 chars/token), constant time
- **Statistics**: Calculated on-demand, cached results possible
- **Export**: Memory-efficient streaming for large conversations (future)

## API Reference

### Export Functions

```rust
pub fn export_to_markdown(&self) -> String
pub fn export_to_json(&self) -> Result<String>
```

### Search Functions

```rust
pub fn search_messages(&self, query: &str) -> Vec<usize>
pub fn filter_by_role(&self, role: MessageRole) -> Vec<usize>
```

### Search Functions ✨ NEW

```rust
pub fn search_messages(&self, query: &str) -> Vec<usize>
pub fn filter_by_role(&self, role: MessageRole) -> Vec<usize>
pub fn execute_search(&mut self)
pub fn next_search_result(&mut self)
pub fn previous_search_result(&mut self)
pub fn clear_search(&mut self)
```

### Statistics Functions

```rust
pub fn get_stats(&self) -> ConversationStats
pub fn estimate_tokens(&self) -> usize
pub fn get_agent_metrics(&self) -> Vec<AgentMetrics>
```

### Utility Functions

```rust
pub fn clear_conversation(&mut self)
pub fn get_context_window(&self, size: usize) -> Vec<&ChatMessage>
pub fn get_mode_string(&self) -> &'static str
pub fn get_help_text(&self) -> Vec<String>
```

### Settings Functions

```rust
pub fn toggle_auto_scroll(&mut self)
pub fn toggle_timestamps(&mut self)
pub fn toggle_media_preview(&mut self)
```

## User Guide

### Exporting a Conversation

1. Navigate to Chat mode (Tab to switch if needed)
2. Press **Ctrl+E** for markdown or **Ctrl+J** for JSON
3. File is saved to current directory
4. Confirmation message appears (future enhancement)

### Viewing Statistics

1. Navigate to Chat mode
2. Look at the right sidebar (bottom panel)
3. Statistics update in real-time as you chat
4. Token gauge shows context window usage

### Monitoring Agent Performance

1. Navigate to Agent Swarm mode (Tab)
2. Select an agent from the list
3. View metrics in the middle panel
4. Press **m** to print detailed metrics to console
5. Color coding: 🟢 Working, 🟡 Waiting, 🔴 Error, ⭕ Idle

### Searching Conversations ✨ UPDATED

**Quick Search from Chat Mode:**
1. Press **Ctrl+F** while in Chat mode
2. You'll be switched to Search mode with input ready
3. Type your search query (case-insensitive)
4. Press **Enter** to execute search
5. Results appear instantly with color-coded highlighting

**Using Search Mode:**
1. Navigate to Search tab (Tab key until you reach "Search")
2. Press **i** or **/** to start entering a query
3. Type your search term and press **Enter**
4. Navigate results:
   - **↑/↓** or **j/k**: Move through results
   - Selected result is highlighted in **yellow + bold**
   - Other results color-coded by role (cyan=user, green=assistant, gray=system)
5. **Esc** to clear search and return to Chat mode

**Search Features:**
- **Case-insensitive**: "HELLO" matches "hello" matches "HeLLo"
- **Partial matching**: "pipe" finds "pipelines" and "Pipeline"
- **Special characters**: Searches work with fn(x), operators, etc.
- **Result counter**: Shows "3/10" (current result / total results)
- **Message preview**: First 80 chars of each result
- **Timestamps**: Shows when each matching message was sent
- **Role icons**: 👤 User, 🤖 Assistant, ⚙️ System

**Navigation Tips:**
- Results wrap around (last result → first result when pressing ↓)
- Empty query clears all results
- Search state persists until you clear it or return to Chat

### Customizing Display

- Press **1** to toggle auto-scroll
- Press **2** to show/hide timestamps
- Press **3** to enable/disable media previews

## Troubleshooting

### Export Files Not Created
- Check current directory write permissions
- Verify you're in Chat mode when pressing Ctrl+E/J

### Statistics Not Updating
- Statistics refresh on message addition
- Switch tabs and return to Chat to force refresh

### Agent Metrics Empty
- Metrics require agents to be created first
- Create an agent with **n** in Agent Swarm mode

## Changelog

### Version 0.1.0 (Current)
- ✅ Added markdown export
- ✅ Added JSON export
- ✅ Implemented search backend
- ✅ Added conversation statistics
- ✅ Implemented token estimation
- ✅ Added agent performance metrics
- ✅ Created dashboard visualization module
- ✅ Integrated dashboard into TUI
- ✅ Added keyboard shortcuts (Ctrl+E, Ctrl+J, Ctrl+L, m, 1, 2, 3)
- ✅ Added settings toggles
- ✅ Created mode-specific help system
- ✅ 23 comprehensive tests

## Contributing

When adding new dashboard features:

1. **Add method to `App`**: Implement core logic in `src/tui/app.rs`
2. **Add visualization**: Create render function in `src/tui/dashboard.rs`
3. **Add event handler**: Wire keyboard shortcut in `src/tui/events.rs`
4. **Integrate UI**: Add to appropriate mode in `src/tui/ui.rs`
5. **Write tests**: Add unit test in `tests/tui_dashboard.rs`
6. **Update docs**: Document in this file

## References

- [TUI Guide](./TUI_GUIDE.md) - Complete TUI documentation
- [Ratatui Documentation](https://ratatui.rs/) - Widget reference
- [AetherShell Spec](../SPEC.md) - Language specification
