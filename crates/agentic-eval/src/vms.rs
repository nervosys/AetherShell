//! Evaluating **VM / sandbox systems** for agentic AI use.
//!
//! An agent runtime does not run one long-lived VM; it spawns a *fleet* of
//! short-lived, isolated execution environments — one per tool call, code run,
//! or sub-agent — and tears them down. That workload rewards different
//! properties than a classic datacenter VM, so this module scores VM/sandbox
//! systems on five agent-native axes:
//!
//! - **start-latency** — how fast a fresh, isolated sandbox is ready. Agent
//!   loops spawn constantly; cold-start dominates wall-clock.
//! - **density** — sandboxes per host (per-instance memory/CPU overhead). Fleet
//!   economics: how many concurrent agents fit on a box.
//! - **isolation** — strength of the security boundary for *untrusted,
//!   agent-generated* code. Hardware virtualization beats a shared kernel.
//! - **snapshotting** — instant fork / snapshot-restore of a warm template
//!   (copy-on-write), so an agent can branch a primed context per call and keep
//!   warm pools.
//! - **agent-control** — is the control plane programmatic and *agent/tool-native*
//!   (an agent can discover and drive lifecycle directly), or bring-your-own glue?
//!
//! Profiles are curated 0.0–1.0 static judgments with `evidence`, like the
//! [`languages`](crate::languages) and [`frameworks`](crate::frameworks)
//! profiles — deterministic, serializable, comparable. Scores reflect each
//! system's design center for the *ephemeral agent-sandbox* workload, not its
//! fitness for every use; a great long-lived datacenter VM can still rank low
//! here, and that is the point.
//!
//! ```
//! use agentic_eval::vms::{profile, rank_vms, Vm};
//! let fc = profile(Vm::Firecracker);
//! assert!(fc.evidence.len() >= 3);
//! let ranked = rank_vms();
//! assert!(ranked[0].fitness() >= ranked[ranked.len() - 1].fitness());
//! ```

/// VM / sandbox systems with curated agentic profiles.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Vm {
    /// AetherVM (HyperMachine) — the agentic-first hypervisor this ecosystem
    /// ships: copy-on-write sandbox spawn, an MCP tool surface, and an agent
    /// runtime. Scored on the same axes as everything else.
    AetherVm,
    Firecracker,
    CloudHypervisor,
    Gvisor,
    KataContainers,
    QemuKvm,
    Docker,
}

impl Vm {
    /// All profiled VM/sandbox systems, in fixed (deterministic) order.
    pub fn all() -> [Vm; 7] {
        [
            Vm::AetherVm,
            Vm::Firecracker,
            Vm::CloudHypervisor,
            Vm::Gvisor,
            Vm::KataContainers,
            Vm::QemuKvm,
            Vm::Docker,
        ]
    }

    /// Canonical lowercase name.
    pub fn name(self) -> &'static str {
        match self {
            Vm::AetherVm => "aethervm",
            Vm::Firecracker => "firecracker",
            Vm::CloudHypervisor => "cloud-hypervisor",
            Vm::Gvisor => "gvisor",
            Vm::KataContainers => "kata",
            Vm::QemuKvm => "qemu-kvm",
            Vm::Docker => "docker",
        }
    }

    /// Parse a (case-insensitive) name; accepts common aliases
    /// (`fc`, `chv`, `runsc`, `kvm`, `runc`, …).
    pub fn from_name(name: &str) -> Option<Vm> {
        match name.to_ascii_lowercase().as_str() {
            "aethervm" | "aether" | "hypermachine" => Some(Vm::AetherVm),
            "firecracker" | "fc" => Some(Vm::Firecracker),
            "cloud-hypervisor" | "cloudhv" | "chv" => Some(Vm::CloudHypervisor),
            "gvisor" | "runsc" => Some(Vm::Gvisor),
            "kata" | "kata-containers" => Some(Vm::KataContainers),
            "qemu-kvm" | "qemu" | "kvm" => Some(Vm::QemuKvm),
            "docker" | "runc" | "containers" => Some(Vm::Docker),
            _ => None,
        }
    }
}

/// A curated agentic profile of a VM/sandbox system across the five
/// agent-native axes, with evidence.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct VmProfile {
    /// Which system this profiles.
    pub vm: Vm,
    /// Cold-start speed of a fresh isolated sandbox (1.0 = sub-100ms).
    pub start_latency: f64,
    /// Sandboxes per host / low per-instance overhead (1.0 = microVM/container class).
    pub density: f64,
    /// Security-boundary strength for untrusted agent-generated code
    /// (1.0 = hardware virtualization, minimal attack surface).
    pub isolation: f64,
    /// Instant CoW fork / snapshot-restore for agent branching and warm pools.
    pub snapshotting: f64,
    /// Agent/tool-native, discoverable control plane to drive lifecycle.
    pub agent_control: f64,
    /// Why: one evidence string per notable factor.
    pub evidence: Vec<&'static str>,
}

impl VmProfile {
    /// Composite agentic fitness: unweighted mean of all five axes.
    pub fn fitness(&self) -> f64 {
        (self.start_latency
            + self.density
            + self.isolation
            + self.snapshotting
            + self.agent_control)
            / 5.0
    }
}

impl std::fmt::Display for VmProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: fitness {:.2} (start {:.2}, density {:.2}, isolation {:.2}, snapshot {:.2}, agent-control {:.2})",
            self.vm.name(),
            self.fitness(),
            self.start_latency,
            self.density,
            self.isolation,
            self.snapshotting,
            self.agent_control
        )
    }
}

/// The curated profile for `vm` (static, documented judgments — see module docs).
pub fn profile(vm: Vm) -> VmProfile {
    match vm {
        Vm::AetherVm => VmProfile {
            vm,
            start_latency: 0.8,
            density: 0.85,
            isolation: 0.8,
            snapshotting: 0.9,
            agent_control: 0.95,
            evidence: vec![
                "copy-on-write spawn from a warm template (SandboxPool) forks a sub-VM without a full boot; HV2 is hosted on KVM/WHPX/HVF",
                "page-level CoW shares one base template across the fleet — high agent-sandbox density by design",
                "hardware-assisted isolation (KVM/WHPX/HVF) for HV2 plus an optional bare-metal Type-1 (HV1); younger and less battle-tested at scale than Firecracker/QEMU",
                "CoW memory templates + first-class snapshot/restore make instant agent branching the core primitive — fork a primed context per tool call",
                "agentic-first control plane: a ~30-tool MCP registry projected to OpenAI/Anthropic/Gemini, an agent runtime, and a REST agent API — an agent drives lifecycle natively without bespoke glue",
            ],
        },
        Vm::Firecracker => VmProfile {
            vm,
            start_latency: 0.9,
            density: 0.9,
            isolation: 0.85,
            snapshotting: 0.8,
            agent_control: 0.5,
            evidence: vec![
                "powers AWS Lambda/Fargate: ~125 ms boot to userspace — the microVM cold-start reference",
                "minimal device model + jailer → a few MB of overhead and thousands of microVMs per host",
                "KVM hardware isolation with a deliberately tiny attack surface; the most battle-tested microVM",
                "snapshot/restore enables warm-clone pools for sandbox prewarming",
                "control is a REST API over a unix socket — programmatic but not agent/tool-native; orchestration is bring-your-own",
            ],
        },
        Vm::CloudHypervisor => VmProfile {
            vm,
            start_latency: 0.85,
            density: 0.8,
            isolation: 0.85,
            snapshotting: 0.8,
            agent_control: 0.5,
            evidence: vec![
                "modern Rust VMM: boot near Firecracker with a richer (but still lean) device model",
                "lightweight; strong density with slightly more overhead than Firecracker",
                "KVM isolation, and the VMM itself is memory-safe Rust",
                "live migration + snapshot/restore for warm clones",
                "REST / D-Bus control plane; capable but not agent-native",
            ],
        },
        Vm::Gvisor => VmProfile {
            vm,
            start_latency: 0.85,
            density: 0.85,
            isolation: 0.6,
            snapshotting: 0.4,
            agent_control: 0.55,
            evidence: vec![
                "userspace kernel (the Sentry) intercepts guest syscalls: container-fast start, no full boot",
                "container-class density; runs as an OCI runtime (runsc)",
                "smaller attack surface than raw namespaces, but it is a userspace kernel — a Sentry bug is host-reachable, not hardware-contained",
                "checkpoint/restore exists but is limited/experimental versus first-class VM snapshots",
                "Docker/Kubernetes drive it via OCI; convenient, but the control plane is not agent-native",
            ],
        },
        Vm::KataContainers => VmProfile {
            vm,
            start_latency: 0.65,
            density: 0.6,
            isolation: 0.85,
            snapshotting: 0.4,
            agent_control: 0.6,
            evidence: vec![
                "OCI/CRI containers backed by a per-workload microVM: hardware isolation with the container UX",
                "per-pod VM overhead sits above namespaces — density between a microVM and a container",
                "hardware-virt isolation per workload — strong for untrusted code",
                "templating exists, but live snapshot/branch is limited",
                "Kubernetes/containerd-native (RuntimeClass); standard tooling, not agent-native",
            ],
        },
        Vm::QemuKvm => VmProfile {
            vm,
            start_latency: 0.4,
            density: 0.45,
            isolation: 0.9,
            snapshotting: 0.85,
            agent_control: 0.45,
            evidence: vec![
                "full machine virtualization: rich device model but multi-second boot (the microvm machine type helps; the base is heavy)",
                "full per-VM overhead → far lower density than microVMs for ephemeral agent sandboxes",
                "the mature hardware-virt reference — though the large device-model attack surface has a long CVE history",
                "mature savevm snapshots + live migration: strong state capture",
                "libvirt/QMP is powerful but heavy, and not designed for an agent to drive directly",
            ],
        },
        Vm::Docker => VmProfile {
            vm,
            start_latency: 0.95,
            density: 0.95,
            isolation: 0.35,
            snapshotting: 0.4,
            agent_control: 0.6,
            evidence: vec![
                "near-instant container start — the fastest spawn of the set",
                "shared-kernel containers: the highest density (minimal per-instance overhead)",
                "namespaces + cgroups + seccomp share the host kernel — a single kernel LPE escapes the sandbox; widely considered insufficient for untrusted agent-generated code",
                "image layers / commit capture the filesystem, not live memory; CRIU checkpoint is niche",
                "the Docker API is what most agent-sandbox stacks drive today (ubiquitous tooling) — control is excellent, isolation is the weak point",
            ],
        },
    }
}

/// Profiles for all systems, in [`Vm::all`] order (deterministic).
pub fn profiles() -> Vec<VmProfile> {
    Vm::all().iter().map(|&v| profile(v)).collect()
}

/// All profiles ranked best-first by [`VmProfile::fitness`]
/// (stable order on ties).
pub fn rank_vms() -> Vec<VmProfile> {
    let mut v = profiles();
    v.sort_by(|a, b| {
        b.fitness()
            .partial_cmp(&a.fitness())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v
}

/// Compare two systems: positive deltas mean `a` fits agentic use better.
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[derive(Debug, Clone)]
pub struct VmComparison {
    /// First system (the subject).
    pub a: VmProfile,
    /// Second system (the baseline).
    pub b: VmProfile,
    /// `a.fitness() - b.fitness()`.
    pub fitness_delta: f64,
    /// Axis name → delta (a − b), in fixed axis order.
    pub axis_deltas: Vec<(&'static str, f64)>,
}

/// Compare system `a` against baseline `b` across all five axes.
pub fn compare_vms(a: Vm, b: Vm) -> VmComparison {
    let pa = profile(a);
    let pb = profile(b);
    let axis_deltas = vec![
        ("start-latency", pa.start_latency - pb.start_latency),
        ("density", pa.density - pb.density),
        ("isolation", pa.isolation - pb.isolation),
        ("snapshotting", pa.snapshotting - pb.snapshotting),
        ("agent-control", pa.agent_control - pb.agent_control),
    ];
    VmComparison {
        fitness_delta: pa.fitness() - pb.fitness(),
        a: pa,
        b: pb,
        axis_deltas,
    }
}

impl std::fmt::Display for VmComparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "{} vs {}: fitness delta {:+.2}",
            self.a.vm.name(),
            self.b.vm.name(),
            self.fitness_delta
        )?;
        for (axis, d) in &self.axis_deltas {
            writeln!(f, "  {axis}: {d:+.2}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_vm_profiles_with_evidence() {
        for vm in Vm::all() {
            let p = profile(vm);
            assert!(
                p.evidence.len() >= 3,
                "{} needs ≥3 evidence lines",
                vm.name()
            );
            for s in [
                p.start_latency,
                p.density,
                p.isolation,
                p.snapshotting,
                p.agent_control,
            ] {
                assert!((0.0..=1.0).contains(&s), "{} score out of range", vm.name());
            }
        }
    }

    #[test]
    fn from_name_roundtrip_and_aliases() {
        for vm in Vm::all() {
            assert_eq!(Vm::from_name(vm.name()), Some(vm));
        }
        assert_eq!(Vm::from_name("FC"), Some(Vm::Firecracker));
        assert_eq!(Vm::from_name("kvm"), Some(Vm::QemuKvm));
        assert_eq!(Vm::from_name("runc"), Some(Vm::Docker));
        assert_eq!(Vm::from_name("hypermachine"), Some(Vm::AetherVm));
        assert_eq!(Vm::from_name("virtualbox"), None);
    }

    #[test]
    fn ranking_is_deterministic_and_sorted() {
        let r1 = rank_vms();
        let r2 = rank_vms();
        let n1: Vec<_> = r1.iter().map(|p| p.vm.name()).collect();
        let n2: Vec<_> = r2.iter().map(|p| p.vm.name()).collect();
        assert_eq!(n1, n2);
        for w in r1.windows(2) {
            assert!(w[0].fitness() >= w[1].fitness());
        }
    }

    #[test]
    fn axis_judgments_hold_directionally() {
        let aether = profile(Vm::AetherVm);
        let fc = profile(Vm::Firecracker);
        let qemu = profile(Vm::QemuKvm);
        let docker = profile(Vm::Docker);
        let gvisor = profile(Vm::Gvisor);
        assert!(
            fc.start_latency > qemu.start_latency,
            "a microVM cold-starts far faster than a full machine VM"
        );
        assert!(
            qemu.isolation > docker.isolation,
            "hardware virtualization beats a shared host kernel for untrusted code"
        );
        assert!(
            docker.density > qemu.density,
            "shared-kernel containers pack far denser than full VMs"
        );
        assert!(
            aether.agent_control > fc.agent_control,
            "an MCP-native control plane beats a bring-your-own REST socket"
        );
        assert!(
            fc.isolation > gvisor.isolation,
            "hardware isolation beats a userspace kernel for untrusted code"
        );
        assert!(
            aether.snapshotting >= qemu.snapshotting,
            "first-class CoW branching is at least as strong as savevm snapshots"
        );
    }

    #[test]
    fn comparison_deltas_are_consistent() {
        let cmp = compare_vms(Vm::AetherVm, Vm::Firecracker);
        let sum: f64 = cmp.axis_deltas.iter().map(|(_, d)| d).sum();
        assert!((sum / 5.0 - cmp.fitness_delta).abs() < 1e-9);
        assert!(format!("{cmp}").contains("aethervm vs firecracker"));
    }
}
