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
