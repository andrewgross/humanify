//! `--fast [tier]` — the post-parity performance tiers
//! (docs/rust-port/20-fast-mode.md). Both tiers are DETERMINISTIC: the same
//! input and the same model answers give the same bytes, run after run.
//!
//! - [`FastTier::Exact`] only reorders or parallelizes work whose outcome
//!   cannot depend on it: it ships the default path's bytes.
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
    /// The parity-faithful default path.
    #[default]
    Off,
    /// Byte-identical to `Off`, faster.
    Exact,
    /// The exact tier plus these decision-changing levers
    /// (`relaxed` = every lever, `relaxed:a,b` = a subset — for sizing).
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

    /// `--fast`'s value: "" / "exact", "relaxed", "relaxed:<lever,...>".
    pub fn parse(value: &str) -> Result<FastTier, String> {
        match value {
            "" | "exact" => Ok(FastTier::Exact),
            "relaxed" => Ok(FastTier::Relaxed(Levers::all())),
            v => match v.strip_prefix("relaxed:") {
                Some(list) => Ok(FastTier::Relaxed(Levers::parse(list)?)),
                None => Err(format!(
                    "--fast must be one of: exact, relaxed, relaxed:<lever,...> (got \"{v}\")"
                )),
            },
        }
    }
}

#[cfg(test)]
mod fast_test {
    use super::*;

    #[test]
    fn tiers_parse() {
        assert_eq!(FastTier::parse(""), Ok(FastTier::Exact));
        assert_eq!(FastTier::parse("exact"), Ok(FastTier::Exact));
        let all = FastTier::parse("relaxed").unwrap();
        assert!(Lever::ALL.iter().all(|&l| all.lever(l)));
        let one = FastTier::parse("relaxed:window-lanes").unwrap();
        assert!(one.lever(Lever::WindowLanes) && !one.lever(Lever::DeferShadowed));
        assert!(FastTier::parse("relaxed:nope").is_err());
        assert!(FastTier::parse("bogus").is_err());
        assert!(!FastTier::Exact.lever(Lever::WindowLanes));
        assert!(!FastTier::Off.on());
    }
}
