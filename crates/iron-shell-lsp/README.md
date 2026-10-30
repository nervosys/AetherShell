# iron-shell-lsp

A [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
implementation for [IronShell](https://github.com/nervosys/AetherShell) — the
typed, functional, multi-agent shell. It gives editors (VS Code, Neovim, Helix,
and any LSP client) language intelligence for `.ish` scripts.

## Features

- Diagnostics (syntax + type errors) as you type
- Hover, completion, and signature help over builtins and pipeline operators
- Document symbols and go-to-definition
- Built on the same `iron-shell` engine that runs the shell, so analysis matches
  runtime behavior

## Install

```sh
cargo install iron-shell-lsp
```

This installs the `iron-shell-lsp` binary. Point your editor's LSP client at it for
files with the `.ish` extension; it speaks LSP over stdio.

## Example (Neovim, `nvim-lspconfig`-style)

```lua
vim.lsp.start({
  name = "ironshell",
  cmd = { "iron-shell-lsp" },
  filetypes = { "ironshell" },
  root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
})
```

## License

Licensed AGPL-3.0-or-later. See [LICENSE](LICENSE).
