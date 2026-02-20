# AetherShell VS Code Web Extension

Language support and Agent API integration for [AetherShell](https://github.com/nervosys/AetherShell) in VS Code Web (vscode.dev).

## Features

- **Syntax highlighting** for `.ae` files
- **Code evaluation** via the Agent API (`Ctrl+Shift+P` → "AetherShell: Evaluate Selection")
- **Hover documentation** for builtins (requires Agent API)
- **Completion** for module builtins (requires Agent API)

## Requirements

For full functionality, run the AetherShell Agent API server:

```bash
ae agent serve  # starts on port 3002
```

## Development

```bash
cd extensions/vscode-web
npm install
npm run build
npm run package  # creates .vsix for web
```
