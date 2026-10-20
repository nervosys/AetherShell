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

Job submission is not implemented in this build. `job_submit` (alias `submit`)
has no job executor behind it, so it refuses rather than return a job id for a
job that would never run. Nothing is recorded:

```aethershell
try { job_submit("my-cluster", "ls src") } catch e { e.error.code }
# "E_UNIMPLEMENTED"
```

### Job Status, Results and Listing

`job_status`, `job_results` and `job_list` read the distributed job table, and
only `job_submit` fills it. Until a job can be submitted, the table is empty:

```aethershell
job_list()
# []

job_results("job-abc123")   # an unknown id gives an empty result
job_status("job-abc123")    # an unknown id is E_NOT_FOUND
```

## Remote Execution

> **`remote_exec` is a stub — it does not run anything.** There is no SSH/RPC
> transport behind it. It validates that the node is registered and echoes the
> request back with `status: "simulated"` and `simulated: true`. This page
> previously showed it returning a real result (`# 15`), which it never did.
>
> For **real** remote execution use `ssh_exec`, which is effect-tagged `Exec` and
> approval-gated in agent mode.

```aethershell
remote_exec "192.168.1.10:3000" 'ls "src" | len'
# { node_id: "192.168.1.10:3000", command: "ls \"src\" | len",
#   status: "simulated", simulated: true,
#   output: "remote_exec is a stub: the command was NOT run. …" }

# Actually run it:
ssh_exec "user@192.168.1.10" "ls src | wc -l"
```

## Result Aggregation

Collect and merge results from multiple nodes:

```aethershell
let results = aggregate_results "my-cluster" "job-batch-1"
# Merges results from all nodes into a single value
```

## NANDA Consensus

For coordinated multi-agent decisions, NANDA keeps a consensus tally in this
process. Two limits apply before you rely on it. Nothing identifies a voter, so
the electorate is the caller's assertion. And the tally is in memory only: it is
gone when the process exits. Nothing is sent to other agents.

```aethershell
# Open a proposal: a name, and a Record that sets its threshold and electorate
let p = nanda_propose("deploy-v2", {threshold: 0.66, electorate: 3})

# Each vote is a Bool against the proposal id
nanda_vote(p.id, true)
nanda_vote(p.id, true)

# How much of the electorate has balloted (quorum is half of it)
nanda_quorum(p.id)
# {quorum_reached: true, participation: 0.67, votes_cast: 2, electorate: 3}

# The approval ratio against the threshold
nanda_consensus(p.id)

# Commit the outcome in this process: needs quorum and a ratio at or above the threshold
nanda_commit(p.id)
```

## TUI Distributed Panel

The TUI provides a dedicated Distributed Agents tab (Tab 5) for visual management:

- **`s`** — Start distributed swarm
- **`d`** — Stop distributed swarm
- **`r`** — Refresh network status
- **`t`** — Test node connections

See [TUI Navigation](../tui/navigation.md) for all key bindings.
