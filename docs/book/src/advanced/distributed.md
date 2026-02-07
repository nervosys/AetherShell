# Distributed Computing

AetherShell supports distributed agent execution across multiple nodes with cluster management, job scheduling, and result aggregation.

## Cluster Management

### Creating a Cluster

```aethershell
cluster_create "my-cluster" { max_nodes: 10, timeout: 30000 }
```

### Adding Nodes

```aethershell
cluster_add_node "my-cluster" { address: "192.168.1.10", port: 3000 }
cluster_add_node "my-cluster" { address: "192.168.1.11", port: 3000 }
```

### Cluster Status

```aethershell
cluster_status "my-cluster"
# {
#   name: "my-cluster",
#   nodes: 2,
#   healthy: 2,
#   total_jobs: 0,
#   uptime_seconds: 120
# }
```

### Node Management

```aethershell
cluster_nodes "my-cluster"
# [
#   { address: "192.168.1.10", port: 3000, status: "active", load: 0.2 },
#   { address: "192.168.1.11", port: 3000, status: "active", load: 0.1 }
# ]

cluster_remove_node "my-cluster" "192.168.1.11"
```

## Job Scheduling

### Submitting Jobs

```aethershell
let job_id = job_submit "my-cluster" {
  code: 'ls "src" | map(fn(f) => f.name)',
  priority: "high"
}
echo job_id   # "job-abc123"
```

### Job Status

```aethershell
job_status "job-abc123"
# { id: "job-abc123", status: "running", node: "192.168.1.10", progress: 0.5 }
```

### Job Results

```aethershell
let results = job_results "job-abc123"
echo results
```

### Listing and Canceling

```aethershell
job_list "my-cluster"
# [{ id: "job-abc123", status: "running" }, { id: "job-def456", status: "completed" }]

job_cancel "job-abc123"
```

## Remote Execution

Execute commands directly on remote nodes:

```aethershell
remote_exec "192.168.1.10:3000" 'ls "src" | len'
# 15
```

## Result Aggregation

Collect and merge results from multiple nodes:

```aethershell
let results = aggregate_results "my-cluster" "job-batch-1"
# Merges results from all nodes into a single value
```

## NANDA Consensus

For coordinated multi-agent decisions, AetherShell provides a consensus protocol:

```aethershell
# Propose a decision
let proposal_id = nanda_propose "Should we deploy v2.0?" {
  options: ["yes", "no", "defer"],
  quorum: 3,
  timeout: 60000
}

# Agents vote
nanda_vote proposal_id "yes" { reason: "All tests pass" }

# Check status
nanda_status proposal_id
# { proposal: "...", votes: 2, quorum: 3, status: "pending" }

# Check if quorum reached
nanda_quorum proposal_id
# false

# Final consensus
nanda_consensus proposal_id
# { decision: "yes", votes_for: 3, votes_against: 0 }
```

## TUI Distributed Panel

The TUI provides a dedicated Distributed Agents tab (Tab 5) for visual management:

- **`s`** — Start distributed swarm
- **`d`** — Stop distributed swarm
- **`r`** — Refresh network status
- **`t`** — Test node connections

See [TUI Navigation](../tui/navigation.md) for all key bindings.
