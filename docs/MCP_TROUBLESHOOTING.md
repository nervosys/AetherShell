# MCP Troubleshooting Guide

## Common Issues and Solutions

### 1. No MCP Servers Detected

**Symptoms:**
```aethershell
let servers = mcp_servers()
// Returns empty array: []
```

**Possible Causes:**
- No MCP servers are running
- Servers are running on non-standard ports
- Network connectivity issues
- Firewall blocking connections

**Solutions:**

#### Check if Servers are Running
```bash
# Check common MCP ports
netstat -an | grep -E ":(3001|3002|3003|3004|3005|8080|8081)"

# On Windows:
netstat -an | findstr ":3001 :3002 :3003 :3004 :3005 :8080 :8081"
```

#### Start MCP Servers
```bash
# Using Docker
docker run -d -p 3001:3001 mcp/filesystem-server

# Using Node.js
npm install -g @modelcontextprotocol/server-toolkit
mcp-server filesystem --port 3001

# Using Python (custom)
python mcp_filesystem_server.py
```

#### Test Connectivity
```bash
# Test if server responds
curl http://localhost:3001/mcp/v1/tools

# Expected response: JSON array of tools
```

#### Debug in AetherShell
```aethershell
// Test specific endpoints
let endpoints = [
    "http://localhost:3001",
    "http://localhost:3002", 
    "http://localhost:3003"
]

endpoints | foreach(fn(endpoint) => {
    let server = mcp_detect(endpoint)
    match server {
        null => print("❌ " + endpoint + " - Not available"),
        _ => print("✅ " + endpoint + " - " + server.name)
    }
})
```

---

### 2. Server Detected but No Tools Available

**Symptoms:**
```aethershell
let server = mcp_detect("http://localhost:3001")
print(len(server.tools))  // Returns 0
```

**Possible Causes:**
- Server is starting up
- Server configuration issues
- Tool registration failed
- Server API endpoint incorrect

**Solutions:**

#### Check Server Logs
```bash
# Docker logs
docker logs mcp-filesystem

# systemd logs  
journalctl -u mcp-filesystem -f

# Direct process logs
tail -f /var/log/mcp-filesystem.log
```

#### Verify API Endpoint
```bash
# Test tools endpoint
curl -v http://localhost:3001/mcp/v1/tools

# Expected: HTTP 200 with JSON tool list
# If 404: Server may not implement MCP correctly
# If 500: Server internal error
```

#### Debug Server Health
```aethershell
// Comprehensive server check
let server = mcp_detect("http://localhost:3001")
match server {
    null => print("Server not responding"),
    _ => {
        print("Server name: " + server.name)
        print("Endpoint: " + server.endpoint)
        print("Available: " + server.available)
        print("Tool count: " + len(server.tools))
        
        match len(server.tools) {
            0 => print("⚠️ No tools registered - check server configuration"),
            _ => {
                print("Available tools:")
                server.tools | foreach(fn(t) => print("  - " + t))
            }
        }
    }
}
```

---

### 3. Connection Timeouts

**Symptoms:**
```aethershell
// Detection takes very long or fails silently
let servers = mcp_servers()  // Takes 20+ seconds
```

**Possible Causes:**
- Server is slow to respond
- Network latency
- Server overloaded
- DNS resolution issues

**Solutions:**

#### Check Network Latency
```bash
# Test local connection speed
time curl http://localhost:3001/mcp/v1/tools

# Should complete in < 100ms for localhost
```

#### Monitor Server Performance
```bash
# Check server resource usage
docker stats mcp-filesystem

# Check system resources
top -p $(pgrep -f mcp-server)
```

#### Optimize Detection
```aethershell
// Test single server instead of full scan
let fs = mcp_detect("http://localhost:3001")
// Much faster than mcp_servers() if you know the endpoint
```

---

### 4. Agent Integration Failures

**Symptoms:**
```aethershell
let server = mcp_detect("http://localhost:3001")
agent("task", ai_detect(), server.tools)
// Agent fails or doesn't use tools
```

**Possible Causes:**
- Tool name mismatches
- Tool parameter incompatibility
- Agent configuration issues
- AI backend problems

**Solutions:**

#### Verify Tool Compatibility
```aethershell
// Check available tools
let server = mcp_detect("http://localhost:3001")
print("Available tools:")
server.tools | foreach(fn(t) => print("  " + t))

// Verify AI backend
let model = ai_detect()
print("AI backend: " + model)
```

#### Test Tool Access
```bash
# Test tool directly via API
curl -X POST http://localhost:3001/mcp/v1/call \
  -H "Content-Type: application/json" \
  -d '{"name": "read_file", "arguments": {"path": "test.txt"}}'
```

#### Debug Agent Execution
```aethershell
// Use dry run mode
agent("task", ai_detect(), server.tools, 3, true)
// Shows what would be executed without actually running
```

---

### 5. Multiple Server Conflicts

**Symptoms:**
```aethershell
let servers = mcp_servers()
// Multiple servers with same tools
```

**Possible Causes:**
- Duplicate servers running
- Port conflicts
- Tool name collisions

**Solutions:**

#### Identify Duplicates
```aethershell
let servers = mcp_servers()
let endpoints = []

servers | foreach(fn(s) => {
    endpoints = endpoints + [s.endpoint]
    print(s.name + " at " + s.endpoint + " with " + len(s.tools) + " tools")
})

print("Unique endpoints: " + len(endpoints))
print("Total servers: " + len(servers))
```

#### Stop Duplicate Servers
```bash
# Find processes on specific ports
lsof -ti:3001 | xargs kill

# Or on Windows:
netstat -ano | findstr :3001
taskkill /PID <process_id> /F
```

#### Select Specific Server
```aethershell
// Use specific endpoint instead of auto-detection
let fs = mcp_detect("http://localhost:3001")
// Avoids conflicts with multiple filesystem servers
```

---

### 6. Permission and Security Issues

**Symptoms:**
- Tools fail with permission errors
- Server refuses connections
- Authentication failures

**Possible Causes:**
- File system permissions
- Server security configuration
- Container security restrictions

**Solutions:**

#### Check File Permissions
```bash
# For filesystem servers
ls -la /workspace
# Ensure server has read/write access

# For Docker
docker run -v $(pwd):/workspace:ro mcp/filesystem-server
# Use read-only mount for security
```

#### Review Server Configuration
```yaml
# docker-compose.yml security settings
services:
  mcp-filesystem:
    image: mcp/filesystem-server
    ports:
      - "127.0.0.1:3001:3001"  # Localhost only
    volumes:
      - ./safe-directory:/workspace:ro  # Read-only
    user: "1000:1000"  # Non-root user
```

#### Test Security Restrictions
```aethershell
// Test with minimal tools
let server = mcp_detect("http://localhost:3001")
let safe_tools = server.tools | where(fn(t) => 
    t == "read_file" || t == "list_dir"
)
agent("Safe file listing", ai_detect(), safe_tools)
```

---

### 7. Performance Issues

**Symptoms:**
- Slow tool execution
- High memory usage
- Timeouts during agent execution

**Solutions:**

#### Monitor Resource Usage
```bash
# Monitor MCP server resources
docker stats --format "table {{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}"

# System monitoring
htop -p $(pgrep -f mcp-server)
```

#### Optimize Tool Selection
```aethershell
// Use only required tools
let server = mcp_detect("http://localhost:3001")
let minimal_tools = server.tools | take(3)  // Use first 3 tools only
agent("task", ai_detect(), minimal_tools)
```

#### Cache Server Detection
```aethershell
// Cache detection results
let cached_servers = mcp_servers()

// Reuse cached results instead of re-detecting
let fs = cached_servers | where(fn(s) => s.endpoint == "http://localhost:3001") | first
```

---

### 8. Development and Testing Issues

**Symptoms:**
- Inconsistent behavior between runs
- Tools work manually but fail in agents
- Test failures

**Solutions:**

#### Create Reproducible Environment
```bash
# Use Docker Compose for consistency
docker-compose up -d

# Wait for servers to be ready
sleep 5

# Run tests
ae examples/15_mcp_basic.ae
```

#### Add Debug Logging
```aethershell
// Enable verbose output
print("=== MCP Debug Info ===")
let servers = mcp_servers()
print("Detected servers: " + len(servers))

servers | foreach(fn(s) => {
    print("Server: " + s.name)
    print("  Endpoint: " + s.endpoint)
    print("  Available: " + s.available)
    print("  Tools: " + len(s.tools))
    print("  Tool list: " + s.tools)
    print("")
})
```

#### Test Tool Isolation
```aethershell
// Test individual tools
let server = mcp_detect("http://localhost:3001")
server.tools | foreach(fn(tool) => {
    print("Testing tool: " + tool)
    agent("Test " + tool, ai_detect(), [tool])
})
```

---

## Diagnostic Scripts

### Full System Check

```aethershell
// File: debug_mcp_system.ae
print("=== MCP System Diagnostics ===")
print("")

// Test 1: Basic detection
print("1. Server Detection:")
let servers = mcp_servers()
print("   Found " + len(servers) + " servers")

// Test 2: Individual server checks
print("")
print("2. Individual Server Status:")
let standard_ports = [3001, 3002, 3003, 3004, 3005, 8080, 8081]
standard_ports | foreach(fn(port) => {
    let endpoint = "http://localhost:" + port
    let server = mcp_detect(endpoint)
    match server {
        null => print("   ❌ Port " + port + ": No server"),
        _ => print("   ✅ Port " + port + ": " + server.name + " (" + len(server.tools) + " tools)")
    }
})

// Test 3: AI Backend Integration
print("")
print("3. AI Backend Status:")
let model = ai_detect()
print("   Selected model: " + model)

// Test 4: Tool Inventory
print("")
print("4. Available Tools:")
let all_tools = []
servers | foreach(fn(s) => {
    print("   " + s.name + ":")
    s.tools | foreach(fn(t) => {
        print("     - " + t)
        all_tools = all_tools + [t]
    })
})
print("   Total tools: " + len(all_tools))

// Test 5: Integration Test
print("")
print("5. Integration Test:")
match len(servers) {
    0 => print("   ⚠️ No servers available for integration test"),
    _ => {
        let first_server = servers | first
        print("   Testing with: " + first_server.name)
        match len(first_server.tools) {
            0 => print("   ⚠️ No tools available for testing"),
            _ => print("   ✅ Ready for agent integration")
        }
    }
}

print("")
print("=== Diagnostics Complete ===")
```

### Network Connectivity Test

```aethershell
// File: test_mcp_connectivity.ae
print("=== MCP Network Connectivity Test ===")
print("")

let test_endpoints = [
    "http://localhost:3001",
    "http://127.0.0.1:3001", 
    "http://0.0.0.0:3001"
]

test_endpoints | foreach(fn(endpoint) => {
    print("Testing: " + endpoint)
    let server = mcp_detect(endpoint)
    match server {
        null => print("  ❌ Connection failed"),
        _ => print("  ✅ Connected - " + server.name)
    }
})
```

### Performance Benchmark

```aethershell
// File: benchmark_mcp.ae
print("=== MCP Performance Benchmark ===")
print("")

// Benchmark single server detection
print("Single server detection (3001):")
let start_single = 0  // Would need proper timing in real implementation
let fs = mcp_detect("http://localhost:3001")
print("  Time: <2000ms (target)")

// Benchmark full scan
print("")
print("Full server scan:")
let start_scan = 0
let all_servers = mcp_servers()
print("  Found: " + len(all_servers) + " servers")
print("  Time: <20000ms (target)")

// Benchmark repeated detection
print("")
print("Repeated detection (10x):")
let count = 0
// Would implement loop here
print("  Average time per detection: <200ms (target)")
```

---

## Quick Reference

### Essential Commands

```aethershell
// Server discovery
mcp_servers()                              // List all servers
mcp_detect()                               // Find first available
mcp_detect("http://localhost:3001")        // Find specific server

// Diagnostics
let s = mcp_detect("http://localhost:3001")
print("Server: " + s.name)                // Server name
print("Tools: " + len(s.tools))           // Tool count
s.tools | foreach(fn(t) => print(t))      // List tools

// Integration
let model = ai_detect()                    // Auto-detect AI
let server = mcp_detect()                  // Auto-detect MCP
agent("task", model, server.tools)        // Use together
```

### Common Port Mappings

| Port | Service    | Typical Tools                       |
| ---- | ---------- | ----------------------------------- |
| 3001 | Filesystem | read_file, write_file, list_dir     |
| 3002 | Git        | git_log, git_diff, git_status       |
| 3003 | Docker     | docker_ps, docker_exec, docker_logs |
| 3004 | AWS        | ec2_list, s3_list, lambda_invoke    |
| 3005 | Database   | sql_query, db_list, table_info      |
| 8080 | Custom 1   | User-defined tools                  |
| 8081 | Custom 2   | User-defined tools                  |

### Error Codes

| Issue         | Symptom                       | Solution                  |
| ------------- | ----------------------------- | ------------------------- |
| No servers    | `len(mcp_servers()) == 0`     | Start MCP servers         |
| No tools      | `len(server.tools) == 0`      | Check server config       |
| Timeout       | Slow detection                | Check network/performance |
| Permission    | Tool execution fails          | Review file permissions   |
| Port conflict | Multiple servers on same port | Stop duplicates           |

---

For additional support, see:
- `docs/MCP_GETTING_STARTED.md` - Setup instructions
- `docs/AI_MCP_INTEGRATION.md` - Integration guide
- `examples/15_mcp_basic.ae` - Basic usage examples
- `tests/mcp_detection.rs` - Implementation details