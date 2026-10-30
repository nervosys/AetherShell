# IronShell JupyterLab Extension

Jupyter kernel and JupyterLab integration for [IronShell](https://github.com/nervosys/AetherShell).

## Features

- **IronShell kernel** — execute `.ish` code in Jupyter notebooks
- **Syntax highlighting** for IronShell cells
- **Code completion** via the Agent API
- **Structured output** — typed results displayed as tables, JSON, etc.

## Prerequisites

The IronShell Agent API server must be running:

```bash
ish agent serve  # starts on port 3002
```

## Installation

```bash
# Install the kernel
cd extensions/jupyterlab
pip install -e .
jupyter kernelspec install jupyterlab_ironshell/kernelspec --user --name ironshell

# Install the JupyterLab extension
jlpm install
jlpm run build
jupyter labextension develop . --overwrite
```

## Usage

1. Start `ish agent serve`
2. Open JupyterLab
3. Create a new notebook with the "IronShell" kernel
4. Write IronShell expressions in cells
