# Chrome Web Store Submission Guide

This document contains all the information needed to submit the AetherShell browser extension to the Chrome Web Store.

## Store Listing Information

### Basic Info

**Extension Name:** AetherShell - AI Terminal for Browser

**Summary (132 chars max):**
AI-powered shell in your browser. Execute typed commands, use AI agents, and process data with functional pipelines.

**Description (16,000 chars max):**
```
AetherShell brings the power of an AI-native typed shell directly to your browser.

⚡ FEATURES

• Terminal Overlay: Press Ctrl+Shift+A on any webpage to open an AetherShell terminal
• Typed Pipelines: Process structured data with map, filter, reduce - no text parsing
• AI Integration: Query AI models, run autonomous agents, analyze images and text
• Context Menu: Right-click selected text for AI explanations, summaries, and translations
• Full WASM Runtime: The complete AetherShell evaluator runs locally in your browser

🚀 QUICK START

1. Click the extension icon or press Ctrl+Shift+A
2. Type AetherShell commands in the terminal
3. Results display with syntax highlighting
4. Use command history with arrow keys

💎 EXAMPLE COMMANDS

[1, 2, 3, 4, 5] | map(fn(x) => x * 2) | sum()
=> 30

{name: "Alice", age: 30} | keys()
=> ["name", "age"]

"Hello World" | split(" ") | len()
=> 2

🤖 AI FEATURES (requires API key in options)

ai("Explain quantum computing")
ai("Summarize this article", {text: selected_text})

📋 CONTEXT MENU

Right-click any selected text to:
• AI: Explain Selection
• AI: Summarize Selection  
• AI: Translate Selection
• Evaluate as AetherShell

⌨️ KEYBOARD SHORTCUTS

Ctrl+Shift+A - Toggle terminal overlay
Ctrl+Shift+I - AI assist on selection
Escape - Close terminal
↑/↓ - Navigate command history

🔒 PRIVACY

• All code execution happens locally via WebAssembly
• No data is sent to external servers without explicit AI actions
• AI features are optional and require user-configured API keys
• No analytics or tracking

🔗 LINKS

GitHub: https://github.com/nervosys/AetherShell
Documentation: https://github.com/nervosys/AetherShell/docs
Crates.io: https://crates.io/crates/aether_shell

Built with ❤️ by Nervosys
```

### Category
Developer Tools

### Language
English

## Graphic Assets Required

### Store Icon (128x128 PNG)
Located at: `integrations/browser-extension/store-assets/icon-128.png`

Requirements:
- 128x128 pixels
- PNG format
- No transparency at edges

### Small Promo Tile (440x280 PNG) - Optional but recommended
Use for promotional placement.

### Screenshots (1280x800 or 640x400 PNG)

Required: At least 1, recommended: 4-5

**Screenshot 1: Terminal Overlay**
- Show the terminal overlay on a webpage
- Caption: "Terminal overlay on any webpage (Ctrl+Shift+A)"

**Screenshot 2: Code Execution**
- Show pipeline commands with colorful output
- Caption: "Typed pipelines with structured data"

**Screenshot 3: AI Features**
- Show AI query and response
- Caption: "Built-in AI integration"

**Screenshot 4: Context Menu**
- Show right-click menu on selected text
- Caption: "AI-powered context menu actions"

**Screenshot 5: Popup Interface**
- Show the extension popup
- Caption: "Quick access popup for fast commands"

## Privacy Policy

**Required URL:** Host at https://aethershell.io/privacy or GitHub

```markdown
# AetherShell Browser Extension Privacy Policy

Last updated: January 2026

## Data Collection

The AetherShell browser extension does NOT collect, store, or transmit:
- Personal information
- Browsing history
- Page content
- User behavior or analytics

## Local Processing

All AetherShell code execution occurs locally in your browser using WebAssembly. 
No code or data is sent to external servers during normal operation.

## AI Features

When you explicitly use AI features:
- Text you select for AI processing is sent to the AI provider you configured
- You must provide your own API key (e.g., OpenAI)
- We do not store or have access to your API keys or AI conversations

## Permissions Explained

- **activeTab**: Required to inject terminal overlay on current page
- **storage**: Save your preferences and command history locally
- **contextMenus**: Provide right-click menu options
- **scripting**: Inject content scripts for terminal overlay

## Contact

For privacy questions: contact@nervosys.ai
GitHub: https://github.com/nervosys/AetherShell
```

## Review Checklist

Before submission:

- [ ] Extension loads without errors
- [ ] All declared permissions are justified
- [ ] No remote code execution (WASM is bundled)
- [ ] Privacy policy URL is accessible
- [ ] Screenshots are accurate and current
- [ ] Description matches actual functionality
- [ ] No trademarked terms misused

## Submission Steps

1. Go to https://chrome.google.com/webstore/devconsole
2. Pay one-time $5 developer fee (if not already)
3. Click "New Item"
4. Upload ZIP of browser-extension folder
5. Fill in store listing info above
6. Upload graphic assets
7. Add privacy policy URL
8. Select "Developer Tools" category
9. Submit for review (typically 1-3 business days)

## Post-Submission

- Monitor developer console for review feedback
- Respond promptly to any reviewer questions
- Update listing with version changes
- Track installs and ratings in dashboard
