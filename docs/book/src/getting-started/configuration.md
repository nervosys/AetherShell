# Configuration

IronShell reads one TOML settings file and environment variables. AI models are stored in a separate directory, described under [Model Storage](#model-storage). Some keys in the TOML file are read but have no effect; the tables below say which.

## Configuration File

### Where it is read from

IronShell uses the first of these that applies:

1. The file named by `IRONSHELL_CONFIG`, if that variable is set. It is used even if it does not exist; then the defaults apply.
2. `config.toml` in the config directory, if that file exists. The config directory is `IRONSHELL_CONFIG_HOME` if set. Otherwise it is an `ironshell` directory in the OS config directory:
   - Linux: `$XDG_CONFIG_HOME/ironshell` (`~/.config/ironshell` when `XDG_CONFIG_HOME` is unset)
   - macOS: `~/Library/Application Support/ironshell`
   - Windows: `%APPDATA%\ironshell\config.toml`
3. `~/.ironshellrc`, if it exists. It is parsed as TOML if it looks like TOML; otherwise it is read as legacy `key = value` lines. The legacy keys it reads are `color`/`colors`, `theme`, `vi_mode`/`vi-mode`, `auto_cd`/`auto-cd`, `banner`/`show_banner`, `history_size`/`history-size`, `editor`/`EDITOR`, `ai_model`/`model`, and `ai_provider`/`provider`. Other lines are ignored.
4. Otherwise the built-in defaults. IronShell does not create a file at startup.

If the file exists but cannot be read or does not parse, IronShell starts with the defaults and prints one line to stderr naming the file and the error. `config_set()` and `config_reload()` report the error.

### Creating and changing it

- `config_init()` writes a commented default file to the config directory. It does nothing if `config.toml` already exists.
- `config_set(path, value)` changes one key and saves the file. It accepts only these paths: `colors.enabled`, `colors.force`, `colors.true_color`, `colors.theme`, `shell.show_banner`, `shell.show_tips`, `shell.vi_mode`, `shell.auto_cd`, `ai.default_model`, `ai.default_provider`, `ai.max_tokens`, `ai.streaming`, `history.enabled`, `history.max_size`.
  - It writes the same file that the shell reads, which is the file `config_path()` reports as `config_file`: `IRONSHELL_CONFIG` if that is set; otherwise `config.toml` in the config directory if it exists; otherwise `~/.ironshellrc` if it exists; otherwise a new `config.toml` in the config directory. Those are the rules in the list above. A file that the shell reads from `~/.ironshellrc` is written back there, as TOML.
  - After the save, the running shell reloads the file, so the new value is in effect and `config_get()` returns it at once. The save rewrites the whole file from the settings IronShell knows, so comments and keys it does not read are dropped.
- Edits to the file take effect when you run `config_reload()` or start IronShell again. `config_reload()` replaces the settings in use; if the file cannot be read or does not parse, it fails and the settings in use stay.

Unknown keys are ignored. A key you leave out takes the default shown below.

### Example

The values are the built-in defaults.

```toml
[shell]
show_banner = true
show_tips = true
default_directory = ""
vi_mode = false
auto_cd = false
glob_expansion = true
command_correction = true
bell_style = "none"

[colors]
enabled = true
theme = "catppuccin"
force = false
true_color = true

[colors.custom]          # used when theme = "custom"
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
format = "{symbol}"
continuation = "… "
right = ""
show_git = true
show_time = true
time_threshold_ms = 1000
transient = false
style = "classic"
symbol = "❯"
segments = ["os", "cwd", "git", "status", "duration"]
powerline_separator = ""
abbreviate_path = true
max_path_segments = 0
two_line = false
show_user_host = false
show_git_dirty = false
autosuggestions = true

[prompt.segment_colors]  # e.g. cwd = "#1e1e2e:#89b4fa"

[prompt.abbreviations]   # e.g. gs = "git.status()"

[ai]
default_provider = "openai"
default_model = "gpt-4o-mini"
suggestions = false
max_tokens = 4096
temperature = 0.7
streaming = true
allowed_tools = []
blocked_tools = []
max_agent_steps = 10

[history]
enabled = true
max_size = 10000
ignore_duplicates = true
ignore_space = true
ignore_patterns = ["^exit$", "^quit$"]
share = false
timestamps = true

[editor]
# external defaults to $EDITOR, else notepad on Windows or vi elsewhere
tab_width = 4
syntax_highlighting = true
line_numbers = true
auto_indent = true
bracket_matching = true

[keybindings]
mode = "emacs"

[keybindings.custom]

[aliases]

[env]
```

### Keys

"Applied" means the shell's own code uses the value. "Not applied" means the key is read into the config and shown by `config()` and `config_get()`, and `config_set()` can change it, but nothing else in the shell acts on it.

#### `[shell]`

| Key | Type | Default | Status |
| --- | --- | --- | --- |
| `show_banner` | Bool | `true` | Applied: the REPL banner |
| `show_tips` | Bool | `true` | Not applied |
| `default_directory` | String | `""` | Not applied |
| `vi_mode` | Bool | `false` | Not applied |
| `auto_cd` | Bool | `false` | Not applied |
| `glob_expansion` | Bool | `true` | Not applied |
| `command_correction` | Bool | `true` | Not applied |
| `bell_style` | String | `"none"` | Not applied |

#### `[colors]`

| Key | Type | Default | Status |
| --- | --- | --- | --- |
| `enabled` | Bool | `true` | Applied: `false` turns colour off. `NO_COLOR` also turns it off |
| `theme` | String | `"catppuccin"` | Applied. Names are those `Theme::from_str` accepts, such as `catppuccin`, `catppuccin-latte`, `monokai`, `dracula`, `nord`, `gruvbox`, `tokyonight`. An unknown name gives Catppuccin. `"custom"` uses `[colors.custom]` |
| `force` | Bool | `false` | Not applied. Forced colour comes from the `FORCE_COLOR` or `CLICOLOR_FORCE` environment variables |
| `true_color` | Bool | `true` | Applied |

`[colors.custom]` holds hex colours for `number`, `string`, `boolean`, `keyword`, `punctuation`, `key`, `uri`, `error`, `warning`, `success`, `dim` and `comment`. Its defaults are the Catppuccin Mocha colours shown in the example. It is applied when `theme = "custom"`.

#### `[prompt]`

Applied. The prompt and the line editor use these keys.

| Key | Type | Default |
| --- | --- | --- |
| `format` | String | `"{symbol}"` |
| `continuation` | String | `"… "` |
| `right` | String | `""` |
| `show_git` | Bool | `true` |
| `show_time` | Bool | `true` |
| `time_threshold_ms` | Integer | `1000` |
| `transient` | Bool | `false` |
| `style` | String | `"classic"`: `classic`, `fish`, `powerline`, `pure` or `custom` |
| `symbol` | String | `"❯"` |
| `segments` | Array of String | `["os", "cwd", "git", "status", "duration"]`: `os`, `user`, `host`, `user@host`, `cwd`, `git`, `status`, `duration`, `time`, `symbol`; other values are shown as literal text |
| `powerline_separator` | String | `""` (needs a Nerd Font) |
| `abbreviate_path` | Bool | `true` |
| `max_path_segments` | Integer | `0` (keep all) |
| `two_line` | Bool | `false` |
| `show_user_host` | Bool | `false` |
| `show_git_dirty` | Bool | `false` (costs a `git status` per prompt) |
| `autosuggestions` | Bool | `true` |
| `[prompt.segment_colors]` | Table | empty. Values are `"#fg"` or `"#fg:#bg"` |
| `[prompt.abbreviations]` | Table | empty. Abbreviations expand when you press space |

#### `[ai]`

Not applied to `ai()`, to agents or to the TUI. These keys are read and shown by `config()` and `config_get()`, and `config_set()` can change some of them. The provider and model come from `IRONSHELL_AI` and the variables in [AI Provider Setup](#ai-provider-setup).

| Key | Type | Default |
| --- | --- | --- |
| `default_provider` | String | `"openai"` |
| `default_model` | String | `"gpt-4o-mini"` |
| `suggestions` | Bool | `false` |
| `max_tokens` | Integer | `4096` |
| `temperature` | Float | `0.7` |
| `streaming` | Bool | `true` |
| `allowed_tools` | Array of String | `[]` |
| `blocked_tools` | Array of String | `[]` |
| `max_agent_steps` | Integer | `10` |

#### `[history]`

| Key | Type | Default | Status |
| --- | --- | --- | --- |
| `enabled` | Bool | `true` | Applied: when `false`, history is neither loaded nor saved |
| `max_size` | Integer | `10000` | Not applied |
| `ignore_duplicates` | Bool | `true` | Applied |
| `ignore_space` | Bool | `true` | Applied: commands that start with a space are not saved |
| `ignore_patterns` | Array of String | `["^exit$", "^quit$"]` | Not applied |
| `share` | Bool | `false` | Not applied |
| `timestamps` | Bool | `true` | Not applied |

History is saved to the `history` file in the data directory.

#### `[editor]`

Not applied. The `external` default is `$EDITOR`, else `notepad` on Windows or `vi` elsewhere.

| Key | Type | Default |
| --- | --- | --- |
| `external` | String | `$EDITOR`, else `notepad` or `vi` |
| `tab_width` | Integer | `4` |
| `syntax_highlighting` | Bool | `true` |
| `line_numbers` | Bool | `true` |
| `auto_indent` | Bool | `true` |
| `bracket_matching` | Bool | `true` |

#### `[keybindings]`

Not applied.

| Key | Type | Default |
| --- | --- | --- |
| `mode` | String | `"emacs"` |
| `[keybindings.custom]` | Table | empty |

#### `[aliases]` and `[env]`

Not applied. `[aliases]` is a map from a name to a command. `[env]` is a map of variable names to values. IronShell does not set these variables. The settings are parsed and kept, but nothing reads them.

## Environment Variables

| Variable            | Description            | Example                      |
| ------------------- | ---------------------- | ---------------------------- |
| `IRONSHELL_AI`         | Default AI provider    | `openai`, `claude`, `ollama` |
| `OPENAI_API_KEY`    | OpenAI API key         | `sk-...`                     |
| `ANTHROPIC_API_KEY` | Anthropic API key      | `sk-ant-...`                 |
| `GOOGLE_API_KEY`    | Google AI API key      | `AIza...`                    |
| `OLLAMA_HOST`       | Ollama address probed by model discovery (chat uses `OLLAMA_URL`) | `http://localhost:11434`     |
| `AGENT_ALLOW_CMDS`  | Comma-separated commands an agent step may run; unset allows none | `ls,cat,grep`                |
| `IRONSHELL_RECORD` | Local evaluation records (off unless set) | `1`, `full` |

The tables below cover every `IRONSHELL_*` variable the code reads, then the
other names IronShell reads to make a decision. Each row gives the code
location that reads it. Mode is `Agent` when the effect applies only in agent
mode (`--agent`, `IRONSHELL_MODE=agent`, `ish agent serve`, `ish mcp serve`),
and `Both` otherwise. Boolean switches accept only the values listed; any other
value counts as unset.

### Provider and model selection

| Variable | Accepted values (default) | Effect | Mode |
| --- | --- | --- | --- |
| `IRONSHELL_AI` | A provider name, or `provider:model`. Unset: IronGate if it answers, else an error | Chooses the backend for `ai()` and the TUI chat (`complete_sync_router`, `src/ai.rs:659`). Names: `irongate` (`gate`, `iron`), `openai`, `anthropic` (`claude`), `google` (`gemini`), `azure`, `ollama`, `vllm`, `tgi`, `llamacpp` (`llama_cpp`), `local` (`lmstudio`), `compat` (`openai_compat`), `openrouter`, `deepseek`, `groq`, `together`, `fireworks`, `perplexity`, `mistral`, `cohere`, `xai` (`grok`) | Both |
| `IRONSHELL_AI_ROUTER` | `registry` (unset: off) | `ai()` tries the provider registry first, with its rules, status and fallback (`src/ai.rs:661`) | Both |
| `IRONSHELL_MODEL_URI` | `provider:model` (unset) | Model for multimodal `ai()` calls with images, audio or video (`src/ai.rs:466`), and the backend `backend_from_env` returns for agents and RLM (`src/ai.rs:2073`) | Both |
| `IRONSHELL_AGENT_MODEL_URI` | `provider:model` (unset: `default`) | Model for `agent` runs that name none (`src/ai.rs:2439`, `2459`; `src/builtins.rs:22773`) | Both |
| `IRONSHELL_SWARM_AGENT_MODEL_URI` | `provider:model` | Model for each swarm member that names none (`src/ai.rs:2714`) | Both |
| `IRONSHELL_RLM_MODEL_URI` | `provider:model` | Subagent model for recursive agents when the config names none (`src/rlm.rs:670`) | Both |
| `IRONSHELL_COMPAT_BASE` | URL (`http://localhost:8000/v1`) | Base URL of an OpenAI-compatible server for `compat` (`src/ai.rs:1035`, `1151`) | Both |
| `IRONSHELL_COMPAT_MODEL` | Model name (`mixtral`) | Model sent to the `compat` server (`src/ai.rs:1153`, `1401`) | Both |
| `IRONSHELL_MODEL_DIR` | Directory | Extra directory scanned for local models (`src/ai_api/providers.rs:866`) | Both |

### Agent mode and safety

| Variable | Accepted values (default) | Effect | Mode |
| --- | --- | --- | --- |
| `IRONSHELL_MODE` | `agent` (case-insensitive). Any other value, or unset, is human mode | Selects agent mode: default-deny for dangerous effects, and the workspace jail (`current_mode`, `src/safety.rs:1458`). `--agent` sets it (`src/main.rs:330`). `ish agent serve` and `ish mcp serve` set it when it is unset (`src/main.rs:353`) | Sets the mode |
| `IRONSHELL_AGENT` | `1`, `true`, `yes`, `on` | Also selects agent mode (`src/safety.rs:1463`) | Sets the mode |
| `IRONSHELL_POLICY` | `permissive` (case-insensitive). Other values are ignored (strict) | Makes the guard and the egress and clipboard checks let agent mode through, as human mode does (`policy_permissive`, `src/safety.rs:1474`). `--policy` sets it (`src/main.rs:357`) | Agent |
| `IRONSHELL_WORKSPACE` | A directory (unset: the current directory) | Workspace root. Writes are confined to it. Setting it jails writes outside agent mode too (`workspace_root`, `src/safety.rs:2673`; `src/security.rs:492`), and module imports are held to it (`src/module_loader.rs:303`). `--workspace` sets it (`src/main.rs:333`) | Both |
| `IRONSHELL_FIPS` | `1`, `on`, `true` (unset: off) | Refuses non-FIPS hash algorithms, MD5 and SHA-1 (`fips_enabled`, `src/safety.rs:119`, `140`) | Both |
| `IRONSHELL_NET_ALLOW` | Hosts separated by `,` or `;`. `*` allows all (unset: no allowlist) | Egress allowlist for network builtins. A call naming an unlisted host is refused. The check is not mode-specific, and it is skipped under `IRONSHELL_POLICY=permissive` (`net_allowlist`, `src/safety.rs:4734`; `egress_permitted`, `src/safety.rs:5432`) | Both |
| `IRONSHELL_ALLOW_SH` | `true`, `1`, `yes` (exact, lowercase) | Enables the `sh()` builtin, which is otherwise denied (`validate_sh_allowed`, `src/security.rs:942`) | Both |
| `IRONSHELL_REDACT` | Unset: on. `off`, `0`, `false`, `no` (lowercase) turn it off | Redacts secret-looking values from output (`redaction_enabled`, `src/safety.rs:6796`) | Both |
| `IRONSHELL_SECRETS` | `allow`, `1`, `true` (lowercase; unset: denied) | Permits agent mode to read secret-named environment variables in the clear (`env_secret_gated`, `src/safety.rs:6838`) and to read the clipboard (`clipboard_read_gate`, `src/safety.rs:6851`) | Agent |
| `IRONSHELL_CHILD_ENV_KEEP` | Comma-separated variable names (unset: none) | Secret-named variables a child process keeps in agent mode (`child_env_keep`, `src/safety.rs:2060`) | Agent |
| `IRONSHELL_CRYPTO_LEGACY_DECRYPT` | `1`, `true`, `yes`, `on` | Allows decrypting legacy unauthenticated CBC ciphertext, which is otherwise refused (`src/builtins.rs:43428`) | Both |
| `IRONSHELL_APPROVE` | Comma-separated approval tokens | Approves the listed tokens, such as a plan token or a network call (`is_token_approved`, `src/safety.rs:3075`) | Both |
| `IRONSHELL_APPROVE_ALL` | `1`, `true`, `yes`, `on` | Approves every token. For trusted automation only (`src/safety.rs:3071`) | Both |
| `IRONSHELL_PRINCIPAL` | User id (unset: anonymous) | Acting principal for RBAC checks (`init_rbac_from_env`, `src/safety.rs:3132`) | Both |
| `IRONSHELL_RBAC_CONFIG` | Path to an RBAC file (unset: `<workspace>/.ironshell/rbac.toml`, if it exists) | RBAC roles and permissions (`src/safety.rs:3137`) | Both |
| `IRONSHELL_PLUGINS` | `off` (case-insensitive) or `0` disables native plugins | Kill switch for loading native plugins (`src/plugins.rs:351`, `1939`) | Both |
| `IRONSHELL_PLUGIN_ALLOW` | Directories separated by `;` | Directories native plugins may load from. In agent mode, a plugin outside them is refused (`src/plugins.rs:356`, `1943`) | Agent |

### Limits and output

| Variable | Accepted values (default) | Effect | Mode |
| --- | --- | --- | --- |
| `IRONSHELL_MAX_OPS` | Non-negative integer (unset: no limit) | Maximum guarded operations per run (`governor_admit`, `src/safety.rs:3940`) | Agent |
| `IRONSHELL_MAX_FILES` | Non-negative integer (unset: no limit) | Maximum file operations per run (`src/safety.rs:3952`) | Agent |
| `IRONSHELL_MAX_PROCS` | Non-negative integer (unset: no limit) | Maximum processes started per run (`src/safety.rs:3964`) | Agent |
| `IRONSHELL_MAX_NET` | Non-negative integer (unset: no limit) | Maximum network requests per run (`src/safety.rs:3976`) | Agent |
| `IRONSHELL_TIMEOUT_MS` | Milliseconds (unset: no limit) | Wall-clock budget, started at the first guarded operation (`src/safety.rs:3928`) | Agent |
| `IRONSHELL_TOKEN_BUDGET` | Positive integer, in estimated tokens (unset: no budget) | Budget for agent-mode rendering of arrays and tables, which are paged or truncated to fit (`src/agent_api.rs:738`, `src/repl.rs:244`, `src/mcp.rs:445`). `--budget` sets it (`src/main.rs:326`) | Agent |
| `IRONSHELL_HANDLE_BYTES` | Integer bytes (default `2048`). `0` disables handles | A structured result larger than this is kept as a session handle that the agent can query, not sent whole (`threshold_bytes`, `src/handles.rs:251`; `src/builtins.rs:20200`) | Agent |
| `IRONSHELL_MAX_OUTPUT_BYTES` | Non-negative integer (unset: no cap) | Caps the output of `ish -c`. A value that is not a number is ignored, with a warning (`src/repl.rs:195`) | Both |
| `IRONSHELL_DETERMINISTIC` | `1` or `true` (case-insensitive) | Renders results as canonical, byte-stable JSON. `--deterministic` sets it (`src/agent_api.rs:346`, `src/repl.rs:252`, `src/main.rs:360`) | Both |
| `IRONSHELL_MCP_TOOLS` | `all` or `full` (default: compact) | MCP tool list. The default is three discovery tools; `all` lists every builtin (`tools_compact_mode`, `src/mcp.rs:320`) | Both |

### Audit, journal and telemetry

| Variable | Accepted values (default) | Effect | Mode |
| --- | --- | --- | --- |
| `IRONSHELL_AUDIT_LOG` | A path (agent default: `<workspace>/.ironshell/audit.log`) | Hash-chained audit log (`audit_path`, `src/safety.rs:3211`). Human mode writes no audit log unless this is set | Both |
| `IRONSHELL_AUDIT_SINK` | A path | Each audit line is also appended here, as a mirror (`mirror_to_sink`, `src/safety.rs:3621`). A sink inside the workspace is protected from writes (`src/safety.rs:2709`) | Both |
| `IRONSHELL_AUDIT_REQUIRED` | `1`, `true`, `yes`, `on` | An audit write failure fails the guarded operation, instead of only logging a warning (`src/safety.rs:3705`, `3713`) | Both |
| `IRONSHELL_AUDIT_KEY` | Hex that decodes to at least 32 bytes, or at least 32 raw bytes | Key for the keyed audit chain. It is read once and then removed from the environment. A shorter key leaves the chain unkeyed, with an error (`load_audit_key`, `src/safety.rs:3274`, `AUDIT_KEY_MIN_BYTES` at `src/safety.rs:3260`) | Both |
| `IRONSHELL_AUDIT_KEY_FILE` | A path to a file with the key in the same format | Key source when `IRONSHELL_AUDIT_KEY` is unset (`src/safety.rs:3281`) | Both |
| `IRONSHELL_JOURNAL` | `on`, `1`, `true` or `off`, `0`, `false` (unset: on in agent mode, off for humans) | Forces the undo journal on or off (`enabled`, `src/journal.rs:287`) | Both |
| `IRONSHELL_TELEMETRY` | `1` or `true` (case-insensitive; unset: off) | Starting state of in-process telemetry, which is opt-in (`src/builtins.rs:1725`) | Both |

### Directories, sessions and features

| Variable | Accepted values (default) | Effect | Mode |
| --- | --- | --- | --- |
| `IRONSHELL_CONFIG` | A path to a file | Config file location. Replaces `config.toml` in the config directory (`config_file`, `src/config.rs:451`) | Both |
| `IRONSHELL_CONFIG_HOME` | A directory (default: `ironshell` in the OS config directory) | Config directory (`config_dir`, `src/config.rs:408`) | Both |
| `IRONSHELL_DATA_HOME` | A directory (default: `ironshell` in the OS data directory) | Data directory, including the alert store (`data_dir`, `src/config.rs:422`) | Both |
| `IRONSHELL_CACHE_HOME` | A directory (default: `ironshell` in the OS cache directory) | Cache directory (`cache_dir`, `src/config.rs:436`) | Both |
| `IRONSHELL_SESSION_DIR` | A directory | Session directory. Handles and `record.jsonl` go here directly; the journal uses a `journal` subdirectory (`src/handles.rs:44`, `src/journal.rs:106`) | Both |
| `IRONSHELL_SESSION` | Any key string (unset: an MD5 of the working directory) | Names the per-project cache subdirectory for handles and the journal (`src/handles.rs:47`, `src/journal.rs:109`) | Both |
| `IRONSHELL_FEATURES` | Comma-separated feature names | Features that are enabled in feature conditions in `cfg` expressions and in the feature registry (`src/eval.rs:23`, `src/builtins.rs:1608`) | Both |

### Network servers, registries and marketplace

| Variable | Accepted values (default) | Effect | Mode |
| --- | --- | --- | --- |
| `IRONSHELL_API_TOKEN` | A string | Bearer token for `ish agent serve` (its `--token` flag wins) and `ish mcp serve` (`src/main.rs:635`, `694`) | Both |
| `IRONSHELL_ALLOW_REMOTE_BIND` | `1` or `true` | `ish mcp serve` refuses a non-loopback address unless this is set (`src/mcp.rs:1303`). The agent API only prints a warning for one (`src/agent_api.rs:4869`) | Both |
| `IRONSHELL_REGISTRY_TOKEN` | A string | Package registry token. Checked before `IRONSHELL_TOKEN` (`src/packages.rs:362`) | Both |
| `IRONSHELL_TOKEN` | A string | Fallback package registry token (`src/packages.rs:363`) | Both |
| `IRONSHELL_API_KEY` | A string | Registry key, required to publish an agent to the marketplace (`src/marketplace.rs:650`) | Both |

### Variables read from other tools

These are not IronShell settings. Each row names the decision the code makes
with it, or says that the value is only reported.

| Variable | Read at | Decision it affects |
| --- | --- | --- |
| `OPENAI_API_KEY`, `ANTHROPIC_API_KEY`, `GOOGLE_API_KEY`, `AZURE_OPENAI_API_KEY`, `GROQ_API_KEY`, `MISTRAL_API_KEY`, `COHERE_API_KEY`, `DEEPSEEK_API_KEY`, `XAI_API_KEY`, `OPENROUTER_API_KEY`, `TOGETHER_API_KEY`, `FIREWORKS_API_KEY`, `PERPLEXITY_API_KEY`, `LOCAL_API_KEY`, `IRONGATE_API_KEY` | `src/providers/mod.rs:210-229` | The key a provider is sent with. A provider counts as configured when its key is set (`src/providers/registry.rs:86`). Bedrock has no key here: `AWS_SECRET_ACCESS_KEY` is never read, and `bedrock:` is refused with `E_UNIMPLEMENTED` because SigV4 signing is not implemented |
| `IRONGATE_URL` | `src/model_plane.rs:55`, `src/providers/mod.rs:241` | Gateway address. Default `http://localhost:7700/v1` |
| `IRONGATE_MODEL` | `src/model_plane.rs:73` | Model name asked of the gateway. Default `auto` |
| `IRONGATE_TOKEN` | `src/model_plane.rs:82` | Bearer token, used when `IRONGATE_API_KEY` is unset |
| `IRONVAULT_BIN` | `src/model_plane.rs:230` | Vault CLI to run. Default `iv` |
| `OLLAMA_URL` | `src/ai.rs:615`, `1128` | Ollama chat endpoint. Default `http://localhost:11434` |
| `OLLAMA_MODEL` | `src/ai.rs:616`, `1129` | Ollama model. Default `llama3` (`llava` in the multimodal path) |
| `OLLAMA_HOST` | `src/ai.rs:1666` | Address probed when discovering local models. Default `http://localhost:11434` |
| `OPENAI_MODEL` | `src/ai.rs:514`, `1079`, `1281`, `1344` | OpenAI model name. Default `gpt-4o-mini` |
| `AZURE_OPENAI_ENDPOINT` | `src/ai.rs:751`, `1017` | Azure OpenAI deployment URL |
| `VLLM_URL`, `VLLM_BASE_URL`, `VLLM_MODEL`, `VLLM_API_KEY` | `src/ai.rs:764`, `1025`, `1297`, `1524`, `1525`, `1705`; `src/ai_api/providers.rs:1336` | vLLM endpoint, model and key. Default endpoint `http://localhost:8000/v1` |
| `TGI_URL`, `TGI_BASE_URL` | `src/ai.rs:1031`, `1191`, `1426`, `1784` | TGI endpoint. Default `http://localhost:8080` |
| `LLAMACPP_URL`, `LLAMA_CPP_URL`, `LLAMACPP_MODEL`, `LLAMACPP_API_KEY` | `src/ai.rs:1302`, `1553`, `1554`, `1745`; `src/ai_api/providers.rs:871`, `1747` | llama.cpp endpoint, model and key. Default `http://localhost:8080/v1` |
| `LMSTUDIO_MODEL` | `src/ai.rs:1306` | LM Studio model name. Default `default` |
| `TENSORRT_LLM_API_KEY`, `SGLANG_API_KEY` | `src/ai_api/providers.rs:1476`, `1610` | Keys for those servers |
| `HF_TOKEN`, `HUGGINGFACE_TOKEN` | `src/ai_api/server.rs:730`; `src/bin/aimodel.rs:1030`, `1032` | HuggingFace credential. Either one marks it configured |
| `PATH`, `PATHEXT` | `src/providers/ontology.rs:2487`, `src/builtins.rs:52210` | Where commands are looked up |
| `HOME`, `USERPROFILE` | `src/builtins.rs:14015`; `src/ai_api/config.rs:344`; `src/ai_api/storage.rs:108` | Home directory, and the fallback for config and model paths |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME` | `src/ai_api/config.rs:342`; `src/ai_api/storage.rs:106`, `122`; `src/safety.rs:5961` | Base directories for `ai-models` storage, and for the user git config the remote check reads |
| `APPDATA` | `src/ai_api/config.rs:348`; `src/ai_api/storage.rs:112`, `128`; `src/ai_api/providers.rs:857` | Windows fallback for the `ai-models` directories |
| `EDITOR` | `src/config.rs:376` | Default external editor |
| `NO_COLOR` | `src/config.rs:1499` | Any non-empty value turns colour off |
| `SSL_CERT_FILE` | `src/certificates.rs:303` | PEM bundle of trusted CA certificates. Size-limited |
| `USER`, `USERNAME` | `src/security.rs:94-160`; `src/prompt.rs:418` | Principal name for audit records and the prompt |
| `GIT_DIR`, `GIT_CONFIG`, `GIT_CONFIG_GLOBAL`, `GIT_CONFIG_SYSTEM`, `GIT_CONFIG_PARAMETERS`, `GIT_CONFIG_COUNT` | `src/safety.rs:5791` | Any of them set makes the remote check for `git fetch`, `pull` and `push` refuse, rather than read `.git/config` |
| `GIT_CONFIG_NOSYSTEM` | `src/safety.rs:5948` | Any value other than `0`, `false`, `no`, `off` skips the system git config files in that check |
| `DOCKER_HOST`, `DOCKER_CONTEXT` | `src/safety.rs:5169` | Set means a remote daemon, so the registry-mirror check refuses rather than read the local config |
| `CONTAINER_HOST`, `CONTAINER_CONNECTION` | `src/safety.rs:5177` | The same refusal for podman |
| `CONTAINERS_REGISTRIES_CONF` | `src/safety.rs:5148`, `5180` | Set means a registries file the check does not read, so the call is refused |
| `container` | `src/builtins.rs:52089` | Read by the container-detection builtin, with `/.dockerenv` |
| `VIRTUAL_ENV` | `src/builtins.rs:52097` | Reported by the virtualenv query |
| `DISPLAY`, `WAYLAND_DISPLAY` | `src/builtins.rs:53626` | Reports `has_gui` on Linux |
| `ProgramData` | `src/plugins.rs:830`; `src/safety.rs:5950` | Windows system-wide directory for plugins and the git system config |
| `SUDO_USER`, `SUDO_UID` | `src/accounts/macos.rs:706`, `707` | Finds the invoking user for macOS account operations |
| `SystemRoot`, `SystemDrive`, `COMSPEC`, `COMPUTERNAME`, `USERDOMAIN`, `PSModulePath`, `PROCESSOR_ARCHITECTURE`, `WindowsSdkDir`, `WindowsSDKVersion`, `VSINSTALLDIR`, `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT`, `CUDNN_PATH`, `SHELL`, `TERM`, `LANG`, `LC_ALL`, `TZ`, `CARGO_HOME`, `RUSTUP_HOME`, `PYTHONPATH`, `GOPATH` | `src/builtins.rs` (platform and toolchain detection, `52540`), `src/acl_mount.rs:1174`, `src/prompt.rs:425` | Platform, shell and toolchain detection and reporting. Not safety settings |

Child processes: the shell removes its own `IRONSHELL_*` variables from a
child's environment, except the mode, policy, workspace, limit, allowlist, FIPS,
deterministic, RBAC and audit settings, which pass through (`CONFINING_ENV`,
`strip_ironshell_env`, `src/safety.rs:1976`, `2017`). In agent mode, the child
also loses secret-named variables unless they are in `IRONSHELL_CHILD_ENV_KEEP`,
and the child receives `IRONSHELL_MODE=agent`. A variable set explicitly on the
child's command is kept.

### Local evaluation records (`IRONSHELL_RECORD`)

Off by default. With `IRONSHELL_RECORD=1`, each top-level evaluation appends one
JSON line to `record.jsonl` in the session directory (`IRONSHELL_SESSION_DIR`, or
the per-project cache directory). Nothing is sent anywhere. Each line holds a
version, a timestamp, a per-process sequence number, the surface (`c`, `file`,
`repl`, `api`, `sh`), whether agent mode was on, the names of the builtins called at the
top level, a 64-bit FNV-1a hash of the source, the duration, and on failure the
error code, whether it is retryable, and whether it was a refusal. Outputs and
argument values are not stored.

Each Agent API evaluation is one attempt: `eval`, `call`, `pipeline`, and their
streamed and WebSocket forms. A pipeline is recorded as the
IronShell source its steps compile to. A POSIX-subset script (`ish -b`, `--bash`
input, and `.sh` files it runs natively) has surface `sh`. Its heads are the first
word of each command, or an empty string when that word is not a literal. Its `ok`
means exit status 0, and a non-zero status has no code, because the script's own
exit is not an IronShell failure. A failure that a `try { } catch { }` handles is
not an attempt, so it is not recorded; only a failure that leaves the evaluation
is.

`IRONSHELL_RECORD=full` also stores the source text in a `cmd` field. Use it only
when that text is safe to keep, since it can contain secrets.

`record_summary()` reads the current session's file and reports attempts,
failures, refusals, failures by code, retries, recoveries and first-try successes.
Any other value of the variable records nothing.

### Setting Environment Variables

In your shell config (`.bashrc`, `.zshrc`, etc.):

```bash
export OPENAI_API_KEY="sk-your-key-here"
export IRONSHELL_AI="openai"
export AGENT_ALLOW_CMDS="ls,cat,grep,find,curl,http_get"
```

Or in IronShell:

```ironshell
env_set("OPENAI_API_KEY", "sk-your-key-here")
```

## AI Provider Setup

`IRONSHELL_AI` chooses the backend that `ai()` uses. The `[ai]` keys in `config.toml` do not choose it. Set the key variable for the provider, then select the provider with `IRONSHELL_AI`. `IRONSHELL_AI=<name>:<model>` also sets the model for the providers listed below.

### OpenAI

```bash
export OPENAI_API_KEY="sk-..."
export OPENAI_MODEL="gpt-4o"   # optional
```

With `IRONSHELL_AI=openai`, the model is `OPENAI_MODEL`, or `gpt-4o-mini` when that is unset.

### Anthropic (Claude)

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

With `IRONSHELL_AI=anthropic` (or `claude`) the model is `claude-sonnet-4-20250514`. No variable changes it. Use `IRONSHELL_AI=anthropic:<model>` to choose another model.

### Google (Gemini)

```bash
export GOOGLE_API_KEY="AIza..."
```

With `IRONSHELL_AI=google` (or `gemini`) the model is `gemini-2.5-flash`. Use `IRONSHELL_AI=google:<model>` to choose another model.

### Local Models (Ollama)

1. Install Ollama: https://ollama.ai
2. Pull a model: `ollama pull llama3`
3. Use in IronShell:

```ironshell
IRONSHELL_AI=ollama ish -c 'ai("Hello")'
```

`OLLAMA_URL` sets the server (default `http://localhost:11434`), and `OLLAMA_MODEL` sets the model (default `llama3`).

### Other providers

| `IRONSHELL_AI` value | Key variable | Model when none is given |
| --- | --- | --- |
| `deepseek` | `DEEPSEEK_API_KEY` | `deepseek-chat` |
| `groq` | `GROQ_API_KEY` | `llama-3.3-70b-versatile` |
| `together` | `TOGETHER_API_KEY` | `meta-llama/Llama-3.3-70B-Instruct-Turbo` |
| `fireworks` | `FIREWORKS_API_KEY` | `accounts/fireworks/models/llama-v3p3-70b-instruct` |
| `perplexity` | `PERPLEXITY_API_KEY` | `sonar` |
| `xai` (`grok`) | `XAI_API_KEY` | `grok-3` |
| `openrouter` | `OPENROUTER_API_KEY` | `openai/gpt-4o` |
| `mistral` | `MISTRAL_API_KEY` | `mistral-large-latest` |
| `cohere` | `COHERE_API_KEY` | `command-a` |
| `azure` | `AZURE_OPENAI_API_KEY`, endpoint from `AZURE_OPENAI_ENDPOINT` | `gpt-4o` |
| `vllm` | none (no key is sent) | `default`, at `VLLM_BASE_URL` (default `http://localhost:8000/v1`) |
| `llamacpp` (`llama_cpp`) | none | `default`, at `http://localhost:8080/v1` |
| `local` (`lmstudio`) | none (no key is sent) | `default`, at `http://localhost:1234/v1` |

### Multiple Providers

`ai()` takes no model argument. Choose the provider per run with `IRONSHELL_AI`:

```bash
IRONSHELL_AI=openai ish -c 'ai("Query")'
IRONSHELL_AI=ollama ish -c 'ai("Query")'
IRONSHELL_AI=openrouter ish -c 'ai("Query")'
```

## Model Storage

AI model files and their index are stored in an `ai-models` directory, not in the IronShell data directory. `IRONSHELL_DATA_HOME` does not move them.

- **Data directory** (models, metadata, cache, index): `$XDG_DATA_HOME/ai-models` if `XDG_DATA_HOME` is set, else `$HOME/.local/share/ai-models` if `HOME` is set, else `%APPDATA%\ai-models`.
- **Config directory** (`config.toml`, `providers.toml`, `aliases.toml`): `$XDG_CONFIG_HOME/ai-models`, else `$HOME/.config/ai-models`, else `%APPDATA%\ai-models`.

Inside the data directory:

- `models/gguf`, `models/safetensors`, `models/pytorch` and `models/onnx` hold the model files, one directory per format.
- `metadata/` holds model metadata.
- `cache/` holds temporary files and downloads.
- `index.json` is the model index.

The `storage` section of the `ai-models` `config.toml` can set `custom_data_dir` and `custom_config_dir` to use other directories. It can also set `max_cache_size_gb` and `auto_cleanup_days`. The same file holds the `[server]`, `[providers]`, `[security]` and `[logging]` sections for the model API. `IRONSHELL_MODEL_DIR` adds one more directory that is scanned for local models.

## Command-Line Options

```text
ish [OPTIONS] [FILE]
ish <SUBCOMMAND>
```

With no file, no `-c` and no subcommand, `ish` starts the REPL.

| Option | Meaning |
| --- | --- |
| `FILE` | Run this script |
| `-c, --command <CMD>` | Run one command string and exit |
| `-b, --bash` | Bash compatibility. With no `FILE` or `-c`, read the script from stdin |
| `-z, --zsh` | Zsh compatibility. With no `FILE` or `-c`, read from stdin |
| `-p, --pwsh` | PowerShell compatibility. With no `FILE` or `-c`, read from stdin |
| `-a, --agentic` | Agentic syntax. With no `FILE` or `-c`, read from stdin |
| `--agent` | Agent mode: default-deny for dangerous effects (`IRONSHELL_MODE=agent`) |
| `--workspace <DIR>` | Workspace root that confines writes (`IRONSHELL_WORKSPACE`) |
| `--policy <strict\|permissive>` | Safety policy (`IRONSHELL_POLICY`) |
| `--budget <N>` | Token budget for output (`IRONSHELL_TOKEN_BUDGET`) |
| `--explain` | Print the static effect report as JSON instead of running. Use with `-c`, a file, or stdin |
| `--deterministic` | Canonical, byte-stable JSON output (`IRONSHELL_DETERMINISTIC=1`) |
| `-h, --help` | Print help |
| `-V, --version` | Print the version |

Subcommands:

| Subcommand | Alias | Meaning |
| --- | --- | --- |
| `tui` | | Start the terminal UI |
| `ai` | `model` | AI models: `serve [--host 127.0.0.1] [--port 8080] [--cors]`, `list [--provider <P>] [--local]` (alias `ls`), `download <MODEL>`, `config`, `keys store\|get\|delete\|list` (alias `key`) |
| `mcp` | `mcp-server` | MCP server: `serve [--host 127.0.0.1] [--port 3001] [--cors] [--safety caution] [--admin]`, `tools [--category <C>]`, `stdio` (JSON-RPC over stdin and stdout) |
| `agent` | `agent-api` | Agent API: `serve [--host 127.0.0.1] [--port 3002] [--cors] [--token <T>] [--request-timeout 300]`, `execute [REQUEST]` (alias `exec`; reads stdin when no request is given), `schema [--format compact] [--output <F>] [--module <M>] [-q <TEXT>]`, `builtins [--category <C>] [--json]` (alias `ls`), `interactive` |
| `assist` | `ai-assist` | Natural language to IronShell code: `[QUERY] [-x] [-i] [-c] [-s]`. `-x` runs the result, `-i` is interactive, `-c` adds system context, `-s` suggests commands |

`ish agent serve` listens on port 3002 by default, and `ish ai serve` on port 8080. The API token can come from `IRONSHELL_API_TOKEN` instead of `--token`, which keeps it out of the process list.

## Startup Scripts

IronShell does not run a startup script. `config_path()` reports the path of `init.ish` in the config directory, but nothing executes that file. No code reads `login.ish`.
