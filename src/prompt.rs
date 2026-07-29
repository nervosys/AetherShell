//! Prompt rendering engine for AetherShell.
//!
//! `PromptConfig` has always carried a `format` string documenting `{cwd}`,
//! `{git_branch}` and friends, but nothing ever expanded it — the REPL printed a
//! hard-coded `æ❯`. This module is that missing renderer, plus the two prompt
//! families users actually ask for:
//!
//! - **fish-inspired** (`style = "fish"`) — a single compact line with the path
//!   abbreviated the way fish does it (`~/d/n/c/AetherShell`), the git branch
//!   inline, and a status-colored arrow. No background fills, no glyph
//!   dependencies beyond what a normal font has.
//! - **oh-my-posh-inspired** (`style = "powerline"`) — filled segment blocks
//!   separated by powerline glyphs (``), each with its own foreground and
//!   background, optionally with a right-aligned block and a transient prompt.
//!
//! Everything here is a pure function over [`PromptContext`], so the whole
//! renderer is testable without a TTY, a git repo, or a terminal size.
//!
//! # Example
//!
//! ```
//! use aethershell::prompt::{PromptContext, PromptStyle, render_left};
//! use aethershell::config::PromptConfig;
//!
//! let cfg = PromptConfig { style: "fish".into(), ..Default::default() };
//! let ctx = PromptContext::for_test("/home/ada/dev/proj", "ada", "box");
//! let line = render_left(&cfg, &ctx);
//! assert!(line.contains("~/d/proj"));
//! ```

use crate::config::{CustomColors, PromptConfig, Theme};
use std::path::Path;

/// Which family of prompt to render.
///
/// Parsed from `PromptConfig::style`; unknown values fall back to
/// [`PromptStyle::Classic`] so a typo in the config degrades to a working
/// prompt instead of an error at every keystroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptStyle {
    /// `æ❯` — the historical AetherShell prompt.
    Classic,
    /// fish-like: abbreviated path, inline git, status-colored arrow.
    Fish,
    /// oh-my-posh-like: filled powerline segment blocks.
    Powerline,
    /// Minimal two-line prompt in the spirit of `pure`/`starship`.
    Pure,
    /// Expand `PromptConfig::format` literally.
    Custom,
}

impl PromptStyle {
    /// Parse a style name case-insensitively, defaulting to [`Self::Classic`].
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "fish" => Self::Fish,
            "powerline" | "omp" | "oh-my-posh" => Self::Powerline,
            "pure" | "minimal" => Self::Pure,
            "custom" => Self::Custom,
            _ => Self::Classic,
        }
    }
}

/// Git state for the prompt, read cheaply from `.git` rather than by
/// shelling out (a subprocess per keystroke is not acceptable in a prompt).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitInfo {
    /// Branch name, or a short detached-HEAD sha.
    pub branch: String,
    /// Whether the worktree has uncommitted changes. Only populated when
    /// `show_git_dirty` is enabled, because it costs a `git status`.
    pub dirty: bool,
    /// True when HEAD is detached (branch holds an abbreviated sha).
    pub detached: bool,
}

/// Everything the renderer is allowed to know about the world.
///
/// Constructing this is the only part that touches the filesystem, the clock,
/// or the terminal; rendering itself is pure.
#[derive(Debug, Clone)]
pub struct PromptContext {
    /// Current working directory.
    pub cwd: String,
    /// The user's home directory, used to produce the leading `~`.
    pub home: String,
    /// Current username.
    pub user: String,
    /// Short hostname.
    pub host: String,
    /// Git state, when inside a repository.
    pub git: Option<GitInfo>,
    /// Exit status of the previous command (0 = success).
    pub status: i32,
    /// Duration of the previous command in milliseconds.
    pub duration_ms: u64,
    /// Preformatted clock string (e.g. `14:03`).
    pub time: String,
    /// Terminal width in columns, used to right-align the right prompt.
    pub columns: u16,
    /// Whether ANSI escapes may be emitted at all.
    pub colors: bool,
}

impl Default for PromptContext {
    fn default() -> Self {
        Self {
            cwd: String::new(),
            home: String::new(),
            user: String::new(),
            host: String::new(),
            git: None,
            status: 0,
            duration_ms: 0,
            time: String::new(),
            columns: 80,
            colors: true,
        }
    }
}

impl PromptContext {
    /// Build a context from the live environment.
    ///
    /// Git discovery walks up from `cwd` looking for `.git`; the dirty flag is
    /// only computed when `cfg.show_git_dirty` is set, since it requires a
    /// `git status` subprocess.
    pub fn current(cfg: &PromptConfig) -> Self {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let home = dirs::home_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let git = if cfg.show_git {
            discover_git(Path::new(&cwd), cfg.show_git_dirty)
        } else {
            None
        };
        Self {
            git,
            user: current_user(),
            host: short_hostname(),
            time: chrono::Local::now().format("%H:%M").to_string(),
            columns: terminal_columns(),
            colors: crate::config::get_config().colors.enabled,
            cwd,
            home,
            ..Default::default()
        }
    }

    /// Deterministic context for tests and doc examples.
    pub fn for_test(cwd: &str, user: &str, host: &str) -> Self {
        Self {
            cwd: cwd.to_string(),
            home: "/home/ada".to_string(),
            user: user.to_string(),
            host: host.to_string(),
            time: "12:00".to_string(),
            ..Default::default()
        }
    }

    /// Record the outcome of the command that just ran.
    pub fn with_result(mut self, status: i32, duration_ms: u64) -> Self {
        self.status = status;
        self.duration_ms = duration_ms;
        self
    }
}

// ============================================================================
// COLOR
// ============================================================================

/// Convert `#rrggbb` (or a bare `rrggbb`) into an ANSI truecolor foreground.
fn fg(hex: &str, on: bool) -> String {
    ansi_color(hex, 38, on)
}

/// Convert `#rrggbb` into an ANSI truecolor background.
fn bg(hex: &str, on: bool) -> String {
    ansi_color(hex, 48, on)
}

fn ansi_color(hex: &str, base: u8, on: bool) -> String {
    if !on {
        return String::new();
    }
    match parse_hex(hex) {
        Some((r, g, b)) => format!("\x1b[{base};2;{r};{g};{b}m"),
        None => String::new(),
    }
}

/// ANSI reset, emitted only when colors are enabled.
fn reset(on: bool) -> &'static str {
    if on {
        "\x1b[0m"
    } else {
        ""
    }
}

fn bold(on: bool) -> &'static str {
    if on {
        "\x1b[1m"
    } else {
        ""
    }
}

/// Parse `#rrggbb` / `rrggbb` into RGB components.
pub fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim().trim_start_matches('#');
    if h.len() != 6 {
        return None;
    }
    Some((
        u8::from_str_radix(&h[0..2], 16).ok()?,
        u8::from_str_radix(&h[2..4], 16).ok()?,
        u8::from_str_radix(&h[4..6], 16).ok()?,
    ))
}

/// Resolve the active palette, honoring `colors.theme = "custom"`.
fn palette() -> CustomColors {
    let config = crate::config::get_config();
    if config.colors.theme == "custom" {
        config.colors.custom.clone()
    } else {
        Theme::from_str(&config.colors.theme).colors()
    }
}

// ============================================================================
// PATH ABBREVIATION
// ============================================================================

/// Abbreviate a path the way fish does: every parent component collapses to its
/// first character (keeping a leading dot, so `.config` becomes `.c`), while the
/// final component is always kept in full.
///
/// ```
/// use aethershell::prompt::abbreviate_path;
/// assert_eq!(
///     abbreviate_path("/home/ada/dev/nervosys/cli/AetherShell", "/home/ada", true, 0),
///     "~/d/n/c/AetherShell"
/// );
/// ```
pub fn abbreviate_path(cwd: &str, home: &str, abbreviate: bool, max_segments: usize) -> String {
    let normalized = cwd.replace('\\', "/");
    let home_n = home.replace('\\', "/");

    // Collapse the home prefix to `~`, but only on a component boundary so
    // `/home/adamm2` is not mistaken for a child of `/home/adamm`.
    let (prefix, rest) = if !home_n.is_empty()
        && (normalized == home_n
            || normalized.starts_with(&format!("{}/", home_n.trim_end_matches('/'))))
    {
        (
            "~",
            normalized[home_n.trim_end_matches('/').len()..].to_string(),
        )
    } else {
        ("", normalized.clone())
    };

    let mut parts: Vec<String> = rest
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // Keep only the trailing `max_segments` components (0 = unlimited).
    let mut elided = false;
    if max_segments > 0 && parts.len() > max_segments {
        parts = parts.split_off(parts.len() - max_segments);
        elided = true;
    }

    if abbreviate && parts.len() > 1 {
        let last = parts.len() - 1;
        for (i, part) in parts.iter_mut().enumerate() {
            if i == last {
                continue;
            }
            *part = shorten_component(part);
        }
    }

    let joined = parts.join("/");
    match (prefix.is_empty(), elided) {
        // Root-relative and truncated: signal the elision rather than lying
        // about the path being absolute.
        (true, true) => format!("…/{joined}"),
        // A Windows path already carries its own root (`C:`); adding a leading
        // slash would produce the nonexistent `/C:/...`.
        (true, false)
            if joined.starts_with(|c: char| c.is_ascii_alphabetic())
                && joined[1..].starts_with(':') =>
        {
            joined
        }
        (true, false) => format!("/{joined}"),
        (false, true) => format!("~/…/{joined}"),
        (false, false) => {
            if joined.is_empty() {
                prefix.to_string()
            } else {
                format!("{prefix}/{joined}")
            }
        }
    }
}

/// Collapse one path component to its shortest recognizable form: the first
/// character, or a dot plus the first character for hidden directories.
///
/// Windows drive letters (`C:`) are left alone — shortening `C:` to `C` would
/// silently turn an absolute path into something that reads like a directory.
fn shorten_component(part: &str) -> String {
    if part.ends_with(':') {
        return part.to_string();
    }
    let mut chars = part.chars();
    match chars.next() {
        Some('.') => match chars.next() {
            Some(c) => format!(".{c}"),
            None => ".".to_string(),
        },
        Some(c) => c.to_string(),
        None => String::new(),
    }
}

// ============================================================================
// GIT DISCOVERY
// ============================================================================

/// Walk up from `start` looking for a `.git` directory or file, returning the
/// branch (or short sha when detached).
///
/// This reads `.git/HEAD` directly instead of invoking git: a prompt runs
/// before every keystroke's worth of input, and process spawn latency is the
/// single most common cause of a sluggish shell.
pub fn discover_git(start: &Path, want_dirty: bool) -> Option<GitInfo> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let dot_git = d.join(".git");
        if dot_git.exists() {
            let head_path = if dot_git.is_dir() {
                dot_git.join("HEAD")
            } else {
                // Worktree/submodule: `.git` is a file containing `gitdir: <path>`.
                let contents = std::fs::read_to_string(&dot_git).ok()?;
                let gitdir = contents.trim().strip_prefix("gitdir:")?.trim();
                Path::new(gitdir).join("HEAD")
            };
            let head = std::fs::read_to_string(head_path).ok()?;
            let info = parse_head(head.trim());
            return Some(GitInfo {
                dirty: want_dirty && worktree_dirty(d),
                ..info
            });
        }
        dir = d.parent();
    }
    None
}

/// Parse the contents of `.git/HEAD`.
pub fn parse_head(head: &str) -> GitInfo {
    if let Some(rest) = head.strip_prefix("ref:") {
        let branch = rest
            .trim()
            .rsplit('/')
            .next()
            .unwrap_or(rest.trim())
            .to_string();
        GitInfo {
            branch,
            dirty: false,
            detached: false,
        }
    } else {
        // Detached HEAD: show an abbreviated sha.
        GitInfo {
            branch: head.chars().take(7).collect(),
            dirty: false,
            detached: true,
        }
    }
}

/// Ask git whether the worktree is dirty. Opt-in — this is the expensive path.
fn worktree_dirty(repo: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .map(|o| !o.stdout.is_empty())
        .unwrap_or(false)
}

// ============================================================================
// ENVIRONMENT
// ============================================================================

fn current_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "user".to_string())
}

fn short_hostname() -> String {
    let raw = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "localhost".to_string());
    raw.split('.').next().unwrap_or(&raw).to_string()
}

/// Terminal width, falling back to 80 when stdout is not a terminal.
fn terminal_columns() -> u16 {
    #[cfg(feature = "native")]
    {
        crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80)
    }
    #[cfg(not(feature = "native"))]
    {
        80
    }
}

// ============================================================================
// DURATION FORMATTING
// ============================================================================

/// Format a command duration the way fish does: `1.2s`, `3m14s`, `1h02m`.
///
/// ```
/// use aethershell::prompt::format_duration;
/// assert_eq!(format_duration(1_250), "1.2s");
/// assert_eq!(format_duration(194_000), "3m14s");
/// ```
pub fn format_duration(ms: u64) -> String {
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let total_secs = ms / 1000;
    if total_secs < 60 {
        return format!("{}.{}s", total_secs, (ms % 1000) / 100);
    }
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    if mins < 60 {
        return format!("{mins}m{secs:02}s");
    }
    format!("{}h{:02}m", mins / 60, mins % 60)
}

// ============================================================================
// WIDTH
// ============================================================================

/// Visible width of a rendered prompt, ignoring ANSI escape sequences.
///
/// Used to right-align the right prompt. Powerline glyphs and CJK are counted
/// as one column each, which matches how terminals lay out the private-use
/// area glyphs these themes rely on.
pub fn visible_width(s: &str) -> usize {
    let mut width = 0usize;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip a CSI sequence: ESC [ params final-byte, where the final
            // byte is in @-~. The `[` introducer is itself in that range, so it
            // has to be consumed before scanning for the terminator.
            let mut peek = chars.clone();
            if peek.next() == Some('[') {
                chars.next();
            }
            for c2 in chars.by_ref() {
                if ('\u{40}'..='\u{7e}').contains(&c2) {
                    break;
                }
            }
            continue;
        }
        if c == '\n' {
            width = 0;
            continue;
        }
        width += 1;
    }
    width
}

// ============================================================================
// SEGMENTS
// ============================================================================

/// One rendered block of the prompt, before styling is applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Segment name, matching the identifiers used in `prompt.segments`.
    pub name: String,
    /// Text content, already formatted.
    pub text: String,
    /// Foreground color as `#rrggbb`.
    pub fg: String,
    /// Background color as `#rrggbb` (powerline only).
    pub bg: String,
}

/// Build the content of a named segment, or `None` when it has nothing to say
/// (no git repo, a fast command, a zero exit status).
pub fn build_segment(name: &str, cfg: &PromptConfig, ctx: &PromptContext) -> Option<Segment> {
    let p = palette();

    // In a powerline prompt the theme's accent color is the segment's
    // *background*; the text on top of it has to be a contrasting base color.
    // Using an accent for both — as an earlier version did for `git` — renders
    // the segment invisible.
    const DARK_TEXT: &str = "#1e1e2e"; // on accent (bright) backgrounds
    const LIGHT_TEXT: &str = "#cdd6f4"; // on chrome (dark) backgrounds
    const CHROME_BG: &str = "#45475a";
    const CHROME_BG_ALT: &str = "#585b70";

    let (text, fg_color, bg_color) = match name {
        "os" => (
            os_glyph().to_string(),
            LIGHT_TEXT.to_string(),
            CHROME_BG.to_string(),
        ),
        "user" => (
            ctx.user.clone(),
            LIGHT_TEXT.to_string(),
            CHROME_BG_ALT.to_string(),
        ),
        "host" => (
            ctx.host.clone(),
            LIGHT_TEXT.to_string(),
            CHROME_BG_ALT.to_string(),
        ),
        "user@host" => (
            format!("{}@{}", ctx.user, ctx.host),
            LIGHT_TEXT.to_string(),
            CHROME_BG_ALT.to_string(),
        ),
        "cwd" | "path" => (
            abbreviate_path(
                &ctx.cwd,
                &ctx.home,
                cfg.abbreviate_path,
                cfg.max_path_segments,
            ),
            DARK_TEXT.to_string(),
            p.punctuation.clone(),
        ),
        "git" | "git_branch" => {
            let g = ctx.git.as_ref()?;
            let glyph = if g.detached { "➦" } else { GLYPH_BRANCH };
            let dirty = if g.dirty { " ●" } else { "" };
            (
                format!("{glyph} {}{dirty}", g.branch),
                DARK_TEXT.to_string(),
                // A dirty worktree shifts the whole block to the warning color,
                // which reads at a glance far better than a small marker.
                if g.dirty {
                    p.warning.clone()
                } else {
                    p.success.clone()
                },
            )
        }
        "status" => {
            if ctx.status == 0 {
                return None;
            }
            (
                format!("✘ {}", ctx.status),
                DARK_TEXT.to_string(),
                p.error.clone(),
            )
        }
        "duration" | "time_taken" => {
            if !cfg.show_time || ctx.duration_ms < cfg.time_threshold_ms {
                return None;
            }
            (
                format!("󰔟 {}", format_duration(ctx.duration_ms)),
                DARK_TEXT.to_string(),
                p.warning.clone(),
            )
        }
        "time" | "clock" => (
            ctx.time.clone(),
            LIGHT_TEXT.to_string(),
            CHROME_BG.to_string(),
        ),
        "symbol" => (
            cfg.symbol.clone(),
            DARK_TEXT.to_string(),
            if ctx.status == 0 {
                p.success.clone()
            } else {
                p.error.clone()
            },
        ),
        // Anything else is treated as literal text, which makes
        // `segments = ["cwd", "»", "git"]` work without extra config syntax.
        other => (
            other.to_string(),
            LIGHT_TEXT.to_string(),
            CHROME_BG.to_string(),
        ),
    };

    if text.trim().is_empty() {
        return None;
    }

    // An explicit per-segment override always wins over the palette default.
    let (fg_color, bg_color) = match cfg.segment_colors.get(name) {
        Some(spec) => parse_color_spec(spec, &fg_color, &bg_color),
        None => (fg_color, bg_color),
    };

    Some(Segment {
        name: name.to_string(),
        text,
        fg: fg_color,
        bg: bg_color,
    })
}

/// Parse a `"fg"` or `"fg:bg"` color override, falling back per-component.
fn parse_color_spec(spec: &str, default_fg: &str, default_bg: &str) -> (String, String) {
    let mut parts = spec.splitn(2, ':');
    let f = parts.next().unwrap_or("").trim();
    let b = parts.next().unwrap_or("").trim();
    (
        if f.is_empty() {
            default_fg.to_string()
        } else {
            f.to_string()
        },
        if b.is_empty() {
            default_bg.to_string()
        } else {
            b.to_string()
        },
    )
}

/// A per-platform glyph for the `os` segment.
/// Nerd Font glyphs are written as escapes rather than literals: the raw
/// private-use codepoints do not survive every editor, patch, and terminal
/// round-trip, and a silently-emptied glyph turns into a missing segment.
const GLYPH_WINDOWS: &str = "\u{e70f}"; // nf-dev-windows
const GLYPH_APPLE: &str = "\u{f179}"; // nf-fa-apple
const GLYPH_LINUX: &str = "\u{f17c}"; // nf-fa-linux
/// Branch glyph, U+E0A0 — the powerline branch symbol.
const GLYPH_BRANCH: &str = "\u{e0a0}";

fn os_glyph() -> &'static str {
    if cfg!(target_os = "windows") {
        GLYPH_WINDOWS
    } else if cfg!(target_os = "macos") {
        GLYPH_APPLE
    } else {
        GLYPH_LINUX
    }
}

// ============================================================================
// RENDERERS
// ============================================================================

/// Render the left-hand prompt for the configured style.
pub fn render_left(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    match PromptStyle::from_str(&cfg.style) {
        PromptStyle::Classic => render_classic(cfg, ctx),
        PromptStyle::Fish => render_fish(cfg, ctx),
        PromptStyle::Powerline => render_powerline(cfg, ctx),
        PromptStyle::Pure => render_pure(cfg, ctx),
        PromptStyle::Custom => expand_format(&cfg.format, cfg, ctx),
    }
}

/// Render the right-hand prompt, or an empty string when none is configured.
pub fn render_right(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    if cfg.right.is_empty() {
        return String::new();
    }
    expand_format(&cfg.right, cfg, ctx)
}

/// Compose left and right prompts onto one line, padding so the right prompt
/// ends flush with the terminal's last column.
///
/// If the two would collide, the right prompt is dropped — a wrapped prompt is
/// far more disruptive than a missing clock.
pub fn render_line(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let left = render_left(cfg, ctx);
    let right = render_right(cfg, ctx);
    if right.is_empty() {
        return left;
    }
    let (lw, rw) = (visible_width(&left), visible_width(&right));
    let cols = ctx.columns as usize;
    if lw + rw + 1 >= cols {
        return left;
    }
    format!("{left}{}{right}", " ".repeat(cols - lw - rw))
}

/// The historical prompt: `æ❯`, dimmed chevron, colored by exit status.
fn render_classic(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let p = palette();
    let on = ctx.colors;
    let status_color = if ctx.status == 0 { &p.key } else { &p.error };
    format!(
        "{}æ{}{}{} ",
        fg(status_color, on),
        fg(&p.dim, on),
        cfg.symbol,
        reset(on)
    )
}

/// fish-inspired: `~/d/n/AetherShell  master ❯`.
///
/// One line, no background fills, path abbreviated, git inline, and the arrow
/// carrying the exit status as color — the details that make fish's prompt
/// readable at a glance.
fn render_fish(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let p = palette();
    let on = ctx.colors;
    let mut out = String::new();

    if cfg.show_user_host {
        out.push_str(&format!(
            "{}{}@{}{} ",
            fg(&p.string, on),
            ctx.user,
            ctx.host,
            reset(on)
        ));
    }

    out.push_str(&format!(
        "{}{}{}{}",
        bold(on),
        fg(&p.keyword, on),
        abbreviate_path(
            &ctx.cwd,
            &ctx.home,
            cfg.abbreviate_path,
            cfg.max_path_segments
        ),
        reset(on)
    ));

    if let Some(g) = ctx.git.as_ref() {
        let color = if g.dirty { &p.warning } else { &p.success };
        let dirty = if g.dirty { "●" } else { "" };
        out.push_str(&format!(
            " {}on {GLYPH_BRANCH} {}{}{}",
            fg(color, on),
            g.branch,
            dirty,
            reset(on)
        ));
    }

    if cfg.show_time && ctx.duration_ms >= cfg.time_threshold_ms {
        out.push_str(&format!(
            " {}{}{}",
            fg(&p.dim, on),
            format_duration(ctx.duration_ms),
            reset(on)
        ));
    }

    if ctx.status != 0 {
        out.push_str(&format!(
            " {}[{}]{}",
            fg(&p.error, on),
            ctx.status,
            reset(on)
        ));
    }

    let arrow_color = if ctx.status == 0 {
        &p.success
    } else {
        &p.error
    };
    out.push_str(&format!(
        " {}{}{} ",
        fg(arrow_color, on),
        cfg.symbol,
        reset(on)
    ));
    out
}

/// oh-my-posh-inspired: filled blocks joined by powerline separators.
///
/// Each segment's background bleeds into the next separator's foreground,
/// which is what produces the continuous ribbon effect.
fn render_powerline(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let on = ctx.colors;
    let segments: Vec<Segment> = cfg
        .segments
        .iter()
        .filter_map(|name| build_segment(name, cfg, ctx))
        .collect();

    if segments.is_empty() {
        return render_classic(cfg, ctx);
    }

    let sep = &cfg.powerline_separator;
    let mut out = String::new();

    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&bg(&seg.bg, on));
        out.push_str(&fg(&seg.fg, on));
        out.push_str(&format!(" {} ", seg.text.trim()));

        // The separator is drawn in this segment's background color, over the
        // next segment's background — so the transition has no seam.
        match segments.get(i + 1) {
            Some(next) => {
                out.push_str(reset(on));
                out.push_str(&bg(&next.bg, on));
                out.push_str(&fg(&seg.bg, on));
                out.push_str(sep);
            }
            None => {
                out.push_str(reset(on));
                out.push_str(&fg(&seg.bg, on));
                out.push_str(sep);
                out.push_str(reset(on));
            }
        }
    }

    if cfg.two_line {
        out.push('\n');
        let p = palette();
        let arrow_color = if ctx.status == 0 {
            &p.success
        } else {
            &p.error
        };
        out.push_str(&format!(
            "{}{}{} ",
            fg(arrow_color, on),
            cfg.symbol,
            reset(on)
        ));
    } else {
        out.push(' ');
    }
    out
}

/// Minimal two-line prompt: path on one line, bare arrow on the next, so typed
/// commands always start at the same column.
fn render_pure(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let p = palette();
    let on = ctx.colors;
    let mut out = format!(
        "{}{}{}",
        fg(&p.keyword, on),
        abbreviate_path(&ctx.cwd, &ctx.home, false, cfg.max_path_segments),
        reset(on)
    );
    if let Some(g) = ctx.git.as_ref() {
        out.push_str(&format!(
            " {}{}{}{}",
            fg(&p.dim, on),
            g.branch,
            if g.dirty { "*" } else { "" },
            reset(on)
        ));
    }
    let arrow_color = if ctx.status == 0 {
        &p.keyword
    } else {
        &p.error
    };
    out.push_str(&format!(
        "\n{}{}{} ",
        fg(arrow_color, on),
        cfg.symbol,
        reset(on)
    ));
    out
}

/// Expand the documented `{...}` placeholders in a format string.
///
/// This is what `PromptConfig::format` promised since the config was
/// introduced; every placeholder listed in the shipped `config.toml` comment is
/// supported here.
pub fn expand_format(format: &str, cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let p = palette();
    let on = ctx.colors;

    let git = ctx
        .git
        .as_ref()
        .map(|g| {
            format!(
                "{}{}{}{}",
                fg(&p.success, on),
                g.branch,
                if g.dirty { "*" } else { "" },
                reset(on)
            )
        })
        .unwrap_or_default();

    let status = if ctx.status == 0 {
        String::new()
    } else {
        format!("{}{}{}", fg(&p.error, on), ctx.status, reset(on))
    };

    let symbol_color = if ctx.status == 0 { &p.key } else { &p.error };

    let mut out = format.to_string();
    for (key, value) in [
        (
            "{cwd}",
            abbreviate_path(
                &ctx.cwd,
                &ctx.home,
                cfg.abbreviate_path,
                cfg.max_path_segments,
            ),
        ),
        ("{full_cwd}", ctx.cwd.clone()),
        ("{user}", ctx.user.clone()),
        ("{host}", ctx.host.clone()),
        ("{git_branch}", git),
        ("{time}", ctx.time.clone()),
        ("{status}", status),
        ("{duration}", format_duration(ctx.duration_ms)),
        (
            "{symbol}",
            format!("{}{}{}", fg(symbol_color, on), cfg.symbol, reset(on)),
        ),
        ("{newline}", "\n".to_string()),
    ] {
        if out.contains(key) {
            out = out.replace(key, &value);
        }
    }
    out
}

/// The compact prompt reprinted in place of the full one once a command has
/// been submitted, so scrollback stays readable. Mirrors oh-my-posh's
/// transient prompt.
pub fn render_transient(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let p = palette();
    let on = ctx.colors;
    format!("{}{}{} ", fg(&p.dim, on), cfg.symbol, reset(on))
}

/// The continuation prompt shown for unterminated multi-line input.
pub fn render_continuation(cfg: &PromptConfig, ctx: &PromptContext) -> String {
    let p = palette();
    format!(
        "{}{}{}",
        fg(&p.dim, ctx.colors),
        cfg.continuation,
        reset(ctx.colors)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(style: &str) -> PromptConfig {
        PromptConfig {
            style: style.to_string(),
            ..Default::default()
        }
    }

    fn ctx() -> PromptContext {
        PromptContext {
            colors: false,
            ..PromptContext::for_test("/home/ada/dev/nervosys/cli/AetherShell", "ada", "box")
        }
    }

    #[test]
    fn abbreviates_like_fish() {
        assert_eq!(
            abbreviate_path(
                "/home/ada/dev/nervosys/cli/AetherShell",
                "/home/ada",
                true,
                0
            ),
            "~/d/n/c/AetherShell"
        );
    }

    #[test]
    fn keeps_full_path_when_abbreviation_disabled() {
        assert_eq!(
            abbreviate_path("/home/ada/dev/proj", "/home/ada", false, 0),
            "~/dev/proj"
        );
    }

    #[test]
    fn home_itself_is_just_tilde() {
        assert_eq!(abbreviate_path("/home/ada", "/home/ada", true, 0), "~");
    }

    #[test]
    fn sibling_of_home_is_not_collapsed() {
        // `/home/ada2` must not be treated as living inside `/home/ada`.
        let out = abbreviate_path("/home/ada2/work", "/home/ada", true, 0);
        assert!(!out.starts_with('~'), "got {out}");
    }

    #[test]
    fn hidden_directories_keep_their_dot() {
        assert_eq!(
            abbreviate_path("/home/ada/.config/aether/themes", "/home/ada", true, 0),
            "~/.c/a/themes"
        );
    }

    #[test]
    fn max_segments_elides_the_middle() {
        let out = abbreviate_path("/home/ada/a/b/c/d/e", "/home/ada", true, 2);
        assert_eq!(out, "~/…/d/e");
    }

    #[test]
    fn windows_separators_are_normalized() {
        // The drive letter survives; only the interior components collapse, and
        // no spurious leading slash is added.
        let out = abbreviate_path(r"C:\Users\ada\dev\proj", "", true, 0);
        assert_eq!(out, "C:/U/a/d/proj");
    }

    #[test]
    fn parses_branch_from_head() {
        let info = parse_head("ref: refs/heads/feature/prompt-styles");
        assert_eq!(info.branch, "prompt-styles");
        assert!(!info.detached);
    }

    #[test]
    fn parses_detached_head_as_short_sha() {
        let info = parse_head("6a5c11ad9f3e2b1c0d");
        assert_eq!(info.branch, "6a5c11a");
        assert!(info.detached);
    }

    #[test]
    fn formats_durations_by_magnitude() {
        assert_eq!(format_duration(250), "250ms");
        assert_eq!(format_duration(1_250), "1.2s");
        assert_eq!(format_duration(194_000), "3m14s");
        assert_eq!(format_duration(7_320_000), "2h02m");
    }

    #[test]
    fn visible_width_ignores_ansi() {
        let colored = format!("{}abc{}", fg("#ff0000", true), reset(true));
        assert_eq!(visible_width(&colored), 3);
    }

    #[test]
    fn fish_prompt_shows_abbreviated_path_and_branch() {
        let mut c = ctx();
        c.git = Some(GitInfo {
            branch: "master".into(),
            dirty: false,
            detached: false,
        });
        let out = render_fish(&cfg("fish"), &c);
        assert!(out.contains("~/d/n/c/AetherShell"), "got {out}");
        assert!(out.contains("master"), "got {out}");
    }

    #[test]
    fn fish_prompt_reports_failure_status() {
        let c = ctx().with_result(127, 0);
        let out = render_fish(&cfg("fish"), &c);
        assert!(out.contains("[127]"), "got {out}");
    }

    #[test]
    fn duration_is_hidden_below_the_threshold() {
        let c = ctx().with_result(0, 10);
        let out = render_fish(&cfg("fish"), &c);
        assert!(!out.contains("10ms"), "got {out}");
    }

    #[test]
    fn powerline_joins_segments_with_separators() {
        let mut c = ctx();
        c.git = Some(GitInfo {
            branch: "master".into(),
            dirty: true,
            detached: false,
        });
        let mut config = cfg("powerline");
        config.segments = vec!["cwd".into(), "git".into()];
        let out = render_powerline(&config, &c);
        assert!(out.contains("~/d/n/c/AetherShell"), "got {out}");
        assert!(out.contains("master"), "got {out}");
        assert!(out.contains(&config.powerline_separator), "got {out}");
    }

    #[test]
    fn powerline_omits_empty_segments() {
        // No git repo and a zero exit status: both segments must vanish rather
        // than render as empty blocks.
        let mut config = cfg("powerline");
        config.segments = vec!["cwd".into(), "git".into(), "status".into()];
        let segs: Vec<_> = config
            .segments
            .iter()
            .filter_map(|n| build_segment(n, &config, &ctx()))
            .collect();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].name, "cwd");
    }

    #[test]
    fn powerline_falls_back_when_all_segments_are_empty() {
        let mut config = cfg("powerline");
        config.segments = vec!["git".into(), "status".into()];
        let out = render_powerline(&config, &ctx());
        assert!(out.contains('æ'), "expected classic fallback, got {out}");
    }

    #[test]
    fn custom_style_expands_documented_placeholders() {
        let mut config = cfg("custom");
        config.format = "{user}@{host} {cwd} {symbol}".into();
        let out = render_left(&config, &ctx());
        assert!(out.contains("ada@box"), "got {out}");
        assert!(out.contains("~/d/n/c/AetherShell"), "got {out}");
        assert!(out.contains('❯'), "got {out}");
    }

    #[test]
    fn right_prompt_is_padded_to_the_terminal_edge() {
        let mut config = cfg("classic");
        config.right = "{time}".into();
        let mut c = ctx();
        c.columns = 40;
        let line = render_line(&config, &c);
        assert_eq!(visible_width(&line), 40);
    }

    #[test]
    fn right_prompt_is_dropped_when_it_would_not_fit() {
        let mut config = cfg("classic");
        config.right = "{time}".into();
        let mut c = ctx();
        c.columns = 4;
        let line = render_line(&config, &c);
        assert!(!line.contains("12:00"), "got {line}");
    }

    #[test]
    fn unknown_style_degrades_to_classic() {
        assert_eq!(PromptStyle::from_str("nonsense"), PromptStyle::Classic);
        let out = render_left(&cfg("nonsense"), &ctx());
        assert!(out.contains('æ'), "got {out}");
    }

    #[test]
    fn colors_disabled_emits_no_escapes() {
        let mut c = ctx();
        c.colors = false;
        for style in ["classic", "fish", "powerline", "pure"] {
            let out = render_left(&cfg(style), &c);
            assert!(!out.contains('\x1b'), "{style} emitted escapes: {out:?}");
        }
    }

    #[test]
    fn glyphs_are_non_empty() {
        // Regression: these were once written as raw private-use codepoints and
        // silently became empty strings in transit, which made the `os` segment
        // vanish entirely (an empty segment is dropped by design).
        assert!(!os_glyph().is_empty());
        assert!(!GLYPH_BRANCH.is_empty());
        for g in [GLYPH_WINDOWS, GLYPH_APPLE, GLYPH_LINUX, GLYPH_BRANCH] {
            assert_eq!(g.chars().count(), 1, "expected exactly one codepoint");
        }
    }

    #[test]
    fn os_segment_renders() {
        let seg = build_segment("os", &cfg("powerline"), &ctx());
        assert!(seg.is_some(), "the os segment must not be dropped as empty");
    }

    #[test]
    fn every_segment_has_contrasting_foreground_and_background() {
        // Regression: the `git` segment once used the same accent for both,
        // rendering its text invisible against its own block.
        let mut c = ctx();
        c.git = Some(GitInfo {
            branch: "master".into(),
            dirty: false,
            detached: false,
        });
        let dirty_ctx = PromptContext {
            git: Some(GitInfo {
                branch: "master".into(),
                dirty: true,
                detached: false,
            }),
            ..c.clone()
        }
        .with_result(1, 5_000);

        let config = cfg("powerline");
        for name in [
            "os",
            "user",
            "host",
            "user@host",
            "cwd",
            "git",
            "status",
            "duration",
            "time",
            "symbol",
            "»",
        ] {
            for context in [&c, &dirty_ctx] {
                if let Some(seg) = build_segment(name, &config, context) {
                    assert_ne!(
                        seg.fg, seg.bg,
                        "segment '{name}' is invisible: fg == bg == {}",
                        seg.fg
                    );
                    assert!(
                        parse_hex(&seg.fg).is_some(),
                        "segment '{name}' has an unparseable fg {:?}",
                        seg.fg
                    );
                    assert!(
                        parse_hex(&seg.bg).is_some(),
                        "segment '{name}' has an unparseable bg {:?}",
                        seg.bg
                    );
                }
            }
        }
    }

    #[test]
    fn segment_color_override_wins_over_palette() {
        let mut config = cfg("powerline");
        config
            .segment_colors
            .insert("cwd".into(), "#123456:#654321".into());
        let seg = build_segment("cwd", &config, &ctx()).unwrap();
        assert_eq!(seg.fg, "#123456");
        assert_eq!(seg.bg, "#654321");
    }

    #[test]
    fn segment_color_override_accepts_foreground_only() {
        let mut config = cfg("powerline");
        config.segment_colors.insert("cwd".into(), "#123456".into());
        let seg = build_segment("cwd", &config, &ctx()).unwrap();
        assert_eq!(seg.fg, "#123456");
        assert!(!seg.bg.is_empty(), "background should keep its default");
    }
}
