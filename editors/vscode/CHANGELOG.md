# Changelog

All notable changes to the AetherShell VS Code extension are documented here.

The extension is versioned independently of the AetherShell shell itself; the
shell's changelog lives at the repository root.

## [1.6.0] - 2026-08-31

Correctness pass. The extension is a second, hand-maintained description of the
language, and nothing linked it to the shell — so it had drifted in the same way
`AGENTS.md` had, describing things that were not there. Each item below was
measured against the running dispatcher and parser, not read off the source.

### Fixed

- **The `.ae` file icon never worked.** `package.json` set it to
  `./icons/ae-light.svg` and `./icons/ae-dark.svg`; neither has ever existed in
  the repository (`git log --diff-filter=D` finds no deletion, so they were
  never there to delete). Both now point at `./icons/ae-icon.svg`, which is a
  theme-neutral gradient and reads correctly on light and dark backgrounds. A
  missing packaged asset is silent at runtime, which is why this survived four
  releases.
- **21 of 86 "builtins" in the grammar were not callable.** `sin`, `cos`, `tan`,
  `parse_json`, `to_json`, `to_string`, `to_int`, `to_float`, `substring`,
  `entries`, `enumerate`, `http_post`, `http_put`, `http_delete`, `mcp_list`
  exist nowhere in the dispatcher. `log`, `merge`, `get`, `set`, `exec`,
  `spawn` and `download` are module *members* — `git.log`, `ssh.exec` — already
  scoped by the member-access rule, so matching them as bare words coloured
  every ordinary variable named `log` or `set` as though it were a builtin.

### Added

- **The 11 keywords the grammar had been ignoring**: `else`, `try`, `catch`,
  `throw`, `import`, `export`, `from`, `as`, `pub`, `async`, `await`. `else`
  had been unhighlighted since the grammar was written.
- Keywords are now scoped by role — control flow, exceptions, async, imports,
  storage modifiers — rather than lumped into one `keyword.control`, so themes
  can distinguish them.

### Internal

- `tests/vscode_extension_agreement.rs` in the main repository now pins all of
  this mechanically: every builtin the grammar highlights must be answered by
  the dispatcher, every keyword the parser accepts must be highlighted (and no
  others), and every file `package.json` points at must exist. Verified red
  against the pre-fix files before being made green — three of its four checks
  fail on 1.5.0.

## [1.5.0] and earlier

Not documented here; see the repository history.
