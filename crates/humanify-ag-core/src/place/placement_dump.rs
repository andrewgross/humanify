//! The placement regime dispatch (`stableSplitFromCode`'s branch choice):
//! [`assign_regime`] runs the fossil / prior-tiers / fresh-cluster
//! placement for the split stage ([`crate::emit::stable_split`]).

use super::assign::cluster::{ClusterNamers, ClusterSizing, assign_clustered};
use super::assign::fossil::{FossilOptions, FossilSplitError, MIN_FOLDER_FILES, assign_fossil};
use super::assign::namer::{SplitNamer, TreeReviser};
use super::input::SplitInput;
use super::ledger::FossilLedgerModule;
use super::ledger::StableSplitLedger;
use super::tiers::{
    PlacementSwitches, PriorCarry, TierInput, TierStats, Unclaimed, assign_with_prior,
    placement_summary,
};
use super::trail::{PlacementTrail, TrailEntry};

/// Which `stableSplitFromCode` branch to run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Regime {
    /// The bundle's module fossils (`options.fossil`).
    Fossil,
    /// The prior-carried `PLACEMENT_TIERS` (`options.prior`, no fossils).
    Tiers,
    /// The fresh seam-clustered grouping (no prior, no fossils).
    Cluster,
}

/// What placement is asked to do. The namers take their own lifetime: the
/// mutable references are invariant, and the gate otherwise mixes the
/// split's borrowed data with them.
pub struct PlacementGate<'a, 'n> {
    pub regime: Regime,
    /// The tiers' carry (the naming stage's `PriorCarry`, in-process).
    pub carry: Option<PriorCarry>,
    pub switches: PlacementSwitches,
    /// The cluster regime's file+folder namer (the fossil regime names
    /// files by their module names, read from the wrappers — no namer).
    pub namer: Option<&'n mut dyn SplitNamer>,
    /// The cluster regime's holistic top-level reviser.
    pub reviser: Option<&'n mut dyn TreeReviser>,
    /// The run's module wrapper grammar (P3): the module helper the
    /// cluster regime's vendor bucket takes (review R7).
    pub module_wrappers: crate::toolchain::ModuleWrapperGrammar,
    /// The cluster regime's forced file ends (body indexes): the module
    /// markers' boundaries when the markers do not describe the whole
    /// bundle ([`crate::place::method`]). Empty: none.
    pub module_ends: Vec<usize>,
    /// The names mode's placed modules (library-names plan step 4,
    /// exp156): `(module index, vendor path)` the fossil regime
    /// pre-fills before any match or fresh name. The split refuses a
    /// non-empty layout under any other regime.
    pub library: &'a [(usize, String)],
    /// The factory statements (body indexes) the unpack kept as the app's
    /// OWN modules (exp172): the cluster regime groups them with the app
    /// instead of vendoring them. Empty: none.
    pub app_factories: std::collections::HashSet<usize>,
}

/// Why a placement regime could not place the bundle.
#[derive(Debug)]
pub enum PlaceError {
    /// The fossil regime failed.
    Fossil(FossilSplitError),
    /// The tiers regime was asked for without a prior ledger.
    NoPriorLedger,
    /// The prior-carried tiers left a statement unplaced.
    Unclaimed(Unclaimed),
}

impl std::fmt::Display for PlaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fossil(e) => e.fmt(f),
            Self::NoPriorLedger => f.write_str("the tiers regime needs --prior-ledger"),
            Self::Unclaimed(e) => e.fmt(f),
        }
    }
}

impl From<FossilSplitError> for PlaceError {
    fn from(e: FossilSplitError) -> Self {
        Self::Fossil(e)
    }
}

impl From<Unclaimed> for PlaceError {
    fn from(e: Unclaimed) -> Self {
        Self::Unclaimed(e)
    }
}

/// What one placement regime decided.
#[derive(Debug, Default)]
pub struct Placed {
    /// The file per wrapper statement, bundle order.
    pub assignment: Vec<String>,
    /// The regime's summary line (the run log's).
    pub summary: String,
    /// The fossil regime's `fossilModules` (the next hop's match targets).
    pub fossil_modules: Option<Vec<FossilLedgerModule>>,
    /// The prior-carried tiers' counters (`StableSplitStats`' transfer
    /// half); all zero for the fossil and fresh regimes
    /// (`zeroTransferStats`).
    pub tier_stats: TierStats,
}

/// Run one `stableSplitFromCode` placement regime over the split input:
/// the per-statement file assignment and the regime's summary line.
pub fn assign_regime(
    input: &SplitInput,
    shipped: &str,
    gate: PlacementGate<'_, '_>,
    prior: Option<&StableSplitLedger>,
    trail: &mut PlacementTrail,
) -> Result<Placed, PlaceError> {
    let summary;
    let mut fossil_modules = None;
    let mut tier_stats = TierStats::default();
    let assignment = match gate.regime {
        Regime::Fossil => {
            let assigned = assign_fossil(
                &input.body,
                &input.spans,
                &input.hashes,
                prior,
                FossilOptions {
                    min_folder_files: MIN_FOLDER_FILES,
                    trail: Some(trail),
                    library: gate.library,
                },
            )?;
            let s = &assigned.stats;
            summary = format!(
                "{} modules ({} inherited, {} kept by content, {} fresh-named, {} module-named), {} statements kept in their prior file, {} eager",
                s.modules,
                s.inherited_files,
                s.kept_files,
                s.fresh_named_files,
                s.module_named_files,
                s.kept_statements,
                s.eager_statements
            );
            fossil_modules = Some(assigned.fossil_modules);
            assigned.assignment
        }
        Regime::Cluster => {
            let module_helper = gate.module_wrappers.identify_factory_helper(shipped);
            let assignment = assign_clustered(
                &input.body,
                Some((shipped, input.spans.as_slice())),
                module_helper.as_ref().map(|h| h.name.as_str()),
                &gate.app_factories,
                // Scaled to the app (finding B1): exactly the old
                // constants at Claude Code's size.
                ClusterSizing::ScaledToApp,
                &gate.module_ends,
                ClusterNamers {
                    namer: gate.namer,
                    reviser: gate.reviser,
                },
            );
            // The fresh path records no placement trail in the TS (only
            // the fossil and prior-carried regimes do): the gate compares
            // the assignment itself, one row per statement.
            for (i, file) in assignment.iter().enumerate() {
                trail.record(TrailEntry {
                    index: i,
                    span: Some(input.spans[i]),
                    names: Vec::new(),
                    placed_by: "cluster".to_string(),
                    file: file.clone(),
                    ..TrailEntry::default()
                });
            }
            summary = "fresh grouping".to_string();
            assignment
        }
        Regime::Tiers => {
            let prior = prior.ok_or(PlaceError::NoPriorLedger)?;
            let carry = gate.carry;
            let (assignment, stats) = assign_with_prior(
                &TierInput {
                    body: &input.body,
                    spans: &input.spans,
                    hashes: &input.hashes,
                    code: shipped,
                    prior,
                    carry: carry.as_ref(),
                    switches: gate.switches,
                },
                Some(trail),
            )?;
            summary = format!(
                "inherited {}/{} ({})",
                stats.inherited,
                input.body.len(),
                placement_summary(&stats)
            );
            tier_stats = stats;
            assignment
        }
    };
    Ok(Placed {
        assignment,
        summary,
        fossil_modules,
        tier_stats,
    })
}
