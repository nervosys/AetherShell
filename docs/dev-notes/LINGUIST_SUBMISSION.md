# GitHub Linguist Submission Guide for AetherShell

This document describes how to submit AetherShell as a recognized language in
[GitHub Linguist](https://github.com/github-linguist/linguist), enabling native
syntax highlighting for `.ae` files on GitHub.

## Pre-Submission Checklist

- [x] **Extension `.ae` is unclaimed** — verified against `languages.yml` (9277 lines, ~700 languages)
- [x] **TextMate grammar exists** — `editors/vscode/syntaxes/aethershell.tmLanguage.json` (388 lines)
- [x] **Grammar scope**: `source.aethershell`
- [x] **License**: AGPL-3.0-or-later with commercial dual-license (FSF/OSI-approved)
- [x] **Sample files available**: 12+ example `.ae` files in `examples/`
- [x] **Markdown injection grammar**: `aethershell.markdown.tmLanguage.json` (fenced code blocks: ae, aether, aethershell)
- [ ] **Usage threshold**: Linguist requires notable usage on GitHub (guideline: ≥200 unique repos with `.ae` files)

## Proposed `languages.yml` Entry

```yaml
AetherShell:
  type: programming
  color: "#4FC3F7"
  extensions:
  - ".ae"
  aliases:
  - ae
  - aether
  - aethershell
  tm_scope: source.aethershell
  ace_mode: text
  language_id: 835217904
```

### Field Notes

| Field | Value | Rationale |
|-------|-------|-----------|
| `type` | `programming` | AetherShell is a typed, scriptable shell language |
| `color` | `#4FC3F7` | Light blue — aligns with Aether/sky theming, distinct from nearby languages |
| `extensions` | `.ae` | Unclaimed in Linguist — no conflict with any existing language |
| `aliases` | `ae`, `aether`, `aethershell` | Matches fenced code block identifiers in Markdown |
| `tm_scope` | `source.aethershell` | Matches grammar's `scopeName` |
| `ace_mode` | `text` | No Ace editor mode exists; `text` is standard fallback |
| `language_id` | `835217904` | Random unique integer (verified not in use) |

## Submission Steps

### 1. Fork & Clone Linguist

```bash
git clone https://github.com/<your-fork>/linguist.git
cd linguist
git checkout -b add-aethershell
```

### 2. Add Grammar

```bash
script/add-grammar https://github.com/nervosys/AetherShell \
  --scope source.aethershell \
  --path editors/vscode/syntaxes/aethershell.tmLanguage.json
```

This registers the grammar in `grammars.yml` and fetches it into `vendor/grammars/`.

### 3. Add `languages.yml` Entry

Insert the following at the correct alphabetical position (after "AeroScript", before "Agda"):

```yaml
AetherShell:
  type: programming
  color: "#4FC3F7"
  extensions:
  - ".ae"
  aliases:
  - ae
  - aether
  - aethershell
  tm_scope: source.aethershell
  ace_mode: text
  language_id: 835217904
```

### 4. Add Sample Files

Copy 2–3 files to `samples/AetherShell/`:

```bash
mkdir -p samples/AetherShell
```

Recommended samples (demonstrates unique features, not trivial hello-world):

**`samples/AetherShell/pipelines.ae`** — typed pipeline operations:
```ae
# Arrays + typed pipelines with functional transforms
result = [1,2,3,4] | map(fn(x) => x * 2) | reduce(fn(a,b) => a + b, 0)
print(result)  # => 20

# Filter, take, and print
filtered = [5,4,3,2,1] | where(fn(x) => x > 2) | take(2)
print(filtered)  # => [5,4]
```

**`samples/AetherShell/tables.ae`** — structured data and records:
```ae
# List files as a typed table
files = ls(".")

# Filter to show only regular files (not directories)
regular_files = files | where(fn(r) => r.is_dir == false)
print("Regular files: " + len(regular_files))

# Get just the file names
file_names = regular_files | map(fn(f) => f.name)
first_five = file_names | take(5)
print(first_five)

# Show directories
dirs = files | where(fn(r) => r.is_dir == true)
print("Directories: " + len(dirs))
```

**`samples/AetherShell/pattern_matching.ae`** — pattern matching with guards:
```ae
# Option-like demo + pattern matching
value = Some(42)

match value {
  None() => print("no value"),
  Some(x) if x > 40 => print("big: ${x}"),
  Some(x) => print("small: ${x}")
}
```

### 5. Run Tests

```bash
bundle install
bundle exec rake samples
bundle exec rake test
```

### 6. Submit PR

- Title: `Add AetherShell language (.ae)`
- Reference this document and the AetherShell repository
- Link to the VS Code extension in the marketplace (if published)
- Mention unique features: typed pipelines, AI integration, pattern matching

## Interim: `.gitattributes` Workaround

Until the Linguist PR is merged, the repository uses `.gitattributes` to map `.ae`
files to a similar language for syntax highlighting:

```gitattributes
*.ae linguist-language=JavaScript
```

Once `AetherShell` is accepted into Linguist, update to:

```gitattributes
*.ae linguist-language=AetherShell
```

## Usage Threshold Concern

Linguist's contributing guidelines state that languages should have "notable" usage
on GitHub. The general guideline is ≥200 unique repositories containing files with
the target extension. Strategies to build toward this threshold:

1. **Open-source AetherShell scripts**: Encourage community contributions
2. **Template repositories**: Create starter templates using `.ae` files
3. **Documentation**: Use `.ae` code blocks in READMEs (handled by markdown injection grammar)
4. **VS Code marketplace**: The published extension helps establish the language ecosystem

## Grammar Location

The canonical grammar lives in this repository at:
```
editors/vscode/syntaxes/aethershell.tmLanguage.json
```

Linguist will pull from this path via `script/add-grammar`. If the grammar is
updated, Linguist automatically picks up changes on its next release cycle
(typically monthly).

## References

- [Linguist Contributing Guide](https://github.com/github-linguist/linguist/blob/main/CONTRIBUTING.md)
- [languages.yml](https://github.com/github-linguist/linguist/blob/main/lib/linguist/languages.yml)
- [TextMate Grammar Reference](https://macromates.com/manual/en/language_grammars)
- [AetherShell Repository](https://github.com/nervosys/AetherShell)
