//! Benchmark VM / sandbox systems for **agentic AI use** — the spawn-and-tear-down
//! sandbox loop an agent runtime drives (one isolated environment per tool call).
//!
//! Ranks AetherVM against Firecracker, Cloud Hypervisor, gVisor, Kata, QEMU/KVM,
//! and Docker on five agent-native axes (start-latency, density, isolation,
//! snapshotting, agent-control), then shows a head-to-head and the evidence.
//!
//! Run: `cargo run -p agentic-eval --example vm_benchmark`

use agentic_eval::vms::{compare_vms, profile, rank_vms, Vm};

fn main() {
    println!("agentic-eval — VM/sandbox systems for agentic AI use");
    println!("axes: start-latency, density, isolation, snapshotting, agent-control\n");

    // ── Ranked benchmark (best-first by composite agentic fitness) ───────────
    println!(
        "{:<17} {:>7}   {:>5} {:>7} {:>9} {:>8} {:>13}",
        "system", "fitness", "start", "density", "isolation", "snapshot", "agent-control"
    );
    for p in rank_vms() {
        println!(
            "{:<17} {:>7.2}   {:>5.2} {:>7.2} {:>9.2} {:>8.2} {:>13.2}",
            p.vm.name(),
            p.fitness(),
            p.start_latency,
            p.density,
            p.isolation,
            p.snapshotting,
            p.agent_control,
        );
    }

    // ── Head-to-head: AetherVM vs the microVM reference (Firecracker) ────────
    println!("\nhead-to-head (positive = AetherVM fits agentic use better):");
    print!("{}", compare_vms(Vm::AetherVm, Vm::Firecracker));

    // ── Evidence behind the subject's profile ────────────────────────────────
    println!("\nwhy AetherVM scores where it does:");
    for e in &profile(Vm::AetherVm).evidence {
        println!("  - {e}");
    }

    println!(
        "\nReading: AetherVM leads on the agent-native axes it was designed for\n\
         (instant CoW branching + an MCP-native control plane), while microVMs\n\
         (Firecracker/Cloud Hypervisor) lead on raw cold-start and battle-tested\n\
         isolation. Shared-kernel containers (Docker) win speed/density but rank\n\
         low on isolation for untrusted, agent-generated code."
    );
}
