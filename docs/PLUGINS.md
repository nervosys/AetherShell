# AetherShell Plugin System

> **Version:** 0.1.0  
> **Last Updated:** January 21, 2026

The AetherShell plugin system enables extensibility through a modular architecture. Plugins can add AI backends, custom builtins, file handlers, transport protocols, and more.

---

## Table of Contents

- [Quick Start](#quick-start)
- [Plugin Categories](#plugin-categories)
- [Plugin Manifest Format](#plugin-manifest-format)
- [Builtin Functions](#builtin-functions)
- [Plugin Development Guide](#plugin-development-guide)
- [Plugin API Reference](#plugin-api-reference)
- [Built-in Plugins](#built-in-plugins)
- [Example Plugins](#example-plugins)

---

## Quick Start

### List Available Plugins

```aether
# List all registered plugins
plugins

# Get detailed info about a specific plugin
plugin_info("builtin.json")

# List available categories
plugin_categories
```

### Load an External Plugin

```aether
# Load a plugin from its manifest file
plugin_load("~/.config/aethershell/plugins/my-plugin/plugin.toml")

# Enable/disable a plugin
plugin_enable("my-plugin")
plugin_disable("my-plugin")

# Unload a plugin (built-in plugins cannot be unloaded)
plugin_unload("my-plugin")
```

---

## Plugin Categories

AetherShell supports six plugin categories:

| Category       | Description             | Example Use Case                       |
| -------------- | ----------------------- | -------------------------------------- |
| `AIBackend`    | Custom AI/LLM providers | Integrate with Anthropic, local models |
| `Builtin`      | Custom shell functions  | Add domain-specific operations         |
| `FileHandler`  | File format support     | Parse YAML, Excel, Parquet files       |
| `Transport`    | Network protocols       | MQTT, gRPC, WebSocket connections      |
| `Syntax`       | Language extensions     | Custom operators, DSLs                 |
| `TUIComponent` | TUI interface widgets   | Custom dashboard panels                |

---

## Plugin Manifest Format

Plugins are defined using a TOML manifest file (`plugin.toml`):

```toml
[plugin]
id = "my-awesome-plugin"
name = "My Awesome Plugin"
version = "1.0.0"
author = "Your Name <your.email@example.com>"
description = "A fantastic plugin that does amazing things"
categories = ["Builtin", "FileHandler"]
min_aether_version = "0.1.0"
dependencies = []

# Optional: Define plugin-specific configuration
[config]
api_key = ""
timeout = 30
debug = false

# Optional: Define custom builtins (script-based)
[builtins]
# Simple function
double = "fn(x) => x * 2"

# More complex function
greet = """
fn(name) => {
    "Hello, " + name + "! Welcome to AetherShell."
}
"""
```

### Manifest Fields

| Field                | Required | Description                                    |
| -------------------- | -------- | ---------------------------------------------- |
| `id`                 | Yes      | Unique identifier (lowercase, hyphens allowed) |
| `name`               | No       | Human-readable name (defaults to id)           |
| `version`            | No       | Semantic version (defaults to "1.0.0")         |
| `author`             | No       | Author name and email                          |
| `description`        | No       | Brief description of the plugin                |
| `categories`         | No       | Array of plugin categories                     |
| `min_aether_version` | No       | Minimum AetherShell version required           |
| `dependencies`       | No       | Array of required plugin IDs                   |

---

## Builtin Functions

### `plugins` / `plugin_list`

List all registered plugins.

```aether
plugins
# Returns: Array of plugin records
# [
#   { id: "builtin.json", name: "JSON File Handler", version: "1.0.0", ... },
#   { id: "builtin.csv", name: "CSV File Handler", version: "1.0.0", ... },
#   ...
# ]
```

### `plugin_info(id)`

Get detailed information about a specific plugin.

```aether
plugin_info("builtin.json")
# Returns: Record with full plugin details
# {
#   id: "builtin.json",
#   name: "JSON File Handler",
#   version: "1.0.0",
#   author: "AetherShell Team",
#   description: "Native JSON file reading and writing",
#   categories: ["FileHandler"],
#   min_aether_version: "0.1.0",
#   dependencies: [],
#   enabled: true
# }
```

### `plugin_enable(id)`

Enable a disabled plugin.

```aether
plugin_enable("my-plugin")
# Returns: true on success
```

### `plugin_disable(id)`

Disable an enabled plugin.

```aether
plugin_disable("my-plugin")
# Returns: true on success
```

### `plugin_load(path)`

Load a plugin from its manifest file.

```aether
plugin_load("./plugins/my-plugin/plugin.toml")
# Returns: Record with loaded plugin info
# { id: "my-plugin", name: "My Plugin", version: "1.0.0", status: "loaded" }
```

### `plugin_unload(id)`

Unload a dynamically loaded plugin. Built-in plugins cannot be unloaded.

```aether
plugin_unload("my-plugin")
# Returns: Record with unload status
# { id: "my-plugin", status: "unloaded" }
```

### `plugin_categories`

List all available plugin categories.

```aether
plugin_categories
# Returns: ["AIBackend", "Builtin", "FileHandler", "Transport", "Syntax", "TUIComponent"]
```

---

## Plugin Development Guide

### Creating a Simple Builtin Plugin

1. **Create the plugin directory:**

```bash
mkdir -p ~/.config/aethershell/plugins/hello-plugin
```

2. **Create the manifest (`plugin.toml`):**

```toml
[plugin]
id = "hello-plugin"
name = "Hello World Plugin"
version = "1.0.0"
author = "Your Name"
description = "A simple hello world plugin"
categories = ["Builtin"]

[builtins]
hello = "fn(name) => 'Hello, ' + name + '!'"
hello_upper = "fn(name) => upper('Hello, ' + name + '!')"
```

3. **Load and use the plugin:**

```aether
plugin_load("~/.config/aethershell/plugins/hello-plugin/plugin.toml")
hello("World")  # => "Hello, World!"
hello_upper("World")  # => "HELLO, WORLD!"
```

### Creating a File Handler Plugin

For native Rust plugins that implement the `FileHandlerPlugin` trait:

```rust
use aether_shell::plugins::{FileHandlerPlugin, PluginMetadata, PluginCategory};
use aether_shell::value::Value;
use anyhow::Result;

pub struct YamlFileHandler;

impl FileHandlerPlugin for YamlFileHandler {
    fn metadata(&self) -> &PluginMetadata {
        static META: OnceLock<PluginMetadata> = OnceLock::new();
        META.get_or_init(|| PluginMetadata {
            id: "yaml-handler".to_string(),
            name: "YAML File Handler".to_string(),
            version: "1.0.0".to_string(),
            author: "Your Name".to_string(),
            description: "Native YAML file support".to_string(),
            categories: vec![PluginCategory::FileHandler],
            min_aether_version: "0.1.0".to_string(),
            dependencies: vec![],
        })
    }
    
    fn supported_extensions(&self) -> Vec<String> {
        vec!["yaml".to_string(), "yml".to_string()]
    }
    
    fn supported_mime_types(&self) -> Vec<String> {
        vec!["application/yaml".to_string()]
    }
    
    fn read(&self, path: &std::path::Path) -> Result<Value> {
        let content = std::fs::read_to_string(path)?;
        let yaml: serde_yaml::Value = serde_yaml::from_str(&content)?;
        Ok(yaml_to_value(yaml))
    }
    
    fn write(&self, path: &std::path::Path, value: &Value) -> Result<()> {
        let yaml = value_to_yaml(value);
        let content = serde_yaml::to_string(&yaml)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
```

---

## Plugin API Reference

### Traits

#### `AIBackendPlugin`

Implement to add custom AI/LLM providers.

```rust
pub trait AIBackendPlugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    fn is_available(&self) -> bool;
    fn supported_models(&self) -> Vec<String>;
    fn chat_completion(&self, model: &str, messages: Vec<ChatMessage>) -> Result<String>;
    fn embeddings(&self, model: &str, input: &str) -> Result<Vec<f32>>; // optional
    fn supports_streaming(&self) -> bool; // optional
}
```

#### `BuiltinPlugin`

Implement to add custom shell functions.

```rust
pub trait BuiltinPlugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    fn builtin_names(&self) -> Vec<String>;
    fn execute(&self, name: &str, args: Vec<Value>, input: Option<Value>) -> Result<Value>;
    fn help(&self, name: &str) -> Option<String>; // optional
}
```

#### `FileHandlerPlugin`

Implement to add file format support.

```rust
pub trait FileHandlerPlugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    fn supported_extensions(&self) -> Vec<String>;
    fn supported_mime_types(&self) -> Vec<String>;
    fn read(&self, path: &Path) -> Result<Value>;
    fn write(&self, path: &Path, value: &Value) -> Result<()>;
}
```

#### `TransportPlugin`

Implement to add network protocol support.

```rust
pub trait TransportPlugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    fn scheme(&self) -> &str;
    fn connect(&self, uri: &str) -> Result<Box<dyn TransportConnection>>;
}

pub trait TransportConnection: Send + Sync {
    fn send(&self, data: &[u8]) -> Result<()>;
    fn receive(&self) -> Result<Vec<u8>>;
    fn close(&self) -> Result<()>;
    fn is_connected(&self) -> bool;
}
```

---

## Built-in Plugins

AetherShell ships with these built-in plugins:

| Plugin ID      | Category    | Description              |
| -------------- | ----------- | ------------------------ |
| `builtin.json` | FileHandler | Native JSON file support |
| `builtin.csv`  | FileHandler | Native CSV file support  |
| `builtin.toml` | FileHandler | Native TOML file support |

---

## Example Plugins

### 1. Math Utilities Plugin

```toml
# ~/.config/aethershell/plugins/math-utils/plugin.toml
[plugin]
id = "math-utils"
name = "Math Utilities"
version = "1.0.0"
description = "Additional math functions"
categories = ["Builtin"]

[builtins]
factorial = """
fn(n) => {
    if n <= 1 { 1 }
    else { n * factorial(n - 1) }
}
"""
fibonacci = """
fn(n) => {
    if n <= 1 { n }
    else { fibonacci(n - 1) + fibonacci(n - 2) }
}
"""
is_prime = """
fn(n) => {
    if n < 2 { false }
    else {
        [2] | concat(range(3, sqrt(n) + 1, 2))
            | all(fn(d) => n % d != 0)
    }
}
"""
```

### 2. String Utilities Plugin

```toml
# ~/.config/aethershell/plugins/string-utils/plugin.toml
[plugin]
id = "string-utils"
name = "String Utilities"
version = "1.0.0"
description = "Additional string manipulation functions"
categories = ["Builtin"]

[builtins]
title_case = "fn(s) => s | split(' ') | map(fn(w) => upper(first(w)) + lower(slice(w, 1))) | join(' ')"
snake_case = "fn(s) => s | lower | replace(' ', '_')"
camel_case = "fn(s) => s | title_case | replace(' ', '')"
repeat = "fn(s, n) => range(1, n + 1) | map(fn(_) => s) | join('')"
```

### 3. DevOps Helpers Plugin

```toml
# ~/.config/aethershell/plugins/devops/plugin.toml
[plugin]
id = "devops-helpers"
name = "DevOps Helpers"
version = "1.0.0"
description = "Common DevOps utility functions"
categories = ["Builtin"]

[builtins]
docker_ps = "fn() => sh('docker ps --format \"{{.Names}}\\t{{.Status}}\\t{{.Ports}}\"') | split('\\n') | where(fn(l) => len(l) > 0)"
k8s_pods = "fn(ns) => sh('kubectl get pods -n ' + ns + ' -o name') | split('\\n') | where(fn(l) => len(l) > 0)"
git_status = "fn() => sh('git status --porcelain') | split('\\n') | where(fn(l) => len(l) > 0) | map(fn(l) => { status: slice(l, 0, 2), file: trim(slice(l, 3)) })"
```

---

## Plugin Directories

AetherShell searches for plugins in these locations (in order):

1. `~/.aethershell/plugins/` - User plugins (legacy)
2. `~/.local/share/aethershell/plugins/` - XDG data directory (Linux)
3. `~/Library/Application Support/aethershell/plugins/` - macOS
4. `%LOCALAPPDATA%\aethershell\plugins\` - Windows
5. `/usr/share/aethershell/plugins/` - System plugins (Unix)
6. `%ProgramData%\AetherShell\plugins\` - System plugins (Windows)

---

## Best Practices

1. **Use unique plugin IDs** - Prefix with your org name: `myorg.my-plugin`
2. **Specify min_aether_version** - Ensure compatibility
3. **Document your builtins** - Include help text for each function
4. **Handle errors gracefully** - Return meaningful error messages
5. **Test thoroughly** - Plugins can affect the entire shell
6. **Version your plugins** - Use semantic versioning

---

## Troubleshooting

### Plugin won't load

1. Check the manifest syntax: `cat plugin.toml | json_parse` (should error)
2. Verify the path is correct
3. Check for missing required fields (id is required)

### Builtin function not found

1. Ensure the plugin is loaded: `plugins | where(fn(p) => p.id == "my-plugin")`
2. Check if the plugin is enabled: `plugin_info("my-plugin")`
3. Verify the builtin name matches the manifest

### Plugin conflicts

1. Check for duplicate plugin IDs
2. Ensure no builtin name conflicts with core functions
3. Review dependency order

---

## See Also

- [README.md](../README.md) - Main documentation
- [TESTING.md](../TESTING.md) - Testing guide
- [ROADMAP.md](../ROADMAP.md) - Development roadmap
