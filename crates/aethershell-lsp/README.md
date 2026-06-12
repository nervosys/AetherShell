# aethershell-lsp

A [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
implementation for [AetherShell](https://github.com/nervosys/AetherShell) — the
typed, functional, multi-agent shell. It gives editors (VS Code, Neovim, Helix,
and any LSP client) language intelligence for `.ae` scripts.

## Features

- Diagnostics (syntax + type errors) as you type
- Hover, completion, and signature help over builtins and pipeline operators
- Document symbols and go-to-definition
- Built on the same `aethershell` engine that runs the shell, so analysis matches
  runtime behavior

## Install

```sh
cargo install aethershell-lsp
```

This installs the `aethershell-lsp` binary. Point your editor's LSP client at it for
files with the `.ae` extension; it speaks LSP over stdio.

## Example (Neovim, `nvim-lspconfig`-style)

```lua
vim.lsp.start({
  name = "aethershell",
  cmd = { "aethershell-lsp" },
  filetypes = { "ae" },
  root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
})
```

## License

Licensed AGPL-3.0-or-later. See [LICENSE](LICENSE).
