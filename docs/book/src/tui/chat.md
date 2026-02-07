# Chat Interface

The Chat tab is the primary AI interaction mode — a conversational interface with multimodal support, message history, and export capabilities.

## Sending Messages

1. Press `Enter` or `i` to enter Editing mode
2. Type your message
3. Press `Enter` to send

The TUI sends your message to the configured AI model and displays the response.

## Message Types

Messages are color-coded by role:

| Role | Color | Emoji | Description |
|------|-------|-------|-------------|
| **User** | Cyan | 👤 | Your messages |
| **Assistant** | Green | 🤖 | AI responses |
| **System** | Yellow | ⚙️ | Status and system messages |

Each message displays:
```
[14:32:05] 👤 [gpt-4o-mini] How does ownership work in Rust?
[14:32:08] 🤖 [gpt-4o-mini] Ownership is one of Rust's key features...
```

## Media Attachments

Attach media files to your messages for multimodal AI interaction:

1. Switch to the **Media** tab (`m` or `3`)
2. Select files with `Space` or `Enter`
3. Press `b` to return to Chat with files attached

Attached media appears with a 📎 prefix in the sidebar. When you send a message, the selected media is included as context for the AI.

## Chat Layout

The chat area is split into two panels:

```
┌──────────────────────────┬────────────────┐
│                          │ 📎 Media       │
│   Message History        │ 🤖 Agents      │
│                          │ 📊 Stats       │
│                          │ • Total: 24    │
│                          │ • User: 12     │
│                          │ • Chars: 8,432 │
└──────────────────────────┴────────────────┘
```

- **Left (70%)** — Scrollable message history
- **Right (30%)** — Sidebar with attached media, active agents, and conversation statistics

## Conversation Statistics

The sidebar shows real-time stats:

- **Total messages** — Count of all messages
- **User / Assistant / System** — Breakdown by role
- **Total characters** — Sum of all message content
- **Average length** — Mean characters per message
- **Media attachments** — Count of attached files
- **Active agents** — Number of running agents

## Chat Sessions

The TUI supports multiple chat sessions through the `ChatManager`:

### Session Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `auto_summarize` | `false` | Auto-summarize when context grows too large |
| `context_window_size` | 4,096 | Max tokens in context window |
| `enable_media_analysis` | `true` | Analyze media files with vision models |
| `temperature` | 0.7 | AI response temperature |
| `max_tokens` | None | Max tokens per response |
| `system_prompt` | None | Custom system prompt |

### Auto-Summarization

When enabled and the conversation exceeds the context window size, older messages are automatically summarized into a single system message, preserving recent context.

## Search

Search through your conversation history:

1. Press `Ctrl+F` to enter Search mode
2. Type your search query
3. Press `Enter` to search
4. Use `↑`/`↓` to navigate results
5. Press `Esc` to return to Chat

Search is case-insensitive and matches against message content.

## Export

Export your conversation for sharing or archival:

| Shortcut | Format | File |
|----------|--------|------|
| `Ctrl+E` | Markdown | `conversation_export.md` |
| `Ctrl+J` | JSON | `conversation_export.json` |

### Markdown Export

```markdown
# AetherShell Conversation Export

**Model:** gpt-4o-mini
**Messages:** 24
**Exported:** 2024-01-15T14:32:05Z

---

**👤 User** *14:30:00 [gpt-4o-mini]*
How does ownership work?

**🤖 Assistant** *14:30:03 [gpt-4o-mini]*
Ownership is a set of rules that govern...
```

### JSON Export

```json
{
  "exported_at": "2024-01-15T14:32:05Z",
  "model": "gpt-4o-mini",
  "messages": [
    {
      "role": "user",
      "content": "How does ownership work?",
      "timestamp": "2024-01-15T14:30:00Z"
    }
  ]
}
```

## Clear History

- Press `c` (Normal mode) to clear all messages
- Press `Ctrl+L` to clear the conversation
- This removes all messages from the current session

## Tips

- If no AI model is configured, the TUI shows a warning with setup instructions
- Messages are limited by `max_messages` (default 1,000) — oldest messages are dropped
- Auto-scroll keeps the latest messages visible; scroll up manually to review history
- The model name appears in each message header so you can see which model responded
