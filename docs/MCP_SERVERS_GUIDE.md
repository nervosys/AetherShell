# MCP Servers Guide for AetherShell

## 🔧 What Are MCP Servers?

**MCP (Model Context Protocol) Servers** are local services that provide AI agents with **safe, controlled access** to tools and resources. Instead of giving agents raw command execution (dangerous!), MCP servers expose specific, validated operations through a standardized protocol.

### Why MCP Servers > Raw Commands

| Aspect             | Raw Commands                | MCP Servers                         |
| ------------------ | --------------------------- | ----------------------------------- |
| **Safety**         | ❌ Agents can run `rm -rf /` | ✅ Only allowed operations           |
| **Structure**      | ❌ Parse text output         | ✅ Structured JSON responses         |
| **Validation**     | ❌ No input validation       | ✅ Parameter validation              |
| **Auditing**       | ❌ Hard to track             | ✅ Full request/response logs        |
| **Isolation**      | ❌ Full system access        | ✅ Scoped to allowed paths/resources |
| **Cross-platform** | ❌ OS-specific commands      | ✅ Consistent API                    |

### What Makes AetherShell's MCP Integration Unique?

**NO OTHER SHELL HAS THIS!** AetherShell is the only shell where:
- AI agents can use MCP servers natively
- Multiple MCP servers can run simultaneously
- Agents can coordinate across different tool providers
- Type-safe pipeline integration with MCP tools

---

## 📦 Built-In MCP Servers

AetherShell includes several built-in MCP servers ready to use:

### 1. Filesystem Server

Provides safe file operations with path restrictions.

**Configuration:**
```ae
fs_server := mcp_server_start({
  name: "filesystem",
  type: "builtin",
  config: {
    allowed_paths: [
      "./",                  # Current directory
      "~/Documents",         # Documents folder
      "~/Projects"           # Projects folder
    ],
    read_only: false,        # Allow writes
    max_file_size: 10485760, # 10MB limit
    excluded_patterns: [
      ".git/",               # No git internals
      "node_modules/",       # No dependencies
      "*.exe",               # No executables
      "*.dll",               # No libraries
      "target/"              # No build artifacts
    ]
  }
})
```

**Available Tools:**
- `mcp:read_file` - Read file contents
- `mcp:write_file` - Write to file
- `mcp:list_dir` - List directory contents
- `mcp:search_files` - Search for files by pattern
- `mcp:file_stats` - Get file metadata
- `mcp:create_dir` - Create directory
- `mcp:delete_file` - Delete file (if not read_only)

**Example Usage:**
```ae
agent := agent_with_mcp(
  "File organizer",
  ["mcp:read_file", "mcp:list_dir", "mcp:search_files"],
  fs_server.endpoint
)

# Find all TODO comments
todos := agent.call_mcp_tool("search_files", {
  path: "./src",
  pattern: "TODO:|FIXME:|HACK:",
  file_types: [".rs", ".ae", ".md"]
})

# Organize by priority
organized := agent.execute({
  task: "Categorize TODOs by priority and create summary",
  context: todos
})
```

### 2. Git Server

Provides Git repository operations.

**Configuration:**
```ae
git_server := mcp_server_start({
  name: "git",
  type: "builtin",
  config: {
    allowed_repos: [
      "./",                  # Current repo
      "~/Projects/*"         # All project repos
    ],
    allowed_operations: [
      "status",              # Git status
      "log",                 # View history
      "diff",                # View changes
      "branch",              # Branch info
      "commit"               # Commit changes
    ],
    safe_mode: true          # Prevents force push, rebase, etc.
  }
})
```

**Available Tools:**
- `mcp:git_status` - Repository status
- `mcp:git_log` - Commit history
- `mcp:git_diff` - View changes
- `mcp:git_branch` - Branch information
- `mcp:git_commit` - Commit changes
- `mcp:git_blame` - Show file authors

**Example Usage:**
```ae
agent := agent_with_mcp(
  "Git assistant",
  ["mcp:git_status", "mcp:git_diff", "mcp:git_log"],
  git_server.endpoint
)

# Analyze recent changes
analysis := agent.execute({
  task: "Review last 10 commits and identify potential issues",
  tools: ["mcp:git_log", "mcp:git_diff"]
})

# Suggest commit message
suggestion := agent.execute({
  task: "Analyze staged changes and suggest commit message",
  tools: ["mcp:git_diff"]
})
```

### 3. Docker Server

Provides Docker container management.

**Configuration:**
```ae
docker_server := mcp_server_start({
  name: "docker",
  type: "builtin",
  config: {
    docker_host: "unix:///var/run/docker.sock",  # Docker socket
    allowed_operations: [
      "ps",                  # List containers
      "inspect",             # Container details
      "logs",                # View logs
      "stats"                # Resource usage
    ],
    allow_start_stop: false  # Safe mode - no start/stop
  }
})
```

**Available Tools:**
- `mcp:docker_ps` - List containers
- `mcp:docker_inspect` - Container details
- `mcp:docker_logs` - View container logs
- `mcp:docker_stats` - Resource usage
- `mcp:docker_images` - List images
- `mcp:docker_networks` - List networks

**Example Usage:**
```ae
agent := agent_with_mcp(
  "Container monitor",
  ["mcp:docker_ps", "mcp:docker_stats", "mcp:docker_logs"],
  docker_server.endpoint
)

# Health check
health := agent.execute({
  task: "Check all running containers for issues (high CPU, memory leaks, errors)",
  tools: ["mcp:docker_ps", "mcp:docker_stats", "mcp:docker_logs"]
})
```

### 4. Web Server

Provides web scraping and API access.

**Configuration:**
```ae
web_server := mcp_server_start({
  name: "web",
  type: "builtin",
  config: {
    allowed_domains: [
      "*.github.com",        # GitHub
      "*.stackoverflow.com", # Stack Overflow
      "api.example.com"      # Your API
    ],
    rate_limit: 10,          # Requests per minute
    timeout: 30,             # Seconds
    user_agent: "AetherShell-MCP/1.0"
  }
})
```

**Available Tools:**
- `mcp:fetch_url` - Fetch URL content
- `mcp:scrape_page` - Extract data from HTML
- `mcp:api_call` - Make API requests
- `mcp:download_file` - Download files

**Example Usage:**
```ae
agent := agent_with_mcp(
  "Web researcher",
  ["mcp:fetch_url", "mcp:scrape_page"],
  web_server.endpoint
)

# Research GitHub repos
research := agent.execute({
  task: "Find top 10 Rust CLI projects on GitHub and summarize",
  tools: ["mcp:fetch_url", "mcp:scrape_page"]
})
```

---

## ☁️ Cloud MCP Servers

### AWS Server

**Configuration:**
```ae
aws_server := mcp_server_start({
  name: "aws",
  type: "cloud",
  provider: "aws",
  config: {
    region: "us-east-1",
    services: ["s3", "ec2", "lambda", "cloudwatch", "dynamodb"],
    credentials_source: "environment",  # From AWS_* env vars
    read_only: true                     # Safe mode
  }
})
```

**Available Tools:**
- **S3**: `mcp:s3_list_buckets`, `mcp:s3_list_objects`, `mcp:s3_get_object`, `mcp:s3_upload`
- **EC2**: `mcp:ec2_describe_instances`, `mcp:ec2_describe_security_groups`
- **Lambda**: `mcp:lambda_list_functions`, `mcp:lambda_invoke`
- **CloudWatch**: `mcp:cloudwatch_get_metrics`, `mcp:cloudwatch_get_logs`

**Example Usage:**
```ae
# Cost optimization agent
aws_agent := agent_with_mcp(
  "AWS cost optimizer",
  ["mcp:ec2_describe_instances", "mcp:cloudwatch_get_metrics"],
  aws_server.endpoint
)

optimization := aws_agent.execute({
  task: "Find underutilized EC2 instances and suggest cost savings"
})
```

### Azure Server

**Configuration:**
```ae
azure_server := mcp_server_start({
  name: "azure",
  type: "cloud",
  provider: "azure",
  config: {
    subscription_id: "your-sub-id",
    services: ["vm", "storage", "functions", "monitor"],
    credentials_source: "cli",  # From Azure CLI
    read_only: true
  }
})
```

### Google Cloud Server

**Configuration:**
```ae
gcp_server := mcp_server_start({
  name: "gcp",
  type: "cloud",
  provider: "gcp",
  config: {
    project_id: "your-project",
    services: ["compute", "storage", "functions", "monitoring"],
    credentials_file: "~/.gcp/credentials.json",
    read_only: true
  }
})
```

---

## 🗄️ Database MCP Servers

### PostgreSQL Server

**Configuration:**
```ae
db_server := mcp_server_start({
  name: "postgres",
  type: "database",
  config: {
    connection_string: "postgresql://user:pass@localhost:5432/mydb",
    read_only: true,           # No INSERT/UPDATE/DELETE
    allowed_schemas: ["public", "analytics"],
    query_timeout: 30,         # Seconds
    max_rows: 10000,           # Result limit
    explain_queries: true      # Show query plans
  }
})
```

**Available Tools:**
- `mcp:db_query` - Execute SELECT queries
- `mcp:db_schema` - Get schema information
- `mcp:db_tables` - List tables
- `mcp:db_columns` - Get column info
- `mcp:db_explain` - Get query execution plan

**Example Usage:**
```ae
agent := agent_with_mcp(
  "Database analyst",
  ["mcp:db_query", "mcp:db_schema", "mcp:db_explain"],
  db_server.endpoint
)

# Analyze data
analysis := agent.execute({
  task: "Find patterns in user behavior from the events table",
  tools: ["mcp:db_query", "mcp:db_explain"]
})
```

### MySQL, SQLite, MongoDB

Similar configurations available for other database types.

---

## 🛠️ Custom MCP Servers

### Create Your Own in Python

```python
#!/usr/bin/env python3
# custom_mcp_server.py
import json
from http.server import HTTPServer, BaseHTTPRequestHandler

class MCPHandler(BaseHTTPRequestHandler):
    def do_POST(self):
        content_length = int(self.headers["Content-Length"])
        body = json.loads(self.rfile.read(content_length))
        
        tool_name = body.get("tool")
        params = body.get("params", {})
        
        # Implement your tools
        if tool_name == "analyze_sentiment":
            result = self.analyze_sentiment(params)
        elif tool_name == "translate_text":
            result = self.translate_text(params)
        else:
            result = {"error": f"Unknown tool: {tool_name}"}
        
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps(result).encode())
    
    def analyze_sentiment(self, params):
        text = params.get("text")
        # Your sentiment analysis logic
        return {
            "sentiment": "positive",
            "confidence": 0.95,
            "emotions": ["joy", "excitement"]
        }
    
    def translate_text(self, params):
        text = params.get("text")
        target_lang = params.get("target_language")
        # Your translation logic
        return {
            "translated": "Bonjour le monde",
            "source_language": "en",
            "target_language": target_lang
        }

if __name__ == "__main__":
    server = HTTPServer(("localhost", 8080), MCPHandler)
    print("Custom MCP server running on http://localhost:8080")
    server.serve_forever()
```

### Register Custom Server

```ae
# Register your custom MCP server
custom_server := mcp_server_start({
  name: "nlp_tools",
  type: "external",
  endpoint: "http://localhost:8080",
  tools: [
    {
      name: "analyze_sentiment",
      description: "Analyze text sentiment",
      parameters: ["text"]
    },
    {
      name: "translate_text",
      description: "Translate text to another language",
      parameters: ["text", "target_language"]
    }
  ]
})

# Use it
agent := agent_with_mcp(
  "NLP assistant",
  ["mcp:analyze_sentiment", "mcp:translate_text"],
  custom_server.endpoint
)
```

---

## 🔐 Security Best Practices

### 1. Use Read-Only Mode

```ae
# GOOD: Read-only for safety
db_server := mcp_server_start({
  name: "postgres",
  type: "database",
  config: {
    read_only: true  # ✅ Safe
  }
})

# RISKY: Write access
db_server := mcp_server_start({
  name: "postgres",
  type: "database",
  config: {
    read_only: false  # ⚠️ Agents can modify data
  }
})
```

### 2. Restrict Paths and Domains

```ae
# GOOD: Specific allowed paths
fs_server := mcp_server_start({
  name: "filesystem",
  config: {
    allowed_paths: ["./project", "~/Documents/work"]  # ✅ Limited scope
  }
})

# RISKY: Too broad
fs_server := mcp_server_start({
  name: "filesystem",
  config: {
    allowed_paths: ["/"]  # ⚠️ Entire filesystem
  }
})
```

### 3. Use Excluded Patterns

```ae
fs_server := mcp_server_start({
  name: "filesystem",
  config: {
    allowed_paths: ["./"],
    excluded_patterns: [
      ".git/",           # No git internals
      ".env",            # No secrets
      "*.key",           # No keys
      "*.pem",           # No certificates
      "node_modules/",   # No dependencies
      "target/"          # No build artifacts
    ]
  }
})
```

### 4. Rate Limiting

```ae
web_server := mcp_server_start({
  name: "web",
  config: {
    rate_limit: 10,        # Max 10 requests per minute
    timeout: 30,           # 30 second timeout
    max_response_size: 10485760  # 10MB max
  }
})
```

### 5. Safe Mode for Critical Services

```ae
git_server := mcp_server_start({
  name: "git",
  config: {
    safe_mode: true,       # Prevents destructive operations
    allowed_operations: ["status", "log", "diff"]  # Whitelist only
  }
})
```

---

## 🚀 Advanced Patterns

### Multi-Server Agent

```ae
# Agent with access to multiple MCP servers
devops_agent := agent_with_mcp(
  "Full-stack DevOps assistant",
  [
    # Filesystem
    "mcp:read_file",
    "mcp:list_dir",
    # Git
    "mcp:git_status",
    "mcp:git_log",
    # Docker
    "mcp:docker_ps",
    "mcp:docker_logs",
    # AWS
    "mcp:ec2_describe",
    "mcp:s3_list"
  ],
  [
    fs_server.endpoint,
    git_server.endpoint,
    docker_server.endpoint,
    aws_server.endpoint
  ]
)
```

### Agent Swarm with MCP

```ae
# Multiple agents sharing MCP servers
monitoring_swarm := swarm([
  {
    id: "app_monitor",
    model: "openai:gpt-4",
    role: "Monitor application logs",
    tools: ["mcp:docker_logs", "mcp:db_query"]
  },
  {
    id: "infra_monitor",
    model: "anthropic:claude-3-opus",
    role: "Monitor infrastructure",
    tools: ["mcp:ec2_describe", "mcp:cloudwatch_get"]
  },
  {
    id: "cost_monitor",
    model: "openai:gpt-4o-mini",
    role: "Monitor costs",
    tools: ["mcp:cloudwatch_get", "mcp:s3_list"]
  }
], "router", [docker_server.endpoint, aws_server.endpoint, db_server.endpoint])
```

### MCP Server Health Monitoring

```ae
# Monitor all MCP servers
servers := [fs_server, git_server, docker_server, aws_server, db_server]

servers | map(fn(server) => {
  health := mcp_server_health(server.endpoint)
  {
    name: server.name,
    status: health.status,
    uptime: health.uptime_seconds,
    requests: health.total_requests,
    avg_response_time: health.avg_response_time_ms,
    errors: health.error_count
  }
}) | where(fn(s) => s.status != "healthy")
  | each(fn(s) => print("⚠️ ${s.name} is ${s.status}"))
```

---

## 📊 Comparison: MCP vs Alternatives

| Approach         | Safety      | Structure      | Validation | Cross-platform | AetherShell Support |
| ---------------- | ----------- | -------------- | ---------- | -------------- | ------------------- |
| **MCP Servers**  | ✅ Excellent | ✅ JSON         | ✅ Yes      | ✅ Yes          | ✅ Native            |
| Raw Commands     | ❌ Dangerous | ❌ Text parsing | ❌ No       | ❌ OS-specific  | ⚠️ Basic             |
| Direct API Calls | ⚠️ Varies    | ✅ JSON         | ⚠️ Manual   | ⚠️ Varies       | ⚠️ Manual            |
| Tool Allowlist   | ⚠️ Limited   | ❌ Text         | ❌ No       | ❌ OS-specific  | ✅ Yes               |

**Winner: MCP Servers** — Best balance of safety, structure, and flexibility!

---

## 🎯 Real-World Use Cases

### 1. DevOps Automation

```ae
# Automated deployment pipeline
deploy_agent := agent_with_mcp(
  "Deployment agent",
  ["mcp:git_status", "mcp:docker_build", "mcp:ec2_deploy", "mcp:cloudwatch_monitor"],
  [git_server.endpoint, docker_server.endpoint, aws_server.endpoint]
)

deploy_agent.execute({
  task: "Deploy latest changes to production",
  steps: [
    "Check git status and latest commit",
    "Build Docker image",
    "Push to ECR",
    "Update ECS task definition",
    "Monitor deployment via CloudWatch"
  ]
})
```

### 2. Security Auditing

```ae
# Security audit agent
security_agent := agent_with_mcp(
  "Security auditor",
  ["mcp:search_files", "mcp:git_log", "mcp:ec2_describe", "mcp:db_schema"],
  [fs_server.endpoint, git_server.endpoint, aws_server.endpoint, db_server.endpoint]
)

audit := security_agent.execute({
  task: "Perform security audit",
  checks: [
    "Find hardcoded secrets in code",
    "Check for exposed credentials in git history",
    "Audit AWS security groups",
    "Review database permissions"
  ]
})
```

### 3. Data Analysis Pipeline

```ae
# Data analysis agent
analyst_agent := agent_with_mcp(
  "Data analyst",
  ["mcp:db_query", "mcp:s3_get_object", "mcp:write_file"],
  [db_server.endpoint, aws_server.endpoint, fs_server.endpoint]
)

analysis := analyst_agent.execute({
  task: "Analyze Q4 sales data and create report",
  steps: [
    "Query sales data from database",
    "Download additional data from S3",
    "Perform statistical analysis",
    "Generate visualizations",
    "Write comprehensive report"
  ]
})
```

---

## 🔍 Debugging MCP Servers

### Enable Logging

```ae
mcp_server_start({
  name: "filesystem",
  config: {...},
  logging: {
    level: "debug",        # trace, debug, info, warn, error
    file: "mcp_fs.log",
    console: true
  }
})
```

### Test Tools Manually

```ae
# Test a specific tool
result := mcp_call_tool(
  server.endpoint,
  "read_file",
  {path: "./test.txt"}
)
print(result)
```

### Check Server Status

```ae
status := mcp_server_status(server.endpoint)
print("Status: ${status.state}")
print("Uptime: ${status.uptime_seconds}s")
print("Requests: ${status.total_requests}")
print("Errors: ${status.error_count}")
```

---

## 📚 Next Steps

1. **Try the examples**: Run `ae examples/16_mcp_servers.ae`
2. **Start simple**: Begin with filesystem or Git server
3. **Build custom servers**: Create tools specific to your needs
4. **Deploy production**: Use MCP servers in your workflows
5. **Share your servers**: Contribute to the MCP ecosystem!

---

## 🎓 Learn More

- **Examples**: `examples/16_mcp_servers.ae` - Comprehensive MCP server examples
- **Specification**: [MCP Protocol Spec](https://modelcontextprotocol.io)
- **Community**: Share your MCP servers with the AetherShell community
- **Documentation**: See `docs/AI_PROTOCOLS_FINAL_REPORT.md` for protocol details

---

**Remember**: MCP servers are the SAFE way to give AI agents access to tools. Always use read-only mode, restrict paths, and validate inputs! 🔒
