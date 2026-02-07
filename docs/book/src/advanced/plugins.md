# Plugins

AetherShell supports a plugin system for extending the shell with custom builtins, tools, and integrations.

## Managing Plugins

### Listing Plugins

```aethershell
plugins
# [
#   { name: "git-tools", version: "1.0", enabled: true, category: "dev" },
#   { name: "docker-tools", version: "0.5", enabled: false, category: "containers" },
#   ...
# ]
```

### Plugin Information

```aethershell
plugin_info "git-tools"
# {
#   name: "git-tools",
#   version: "1.0",
#   description: "Git integration tools for AetherShell",
#   builtins: ["git_status", "git_log", "git_diff"],
#   enabled: true
# }
```

### Enabling and Disabling

```aethershell
plugin_enable "docker-tools"
plugin_disable "git-tools"
```

### Loading and Unloading

```aethershell
plugin_load "my-custom-plugin"      # Load into memory
plugin_unload "my-custom-plugin"    # Remove from memory
```

### Categories

```aethershell
plugin_categories
# ["dev", "containers", "cloud", "data", "security", ...]
```

## Plugin Architecture

Plugins are Rust crates that implement the tool and builtin interfaces. They provide:

1. **Custom builtins** — New commands available in the shell
2. **Tool schemas** — OpenAI-compatible function descriptions for AI agent use
3. **Configuration** — Plugin-specific settings

## Feature Flags

AetherShell uses feature flags to enable/disable built-in capabilities:

```aethershell
features                    # List all features
feature_enabled "ai"        # Check if AI is enabled
feature_enable "distributed"
feature_disable "experimental"
feature_set "max_agents" 10
```
