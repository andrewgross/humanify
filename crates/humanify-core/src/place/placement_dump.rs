//! The placement regime dispatch (`stableSplitFromCode`'s branch choice):
//! [`assign_regime`] runs the fossil / prior-tiers / fresh-cluster
//! placement for the split stage ([`crate::emit::stable_split`]).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::assign::cluster::{ClusterNamers, DEFAULT_CLUSTER_CONFIG, assign_clustered};
use super::assign::fossil::{FossilOptions, MIN_FOLDER_FILES, assign_fossil};
use super::assign::namer::{SplitNamer, TreeReviser};
use super::input::{SplitInput, top_level_statement_texts};
use super::ledger::FossilLedgerModule;
use super::ledger::StableSplitLedger;
use super::tiers::{
    PlacementSwitches, PriorCarry, TierInput, TierStats, assign_with_prior, placement_summary,
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

/// What placement is asked to do.
pub struct PlacementGate<'a> {
    pub regime: Regime,
    /// The prior release's `split-ledger.json` (the oracle run's
    /// `--prior-version` sibling).
    pub prior_ledger: Option<PathBuf>,
    /// The prior release's humanified text (the tiers' content-anchor
    /// carry: its top-level statement texts).
    pub prior_text: Option<PathBuf>,
    /// The rename matcher's final-name → prior-name map (the tiers'
    /// binding-identity carry; the TS debug-writes it as
    /// `.humanify/prior-match-map.json`).
    pub match_map: Option<PathBuf>,
    /// The carry handed over in-process (the pipeline's: the naming
    /// stage's `PriorCarry`); wins over `prior_text` / `match_map`.
    pub carry: Option<PriorCarry>,
    pub switches: PlacementSwitches,
    /// The mint namer (fossil) / file+folder namer (cluster) — a
    /// replay-only cache client in the gate.
    pub namer: Option<&'a mut dyn SplitNamer>,
    /// The cluster regime's holistic top-level reviser.
    pub reviser: Option<&'a mut dyn TreeReviser>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
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
/// the per-statement file assignment and the regime's summary line. With
/// `out_dir`, the fossil regime also writes its `fossil-modules.json`.
pub fn assign_regime(
    input: &SplitInput,
    shipped: &str,
    gate: PlacementGate,
    prior: Option<&StableSplitLedger>,
    trail: &mut PlacementTrail,
    out_dir: Option<&Path>,
) -> Result<Placed, String> {
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
                    mint_namer: gate.namer,
                    trail: Some(trail),
                },
            )?;
            let s = &assigned.stats;
            summary = format!(
                "{} modules ({} inherited, {} fresh-named, {} llm-named), {} eager",
                s.modules,
                s.inherited_files,
                s.fresh_named_files,
                s.llm_named_mints,
                s.eager_statements
            );
            // The next hop's match targets (tokens included) — diffable
            // against the TS ledger's `fossilModules`.
            if let Some(out_dir) = out_dir {
                fs::write(
                    out_dir.join("fossil-modules.json"),
                    serde_json::to_string(&assigned.fossil_modules).expect("json"),
                )
                .map_err(|e| format!("write fossil modules: {e}"))?;
            }
            fossil_modules = Some(assigned.fossil_modules);
            assigned.assignment
        }
        Regime::Cluster => {
            let assignment = assign_clustered(
                &input.body,
                Some((shipped, input.spans.as_slice())),
                &DEFAULT_CLUSTER_CONFIG,
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
            let prior = prior.ok_or("the tiers regime needs --prior-ledger")?;
            let carry = match gate.carry {
                Some(c) => Some(c),
                None => read_carry(gate.prior_text.as_deref(), gate.match_map.as_deref())?,
            };
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

/// The tiers' `PriorCarry` from the prior text + the match-map JSON (a
/// JSON object of final name → prior name); absent when neither is given.
fn read_carry(
    prior_text: Option<&Path>,
    match_map: Option<&Path>,
) -> Result<Option<PriorCarry>, String> {
    let Some(prior_text) = prior_text else {
        return Ok(None);
    };
    let text = fs::read_to_string(prior_text).map_err(|e| format!("prior text: {e}"))?;
    let statement_texts = top_level_statement_texts(&text)?;
    let match_map: HashMap<String, String> = match match_map {
        Some(path) => read_json(path)?,
        None => HashMap::new(),
    };
    Ok(Some(PriorCarry {
        statement_texts,
        match_map,
    }))
}
