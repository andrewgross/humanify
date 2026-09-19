//! Every pass switch, as data, with ONE predicate (TS original:
//! `src/kill-switches.ts`; the registry IS the single source of truth).
//!
//! Two generations of the same lesson are carried over: these were env vars
//! read inline at 14 sites in three incompatible predicates, then CLI flags
//! validated against the registry at arg-parse (an unknown name is fatal and
//! lists the valid ones). The Rust port makes the registry a CLOSED ENUM:
//! a typo is a compile error rather than a switch that silently never fires
//! — which is the failure mode this whole file is about.
//!
//! Unlike the TS module-global `active` set, the applied state is a VALUE
//! (`SwitchState`, `BTreeSet`-backed): humanify-core receives it as config
//! and never reads the process environment itself (02 §2).

use std::collections::BTreeSet;

/// What a switch does, and what established it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Switch {
    /// naming: the family-permute pass that rotates same-family names into
    /// their prior slots (exp048).
    FamilyPermute,
    /// naming: a per-close-pair shingle census (instrumentation; exp053).
    ShingleProbe,
    /// placement: the content-anchor tier — rare string literals identify a
    /// prior statement (exp041).
    ContentAnchor,
    /// placement: promoting the content anchor above the name vote when
    /// every declared name is a minted counter (exp042).
    AnchorPreempt,
    /// placement: the near-identical disjunct of the anchor preempt — a twin
    /// differing by a few lines out of hundreds (exp043).
    AnchorNearIdent,
    /// placement: the all-same tier, which rescues a statement whose
    /// declared names disagree but whose single-home voters are unanimous
    /// (exp041).
    AllSameVote,
    /// placement: the refusal to let the fingerprint claim a declaration
    /// with no initializers — its mask is only a declarator count (exp058).
    EmptyDeclHashGuard,
    /// placement: exempting export registrars from the load-order barrier
    /// (exp049).
    RegistrarExemption,
    /// emission: aligning within-file statement order to the prior release
    /// (exp037/038).
    EmitAlign,
    /// emission: keying the emission aligner on (hash, declared name)
    /// instead of hash alone (exp050).
    NameAlign,
    /// vendor: reusing a prior release's bytes for a library whose
    /// structural signature is unchanged (exp046).
    VendorInherit,
    /// vendor: emitting _bun-modules.json in the prior release's order
    /// (exp047).
    ManifestPriorOrder,
    /// post-tree: the per-file rename reconcile against the prior tree
    /// (exp054).
    PostSplitReconcile,
    /// placement: assigning statements by the bundle's own module fossils
    /// (bun __esm segments) with module-keyed file naming (exp070).
    FossilSplit,
}

impl Switch {
    /// The flag name on the wire (`--disable a,b` / `--probe c`), exactly
    /// the TS registry's keys.
    pub fn name(&self) -> &'static str {
        match self {
            Switch::FamilyPermute => "family-permute",
            Switch::ShingleProbe => "shingle-probe",
            Switch::ContentAnchor => "content-anchor",
            Switch::AnchorPreempt => "anchor-preempt",
            Switch::AnchorNearIdent => "anchor-nearident",
            Switch::AllSameVote => "allsame-vote",
            Switch::EmptyDeclHashGuard => "empty-decl-hash-guard",
            Switch::RegistrarExemption => "registrar-exemption",
            Switch::EmitAlign => "emit-align",
            Switch::NameAlign => "name-align",
            Switch::VendorInherit => "vendor-inherit",
            Switch::ManifestPriorOrder => "manifest-prior-order",
            Switch::PostSplitReconcile => "post-split-reconcile",
            Switch::FossilSplit => "fossil-split",
        }
    }

    /// The experiment that introduced or gated it — where the numbers live.
    pub fn since(&self) -> &'static str {
        match self {
            Switch::FamilyPermute => "exp048",
            Switch::ShingleProbe => "exp053",
            Switch::ContentAnchor => "exp041",
            Switch::AnchorPreempt => "exp042",
            Switch::AnchorNearIdent => "exp043",
            Switch::AllSameVote => "exp041",
            Switch::EmptyDeclHashGuard => "exp058",
            Switch::RegistrarExemption => "exp049",
            Switch::EmitAlign => "exp037/038",
            Switch::NameAlign => "exp050",
            Switch::VendorInherit => "exp046",
            Switch::ManifestPriorOrder => "exp047",
            Switch::PostSplitReconcile => "exp054",
            Switch::FossilSplit => "exp070",
        }
    }

    /// Every switch, in registry order (pipeline-stage order — how someone
    /// bisecting a regression reads it: naming, placement, emission, vendor,
    /// post-tree). One list, no per-kind re-derivation.
    pub const ALL: [Switch; 14] = [
        Switch::FamilyPermute,
        Switch::ShingleProbe,
        Switch::ContentAnchor,
        Switch::AnchorPreempt,
        Switch::AnchorNearIdent,
        Switch::AllSameVote,
        Switch::EmptyDeclHashGuard,
        Switch::RegistrarExemption,
        Switch::EmitAlign,
        Switch::NameAlign,
        Switch::VendorInherit,
        Switch::ManifestPriorOrder,
        Switch::PostSplitReconcile,
        Switch::FossilSplit,
    ];

    /// Whether a name is a registered switch at all (for validation
    /// messages and `--help`).
    pub fn by_name(name: &str) -> Option<Switch> {
        Switch::ALL.iter().find(|s| s.name() == name).copied()
    }

    /// A `disable`-kind switch; the one probe switch is the complement.
    pub fn is_disable(&self) -> bool {
        !matches!(self, Switch::ShingleProbe)
    }
}

/// The two switch kinds (kill-switches.ts:36-39). Separate flags so "list
/// of things I turned off" stays a true statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchKind {
    /// `--disable` turns a shipped pass OFF.
    Disable,
    /// `--probe` turns instrumentation ON.
    Probe,
}

/// The applied switches, built ONCE at the CLI boundary and passed by value
/// into the pipeline (the TS module-global `active`, made a value).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SwitchState {
    applied: BTreeSet<Switch>,
}

impl SwitchState {
    /// Apply the parsed `--disable`/`--probe` lists. Called ONCE from the
    /// CLI action. An unknown or wrong-kind name is an error with the valid
    /// list — a switch that could not take effect must never look accepted
    /// (the exact failure the env generation allowed: an exported typo was
    /// silently nothing). Blank segments are skipped, as in TS.
    pub fn configure(disable: &[String], probe: &[String]) -> Result<SwitchState, String> {
        let apply = |names: &[String],
                     kind: SwitchKind,
                     out: &mut BTreeSet<Switch>|
         -> Result<(), String> {
            for raw in names {
                let name = raw.trim();
                if name.is_empty() {
                    continue;
                }
                let entry = Switch::by_name(name);
                let wrong_kind = match (entry, kind) {
                    (Some(s), SwitchKind::Disable) if s.is_disable() => false,
                    (Some(s), SwitchKind::Probe) if !s.is_disable() => false,
                    _ => true,
                };
                if entry.is_none() || wrong_kind {
                    let flag = match kind {
                        SwitchKind::Disable => "--disable",
                        SwitchKind::Probe => "--probe",
                    };
                    return Err(format!(
                        "{flag}: unknown {} switch \"{name}\" — valid: {}",
                        match kind {
                            SwitchKind::Disable => "disable",
                            SwitchKind::Probe => "probe",
                        },
                        valid_names(kind).join(", ")
                    ));
                }
                out.insert(entry.unwrap());
            }
            Ok(())
        };
        let mut applied = BTreeSet::new();
        apply(disable, SwitchKind::Disable, &mut applied)?;
        apply(probe, SwitchKind::Probe, &mut applied)?;
        Ok(SwitchState { applied })
    }

    /// Whether a switch was applied. Typed to the registry, so a typo is a
    /// compile error. For `disable` switches, true means the pass is OFF;
    /// for `probe` switches, true means the probe is ON.
    pub fn switch_on(&self, name: Switch) -> bool {
        self.applied.contains(&name)
    }

    /// Every applied switch, sorted — for the run log / selection record, so
    /// a non-default run says so in its own recorded configuration.
    pub fn active(&self) -> Vec<Switch> {
        self.applied.iter().copied().collect()
    }
}

/// Names of a given kind, for validation messages and --help (the TS
/// `switchNames`).
pub fn valid_names(kind: SwitchKind) -> Vec<&'static str> {
    Switch::ALL
        .iter()
        .filter(|s| match kind {
            SwitchKind::Disable => s.is_disable(),
            SwitchKind::Probe => !s.is_disable(),
        })
        .map(|s| s.name())
        .collect()
}
