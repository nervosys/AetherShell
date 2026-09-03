# Changelog

All notable changes to the AetherShell VS Code extension are documented here.

The extension is versioned independently of the AetherShell shell itself; the
shell's changelog lives at the repository root.

## [1.6.1] - 2026-09-03

Two commands had never worked, seven hover entries described builtins that do
not exist, and the 1.6.0 package could not activate. Each was found by running
the thing rather than reading it.

### Fixed

- **Run Selection was broken since it was added.** It emitted
  `ae -e "<code>"`, and `ae` has no `-e` flag: `error: unexpected argument '-e'
  found`. The flag is `-c`. Verified against the clap definition in
  `src/main.rs` and against the published binary.
- **Open TUI was broken the same way.** It launched `ae --tui`; `tui` is a
  subcommand, not a flag, so the terminal opened and immediately errored. Both
  survived because the failure appears in the user's terminal, where it reads
  as a broken install rather than a broken extension.
- **Seven hover entries were not builtins.** `filter`, `skip`, `to_int`,
  `to_float`, `which`, `os` and `arch` all answer `E_UNKNOWN_BUILTIN` at the
  prompt. `filter` is spelled `where`, `skip` is covered by `slice`, and
  `os`/`arch` are fields of `sys_info()`. The table is 56 entries now, each one
  checked against the shell's dispatcher.
- **1.6.0 was packaged without its dependencies** and could not activate.
  `vsce package --no-dependencies` produced 18 files and 38 KB; `extension.ts`
  imports `vscode-languageclient` at the top level, so the editor got
  MODULE_NOT_FOUND on the first `.ae` file opened. The package is 331 files.

### Added

- **Tests for the extension's own code.** The suite was 38 tests over static
  JSON — grammar, language configuration, manifest, snippets — and covered none
  of the six TypeScript modules. It is 78 now, over all of them, using a `vscode`
  stub so providers can be driven outside the editor.
- **Four ratchets**, each guarding the class rather than the instance: no source
  may invoke `ae` with an interface it does not have; every name in the hover
  table must be one the shell dispatches; every declared runtime dependency must
  be present and resolvable in the package; and no script may package without
  dependencies.
- **Open VSX publishing.** Cursor, VSCodium, Gitpod and Theia resolve extensions
  from Open VSX rather than the Visual Studio Marketplace, where this extension
  was absent — so there had never been anything for those users to install.
  `npm run publish:openvsx` and `publish:all` added, with install instructions
  for both registries in the README.

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

### Fixed — found by running the grammar rather than reading it

The three items above came from static comparison. Standing the grammar up
under `vscode-textmate` and `vscode-oniguruma` — the same pair VS Code uses —
found worse, because a rule can look correct and still be unreachable.

- **Everything inside `{ }` lost its highlighting.** The `records` rule matches
  every brace, so it covered function bodies, `if`/`else` blocks and
  `try`/`catch` blocks — not just record literals. Its inner pattern list
  included only comments, strings, numbers, keywords and property keys, so
  inside any block the builtins, function calls, member access, variables and
  operators were all dropped: `git.log()` and `print("x")` came back as
  unscoped text. That is most of the code in a real script. The rule now
  includes `$self`.
- **Two of the three `variables` rules never fired.** `#keywords` is included
  before `#variables` and matches `let` at the same start position, and
  TextMate breaks that tie on list order — so the compound "variable
  declaration" and "function definition" patterns were dead, and no binding
  ever got `variable.other.declaration` or `entity.name.function.definition`.
  They are now a separate `#declarations` group, tried before `#keywords`,
  while the bare-identifier rule stays last so it cannot swallow keywords.
- **The assignment operator was unscoped** in `let y = 1` (though not in
  `let f = fn…`, which took a different rule).

One thing deliberately *not* changed: there is no `$VAR` rule, and none was
added. The parser has no `$` handling at all — the only occurrence in the
examples is inside a comment — so highlighting it would have invented a feature.

### Internal

- **A test suite, where there was none.** `npm test` runs 37 checks over four
  files: tokenization through the real TextMate engine, manifest integrity,
  snippet well-formedness, and language-configuration consistency. A new
  `VS Code Extension` CI job compiles, tests and packages on every push.
- `tests/vscode_extension_agreement.rs` in the main repository pins the static
  half from the Rust side, so it holds even if the Node job is skipped: every
  builtin the grammar highlights must be answered by the dispatcher, every
  keyword the parser accepts must be highlighted and no others, and every file
  `package.json` names must exist.
- Both were verified red before green. Three of the four Rust checks fail on
  1.5.0; four of the tokenization tests fail against the pre-fix grammar,
  naming exactly the block-content and declaration defects above.
- One test was wrong and the code was right: a naive
  `body.replace(/\$\{[^}]*\}/g, "")` reported the `MCP Call` snippet as having
  unbalanced braces, because the default of `${3:{}}` is `{` and the regex
  stopped at the first `}`. The suite now expands snippets properly.

## [1.5.0] and earlier

Not documented here; see the repository history.
