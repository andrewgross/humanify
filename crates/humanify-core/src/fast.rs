//! The naming schedule (docs/rust-port/20-fast-mode.md). Since the
//! 2026-09-28 flip the DEFAULT is [`FastTier::Relaxed`] with every lever
//! on — no flag selects it. `--sequential` (humanify-cli) selects
//! [`FastTier::Exact`], the conservative schedule: the relaxed levers OFF,
//! the byte-identical parallelization kept — the pre-flip default path's
//! bytes at the same `--batch-size`. Individual levers stay selectable
//! with the rust-only `--relaxed-levers <list>` (hidden from help; the
//! `--pipeline-arg` sizing path). Both tiers are DETERMINISTIC: the same
//! input and the same model answers give the same bytes, run after run.
//!
//! - [`FastTier::Exact`] only reorders or parallelizes work whose outcome
//!   cannot depend on it: it ships the conservative path's bytes.
//! - [`FastTier::Relaxed`] adds levers that may change decisions (which call
//!   sees which names, how work is batched) — every change still merges in a
//!   canonical, input-derived order, and its quality is the eval's verdict
//!   (novel/realLn exact, reducible KPIs inside their bands).

/// One decision-changing lever of the relaxed tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Lever {
    /// Every naming window is its own lane: a function's identifiers are
    /// split into lanes of at most `batch_size`, so a lane's call chain is
    /// one window (+ its retries) instead of every window in sequence.
    /// Cross-window name clashes resolve at the barrier (the multi-lane
    /// path every large function already takes).
    WindowLanes,
    /// The shadowed-binding pass (round B) of wave N rides with round A of
    /// wave N+1 instead of costing a round of its own; its prompts read the
    /// same post-barrier-A state, but wave N+1's prompts no longer see its
    /// renames.
    DeferShadowed,
}

impl Lever {
    pub const ALL: [Lever; 2] = [Lever::WindowLanes, Lever::DeferShadowed];

    pub fn name(self) -> &'static str {
        match self {
            Lever::WindowLanes => "window-lanes",
            Lever::DeferShadowed => "defer-shadowed",
        }
    }
}

/// A set of relaxed levers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Levers(u32);

impl Levers {
    pub fn all() -> Levers {
        Lever::ALL.iter().fold(Levers(0), |s, &l| s.with(l))
    }

    pub fn with(self, l: Lever) -> Levers {
        Levers(self.0 | (1 << l as u32))
    }

    pub fn has(self, l: Lever) -> bool {
        self.0 & (1 << l as u32) != 0
    }

    /// `a,b` lever names; unknown names are an error listing the valid set.
    pub fn parse(list: &str) -> Result<Levers, String> {
        let mut out = Levers(0);
        for name in list.split(',').filter(|s| !s.is_empty()) {
            let l = Lever::ALL
                .iter()
                .find(|l| l.name() == name)
                .ok_or_else(|| {
                    let valid: Vec<&str> = Lever::ALL.iter().map(|l| l.name()).collect();
                    format!(
                        "unknown relaxed lever \"{name}\" (valid: {})",
                        valid.join(", ")
                    )
                })?;
            out = out.with(*l);
        }
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum FastTier {
    /// The turn-driver path: no pipelining at all. Byte-identical to
    /// [`FastTier::Exact`] (proven, e2e legacy goldens) and reachable
    /// programmatically only — the CLI offers Exact as `--sequential`.
    #[default]
    Off,
    /// Byte-identical to `Off`, faster: `--sequential`.
    Exact,
    /// The exact tier plus these decision-changing levers — the shipped
    /// default (every lever) or a `--relaxed-levers` subset (for sizing).
    Relaxed(Levers),
}

impl FastTier {
    /// Any fast tier: the byte-identical levers are on.
    pub fn on(self) -> bool {
        self != FastTier::Off
    }

    /// Whether a relaxed lever is on.
    pub fn lever(self, l: Lever) -> bool {
        matches!(self, FastTier::Relaxed(set) if set.has(l))
    }
}

#[cfg(test)]
mod fast_test {
    use super::*;

    #[test]
    fn levers_parse() {
        let all = Levers::all();
        assert!(Lever::ALL.iter().all(|&l| all.has(l)));
        let one = Levers::parse("window-lanes").unwrap();
        assert!(one.has(Lever::WindowLanes) && !one.has(Lever::DeferShadowed));
        let both = Levers::parse("window-lanes,defer-shadowed").unwrap();
        assert_eq!(both, all);
        assert!(Levers::parse("nope").is_err());
        assert!(!FastTier::Exact.lever(Lever::WindowLanes));
        assert!(!FastTier::Off.on());
        assert!(FastTier::Relaxed(Levers::all()).on());
    }
}
