//! Safety: given the effects a program performs, how much of its blast radius is
//! *gated* (requires approval, or denied) versus allowed under an agent policy?
//!
//! For an agent operating with real capabilities, the safety question is not "is
//! this code correct" but "what is the worst this can do, and is the dangerous
//! part gated?" This module classifies a program by the [`Effect`]s it performs,
//! applies a default-deny-for-dangerous agent [`Policy`], and scores how much of
//! the dangerous surface is held behind approval/denial. A program whose only
//! dangerous effects are approval-gated scores high; one that runs privileged or
//! executes arbitrary commands unconditionally scores low.

/// The effect class of an operation — the single property safety reasons about.
/// Ordered from harmless to most dangerous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Effect {
    /// No observable effect (pure computation).
    Pure,
    /// Reads local state (filesystem reads, env, process listing).
    ReadLocal,
    /// Creates/modifies local state non-destructively (write, mkdir).
    WriteLocal,
    /// Performs network I/O.
    Network,
    /// Affects other processes (kill, signal).
    Process,
    /// Irreversibly removes/overwrites local state (rm, truncate, drop).
    Destructive,
    /// Executes an arbitrary external command (shell passthrough).
    Exec,
    /// Requires elevated privileges / affects system-wide state.
    Privileged,
}

impl Effect {
    pub fn name(self) -> &'static str {
        match self {
            Effect::Pure => "pure",
            Effect::ReadLocal => "read_local",
            Effect::WriteLocal => "write_local",
            Effect::Network => "network",
            Effect::Process => "process",
            Effect::Destructive => "destructive",
            Effect::Exec => "exec",
            Effect::Privileged => "privileged",
        }
    }

    /// Parse an effect from its snake_case name (the inverse of [`Self::name`]).
    /// Accepts the same spellings other effect taxonomies use (e.g. AetherShell's
    /// `safety::Effect::as_str`), so a host system's effect classifier can be mapped
    /// straight in. Returns `None` for an unknown name.
    pub fn from_name(name: &str) -> Option<Effect> {
        Some(match name {
            "pure" => Effect::Pure,
            "read_local" => Effect::ReadLocal,
            "write_local" => Effect::WriteLocal,
            "network" => Effect::Network,
            "process" => Effect::Process,
            "destructive" => Effect::Destructive,
            "exec" => Effect::Exec,
            "privileged" => Effect::Privileged,
            _ => return None,
        })
    }

    /// Whether this class is "dangerous" — capable of irreversible or
    /// out-of-sandbox harm, so it *should* be gated for an agent.
    pub fn is_dangerous(self) -> bool {
        matches!(
            self,
            Effect::Destructive | Effect::Process | Effect::Exec | Effect::Privileged
        )
    }
}

/// Who is operating: a human at a REPL, or an autonomous agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Human,
    Agent,
}

/// The policy decision for an effect under a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Runs without friction.
    Allow,
    /// Refused unless an approval token / human-in-the-loop confirms.
    Approve,
    /// Refused outright, no approval path.
    Deny,
}

/// The default agent policy: humans get default-allow (great errors instead of
/// friction); agents get default-deny for the dangerous classes. This mirrors the
/// AetherShell agentic-first model so the score reflects a real, shipped policy.
pub fn decide(effect: Effect, mode: Mode) -> Decision {
    match mode {
        Mode::Human => Decision::Allow,
        Mode::Agent => match effect {
            Effect::Pure | Effect::ReadLocal | Effect::WriteLocal | Effect::Network => {
                Decision::Allow
            }
            Effect::Process | Effect::Destructive | Effect::Exec => Decision::Approve,
            Effect::Privileged => Decision::Deny,
        },
    }
}

/// The safety assessment of a program described by the effects it performs.
#[derive(Debug, Clone)]
pub struct SafetyReport {
    pub mode: Mode,
    pub effects: usize,
    pub allowed: usize,
    pub approval_gated: usize,
    pub denied: usize,
    /// Dangerous effects that the policy would let run *without* gating. For the
    /// default agent policy this is 0 (every dangerous class is gated/denied); a
    /// permissive policy could leave some ungated.
    pub dangerous_ungated: usize,
    /// True iff no dangerous effect is left ungated — the blast radius is bounded.
    pub bounded: bool,
    /// 0.0–1.0 safety score: the fraction of dangerous effects that are gated
    /// (approval or deny). 1.0 when there are no dangerous effects, or all are
    /// gated. Lower as more dangerous effects run unchecked.
    pub score: f64,
    /// A letter grade derived from `score` (A ≥ .9, B ≥ .75, C ≥ .5, D ≥ .25, F).
    pub grade: char,
}

/// Assess a program's safety from the effects it performs, under `mode`.
pub fn assess_safety(effects: &[Effect], mode: Mode) -> SafetyReport {
    let (mut allowed, mut approval_gated, mut denied, mut dangerous, mut dangerous_ungated) =
        (0, 0, 0, 0usize, 0usize);
    for &e in effects {
        let d = decide(e, mode);
        match d {
            Decision::Allow => allowed += 1,
            Decision::Approve => approval_gated += 1,
            Decision::Deny => denied += 1,
        }
        if e.is_dangerous() {
            dangerous += 1;
            if d == Decision::Allow {
                dangerous_ungated += 1;
            }
        }
    }
    let score = if dangerous == 0 {
        1.0
    } else {
        (dangerous - dangerous_ungated) as f64 / dangerous as f64
    };
    let grade = if score >= 0.9 {
        'A'
    } else if score >= 0.75 {
        'B'
    } else if score >= 0.5 {
        'C'
    } else if score >= 0.25 {
        'D'
    } else {
        'F'
    };
    SafetyReport {
        mode,
        effects: effects.len(),
        allowed,
        approval_gated,
        denied,
        dangerous_ungated,
        bounded: dangerous_ungated == 0,
        score,
        grade,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_policy_gates_every_dangerous_class() {
        // A program that reads, writes, deletes, execs, and needs privilege.
        let effects = [
            Effect::ReadLocal,
            Effect::WriteLocal,
            Effect::Destructive,
            Effect::Exec,
            Effect::Privileged,
        ];
        let r = assess_safety(&effects, Mode::Agent);
        assert!(r.bounded, "no dangerous effect left ungated");
        assert_eq!(r.dangerous_ungated, 0);
        assert_eq!(r.score, 1.0);
        assert_eq!(r.grade, 'A');
        assert_eq!(r.denied, 1); // privileged
        assert_eq!(r.approval_gated, 2); // destructive + exec
        assert_eq!(r.allowed, 2); // read + write
    }

    #[test]
    fn human_mode_allows_everything_so_dangerous_is_ungated() {
        let effects = [Effect::Destructive, Effect::Exec];
        let r = assess_safety(&effects, Mode::Human);
        assert_eq!(r.allowed, 2);
        assert!(
            !r.bounded,
            "human mode does not gate — blast radius unbounded"
        );
        assert_eq!(r.dangerous_ungated, 2);
        assert_eq!(r.score, 0.0);
        assert_eq!(r.grade, 'F');
    }

    #[test]
    fn pure_program_is_trivially_safe() {
        let r = assess_safety(&[Effect::Pure, Effect::ReadLocal], Mode::Agent);
        assert_eq!(r.score, 1.0); // no dangerous effects at all
        assert!(r.bounded);
        assert_eq!(r.grade, 'A');
    }

    #[test]
    fn effects_are_ordered_by_danger() {
        assert!(Effect::Pure < Effect::Destructive);
        assert!(Effect::Network < Effect::Privileged);
        assert!(Effect::Destructive.is_dangerous());
        assert!(!Effect::ReadLocal.is_dangerous());
    }

    #[test]
    fn from_name_round_trips_every_effect() {
        for e in [
            Effect::Pure,
            Effect::ReadLocal,
            Effect::WriteLocal,
            Effect::Network,
            Effect::Process,
            Effect::Destructive,
            Effect::Exec,
            Effect::Privileged,
        ] {
            assert_eq!(Effect::from_name(e.name()), Some(e));
        }
        assert_eq!(Effect::from_name("nonsense"), None);
    }
}
