# AetherShell JupyterLab Extension

Jupyter kernel and JupyterLab integration for [AetherShell](https://github.com/nervosys/AetherShell).

## Features

- **AetherShell kernel** — execute `.ae` code in Jupyter notebooks
- **Syntax highlighting** for AetherShell cells
- **Code completion** via the Agent API
- **Structured output** — typed results displayed as tables, JSON, etc.

## Prerequisites

The AetherShell Agent API server must be running:

```bash
ae agent serve  # starts on port 3002
```

## Installation

```bash
# Install the kernel
cd extensions/jupyterlab
pip install -e .
jupyter kernelspec install jupyterlab_aethershell/kernelspec --user --name aethershell

# Install the JupyterLab extension
jlpm install
jlpm run build
jupyter labextension develop . --overwrite
```

## Usage

1. Start `ae agent serve`
2. Open JupyterLab
3. Create a new notebook with the "AetherShell" kernel
4. Write AetherShell expressions in cells
