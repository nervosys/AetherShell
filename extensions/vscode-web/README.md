# IronShell VS Code Web Extension

Language support and Agent API integration for [IronShell](https://github.com/nervosys/AetherShell) in VS Code Web (vscode.dev).

## Features

- **Syntax highlighting** for `.ish` files
- **Code evaluation** via the Agent API (`Ctrl+Shift+P` → "IronShell: Evaluate Selection")
- **Hover documentation** for builtins (requires Agent API)
- **Completion** for module builtins (requires Agent API)

## Requirements

For full functionality, run the IronShell Agent API server:

```bash
ish agent serve  # starts on port 3002
```

## Development

```bash
cd extensions/vscode-web
npm install
npm run build
npm run package  # creates .vsix for web
```
