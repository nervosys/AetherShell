# Distributed Computing

AetherShell has a node registry, a local in-memory consensus tally and a
local `aggregate_results` over an Array. Work that would go to other machines
is not implemented in this build: job submission and remote execution refuse
with `E_UNIMPLEMENTED`, and no node is contacted by the cluster builtins.

## Cluster Management

### Creating a Cluster

```aethershell
cluster_create("my-cluster")
# {name: "my-cluster", status: "created", node_count: 0, created_at: …}
```

`cluster_create` takes an optional name of at most 128 bytes (default
`default`) and at most one argument. It contacts no node and stores the name
nowhere: nodes added with `cluster_add_node` join one process-wide registry
that does not record which cluster they belong to.

### Adding Nodes

```aethershell
cluster_add_node("node-1", "192.168.1.10:3000")
cluster_add_node("node-2", "192.168.1.11:3000", ["compute"])

{id: "node-3", address: "192.168.1.12:3000"} | cluster_add_node
```

The third argument is an Array of String capabilities (default `compute` and
`storage`). The address must have no whitespace. Each call returns
`{id, address, capabilities, status: "registered"}`. Nothing contacts the node,
so it is registered, not online, and no heartbeat is checked.

### Cluster Status

```aethershell
cluster_status()
# {total_nodes, online_nodes, busy_nodes, total_jobs, pending_jobs,
#  running_jobs, average_load}
```

`online_nodes` counts nodes stored with an Online status. A node added by
`cluster_add_node` is stored that way, so this count is not a liveness check.

### Node Management

```aethershell
cluster_nodes()
# [ {id: "node-1", address: "192.168.1.10:3000", load: 0.0, status: …}, … ]

cluster_remove_node("node-2")   # reports whether that node existed
```

## Job Scheduling

### Submitting Jobs

Job submission is not implemented in this build. `job_submit` (alias `submit`)
has no executor behind it, so it refuses with `E_UNIMPLEMENTED`. No job is
queued and nothing runs:

```aethershell
try { job_submit("build", "cargo test") } catch e { e.error.code }
# "E_UNIMPLEMENTED"
```

### Job Status, Results and Listing

`job_status`, `job_results` and `job_list` read the distributed job table. In
this build nothing adds to it, because `job_submit` refuses, so the table is
empty:

```aethershell
job_list()
# []

job_results("job-abc123")   # an unknown id gives an empty result
job_status("job-abc123")    # an unknown id is E_NOT_FOUND
```

## Remote Execution

> **`remote_exec` refuses with `E_UNIMPLEMENTED`; it runs nothing.** No SSH or
> cluster transport is wired in, so the command is not sent to the node. The
> node and command are checked first: a node that is not registered is
> `E_NOT_FOUND`, and a valid call is refused as above.

```aethershell
try { remote_exec("node-1", "ls src") } catch e { e.error.code }
# "E_UNIMPLEMENTED" for a registered node, "E_NOT_FOUND" for an unknown one

# Run a command on a real host:
ssh_exec "user@192.168.1.10" "ls src | wc -l"
```

## Result Aggregation

`aggregate_results` combines an Array that is already in hand. It does not
collect anything from nodes:

```aethershell
aggregate_results([1, 2, 3], "sum")    # adds finite numbers
[1, 2, 3] | aggregate_results("sum")   # the piped form takes the strategy only
aggregate_results(["a", "b"])          # concat, the default, returns the Array
```

The strategies are `concat` (the default), `sum`, `count`, `first` and `last`.
`sum` refuses any element that is not a finite number instead of skipping it,
`first` and `last` refuse an empty Array, and an unknown strategy is
`E_BAD_ARG`.

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

# Commit the outcome in this process: refused unless there is quorum and the
# approval ratio is at or above the threshold. A ratio below the threshold
# rejects the proposal.
nanda_commit(p.id)
```

## TUI Distributed Panel

The TUI provides a dedicated Distributed Agents tab (Tab 5) for visual management:

- **`s`** — Start distributed swarm
- **`d`** — Stop distributed swarm
- **`r`** — Refresh network status
- **`t`** — Test node connections

See [TUI Navigation](../tui/navigation.md) for all key bindings.
