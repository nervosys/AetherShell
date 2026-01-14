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

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            shell: ShellSettings::default(),
            colors: ColorConfig::default(),
            prompt: PromptConfig::default(),
            ai: AiConfig::default(),
            history: HistoryConfig::default(),
            editor: EditorConfig::default(),
            keybindings: KeybindingsConfig::default(),
            aliases: HashMap::new(),
            env: HashMap::new(),
        }
    }
}

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
    Catppuccin,
    Monokai,
    Dracula,
    Nord,
    Gruvbox,
    Solarized,
    Custom,
}

impl Theme {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "catppuccin" => Theme::Catppuccin,
            "monokai" => Theme::Monokai,
            "dracula" => Theme::Dracula,
            "nord" => Theme::Nord,
            "gruvbox" => Theme::Gruvbox,
            "solarized" | "solarized-dark" => Theme::Solarized,
            "custom" => Theme::Custom,
            _ => Theme::Catppuccin, // Default
        }
    }

    /// Get colors for this theme
    pub fn colors(&self) -> CustomColors {
        match self {
            Theme::Catppuccin => CustomColors::default(), // Already Catppuccin
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
        assert_eq!(Theme::from_str("catppuccin"), Theme::Catppuccin);
        assert_eq!(Theme::from_str("DRACULA"), Theme::Dracula);
        assert_eq!(Theme::from_str("unknown"), Theme::Catppuccin);
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
