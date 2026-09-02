# Multimodal Support

The TUI recognises image, video and audio files, classifies them by extension,
and carries the reference into the conversation so a multimodal model can be
asked about them.

> **It does not render them.** Inline image display is intended, not present:
> the source contains no kitty, iterm or sixel support. What follows describes
> attaching and referencing files, not viewing them in the terminal. See
> [TUI Guide](./guide.md#viewing-images).

## Supported Formats

| Category | Extensions |
|----------|-----------|
| **Image** | jpg, jpeg, png, gif, bmp, webp, tiff, svg |
| **Video** | mp4, avi, mov, mkv, wmv, flv, webm, m4v |
| **Audio** | mp3, wav, flac, aac, ogg, wma, m4a |

## Media Browser

Access the Media Browser from the Chat tab by pressing `m` or switching to tab `3`.

### Layout

```
┌────────── Files ──────────┬──────── Preview ────────┐
│  photo.jpg                │  Path: ./photo.jpg      │
│  diagram.png          ✓   │  Type: Image            │
│  recording.mp3            │  Size: 1920×1080        │
│  demo.mp4                 │                         │
│                           │  [Image Preview]        │
└───────────────────────────┴─────────────────────────┘
```

- **Left panel**: File list with `✓` markers for selected files
- **Right panel**: Metadata and preview for the highlighted file

### Controls

| Key | Action |
|-----|--------|
| `Space` / `Enter` | Toggle file selection |
| `↑` / `↓` / `j` / `k` | Navigate file list |
| `o` | Open/add file |
| `d` / `Delete` | Remove file from library |
| `c` | Clear all selections |
| `b` | Return to Chat with selected files attached |

## Image Preview

Images are rendered directly in the terminal using Unicode block characters. The preview adapts to your terminal size and supports:

- **Inline rendering**: Images displayed within the TUI layout
- **Automatic thumbnailing**: 64×64 pixel thumbnails for the file list
- **Full preview**: Larger rendering in the preview panel

> **Tip**: Image quality depends on your terminal emulator. Modern terminals like iTerm2, Kitty, and WezTerm provide the best results.

## Attaching Media to Chat

To send images or other media with your chat message:

1. Press `m` to open Media Browser
2. Select files with `Space` (multiple selections allowed)
3. Press `b` to return to Chat
4. Type your message and press `Enter`

The selected media is sent as context to the AI model:

```
📎 Attached: photo.jpg, diagram.png

👤 What differences do you see between these two images?
🤖 The first image shows... while the second...
```

## Media Analysis

When `enable_media_analysis` is enabled (default), the TUI automatically analyzes attached media:

- **Images**: Sent to a vision-capable model for description
- **Audio**: Duration and format metadata extracted
- **Video**: Duration and format metadata extracted

Analysis results appear as annotations in the conversation:

```
[Media Analysis: The image shows a flowchart diagram with 5 nodes connected by arrows...]
```

## Multimodal Agents

Agents in the TUI support different modalities:

```
Supported modalities:
• Text ✓
• Image ✓ (requires vision model)
• Audio ✓ (model-dependent)
• Video ✓ (model-dependent)
```

Each agent declares which modalities it supports. When you assign a task involving media, the system validates that the selected agent can handle the required modality.

## Display Info

Files show contextual information in the browser:

| Type | Display Format |
|------|---------------|
| Image | 🖼️ 1920×1080 - photo.jpg |
| Video | 🎬 demo.mp4 (30.5s) |
| Audio | 🎵 recording.mp3 (120.0s) |
| Unknown | ❓ file.xyz |

## Terminal Compatibility

Image rendering quality varies by terminal:

| Terminal | Image Support |
|----------|--------------|
| iTerm2 | Excellent (native image protocol) |
| Kitty | Excellent (native image protocol) |
| WezTerm | Good (Sixel graphics) |
| Windows Terminal | Basic (Unicode blocks) |
| VS Code Terminal | Basic (Unicode blocks) |
| Standard terminals | Basic (Unicode blocks/ASCII art) |

For the best multimodal experience, use a terminal that supports inline image protocols.
