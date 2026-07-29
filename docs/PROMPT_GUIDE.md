# Prompt & Line Editing Guide

AetherShell's interactive prompt is configurable in `~/.config/aether/config.toml`
under `[prompt]`. Four built-in styles ship out of the box, plus a `custom` style
that expands a format string of your own.

## Choosing a style

```toml
[prompt]
style = "fish"      # classic | fish | powerline | pure | custom
symbol = "❯"
```

### `classic` (default)

The historical AetherShell prompt: `æ❯`, with the `æ` colored green on success
and red after a failed command.

### `fish`

```
~/d/n/c/AetherShell on  master● 1.2s [1] ❯
```

One line, no background fills, and no font requirements beyond the branch glyph.
Parent path components collapse to a single character the way fish does it, so
deep paths stay short without losing context. The final component is always
shown in full. Hidden directories keep their dot (`.config` → `.c`), and Windows
drive letters are preserved (`C:/U/a/dev/proj`).

Segments appear only when they have something to say: the git branch only inside
a repository, the duration only past `time_threshold_ms`, the exit status only
when nonzero.

### `powerline` (oh-my-posh style)

```
  ~/d/n/c/AetherShell   master   3m14s 
```

Filled segment blocks joined by powerline separators, each with its own
foreground and background. **Requires a [Nerd Font](https://www.nerdfonts.com/)**
for the separator and OS glyphs.

```toml
[prompt]
style = "powerline"
segments = ["os", "cwd", "git", "status", "duration"]
powerline_separator = ""
two_line = false          # put the symbol on its own second line

[prompt.segment_colors]
cwd = "#1e1e2e:#89b4fa"   # "#fg:#bg", or just "#fg"
git = "#1e1e2e:#a6e3a1"
```

Recognized segment names: `os`, `user`, `host`, `user@host`, `cwd`, `git`,
`status`, `duration`, `time`, `symbol`. Any other value renders as literal text,
so `segments = ["cwd", "»", "git"]` works without extra syntax.

### `pure`

A minimal two-line prompt in the spirit of `pure`/`starship` — path and branch on
the first line, a bare symbol on the second, so typed commands always begin at
the same column.

### `custom`

Expands `format` literally. Every placeholder documented in the shipped config is
supported:

| Placeholder    | Expands to                                        |
| -------------- | ------------------------------------------------- |
| `{cwd}`        | Working directory, abbreviated per `abbreviate_path` |
| `{full_cwd}`   | Working directory, never abbreviated              |
| `{user}`       | Username                                          |
| `{host}`       | Short hostname                                    |
| `{git_branch}` | Branch name, with `*` when dirty                  |
| `{time}`       | Clock, `HH:MM`                                    |
| `{status}`     | Exit status, empty when zero                      |
| `{duration}`   | Duration of the previous command                  |
| `{symbol}`     | The prompt symbol, colored by exit status         |
| `{newline}`    | A line break                                      |

```toml
style = "custom"
format = "{user}@{host} {cwd} {git_branch}{newline}{symbol} "
```

## Shared options

```toml
abbreviate_path = true     # ~/d/n/proj instead of ~/dev/nervosys/proj
max_path_segments = 0      # keep only the last N components (0 = all)
show_user_host = false     # add user@host to the fish style
show_git = true
show_git_dirty = false     # costs a `git status` per prompt — opt in
show_time = true
time_threshold_ms = 1000   # hide durations shorter than this
transient = true           # collapse the prompt after submitting a command
right = "{time}"           # right-aligned prompt
```

`right` is padded so it ends flush with the terminal's last column, and is
dropped entirely if it would collide with the left prompt — a wrapped prompt is
more disruptive than a missing clock.

**Performance note.** The git branch is read directly from `.git/HEAD` rather
than by invoking git, so the prompt costs no subprocess. Enabling
`show_git_dirty` reintroduces one `git status` per prompt; leave it off in large
repositories.

## Line editing

The REPL provides fish-style interactive editing.

### Autosuggestions

As you type, the most recent matching command from history appears as dimmed
ghost text. Press **→** (at end of line) or **Ctrl-F** to accept it.

```toml
[prompt]
autosuggestions = true
```

Suggestions are drawn only when the cursor is at the end of the line, and only
when colors are enabled — ghost text indistinguishable from real input is worse
than none.

### Abbreviations

Expanded when you press space or Enter, so what runs and what lands in history
is always the expanded form.

```toml
[prompt.abbreviations]
ll = "ls(\".\") | sort()"
gs = "git.status()"
w  = "where"          # also expands after a pipe: ls(".")|w <space>
```

### Keys

| Key                   | Action                            |
| --------------------- | --------------------------------- |
| `→` / `Ctrl-F`        | Accept autosuggestion             |
| `↑` / `↓`, `Ctrl-P/N` | History; `↓` past the end restores your draft |
| `Ctrl-A` / `Ctrl-E`   | Start / end of line               |
| `Ctrl-W`              | Delete previous word              |
| `Ctrl-U` / `Ctrl-K`   | Delete to start / end of line     |
| `Ctrl-L`              | Clear screen                      |
| `Ctrl-C`              | Abandon the line                  |
| `Ctrl-D`              | Exit (on an empty line)           |

When stdin is not a terminal — pipes, CI, `ae < script.ae` — the editor falls
back to plain buffered reads, so non-interactive behavior is unchanged.

History is persisted to `$XDG_DATA_HOME/aether/history` and honors the
`[history]` settings (`enabled`, `ignore_duplicates`, `ignore_space`).
