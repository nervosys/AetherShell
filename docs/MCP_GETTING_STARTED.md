# MCP Server Getting Started Guide

## Overview

Model Context Protocol (MCP) servers provide tools and capabilities that can be used by AI agents in AetherShell. This guide will help you set up, configure, and use MCP servers effectively.

## What are MCP Servers?

MCP servers are standalone services that expose tools and capabilities to AI agents. They run on specific ports and provide a standardized API for tool discovery and execution.

**Common MCP Server Types:**
- **Filesystem** (Port 3001): File operations (read, write, list, search)
- **Git** (Port 3002): Version control operations
- **Docker** (Port 3003): Container management
- **AWS** (Port 3004): Cloud resource management
- **Database** (Port 3005): Database operations
- **Custom** (Ports 8080-8081): User-defined tools

## Quick Start

### 1. Check for Available Servers

```aethershell
// Detect all available MCP servers
let servers = mcp_servers()
print("Found " + len(servers) + " MCP server(s)")

// Show server details
servers | foreach(fn(s) => 
    print("- " + s.name + " at " + s.endpoint + " (" + len(s.tools) + " tools)")
)
```

### 2. Detect Specific Server

```aethershell
// Find filesystem server
let fs = mcp_detect("http://localhost:3001")

// Find any available server
let any_server = mcp_detect()
```

### 3. Use with AI Agents

```aethershell
// Auto-detect both AI backend and MCP server
let model = ai_detect()
let server = mcp_detect()

// Create agent with tools
agent("List files in current directory", model, server.tools)
```

## Setting Up MCP Servers

### Option 1: Using Docker (Recommended)

```bash
# Filesystem server
docker run -d -p 3001:3001 \
  -v $(pwd):/workspace \
  mcp/filesystem-server

# Git server  
docker run -d -p 3002:3002 \
  -v $(pwd):/repo \
  mcp/git-server

# Docker management server
docker run -d -p 3003:3003 \
  -v /var/run/docker.sock:/var/run/docker.sock \
  mcp/docker-server
```

### Option 2: Node.js Implementation

```bash
# Install MCP server toolkit
npm install -g @modelcontextprotocol/server-toolkit

# Start filesystem server
mcp-server filesystem --port 3001 --root /path/to/workspace

# Start git server
mcp-server git --port 3002 --repo /path/to/repository
```

### Option 3: Python Implementation

```python
# filesystem_server.py
from mcp import Server
import os

server = Server("filesystem")

@server.tool("read_file")
def read_file(path: str) -> str:
    with open(path, 'r') as f:
        return f.read()

@server.tool("list_dir")
def list_dir(path: str = ".") -> list:
    return os.listdir(path)

if __name__ == "__main__":
    server.run(port=3001)
```

```bash
python filesystem_server.py
```

## Common Use Cases

### 1. File Management

```aethershell
// Setup
let fs = mcp_detect("http://localhost:3001")
let model = ai_detect()

// Use cases
agent("Find all .txt files in project", model, fs.tools)
agent("Analyze code quality in src/", model, fs.tools)
agent("Create project documentation", model, fs.tools)
```

### 2. Git Operations

```aethershell
// Setup
let git = mcp_detect("http://localhost:3002")
let model = ai_detect()

// Use cases
agent("Review recent commits for issues", model, git.tools)
agent("Generate changelog from git history", model, git.tools)
agent("Check for merge conflicts", model, git.tools)
```

### 3. Multi-Server Workflows

```aethershell
// Setup multiple servers
let fs = mcp_detect("http://localhost:3001")
let git = mcp_detect("http://localhost:3002")
let model = ai_detect()

// Combine tools from multiple servers
let all_tools = fs.tools + git.tools
agent("Prepare release: update files and commit", model, all_tools)
```

### 4. Conditional Server Usage

```aethershell
// Detect available servers
let servers = mcp_servers()

// Use filesystem if available, otherwise work without tools
match len(servers | where(fn(s) => s.endpoint == "http://localhost:3001")) {
    0 => agent("List directory contents using shell", ai_detect(), []),
    _ => {
        let fs = mcp_detect("http://localhost:3001")
        agent("List directory contents", ai_detect(), fs.tools)
    }
}
```

## MCP Server Configuration

### Server Health Check

```aethershell
// Check if server is responding
let server = mcp_detect("http://localhost:3001")
match server {
    null => print("Server not available"),
    _ => {
        print("Server: " + server.name)
        print("Endpoint: " + server.endpoint)
        print("Tools: " + len(server.tools))
        server.tools | foreach(fn(t) => print("  - " + t))
    }
}
```

### Tool Discovery

```aethershell
// Discover all available tools across servers
let all_servers = mcp_servers()
let all_tools = []

all_servers | foreach(fn(s) => {
    print("Server: " + s.name)
    s.tools | foreach(fn(t) => {
        print("  " + t)
        all_tools = all_tools + [t]
    })
})

print("Total unique tools: " + len(all_tools))
```

## Best Practices

### 1. Server Health Monitoring

Always check server availability before use:

```aethershell
let servers = mcp_servers()
print("Available servers: " + len(servers))

// Validate tools are accessible
servers | foreach(fn(s) => {
    let tool_count = len(s.tools)
    match tool_count {
        0 => print("⚠️  " + s.name + " has no tools"),
        _ => print("✓ " + s.name + " has " + tool_count + " tools")
    }
})
```

### 2. Graceful Degradation

Provide fallbacks when servers aren't available:

```aethershell
// Try MCP server first, fallback to built-in commands
let fs = mcp_detect("http://localhost:3001")
match fs {
    null => {
        print("Using built-in commands")
        ls "." | print
    },
    _ => {
        print("Using MCP filesystem server")
        agent("List directory contents", ai_detect(), fs.tools)
    }
}
```

### 3. Security Considerations

- **Port Security**: Only expose MCP servers on trusted networks
- **Tool Validation**: Review available tools before using
- **Access Control**: Implement authentication if needed
- **Resource Limits**: Set appropriate timeouts and limits

```aethershell
// Validate tools before use
let server = mcp_detect("http://localhost:3001")
let safe_tools = server.tools | where(fn(t) => 
    t != "delete_file" && t != "execute_command"
)
agent("Safe file analysis", ai_detect(), safe_tools)
```

### 4. Performance Optimization

- **Connection Reuse**: Cache server connections
- **Selective Tools**: Only use required tools
- **Timeout Management**: Set appropriate timeouts

```aethershell
// Cache server detection results
let cached_servers = mcp_servers()

// Use specific tools only
let fs = mcp_detect("http://localhost:3001")
let read_tools = fs.tools | where(fn(t) => 
    t == "read_file" || t == "list_dir"
)
agent("Read-only file analysis", ai_detect(), read_tools)
```

## Environment Setup

### Development Environment

```bash
# Start development MCP servers
docker-compose up -d mcp-filesystem mcp-git

# Verify servers are running
ae examples/15_mcp_basic.ae
```

### Production Environment

```bash
# Use systemd for production deployment
sudo systemctl enable mcp-filesystem
sudo systemctl start mcp-filesystem

# Monitor server health
sudo systemctl status mcp-filesystem
```

### Docker Compose Example

```yaml
# docker-compose.yml
version: '3.8'
services:
  mcp-filesystem:
    image: mcp/filesystem-server
    ports:
      - "3001:3001"
    volumes:
      - ./workspace:/workspace
    restart: unless-stopped
    
  mcp-git:
    image: mcp/git-server
    ports:
      - "3002:3002"
    volumes:
      - .:/repo
    restart: unless-stopped
```

## Troubleshooting

### Common Issues

1. **Server Not Found**
   ```aethershell
   // Check if server is running
   let servers = mcp_servers()
   match len(servers) {
       0 => print("No MCP servers detected. Check if servers are running."),
       _ => print("Servers found: " + len(servers))
   }
   ```

2. **Connection Timeout**
   - Verify server is running on expected port
   - Check firewall settings
   - Ensure server accepts connections from localhost

3. **No Tools Available**
   ```aethershell
   let server = mcp_detect("http://localhost:3001")
   match len(server.tools) {
       0 => print("Server running but no tools available"),
       _ => print("Tools available: " + len(server.tools))
   }
   ```

### Debug Commands

```aethershell
// Comprehensive MCP diagnostics
print("=== MCP Diagnostics ===")

// Test each standard port
let ports = [3001, 3002, 3003, 3004, 3005, 8080, 8081]
ports | foreach(fn(port) => {
    let endpoint = "http://localhost:" + port
    let server = mcp_detect(endpoint)
    match server {
        null => print("❌ Port " + port + ": No server"),
        _ => print("✅ Port " + port + ": " + server.name + " (" + len(server.tools) + " tools)")
    }
})

// Test AI backend integration
let model = ai_detect()
print("AI Backend: " + model)

print("=== Ready for Integration ===")
```

## Advanced Usage

### Custom MCP Server

Create your own MCP server for specialized tools:

```python
# custom_server.py
from mcp import Server
import subprocess

server = Server("custom-tools")

@server.tool("system_info")
def system_info() -> str:
    return subprocess.check_output(["uname", "-a"]).decode()

@server.tool("disk_usage")
def disk_usage(path: str = ".") -> str:
    return subprocess.check_output(["du", "-sh", path]).decode()

if __name__ == "__main__":
    server.run(port=8080)
```

### Multi-Agent Coordination

```aethershell
// Setup multiple agents with different server combinations
let fs = mcp_detect("http://localhost:3001")
let git = mcp_detect("http://localhost:3002")
let model = ai_detect()

// File analyzer agent
let file_agent = agent("Analyze file structure", model, fs.tools)

// Git analyzer agent  
let git_agent = agent("Analyze git history", model, git.tools)

// Coordinator agent with all tools
let coordinator = agent("Coordinate file and git analysis", model, fs.tools + git.tools)
```

## Next Steps

1. **Start Simple**: Begin with filesystem server for basic file operations
2. **Add Servers Gradually**: Introduce git, docker, and custom servers as needed
3. **Monitor Performance**: Watch for timeout issues and optimize accordingly
4. **Security Review**: Implement appropriate access controls for production
5. **Custom Tools**: Develop specialized MCP servers for your specific needs

## Resources

- **MCP Specification**: [https://modelcontextprotocol.io](https://modelcontextprotocol.io)
- **Server Examples**: `examples/15_mcp_basic.ae` through `examples/18_mcp_conditional.ae`
- **API Documentation**: `docs/AI_BACKENDS.md` and `docs/AI_MCP_INTEGRATION.md`
- **Test Suite**: `tests/mcp_detection.rs` for implementation examples

---

For additional help, use `help` in AetherShell to see all available MCP functions and their usage patterns.