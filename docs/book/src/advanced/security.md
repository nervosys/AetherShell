# Security

AetherShell includes multiple security layers for safe AI agent execution and enterprise deployments.

## Agent Sandboxing

Agents run in a restricted sandbox with configurable limits:

| Limit | Default | Description |
|-------|---------|-------------|
| Timeout | 30 seconds | Maximum execution time |
| Output | 10 MB | Maximum output size |
| Memory | 512 MB | Maximum memory usage |

### Command Allowlist

Restrict which commands agents can execute:

```bash
export AGENT_ALLOW_CMDS="ls,cat,grep,wc,find"
```

Only listed commands will be available to agents. The allowlist is enforced by `validate_command()` which:
- Checks the tool name against the allowlist
- Blocks shell metacharacters (`;`, `|`, `&&`, `||`, `` ` ``, `$()`)
- Prevents path traversal in arguments

### Prompt Validation

Agent goals are validated before execution:
- Maximum 4,000 characters
- Injection pattern detection
- Sanitization of control characters

### Rate Limiting

| Operation | Limit |
|-----------|-------|
| Agent plans | 10 per minute |
| Agent executions | 5 per minute |

## RBAC (Role-Based Access Control)

Enterprise deployments support RBAC for fine-grained access control:

```aethershell
# Create roles
role_create "developer" { permissions: ["read", "execute", "agent"] }
role_create "admin" { permissions: ["read", "write", "execute", "agent", "admin"] }

# Grant roles to users
role_grant "alice" "developer"
role_grant "bob" "admin"

# Check permissions
check_permission "alice" "execute"    # true
check_permission "alice" "admin"      # false

# List roles
roles_list
user_roles "alice"    # ["developer"]
```

## Audit Logging

Track all operations for compliance:

```aethershell
# View recent audit events
audit_log 20
# [{ timestamp: "...", user: "alice", action: "execute", details: "ls src/" }, ...]

# Query audit log
audit_query { user: "bob", action: "agent", since: "2024-01-01" }

# Export audit log
audit_export "audit_2024.json"

# Audit statistics
audit_stats
# { total_events: 1500, users: 3, actions: { execute: 800, agent: 200, ... } }
```

## SSO (Single Sign-On)

Integrate with enterprise identity providers:

```aethershell
sso_init { provider: "oauth2", client_id: "...", issuer: "https://auth.example.com" }
sso_auth                # Initiate authentication flow
sso_validate            # Validate current session
sso_status              # Check SSO status
sso_logout              # End session
```

## Compliance

Run compliance checks against security policies:

```aethershell
compliance_check
# { passed: 12, failed: 2, warnings: 3 }

compliance_report
# Generates detailed compliance report with remediation steps
```

## Cryptographic Operations

```aethershell
# Hashing
crypto_hash "sha256" "Hello, world!"
crypto_hash_file "sha256" "document.pdf"

# Random data
crypto_random_bytes 32
crypto_uuid                    # Generate UUID v4

# Encoding
crypto_base64_encode "hello"   # "aGVsbG8="
crypto_base64_decode "aGVsbG8="

# Password hashing
let hash = crypto_password_hash "my-password"
crypto_password_verify "my-password" hash   # true

# JWT
crypto_jwt_decode token
```

## API Key Management

API keys are stored securely:

- **Keyring integration**: Keys stored in OS keyring when available
- **Environment fallback**: `OPENAI_API_KEY`, `AETHER_API_KEY` environment variables
- **No logging**: API keys are never written to logs or audit trails

```aethershell
# Keys loaded automatically from keyring or environment
# SecureApiConfig::from_keyring_or_env() handles the priority
```

## Best Practices

1. **Always set `AGENT_ALLOW_CMDS`** in production — never give agents unrestricted access
2. **Use RBAC** for multi-user deployments
3. **Enable audit logging** for compliance-sensitive environments
4. **Review agent plans** with `dry_run: true` before allowing execution of destructive operations
5. **Rotate API keys** regularly and use the keyring for storage
6. **Set appropriate rate limits** to prevent abuse
