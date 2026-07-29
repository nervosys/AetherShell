//! XDG Base Directory compliant configuration for AetherShell
//!
//! Configuration file locations (in order of precedence):
//! 1. `$AETHER_CONFIG` environment variable (if set)
//! 2. `$XDG_CONFIG_HOME/aether/config.toml` (typically `~/.config/aether/config.toml`)
//! 3. `~/.aetherrc` (legacy fallback)
//!
//! Additional configuration files:
//! - `$XDG_CONFIG_HOME/aether/theme.toml` - Custom color themes
//! - `$XDG_CONFIG_HOME/aether/aliases.toml` - Command aliases
//! - `$XDG_CONFIG_HOME/aether/init.ae` - Startup script (executed on shell start)
//!
//! Data directory: `$XDG_DATA_HOME/aether/` (typically `~/.local/share/aether/`)
//! - `history` - Command history
//! - `plugins/` - Installed plugins
//!
//! Cache directory: `$XDG_CACHE_HOME/aether/` (typically `~/.cache/aether/`)
//! - Downloaded resources, temporary files

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Global configuration instance
static CONFIG: OnceLock<ShellConfig> = OnceLock::new();

/// Get the global configuration (loads on first access)
pub fn get_config() -> &'static ShellConfig {
    CONFIG.get_or_init(|| ShellConfig::load().unwrap_or_default())
}

/// Reload configuration from disk
pub fn reload_config() -> Result<ShellConfig> {
    ShellConfig::load()
}

/// Main shell configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct ShellConfig {
    /// General shell settings
    pub shell: ShellSettings,
    /// Color and theme settings
    pub colors: ColorConfig,
    /// Prompt configuration
    pub prompt: PromptConfig,
    /// AI/Agent settings
    pub ai: AiConfig,
    /// History settings
    pub history: HistoryConfig,
    /// Editor settings
    pub editor: EditorConfig,
    /// Keybinding configuration
    pub keybindings: KeybindingsConfig,
    /// Custom aliases
    #[serde(default)]
    pub aliases: HashMap<String, String>,
    /// Environment variables to set on startup
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// General shell settings
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellSettings {
    /// Enable/disable welcome banner
    pub show_banner: bool,
    /// Enable/disable startup tips
    pub show_tips: bool,
    /// Default working directory (empty = current dir)
    pub default_directory: String,
    /// Enable vi mode (vs emacs mode)
    pub vi_mode: bool,
    /// Auto-cd: treat directory names as cd commands
    pub auto_cd: bool,
    /// Glob expansion in arguments
    pub glob_expansion: bool,
    /// Enable command correction suggestions
    pub command_correction: bool,
    /// Bell style: "none", "visible", "audible"
    pub bell_style: String,
}

/// Color and theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorConfig {
    /// Enable/disable colors entirely
    pub enabled: bool,
    /// Color theme: "catppuccin", "monokai", "dracula", "nord", "gruvbox", "solarized", "custom"
    pub theme: String,
    /// Force color output even when not a TTY
    pub force: bool,
    /// True color (24-bit) support
    pub true_color: bool,
    /// Custom colors (used when theme = "custom")
    pub custom: CustomColors,
}

/// Custom color definitions (CSS-style hex or named colors)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomColors {
    /// Color for numbers (Int, Float)
    pub number: String,
    /// Color for strings
    pub string: String,
    /// Color for booleans
    pub boolean: String,
    /// Color for keywords (fn, let, if, etc.)
    pub keyword: String,
    /// Color for operators and punctuation
    pub punctuation: String,
    /// Color for record/object keys
    pub key: String,
    /// Color for URIs
    pub uri: String,
    /// Color for errors
    pub error: String,
    /// Color for warnings
    pub warning: String,
    /// Color for success messages
    pub success: String,
    /// Color for info/dim text
    pub dim: String,
    /// Color for comments
    pub comment: String,
}

/// Prompt configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PromptConfig {
    /// Primary prompt format (supports placeholders)
    /// Placeholders: {cwd}, {user}, {host}, {git_branch}, {time}, {status}
    pub format: String,
    /// Continuation prompt (for multi-line input)
    pub continuation: String,
    /// Right-side prompt (if terminal supports it)
    pub right: String,
    /// Show git branch in prompt
    pub show_git: bool,
    /// Show execution time for long-running commands
    pub show_time: bool,
    /// Threshold in ms for showing execution time
    pub time_threshold_ms: u64,
    /// Transient prompt: clear previous prompts on new command
    pub transient: bool,

    // -- Prompt styles (see `crate::prompt`) ---------------------------------
    /// Prompt style: "classic", "fish", "powerline", "pure", or "custom".
    /// Anything else falls back to "classic".
    pub style: String,
    /// The prompt character itself. Defaults to fish's `❯`.
    pub symbol: String,
    /// Segments rendered by the "powerline" style, in order. Recognized names:
    /// `os`, `user`, `host`, `user@host`, `cwd`, `git`, `status`, `duration`,
    /// `time`, `symbol`. Any other value is rendered as literal text.
    pub segments: Vec<String>,
    /// Per-segment color override, `"#fg"` or `"#fg:#bg"`, keyed by segment name.
    pub segment_colors: HashMap<String, String>,
    /// Glyph joining powerline segments (U+E0B0 by default).
    pub powerline_separator: String,
    /// Abbreviate intermediate path components fish-style (`~/d/n/proj`).
    pub abbreviate_path: bool,
    /// Keep only the trailing N path components (0 = keep all).
    pub max_path_segments: usize,
    /// Render the powerline prompt as two lines, with the symbol on its own line.
    pub two_line: bool,
    /// Include `user@host` in the fish-style prompt (off by default, as in fish
    /// when the session is local).
    pub show_user_host: bool,
    /// Mark a dirty worktree in the git segment. Costs a `git status` per
    /// prompt, so it is opt-in.
    pub show_git_dirty: bool,
    /// Suggest completions from history as dimmed ghost text, fish-style.
    pub autosuggestions: bool,
    /// Abbreviations expanded when you press space, fish-style.
    pub abbreviations: HashMap<String, String>,
}

/// AI and agent configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AiConfig {
    /// Default AI provider: "openai", "ollama", "anthropic", etc.
    pub default_provider: String,
    /// Default model for AI operations
    pub default_model: String,
    /// Enable AI suggestions/completions
    pub suggestions: bool,
    /// Maximum tokens for AI responses
    pub max_tokens: u32,
    /// Temperature for AI responses (0.0-2.0)
    pub temperature: f32,
    /// Enable streaming responses
    pub streaming: bool,
    /// Agent tool whitelist (empty = all tools allowed)
    pub allowed_tools: Vec<String>,
    /// Agent tool blacklist
    pub blocked_tools: Vec<String>,
    /// Maximum agent steps before timeout
    pub max_agent_steps: u32,
}

/// History configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct HistoryConfig {
    /// Enable command history
    pub enabled: bool,
    /// Maximum history entries
    pub max_size: usize,
    /// Ignore duplicate consecutive commands
    pub ignore_duplicates: bool,
    /// Ignore commands starting with space
    pub ignore_space: bool,
    /// Patterns to ignore (regex)
    pub ignore_patterns: Vec<String>,
    /// Share history across sessions
    pub share: bool,
    /// Save timestamps with history
    pub timestamps: bool,
}

/// Editor configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorConfig {
    /// External editor for `edit` command
    pub external: String,
    /// Tab width for display
    pub tab_width: u8,
    /// Enable syntax highlighting in editor
    pub syntax_highlighting: bool,
    /// Enable line numbers
    pub line_numbers: bool,
    /// Enable auto-indent
    pub auto_indent: bool,
    /// Enable bracket matching
    pub bracket_matching: bool,
}

/// Keybindings configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingsConfig {
    /// Keybinding mode: "emacs" or "vi"
    pub mode: String,
    /// Custom keybindings (key -> action)
    #[serde(default)]
    pub custom: HashMap<String, String>,
}

// ============================================================================
// Default implementations
// ============================================================================

impl Default for ShellSettings {
    fn default() -> Self {
        Self {
            show_banner: true,
            show_tips: true,
            default_directory: String::new(),
            vi_mode: false,
            auto_cd: false,
            glob_expansion: true,
            command_correction: true,
            bell_style: "none".to_string(),
        }
    }
}

impl Default for ColorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            theme: "catppuccin".to_string(),
            force: false,
            true_color: true,
            custom: CustomColors::default(),
        }
    }
}

impl Default for CustomColors {
    fn default() -> Self {
        // Catppuccin Mocha colors as defaults
        Self {
            number: "#a6e3a1".to_string(),      // Green
            string: "#a6e3a1".to_string(),      // Green
            boolean: "#cba6f7".to_string(),     // Mauve/Magenta
            keyword: "#cba6f7".to_string(),     // Mauve
            punctuation: "#89b4fa".to_string(), // Blue
            key: "#89dceb".to_string(),         // Sky/Cyan
            uri: "#f9e2af".to_string(),         // Yellow
            error: "#f38ba8".to_string(),       // Red
            warning: "#fab387".to_string(),     // Peach
            success: "#a6e3a1".to_string(),     // Green
            dim: "#6c7086".to_string(),         // Overlay0
            comment: "#6c7086".to_string(),     // Overlay0
        }
    }
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            format: "{symbol}".to_string(), // æ❯
            continuation: "… ".to_string(),
            right: String::new(),
            show_git: true,
            show_time: true,
            time_threshold_ms: 1000,
            transient: false,
            style: "classic".to_string(),
            symbol: "❯".to_string(),
            segments: vec![
                "os".to_string(),
                "cwd".to_string(),
                "git".to_string(),
                "status".to_string(),
                "duration".to_string(),
            ],
            segment_colors: HashMap::new(),
            powerline_separator: "\u{e0b0}".to_string(),
            abbreviate_path: true,
            max_path_segments: 0,
            two_line: false,
            show_user_host: false,
            show_git_dirty: false,
            autosuggestions: true,
            abbreviations: HashMap::new(),
        }
    }
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            default_provider: "openai".to_string(),
            default_model: "gpt-4o-mini".to_string(),
            suggestions: false,
            max_tokens: 4096,
            temperature: 0.7,
            streaming: true,
            allowed_tools: Vec::new(),
            blocked_tools: Vec::new(),
            max_agent_steps: 10,
        }
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size: 10000,
            ignore_duplicates: true,
            ignore_space: true,
            ignore_patterns: vec![r"^exit$".to_string(), r"^quit$".to_string()],
            share: false,
            timestamps: true,
        }
    }
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            external: std::env::var("EDITOR").unwrap_or_else(|_| {
                if cfg!(windows) {
                    "notepad".to_string()
                } else {
                    "vi".to_string()
                }
            }),
            tab_width: 4,
            syntax_highlighting: true,
            line_numbers: true,
            auto_indent: true,
            bracket_matching: true,
        }
    }
}

impl Default for KeybindingsConfig {
    fn default() -> Self {
        Self {
            mode: "emacs".to_string(),
            custom: HashMap::new(),
        }
    }
}

// ============================================================================
// XDG Directory Functions
// ============================================================================

impl ShellConfig {
    /// Get the XDG config directory for AetherShell
    pub fn config_dir() -> PathBuf {
        if let Ok(path) = std::env::var("AETHER_CONFIG_HOME") {
            return PathBuf::from(path);
        }
        dirs::config_dir()
            .map(|p| p.join("aether"))
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".config").join("aether"))
                    .unwrap_or_else(|| PathBuf::from(".config/aether"))
            })
    }

    /// Get the XDG data directory for AetherShell
    pub fn data_dir() -> PathBuf {
        if let Ok(path) = std::env::var("AETHER_DATA_HOME") {
            return PathBuf::from(path);
        }
        dirs::data_dir()
            .map(|p| p.join("aether"))
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".local").join("share").join("aether"))
                    .unwrap_or_else(|| PathBuf::from(".local/share/aether"))
            })
    }

    /// Get the XDG cache directory for AetherShell
    pub fn cache_dir() -> PathBuf {
        if let Ok(path) = std::env::var("AETHER_CACHE_HOME") {
            return PathBuf::from(path);
        }
        dirs::cache_dir()
            .map(|p| p.join("aether"))
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|h| h.join(".cache").join("aether"))
                    .unwrap_or_else(|| PathBuf::from(".cache/aether"))
            })
    }

    /// Get the main config file path
    pub fn config_file() -> PathBuf {
        // Check AETHER_CONFIG env var first
        if let Ok(path) = std::env::var("AETHER_CONFIG") {
            return PathBuf::from(path);
        }

        let xdg_config = Self::config_dir().join("config.toml");
        if xdg_config.exists() {
            return xdg_config;
        }

        // Legacy fallback
        if let Some(home) = dirs::home_dir() {
            let legacy = home.join(".aetherrc");
            if legacy.exists() {
                return legacy;
            }
        }

        // Default to XDG location
        xdg_config
    }

    /// Get the init script path
    pub fn init_script() -> PathBuf {
        Self::config_dir().join("init.ae")
    }

    /// Get the history file path
    pub fn history_file() -> PathBuf {
        Self::data_dir().join("history")
    }

    /// Get the plugins directory
    pub fn plugins_dir() -> PathBuf {
        Self::data_dir().join("plugins")
    }

    /// Get the themes directory
    pub fn themes_dir() -> PathBuf {
        Self::config_dir().join("themes")
    }

    /// Load configuration from disk
    pub fn load() -> Result<Self> {
        let config_file = Self::config_file();

        if !config_file.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_file)
            .with_context(|| format!("Failed to read config file: {:?}", config_file))?;

        // Support both TOML and legacy formats
        if config_file
            .extension()
            .map(|e| e == "toml")
            .unwrap_or(false)
            || content.trim().starts_with('[')
            || content.contains(" = ")
        {
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config file: {:?}", config_file))
        } else {
            // Legacy .aetherrc format (simple key=value)
            Self::parse_legacy(&content)
        }
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<()> {
        let config_dir = Self::config_dir();
        fs::create_dir_all(&config_dir)?;

        let config_file = config_dir.join("config.toml");
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_file, content)?;
        Ok(())
    }

    /// Initialize all XDG directories
    pub fn init_dirs() -> Result<()> {
        fs::create_dir_all(Self::config_dir())?;
        fs::create_dir_all(Self::data_dir())?;
        fs::create_dir_all(Self::cache_dir())?;
        fs::create_dir_all(Self::plugins_dir())?;
        fs::create_dir_all(Self::themes_dir())?;
        Ok(())
    }

    /// Generate a default config file with comments
    pub fn generate_default_config() -> String {
        r##"# AetherShell Configuration
# Location: ~/.config/aether/config.toml
# Documentation: https://github.com/nervosys/AetherShell#configuration

[shell]
# Show welcome banner on startup
show_banner = true
# Show helpful tips
show_tips = true
# Default directory (empty = current directory)
default_directory = ""
# Enable vi editing mode (false = emacs mode)
vi_mode = false
# Treat directory names as cd commands
auto_cd = false
# Enable glob pattern expansion
glob_expansion = true
# Suggest corrections for typos
command_correction = true
# Bell style: "none", "visible", "audible"
bell_style = "none"

[colors]
# Enable colored output
enabled = true
# Theme: "catppuccin", "monokai", "dracula", "nord", "gruvbox", "solarized", "custom"
theme = "catppuccin"
# Force colors even when output is not a TTY
force = false
# Enable 24-bit true color
true_color = true

# Custom colors (only used when theme = "custom")
[colors.custom]
number = "#a6e3a1"
string = "#a6e3a1"
boolean = "#cba6f7"
keyword = "#cba6f7"
punctuation = "#89b4fa"
key = "#89dceb"
uri = "#f9e2af"
error = "#f38ba8"
warning = "#fab387"
success = "#a6e3a1"
dim = "#6c7086"
comment = "#6c7086"

[prompt]
# Prompt format - placeholders: {symbol}, {cwd}, {user}, {host}, {git_branch}, {time}, {status}
format = "{symbol}"
# Continuation prompt for multi-line input
continuation = "... "
# Right-side prompt (optional)
right = ""
# Show git branch in prompt
show_git = true
# Show execution time for commands
show_time = true
# Only show time if command took longer than this (ms)
time_threshold_ms = 1000
# Clear previous prompts when entering new command
transient = false

# Prompt style: "classic" (æ❯), "fish", "powerline" (oh-my-posh style),
# "pure" (minimal two-line), or "custom" (expands `format` above).
style = "classic"
# The prompt character
symbol = "❯"
# Abbreviate parent path components fish-style: ~/d/n/AetherShell
abbreviate_path = true
# Keep only the last N path components (0 = keep all)
max_path_segments = 0
# Show user@host in the fish style
show_user_host = false
# Mark a dirty worktree in the git segment (costs a `git status` per prompt)
show_git_dirty = false
# Dimmed ghost-text suggestions from history, fish-style
autosuggestions = true

# Segments for the "powerline" style, in order. Recognized names:
# os, user, host, user@host, cwd, git, status, duration, time, symbol.
# Anything else is rendered as literal text.
segments = ["os", "cwd", "git", "status", "duration"]
# Powerline separator glyph (needs a Nerd Font)
powerline_separator = ""
# Put the prompt symbol on its own second line
two_line = false

# Per-segment colors: "#fg" or "#fg:#bg"
[prompt.segment_colors]
# cwd = "#1e1e2e:#89b4fa"

# fish-style abbreviations, expanded when you press space
[prompt.abbreviations]
# gco = "git.checkout"
# ll = "ls(\".\") | sort()"

[ai]
# Default AI provider
default_provider = "openai"
# Default model
default_model = "gpt-4o-mini"
# Enable AI-powered suggestions
suggestions = false
# Maximum tokens for responses
max_tokens = 4096
# Temperature (0.0 = deterministic, 2.0 = creative)
temperature = 0.7
# Stream responses as they generate
streaming = true
# Tools agents are allowed to use (empty = all)
allowed_tools = []
# Tools agents are blocked from using
blocked_tools = []
# Maximum steps before agent timeout
max_agent_steps = 10

[history]
# Enable command history
enabled = true
# Maximum history entries
max_size = 10000
# Don't save duplicate consecutive commands
ignore_duplicates = true
# Ignore commands starting with a space
ignore_space = true
# Regex patterns to ignore
ignore_patterns = ["^exit$", "^quit$"]
# Share history between shell sessions
share = false
# Save timestamps with history
timestamps = true

[editor]
# External editor command
external = ""
# Tab display width
tab_width = 4
# Enable syntax highlighting
syntax_highlighting = true
# Show line numbers
line_numbers = true
# Auto-indent new lines
auto_indent = true
# Highlight matching brackets
bracket_matching = true

[keybindings]
# Mode: "emacs" or "vi"
mode = "emacs"

[keybindings.custom]
# Custom keybindings (key = action)
# Example: ctrl-r = "history_search"

[aliases]
# Command aliases
# Example: ll = "ls -la"

[env]
# Environment variables to set on startup
# Example: EDITOR = "nvim"
"##
        .to_string()
    }

    /// Parse legacy .aetherrc format (simple key=value)
    fn parse_legacy(content: &str) -> Result<Self> {
        let mut config = Self::default();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim();
                let value = value.trim().trim_matches('"').trim_matches('\'');

                match key {
                    "color" | "colors" => config.colors.enabled = value.parse().unwrap_or(true),
                    "theme" => config.colors.theme = value.to_string(),
                    "vi_mode" | "vi-mode" => config.shell.vi_mode = value.parse().unwrap_or(false),
                    "auto_cd" | "auto-cd" => config.shell.auto_cd = value.parse().unwrap_or(false),
                    "banner" | "show_banner" => {
                        config.shell.show_banner = value.parse().unwrap_or(true)
                    }
                    "history_size" | "history-size" => {
                        config.history.max_size = value.parse().unwrap_or(10000)
                    }
                    "editor" | "EDITOR" => config.editor.external = value.to_string(),
                    "ai_model" | "model" => config.ai.default_model = value.to_string(),
                    "ai_provider" | "provider" => config.ai.default_provider = value.to_string(),
                    _ => {} // Ignore unknown keys
                }
            }
        }

        Ok(config)
    }
}

// ============================================================================
// Color Theme System
// ============================================================================

/// Built-in color themes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    // Popular dark themes
    Catppuccin,
    CatppuccinLatte,
    Monokai,
    Dracula,
    Nord,
    Gruvbox,
    GruvboxLight,
    Solarized,
    SolarizedLight,
    TokyoNight,
    TokyoNightStorm,
    TokyoNightLight,
    OneDark,
    OneLight,
    Material,
    MaterialOcean,
    Palenight,
    Ayu,
    AyuLight,
    AyuMirage,
    Synthwave84,
    Cyberpunk,
    Everforest,
    EverforestLight,
    Kanagawa,
    RosePine,
    RosePineMoon,
    RosePineDawn,
    Nightfox,
    Dawnfox,
    Github,
    GithubLight,
    Cobalt2,
    Horizon,
    Spacegray,
    Atom,
    Sublime,
    VsCode,
    Custom,
}

impl Theme {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().replace(['-', '_'], "").as_str() {
            "catppuccin" | "catppuccinmocha" => Theme::Catppuccin,
            "catppuccinlatte" => Theme::CatppuccinLatte,
            "monokai" | "monokaiclassic" => Theme::Monokai,
            "dracula" => Theme::Dracula,
            "nord" => Theme::Nord,
            "gruvbox" | "gruvboxdark" => Theme::Gruvbox,
            "gruvboxlight" => Theme::GruvboxLight,
            "solarized" | "solarizeddark" => Theme::Solarized,
            "solarizedlight" => Theme::SolarizedLight,
            "tokyonight" | "tokyonightnight" => Theme::TokyoNight,
            "tokyonightstorm" => Theme::TokyoNightStorm,
            "tokyonightlight" | "tokyonightday" => Theme::TokyoNightLight,
            "onedark" | "atomonedark" => Theme::OneDark,
            "onelight" | "atomonelight" => Theme::OneLight,
            "material" | "materialdark" => Theme::Material,
            "materialocean" => Theme::MaterialOcean,
            "palenight" | "materialpalenight" => Theme::Palenight,
            "ayu" | "ayudark" => Theme::Ayu,
            "ayulight" => Theme::AyuLight,
            "ayumirage" => Theme::AyuMirage,
            "synthwave" | "synthwave84" => Theme::Synthwave84,
            "cyberpunk" | "neon" => Theme::Cyberpunk,
            "everforest" | "everforestdark" => Theme::Everforest,
            "everforestlight" => Theme::EverforestLight,
            "kanagawa" | "kanagawawave" => Theme::Kanagawa,
            "rosepine" | "rosepinedark" => Theme::RosePine,
            "rosepinemoon" => Theme::RosePineMoon,
            "rosepinedawn" => Theme::RosePineDawn,
            "nightfox" => Theme::Nightfox,
            "dawnfox" => Theme::Dawnfox,
            "github" | "githubdark" => Theme::Github,
            "githublight" => Theme::GithubLight,
            "cobalt" | "cobalt2" => Theme::Cobalt2,
            "horizon" => Theme::Horizon,
            "spacegray" => Theme::Spacegray,
            "atom" => Theme::Atom,
            "sublime" | "sublimetext" => Theme::Sublime,
            "vscode" | "vscodedark" | "darkplus" => Theme::VsCode,
            "custom" => Theme::Custom,
            _ => Theme::Catppuccin, // Default
        }
    }

    /// List all available theme names
    pub fn list() -> Vec<&'static str> {
        vec![
            "catppuccin",
            "catppuccin-latte",
            "monokai",
            "dracula",
            "nord",
            "gruvbox",
            "gruvbox-light",
            "solarized",
            "solarized-light",
            "tokyo-night",
            "tokyo-night-storm",
            "tokyo-night-light",
            "one-dark",
            "one-light",
            "material",
            "material-ocean",
            "palenight",
            "ayu",
            "ayu-light",
            "ayu-mirage",
            "synthwave84",
            "cyberpunk",
            "everforest",
            "everforest-light",
            "kanagawa",
            "rose-pine",
            "rose-pine-moon",
            "rose-pine-dawn",
            "nightfox",
            "dawnfox",
            "github",
            "github-light",
            "cobalt2",
            "horizon",
            "spacegray",
            "atom",
            "sublime",
            "vscode",
        ]
    }

    /// Get colors for this theme
    pub fn colors(&self) -> CustomColors {
        match self {
            // === Catppuccin (Mocha - default) ===
            Theme::Catppuccin => CustomColors::default(),

            // === Catppuccin Latte (Light) ===
            Theme::CatppuccinLatte => CustomColors {
                number: "#40a02b".to_string(),      // Green
                string: "#40a02b".to_string(),      // Green
                boolean: "#8839ef".to_string(),     // Mauve
                keyword: "#8839ef".to_string(),     // Mauve
                punctuation: "#1e66f5".to_string(), // Blue
                key: "#04a5e5".to_string(),         // Sky
                uri: "#df8e1d".to_string(),         // Yellow
                error: "#d20f39".to_string(),       // Red
                warning: "#fe640b".to_string(),     // Peach
                success: "#40a02b".to_string(),     // Green
                dim: "#9ca0b0".to_string(),         // Overlay0
                comment: "#9ca0b0".to_string(),
            },

            // === Monokai Classic ===
            Theme::Monokai => CustomColors {
                number: "#ae81ff".to_string(),
                string: "#e6db74".to_string(),
                boolean: "#ae81ff".to_string(),
                keyword: "#f92672".to_string(),
                punctuation: "#f8f8f2".to_string(),
                key: "#66d9ef".to_string(),
                uri: "#e6db74".to_string(),
                error: "#f92672".to_string(),
                warning: "#fd971f".to_string(),
                success: "#a6e22e".to_string(),
                dim: "#75715e".to_string(),
                comment: "#75715e".to_string(),
            },

            // === Dracula ===
            Theme::Dracula => CustomColors {
                number: "#bd93f9".to_string(),
                string: "#f1fa8c".to_string(),
                boolean: "#bd93f9".to_string(),
                keyword: "#ff79c6".to_string(),
                punctuation: "#f8f8f2".to_string(),
                key: "#8be9fd".to_string(),
                uri: "#f1fa8c".to_string(),
                error: "#ff5555".to_string(),
                warning: "#ffb86c".to_string(),
                success: "#50fa7b".to_string(),
                dim: "#6272a4".to_string(),
                comment: "#6272a4".to_string(),
            },

            // === Nord ===
            Theme::Nord => CustomColors {
                number: "#b48ead".to_string(),
                string: "#a3be8c".to_string(),
                boolean: "#b48ead".to_string(),
                keyword: "#81a1c1".to_string(),
                punctuation: "#d8dee9".to_string(),
                key: "#88c0d0".to_string(),
                uri: "#ebcb8b".to_string(),
                error: "#bf616a".to_string(),
                warning: "#d08770".to_string(),
                success: "#a3be8c".to_string(),
                dim: "#4c566a".to_string(),
                comment: "#4c566a".to_string(),
            },

            // === Gruvbox Dark ===
            Theme::Gruvbox => CustomColors {
                number: "#d3869b".to_string(),
                string: "#b8bb26".to_string(),
                boolean: "#d3869b".to_string(),
                keyword: "#fb4934".to_string(),
                punctuation: "#ebdbb2".to_string(),
                key: "#83a598".to_string(),
                uri: "#fabd2f".to_string(),
                error: "#fb4934".to_string(),
                warning: "#fe8019".to_string(),
                success: "#b8bb26".to_string(),
                dim: "#928374".to_string(),
                comment: "#928374".to_string(),
            },

            // === Gruvbox Light ===
            Theme::GruvboxLight => CustomColors {
                number: "#8f3f71".to_string(),
                string: "#79740e".to_string(),
                boolean: "#8f3f71".to_string(),
                keyword: "#9d0006".to_string(),
                punctuation: "#3c3836".to_string(),
                key: "#076678".to_string(),
                uri: "#b57614".to_string(),
                error: "#9d0006".to_string(),
                warning: "#af3a03".to_string(),
                success: "#79740e".to_string(),
                dim: "#928374".to_string(),
                comment: "#928374".to_string(),
            },

            // === Solarized Dark ===
            Theme::Solarized => CustomColors {
                number: "#d33682".to_string(),
                string: "#2aa198".to_string(),
                boolean: "#d33682".to_string(),
                keyword: "#859900".to_string(),
                punctuation: "#839496".to_string(),
                key: "#268bd2".to_string(),
                uri: "#b58900".to_string(),
                error: "#dc322f".to_string(),
                warning: "#cb4b16".to_string(),
                success: "#859900".to_string(),
                dim: "#586e75".to_string(),
                comment: "#586e75".to_string(),
            },

            // === Solarized Light ===
            Theme::SolarizedLight => CustomColors {
                number: "#d33682".to_string(),
                string: "#2aa198".to_string(),
                boolean: "#d33682".to_string(),
                keyword: "#859900".to_string(),
                punctuation: "#657b83".to_string(),
                key: "#268bd2".to_string(),
                uri: "#b58900".to_string(),
                error: "#dc322f".to_string(),
                warning: "#cb4b16".to_string(),
                success: "#859900".to_string(),
                dim: "#93a1a1".to_string(),
                comment: "#93a1a1".to_string(),
            },

            // === Tokyo Night ===
            Theme::TokyoNight => CustomColors {
                number: "#ff9e64".to_string(),
                string: "#9ece6a".to_string(),
                boolean: "#ff9e64".to_string(),
                keyword: "#bb9af7".to_string(),
                punctuation: "#c0caf5".to_string(),
                key: "#7dcfff".to_string(),
                uri: "#e0af68".to_string(),
                error: "#f7768e".to_string(),
                warning: "#e0af68".to_string(),
                success: "#9ece6a".to_string(),
                dim: "#565f89".to_string(),
                comment: "#565f89".to_string(),
            },

            // === Tokyo Night Storm ===
            Theme::TokyoNightStorm => CustomColors {
                number: "#ff9e64".to_string(),
                string: "#9ece6a".to_string(),
                boolean: "#ff9e64".to_string(),
                keyword: "#bb9af7".to_string(),
                punctuation: "#a9b1d6".to_string(),
                key: "#7dcfff".to_string(),
                uri: "#e0af68".to_string(),
                error: "#f7768e".to_string(),
                warning: "#e0af68".to_string(),
                success: "#9ece6a".to_string(),
                dim: "#565f89".to_string(),
                comment: "#565f89".to_string(),
            },

            // === Tokyo Night Light ===
            Theme::TokyoNightLight => CustomColors {
                number: "#965027".to_string(),
                string: "#485e30".to_string(),
                boolean: "#965027".to_string(),
                keyword: "#7847bd".to_string(),
                punctuation: "#343b58".to_string(),
                key: "#0f4b6e".to_string(),
                uri: "#8c6c3e".to_string(),
                error: "#8c4351".to_string(),
                warning: "#8c6c3e".to_string(),
                success: "#485e30".to_string(),
                dim: "#9699a3".to_string(),
                comment: "#9699a3".to_string(),
            },

            // === One Dark (Atom) ===
            Theme::OneDark => CustomColors {
                number: "#d19a66".to_string(),
                string: "#98c379".to_string(),
                boolean: "#d19a66".to_string(),
                keyword: "#c678dd".to_string(),
                punctuation: "#abb2bf".to_string(),
                key: "#56b6c2".to_string(),
                uri: "#e5c07b".to_string(),
                error: "#e06c75".to_string(),
                warning: "#e5c07b".to_string(),
                success: "#98c379".to_string(),
                dim: "#5c6370".to_string(),
                comment: "#5c6370".to_string(),
            },

            // === One Light ===
            Theme::OneLight => CustomColors {
                number: "#986801".to_string(),
                string: "#50a14f".to_string(),
                boolean: "#986801".to_string(),
                keyword: "#a626a4".to_string(),
                punctuation: "#383a42".to_string(),
                key: "#0184bc".to_string(),
                uri: "#c18401".to_string(),
                error: "#e45649".to_string(),
                warning: "#c18401".to_string(),
                success: "#50a14f".to_string(),
                dim: "#a0a1a7".to_string(),
                comment: "#a0a1a7".to_string(),
            },

            // === Material Dark ===
            Theme::Material => CustomColors {
                number: "#f78c6c".to_string(),
                string: "#c3e88d".to_string(),
                boolean: "#f78c6c".to_string(),
                keyword: "#c792ea".to_string(),
                punctuation: "#eeffff".to_string(),
                key: "#89ddff".to_string(),
                uri: "#ffcb6b".to_string(),
                error: "#ff5370".to_string(),
                warning: "#ffcb6b".to_string(),
                success: "#c3e88d".to_string(),
                dim: "#546e7a".to_string(),
                comment: "#546e7a".to_string(),
            },

            // === Material Ocean ===
            Theme::MaterialOcean => CustomColors {
                number: "#f78c6c".to_string(),
                string: "#c3e88d".to_string(),
                boolean: "#f78c6c".to_string(),
                keyword: "#c792ea".to_string(),
                punctuation: "#a6accd".to_string(),
                key: "#89ddff".to_string(),
                uri: "#ffcb6b".to_string(),
                error: "#ff5370".to_string(),
                warning: "#ffcb6b".to_string(),
                success: "#c3e88d".to_string(),
                dim: "#464b5d".to_string(),
                comment: "#464b5d".to_string(),
            },

            // === Material Palenight ===
            Theme::Palenight => CustomColors {
                number: "#f78c6c".to_string(),
                string: "#c3e88d".to_string(),
                boolean: "#f78c6c".to_string(),
                keyword: "#c792ea".to_string(),
                punctuation: "#a6accd".to_string(),
                key: "#82aaff".to_string(),
                uri: "#ffcb6b".to_string(),
                error: "#ff5370".to_string(),
                warning: "#ffcb6b".to_string(),
                success: "#c3e88d".to_string(),
                dim: "#676e95".to_string(),
                comment: "#676e95".to_string(),
            },

            // === Ayu Dark ===
            Theme::Ayu => CustomColors {
                number: "#e6b450".to_string(),
                string: "#aad94c".to_string(),
                boolean: "#e6b450".to_string(),
                keyword: "#ff8f40".to_string(),
                punctuation: "#bfbdb6".to_string(),
                key: "#59c2ff".to_string(),
                uri: "#ffb454".to_string(),
                error: "#d95757".to_string(),
                warning: "#ffb454".to_string(),
                success: "#aad94c".to_string(),
                dim: "#636a72".to_string(),
                comment: "#636a72".to_string(),
            },

            // === Ayu Light ===
            Theme::AyuLight => CustomColors {
                number: "#ff9940".to_string(),
                string: "#86b300".to_string(),
                boolean: "#ff9940".to_string(),
                keyword: "#fa8d3e".to_string(),
                punctuation: "#5c6166".to_string(),
                key: "#399ee6".to_string(),
                uri: "#f2ae49".to_string(),
                error: "#e65050".to_string(),
                warning: "#f2ae49".to_string(),
                success: "#86b300".to_string(),
                dim: "#8a9199".to_string(),
                comment: "#8a9199".to_string(),
            },

            // === Ayu Mirage ===
            Theme::AyuMirage => CustomColors {
                number: "#ffcc66".to_string(),
                string: "#d5ff80".to_string(),
                boolean: "#ffcc66".to_string(),
                keyword: "#ffae57".to_string(),
                punctuation: "#cbccc6".to_string(),
                key: "#73d0ff".to_string(),
                uri: "#ffd580".to_string(),
                error: "#ff6666".to_string(),
                warning: "#ffd580".to_string(),
                success: "#d5ff80".to_string(),
                dim: "#5c6773".to_string(),
                comment: "#5c6773".to_string(),
            },

            // === Synthwave '84 ===
            Theme::Synthwave84 => CustomColors {
                number: "#f97e72".to_string(),
                string: "#ff8b39".to_string(),
                boolean: "#f97e72".to_string(),
                keyword: "#fede5d".to_string(),
                punctuation: "#ffffff".to_string(),
                key: "#36f9f6".to_string(),
                uri: "#ff7edb".to_string(),
                error: "#fe4450".to_string(),
                warning: "#fede5d".to_string(),
                success: "#72f1b8".to_string(),
                dim: "#848bbd".to_string(),
                comment: "#848bbd".to_string(),
            },

            // === Cyberpunk / Neon ===
            Theme::Cyberpunk => CustomColors {
                number: "#ff00ff".to_string(), // Magenta
                string: "#00ffff".to_string(), // Cyan
                boolean: "#ff00ff".to_string(),
                keyword: "#ffff00".to_string(), // Yellow
                punctuation: "#ffffff".to_string(),
                key: "#00ff00".to_string(),     // Green
                uri: "#ff69b4".to_string(),     // Hot pink
                error: "#ff0000".to_string(),   // Red
                warning: "#ffa500".to_string(), // Orange
                success: "#00ff00".to_string(),
                dim: "#808080".to_string(),
                comment: "#808080".to_string(),
            },

            // === Everforest Dark ===
            Theme::Everforest => CustomColors {
                number: "#d699b6".to_string(),
                string: "#a7c080".to_string(),
                boolean: "#d699b6".to_string(),
                keyword: "#e67e80".to_string(),
                punctuation: "#d3c6aa".to_string(),
                key: "#7fbbb3".to_string(),
                uri: "#dbbc7f".to_string(),
                error: "#e67e80".to_string(),
                warning: "#e69875".to_string(),
                success: "#a7c080".to_string(),
                dim: "#859289".to_string(),
                comment: "#859289".to_string(),
            },

            // === Everforest Light ===
            Theme::EverforestLight => CustomColors {
                number: "#df69ba".to_string(),
                string: "#8da101".to_string(),
                boolean: "#df69ba".to_string(),
                keyword: "#f85552".to_string(),
                punctuation: "#5c6a72".to_string(),
                key: "#35a77c".to_string(),
                uri: "#dfa000".to_string(),
                error: "#f85552".to_string(),
                warning: "#f57d26".to_string(),
                success: "#8da101".to_string(),
                dim: "#939f91".to_string(),
                comment: "#939f91".to_string(),
            },

            // === Kanagawa ===
            Theme::Kanagawa => CustomColors {
                number: "#d27e99".to_string(),
                string: "#98bb6c".to_string(),
                boolean: "#d27e99".to_string(),
                keyword: "#957fb8".to_string(),
                punctuation: "#dcd7ba".to_string(),
                key: "#7e9cd8".to_string(),
                uri: "#e6c384".to_string(),
                error: "#c34043".to_string(),
                warning: "#ff9e3b".to_string(),
                success: "#98bb6c".to_string(),
                dim: "#727169".to_string(),
                comment: "#727169".to_string(),
            },

            // === Rosé Pine ===
            Theme::RosePine => CustomColors {
                number: "#ebbcba".to_string(),
                string: "#f6c177".to_string(),
                boolean: "#ebbcba".to_string(),
                keyword: "#c4a7e7".to_string(),
                punctuation: "#e0def4".to_string(),
                key: "#9ccfd8".to_string(),
                uri: "#f6c177".to_string(),
                error: "#eb6f92".to_string(),
                warning: "#f6c177".to_string(),
                success: "#31748f".to_string(),
                dim: "#6e6a86".to_string(),
                comment: "#6e6a86".to_string(),
            },

            // === Rosé Pine Moon ===
            Theme::RosePineMoon => CustomColors {
                number: "#ea9a97".to_string(),
                string: "#f6c177".to_string(),
                boolean: "#ea9a97".to_string(),
                keyword: "#c4a7e7".to_string(),
                punctuation: "#e0def4".to_string(),
                key: "#9ccfd8".to_string(),
                uri: "#f6c177".to_string(),
                error: "#eb6f92".to_string(),
                warning: "#f6c177".to_string(),
                success: "#3e8fb0".to_string(),
                dim: "#6e6a86".to_string(),
                comment: "#6e6a86".to_string(),
            },

            // === Rosé Pine Dawn (Light) ===
            Theme::RosePineDawn => CustomColors {
                number: "#d7827e".to_string(),
                string: "#ea9d34".to_string(),
                boolean: "#d7827e".to_string(),
                keyword: "#907aa9".to_string(),
                punctuation: "#575279".to_string(),
                key: "#56949f".to_string(),
                uri: "#ea9d34".to_string(),
                error: "#b4637a".to_string(),
                warning: "#ea9d34".to_string(),
                success: "#286983".to_string(),
                dim: "#9893a5".to_string(),
                comment: "#9893a5".to_string(),
            },

            // === Nightfox ===
            Theme::Nightfox => CustomColors {
                number: "#f4a261".to_string(),
                string: "#81b29a".to_string(),
                boolean: "#f4a261".to_string(),
                keyword: "#9d79d6".to_string(),
                punctuation: "#cdcecf".to_string(),
                key: "#63cdcf".to_string(),
                uri: "#dbc074".to_string(),
                error: "#c94f6d".to_string(),
                warning: "#dbc074".to_string(),
                success: "#81b29a".to_string(),
                dim: "#738091".to_string(),
                comment: "#738091".to_string(),
            },

            // === Dawnfox (Light) ===
            Theme::Dawnfox => CustomColors {
                number: "#b95d76".to_string(),
                string: "#618774".to_string(),
                boolean: "#b95d76".to_string(),
                keyword: "#806e9c".to_string(),
                punctuation: "#575279".to_string(),
                key: "#597b8c".to_string(),
                uri: "#b79a3e".to_string(),
                error: "#9d4059".to_string(),
                warning: "#b79a3e".to_string(),
                success: "#618774".to_string(),
                dim: "#898b93".to_string(),
                comment: "#898b93".to_string(),
            },

            // === GitHub Dark ===
            Theme::Github => CustomColors {
                number: "#79c0ff".to_string(),
                string: "#a5d6ff".to_string(),
                boolean: "#79c0ff".to_string(),
                keyword: "#ff7b72".to_string(),
                punctuation: "#c9d1d9".to_string(),
                key: "#7ee787".to_string(),
                uri: "#a5d6ff".to_string(),
                error: "#ff7b72".to_string(),
                warning: "#d29922".to_string(),
                success: "#7ee787".to_string(),
                dim: "#8b949e".to_string(),
                comment: "#8b949e".to_string(),
            },

            // === GitHub Light ===
            Theme::GithubLight => CustomColors {
                number: "#0550ae".to_string(),
                string: "#0a3069".to_string(),
                boolean: "#0550ae".to_string(),
                keyword: "#cf222e".to_string(),
                punctuation: "#24292f".to_string(),
                key: "#116329".to_string(),
                uri: "#0a3069".to_string(),
                error: "#cf222e".to_string(),
                warning: "#9a6700".to_string(),
                success: "#116329".to_string(),
                dim: "#6e7781".to_string(),
                comment: "#6e7781".to_string(),
            },

            // === Cobalt2 ===
            Theme::Cobalt2 => CustomColors {
                number: "#ff628c".to_string(),
                string: "#a5ff90".to_string(),
                boolean: "#ff628c".to_string(),
                keyword: "#ff9d00".to_string(),
                punctuation: "#ffffff".to_string(),
                key: "#9effff".to_string(),
                uri: "#ffc600".to_string(),
                error: "#ff628c".to_string(),
                warning: "#ffc600".to_string(),
                success: "#a5ff90".to_string(),
                dim: "#0088ff".to_string(),
                comment: "#0088ff".to_string(),
            },

            // === Horizon ===
            Theme::Horizon => CustomColors {
                number: "#f09383".to_string(),
                string: "#fab795".to_string(),
                boolean: "#f09383".to_string(),
                keyword: "#ee64ae".to_string(),
                punctuation: "#e0e0e0".to_string(),
                key: "#25b0bc".to_string(),
                uri: "#fac29a".to_string(),
                error: "#e95678".to_string(),
                warning: "#fab795".to_string(),
                success: "#29d398".to_string(),
                dim: "#6c6f93".to_string(),
                comment: "#6c6f93".to_string(),
            },

            // === Spacegray ===
            Theme::Spacegray => CustomColors {
                number: "#a78cfa".to_string(),
                string: "#99ffc4".to_string(),
                boolean: "#a78cfa".to_string(),
                keyword: "#ff6e6e".to_string(),
                punctuation: "#ffffff".to_string(),
                key: "#6eb4ff".to_string(),
                uri: "#ffffa5".to_string(),
                error: "#ff6e6e".to_string(),
                warning: "#ffffa5".to_string(),
                success: "#99ffc4".to_string(),
                dim: "#767b8c".to_string(),
                comment: "#767b8c".to_string(),
            },

            // === Atom ===
            Theme::Atom => CustomColors {
                number: "#d19a66".to_string(),
                string: "#98c379".to_string(),
                boolean: "#d19a66".to_string(),
                keyword: "#c678dd".to_string(),
                punctuation: "#abb2bf".to_string(),
                key: "#61afef".to_string(),
                uri: "#e5c07b".to_string(),
                error: "#e06c75".to_string(),
                warning: "#e5c07b".to_string(),
                success: "#98c379".to_string(),
                dim: "#5c6370".to_string(),
                comment: "#5c6370".to_string(),
            },

            // === Sublime Text (Mariana) ===
            Theme::Sublime => CustomColors {
                number: "#f9ae58".to_string(),
                string: "#99c794".to_string(),
                boolean: "#f9ae58".to_string(),
                keyword: "#c695c6".to_string(),
                punctuation: "#d8dee9".to_string(),
                key: "#6699cc".to_string(),
                uri: "#fac761".to_string(),
                error: "#ec5f66".to_string(),
                warning: "#fac761".to_string(),
                success: "#99c794".to_string(),
                dim: "#a6acb9".to_string(),
                comment: "#a6acb9".to_string(),
            },

            // === VS Code Dark+ ===
            Theme::VsCode => CustomColors {
                number: "#b5cea8".to_string(),
                string: "#ce9178".to_string(),
                boolean: "#569cd6".to_string(),
                keyword: "#c586c0".to_string(),
                punctuation: "#d4d4d4".to_string(),
                key: "#9cdcfe".to_string(),
                uri: "#ce9178".to_string(),
                error: "#f14c4c".to_string(),
                warning: "#cca700".to_string(),
                success: "#89d185".to_string(),
                dim: "#6a9955".to_string(),
                comment: "#6a9955".to_string(),
            },

            Theme::Custom => CustomColors::default(),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ShellConfig::default();
        assert!(config.colors.enabled);
        assert_eq!(config.colors.theme, "catppuccin");
        assert!(config.history.enabled);
        assert_eq!(config.history.max_size, 10000);
    }

    #[test]
    fn test_theme_parsing() {
        // Test basic themes
        assert_eq!(Theme::from_str("catppuccin"), Theme::Catppuccin);
        assert_eq!(Theme::from_str("DRACULA"), Theme::Dracula);
        assert_eq!(Theme::from_str("unknown"), Theme::Catppuccin);

        // Test new themes with various formats
        assert_eq!(Theme::from_str("tokyo-night"), Theme::TokyoNight);
        assert_eq!(Theme::from_str("tokyo_night"), Theme::TokyoNight);
        assert_eq!(Theme::from_str("tokyonight"), Theme::TokyoNight);
        assert_eq!(Theme::from_str("one-dark"), Theme::OneDark);
        assert_eq!(Theme::from_str("onedark"), Theme::OneDark);
        assert_eq!(Theme::from_str("rose-pine"), Theme::RosePine);
        assert_eq!(Theme::from_str("rosepine"), Theme::RosePine);
        assert_eq!(Theme::from_str("kanagawa"), Theme::Kanagawa);
        assert_eq!(Theme::from_str("material-ocean"), Theme::MaterialOcean);
        assert_eq!(Theme::from_str("synthwave84"), Theme::Synthwave84);
        assert_eq!(Theme::from_str("everforest"), Theme::Everforest);
        assert_eq!(Theme::from_str("gruvbox-light"), Theme::GruvboxLight);
        assert_eq!(Theme::from_str("catppuccin-latte"), Theme::CatppuccinLatte);
        assert_eq!(Theme::from_str("github"), Theme::Github);
        assert_eq!(Theme::from_str("vscode"), Theme::VsCode);
        assert_eq!(Theme::from_str("cobalt2"), Theme::Cobalt2);
        assert_eq!(Theme::from_str("nightfox"), Theme::Nightfox);

        // Test theme list
        let themes = Theme::list();
        assert!(themes.len() >= 38);
        assert!(themes.contains(&"catppuccin"));
        assert!(themes.contains(&"tokyo-night"));
        assert!(themes.contains(&"rose-pine"));
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = ShellConfig::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: ShellConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.colors.theme, parsed.colors.theme);
    }

    #[test]
    fn test_legacy_parsing() {
        let legacy = r#"
            color = true
            theme = monokai
            vi_mode = true
            history_size = 5000
        "#;
        let config = ShellConfig::parse_legacy(legacy).unwrap();
        assert!(config.colors.enabled);
        assert_eq!(config.colors.theme, "monokai");
        assert!(config.shell.vi_mode);
        assert_eq!(config.history.max_size, 5000);
    }

    #[test]
    fn test_generate_default_config() {
        let default_config = ShellConfig::generate_default_config();
        assert!(default_config.contains("[shell]"));
        assert!(default_config.contains("[colors]"));
        assert!(default_config.contains("[prompt]"));
    }
}
