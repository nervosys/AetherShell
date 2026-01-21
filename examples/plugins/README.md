# Example Plugins for AetherShell

This directory contains example plugins that demonstrate the AetherShell plugin system.

## Plugins

### hello-plugin

A simple "Hello World" plugin demonstrating basic builtin functions.

```aether
plugin_load("examples/plugins/hello-plugin/plugin.toml")
hello("World")  # => "Hello, World!"
```

### math-utils

Mathematical utility functions including factorial, fibonacci, and prime checking.

```aether
plugin_load("examples/plugins/math-utils/plugin.toml")
factorial(5)  # => 120
fibonacci(10)  # => 55
is_even(42)  # => true
```

### string-utils

String manipulation utilities for common transformations.

```aether
plugin_load("examples/plugins/string-utils/plugin.toml")
title_case("hello world")  # => "Hello World"
snake_case("Hello World")  # => "hello_world"
```

## Loading Plugins

```aether
# Load a single plugin
plugin_load("examples/plugins/hello-plugin/plugin.toml")

# List all loaded plugins
plugins

# Check plugin details
plugin_info("hello-plugin")

# Disable a plugin
plugin_disable("hello-plugin")

# Unload a plugin
plugin_unload("hello-plugin")
```

## Creating Your Own Plugin

1. Create a directory for your plugin
2. Add a `plugin.toml` manifest file
3. Define your builtins in the `[builtins]` section
4. Load with `plugin_load("path/to/plugin.toml")`

See the [Plugin Documentation](../../docs/PLUGINS.md) for the full guide.
