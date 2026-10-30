# Plugins

IronShell supports a plugin system for extending the shell with custom builtins, tools, and integrations.

## Managing Plugins

### Listing Plugins

```ironshell
plugins
# [
#   { name: "git-tools", version: "1.0", enabled: true, category: "dev" },
#   { name: "docker-tools", version: "0.5", enabled: false, category: "containers" },
#   ...
# ]
```

### Plugin Information

```ironshell
plugin_info "git-tools"
# {
#   name: "git-tools",
#   version: "1.0",
#   description: "Git integration tools for IronShell",
#   builtins: ["git_status", "git_log", "git_diff"],
#   enabled: true
# }
```

### Enabling and Disabling

```ironshell
plugin_enable "docker-tools"
plugin_disable "git-tools"
```

### Loading and Unloading

`plugin_load` takes the path to a plugin's TOML manifest, not a plugin name. It
registers the plugin with `status: "registered"`. Each `[builtins]` entry is one
lambda literal and its name becomes callable. A core name, or a name another
loaded plugin provides, is refused. A manifest with `[builtins]` is code, so
`IRONSHELL_PLUGINS=off` refuses it and agent mode needs `IRONSHELL_PLUGIN_ALLOW`. Agent
mode approves each plugin builtin call. Native and script plugins are not loaded
by this builtin. A duplicate id is `E_BAD_STATE`.

```ironshell
plugin_load "./plugins/my-custom-plugin/plugin.toml"   # Register its metadata
plugin_unload "my-custom-plugin"                       # Remove it (built-in plugins cannot be unloaded)
```

### Categories

```ironshell
plugin_categories
# ["dev", "containers", "cloud", "data", "security", ...]
```

## Plugin Architecture

Plugins are Rust crates that implement the tool and builtin interfaces. They provide:

1. **Custom builtins** — New commands available in the shell
2. **Tool schemas** — OpenAI-compatible function descriptions for AI agent use
3. **Configuration** — Plugin-specific settings

## Feature Flags

IronShell uses feature flags to enable/disable built-in capabilities:

```ironshell
features                    # List all features
feature_enabled "ai"        # Check if AI is enabled
feature_enable "distributed"
feature_disable "experimental"
feature_set "max_agents" 10
```
