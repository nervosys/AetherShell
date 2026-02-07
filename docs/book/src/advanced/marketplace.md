# Marketplace

The AetherShell Marketplace is a registry for sharing and discovering pre-built AI agents.

## Browsing

### Search

```aethershell
# Search for agents
marketplace_search "code review"
# [
#   { name: "code-reviewer", author: "aethershell", version: "1.0.0", downloads: 500 },
#   { name: "pr-analyzer", author: "community", version: "0.8.0", downloads: 120 },
#   ...
# ]
```

The web dashboard provides a visual marketplace browser at the **Marketplace** tab with filtering, sorting, and one-click installation.

### API Endpoints

| Method | Endpoint | Description |
|--------|----------|-------------|
| GET | `/api/v1/marketplace/search?q=...` | Search agents |
| GET | `/api/v1/marketplace/agents` | List all agents |
| POST | `/api/v1/marketplace/install` | Install an agent |
| POST | `/api/v1/marketplace/uninstall` | Uninstall an agent |
| POST | `/api/v1/marketplace/publish` | Publish an agent |

## Installing Agents

```aethershell
marketplace_install "code-reviewer"
marketplace_install "code-reviewer" "1.0.0"    # Specific version
```

Via the API:

```bash
curl -X POST http://localhost:3000/api/v1/marketplace/install \
  -H "Content-Type: application/json" \
  -d '{"name": "code-reviewer", "version": "1.0.0"}'
```

## Uninstalling

```aethershell
marketplace_uninstall "code-reviewer"
```

## Publishing

Share your agents with the community:

```aethershell
marketplace_publish {
  name: "my-agent",
  description: "Analyzes Rust code for common patterns",
  system_prompt: "You are an expert Rust developer...",
  tools: ["cat", "grep", "ls"],
  model: "openai:gpt-4o-mini",
  tags: ["rust", "code-analysis"]
}
```

### Via the Dashboard

1. Open the **Marketplace** tab
2. Click **Publish Agent**
3. Fill in the form: name, description, system prompt, tools, model
4. Click **Publish**

## Agent Structure

Marketplace agents include:

| Field | Description |
|-------|-------------|
| `name` | Unique agent identifier |
| `description` | What the agent does |
| `author` | Publisher name |
| `version` | Semantic version |
| `system_prompt` | Agent's system instructions |
| `tools` | Allowed builtin tools |
| `model` | Recommended model URI |
| `tags` | Category tags for discovery |
| `downloads` | Download count |
| `stars` | Community rating |

## Local Registry

The `RegistryClient` manages local state:

- **Search**: Full-text search against the registry (local fallback when offline)
- **Install**: Downloads agent config and registers locally
- **Cache**: Installed agents stored in `~/.aethershell/marketplace/`

## Dashboard Integration

The web dashboard's Marketplace page provides:

- **Search bar** with real-time results
- **Category filtering** and sorting
- **Install/Uninstall buttons** with loading states
- **Publish dialog** with form validation
- **Agent cards** showing name, description, version, downloads, and verified badges
