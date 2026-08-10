# DevOps

Examples of using AetherShell for DevOps automation, system administration, and infrastructure management.

## System Monitoring

### Health Dashboard

```aethershell
let cpu = sys_cpu_info
let mem = sys_mem_info
let disk = sys_disk_info
let load = sys_load_avg

echo "=== System Health ==="
echo "Hostname: ${sys_hostname}"
echo "OS: ${sys_os} ${sys_arch}"
echo "Uptime: ${sys_uptime}"
echo "Load: ${load}"
echo "CPU: ${cpu.cores} cores"
echo "Memory: ${round(mem.used / 1048576)}MB / ${round(mem.total / 1048576)}MB"
echo "Disk: ${round(disk.used / 1073741824)}GB / ${round(disk.total / 1073741824)}GB"
```

### Process Monitor

```aethershell
# Find top memory-consuming processes
proc_list
  | sort_by "mem_usage" "desc"
  | take 10
  | map(fn(p) => { pid: p.pid, name: p.name, mem_mb: round(p.mem_usage / 1048576) })
```

### Port Scanning

```aethershell
# Check which services are listening
net_ports
  | where(fn(p) => p.state == "LISTEN")
  | map(fn(p) => { port: p.port, pid: p.pid, process: p.process })
  | sort_by "port" "asc"
```

## Service Management

### Service Status Check

```aethershell
let services = ["nginx", "postgresql", "redis"]

services | map(fn(svc) => {
  let status = svc_status svc
  { name: svc, status: status.state, pid: status.pid }
})
```

### Restart All Services

```aethershell
["nginx", "postgresql", "redis"]
  | each(fn(svc) => {
      echo "Restarting ${svc}..."
      svc_restart svc
  })
```

## Deployment Pipeline

### Build and Deploy

```aethershell
# Build
echo "Building..."
let build = sh "cargo build --release 2>&1"
echo build

# Run tests
echo "Testing..."
let test_result = sh "cargo test 2>&1"
if contains test_result "FAILED" {
  echo "Tests failed! Aborting deployment."
  exit 1
}

# Deploy
echo "Deploying..."
file_copy "target/release/ae" "/opt/aethershell/ae"
svc_restart "aethershell"
echo "Deployed successfully"
```

### Rolling Deployment

```aethershell
let nodes = ["192.168.1.10", "192.168.1.11", "192.168.1.12"]

nodes | each(fn(node) => {
  echo "Deploying to ${node}..."

  # Remove from load balancer
  echo "  Draining connections..."
  sleep 5000

  # Deploy. Use ssh_exec: it really runs the command. `remote_exec` is a stub
  # that reports `simulated` and executes nothing — in a rolling deploy that
  # would mean every node "succeeds" without ever being restarted.
  ssh_exec "${node}" "sudo systemctl restart aethershell"

  # Health check
  let health = web_check_url "http://${node}:3000/health"
  if health.reachable {
    echo "  ✓ ${node} healthy"
  } else {
    echo "  ✗ ${node} FAILED - rolling back"
    exit 1
  }
})

echo "All nodes deployed successfully"
```

## Git Operations

### Change Summary

```aethershell
# Summarize recent changes
let log = sh "git log --oneline -20"
echo "Recent commits:"
echo log

# AI-powered summary
ai "Summarize these git commits into a changelog entry:\n${log}"
```

### Branch Cleanup

```aethershell
# Find merged branches
let branches = sh "git branch --merged" | split "\n" | map(fn(b) => trim b)
  | where(fn(b) => b != "main" && b != "master" && !starts_with(b, "*"))

echo "Merged branches to clean up:"
branches | each(fn(b) => echo "  ${b}")
```

## Container Management

```aethershell
# List running containers
sh "docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}'"
  | split "\n"
  | map(fn(line) => {
      let parts = split line "\t"
      { name: parts[0], status: parts[1], ports: parts[2] }
  })
```

## Backup Script

```aethershell
let timestamp = sh "date +%Y%m%d_%H%M%S" | trim
let backup_dir = "/backups/${timestamp}"

mkdir backup_dir

echo "Backing up database..."
sh "pg_dump mydb > ${backup_dir}/db.sql"

echo "Backing up config..."
file_copy "/etc/aethershell" "${backup_dir}/config"

echo "Backing up data..."
sh "tar czf ${backup_dir}/data.tar.gz /var/lib/aethershell"

# Cleanup old backups (keep last 7)
ls "/backups"
  | sort_by "name" "desc"
  | slice 7 (ls "/backups" | len)
  | each(fn(old) => {
      echo "Removing old backup: ${old.name}"
      sh "rm -rf ${old.path}"
  })

echo "Backup complete: ${backup_dir}"
```

## Cron Jobs

```aethershell
# List scheduled jobs
cron_list
# [{ id: "1", schedule: "0 * * * *", command: "ae health-check.ae" }, ...]

# Add a new cron job
cron_add "0 2 * * *" "ae backup.ae"    # Daily at 2 AM

# Remove a job
cron_remove "1"
```

## Network Diagnostics

```aethershell
let target = "api.example.com"

echo "=== Network Diagnostics for ${target} ==="

# DNS resolution
let dns = net_dns_lookup target
echo "DNS: ${dns}"

# Ping
let ping = net_ping target
echo "Ping: ${ping.latency_ms}ms (${if ping.reachable { 'OK' } else { 'FAILED' }})"

# Traceroute
echo "Route:"
net_traceroute target | each(fn(hop) => {
  echo "  ${hop.hop}. ${hop.ip} (${hop.latency_ms}ms)"
})
```
