# AetherShell Browser Extension

A Chrome/Firefox extension that brings AetherShell's typed shell capabilities directly to your browser through WebAssembly.

## Features

- **Terminal Overlay**: Press `Ctrl+Shift+A` (or `Cmd+Shift+A` on Mac) to toggle an AetherShell terminal on any webpage
- **Popup Interface**: Quick code execution from the extension popup
- **Context Menu**: Right-click selected text for AI actions and AetherShell evaluation
- **Full WASM Runtime**: The complete AetherShell evaluator runs locally in your browser

## Installation

### Development Build

1. **Build the WASM module:**
   ```powershell
   # From the browser-extension directory:
   .\build.ps1
   
   # Or for release build:
   .\build.ps1 -Release
   ```

2. **Load in Chrome:**
   - Open `chrome://extensions/`
   - Enable "Developer mode" (top right toggle)
   - Click "Load unpacked"
   - Select the `browser-extension` folder

3. **Load in Firefox:**
   - Open `about:debugging`
   - Click "This Firefox"
   - Click "Load Temporary Add-on"
   - Select `manifest.json` from this folder

## Usage

### Terminal Overlay

- Press `Ctrl+Shift+A` to open the terminal overlay
- Type AetherShell commands and press Enter
- Use arrow keys for command history
- Press `Escape` or click outside to close

Example commands:
```aether
# Basic arithmetic
1 + 2 * 3

# Array operations
[1, 2, 3] | map(fn(x) => x * 2)

# Records
let person = {name: "Alice", age: 30}
person.name

# String operations
"Hello, World!" | split(",") | first
```

### Popup Interface

- Click the extension icon to open the popup
- Enter AetherShell code in the textarea
- Click "Run" or press `Ctrl+Enter`
- Results are displayed with syntax highlighting

### Context Menu

Right-click selected text to access:
- **AI: Explain Selection** - Get an AI explanation of the text
- **AI: Summarize Selection** - Summarize the selected content
- **AI: Translate Selection** - Translate text to English
- **Evaluate as AetherShell** - Run selected text as AetherShell code

### Keyboard Shortcuts

| Shortcut       | Action                   |
| -------------- | ------------------------ |
| `Ctrl+Shift+A` | Toggle terminal overlay  |
| `Ctrl+Shift+I` | AI assist on selection   |
| `Ctrl+Enter`   | Run code (in popup)      |
| `Escape`       | Close terminal overlay   |
| `↑` / `↓`      | Navigate command history |

## Architecture

```
browser-extension/
├── manifest.json       # Extension manifest (MV3)
├── background.js       # Service worker (WASM init, context menus)
├── content/
│   ├── content.js      # Terminal overlay injection
│   └── content.css     # Overlay styles
├── popup/
│   ├── popup.html      # Popup UI
│   └── popup.js        # Popup logic
├── wasm/               # Built WASM module (generated)
│   ├── aether_wasm.js
│   ├── aether_wasm_bg.wasm
│   └── aether_wasm.d.ts
└── icons/              # Extension icons
```

## Requirements

- Chrome 88+ or Firefox 109+ (for Manifest V3 support)
- wasm-pack (for building from source)
- Rust with wasm32-unknown-unknown target

## Building from Source

```powershell
# Install wasm-pack if needed
cargo install wasm-pack

# Build extension
cd integrations/browser-extension
.\build.ps1

# Clean build artifacts
.\build.ps1 -Clean
```

## Notes

- The WASM module is ~2MB which may affect initial load time
- AI features require additional configuration (API keys in options)
- The terminal overlay has highest z-index to work on most pages
- State is preserved in `chrome.storage.local` between sessions

## License

AGPL-3.0-or-later with commercial dual-license option — see the main AetherShell [LICENSE](https://github.com/nervosys/AetherShell/blob/master/LICENSE) file.
