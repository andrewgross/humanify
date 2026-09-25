//! The WP5.1/5.2 gate's Rust-side dump: run placement on a TS dump's
//! SHIPPED text (the split's input — `artifactDump.texts.shipped =
//! renameResult.code`) against the prior release's split ledger, and write
//! a `placement.json` the differ compares (`compare --sections placement`).
//! Independent of naming parity by construction: the shipped text already
//! carries the TS's settled names. Migration scaffolding — deleted at
//! phase 6 with the TS core.
//!
//! The one seam (lesson 16): statement-hash BYTES differ between the two
//! implementations by design (02 §4a), and placement reads them in two
//! places — a prior ledger's hashes (TS-written) and a declaration-less
//! module's `module-<hash8>` stem. [`inject_ts_statement_hashes`] first
//! PROVES the Rust and TS statement partitions are one partition (a
//! bijection between the classes, statement by statement), then
//! substitutes the TS bytes. Everything after it is the Rust's own
//! decision-making.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::dump::{PartitionsFile, PromptRecord};
use humanify_model::llm::{CacheKeyParams, LlmCall, cache_key_of};
use serde_json::Value;

use super::assign::cluster::{ClusterNamers, DEFAULT_CLUSTER_CONFIG, assign_clustered};
use super::assign::fossil::{FossilOptions, MIN_FOLDER_FILES, assign_fossil};
use super::assign::namer::{SplitNamer, TreeReviser};
use super::input::{SplitInput, split_input, top_level_statement_texts};
use super::ledger::FossilLedgerModule;
use super::ledger::{StableSplitLedger, read_ledger};
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

/// What the verb was asked to do.
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
    /// Substitute the TS statement-hash bytes from the dump's
    /// partitions.json after proving the bijection.
    pub inject_ts_hashes: bool,
}

/// What the verb did.
#[derive(Debug, Default)]
pub struct PlacementReport {
    pub rows: usize,
    pub files: usize,
    /// (statements, classes) proven bijective, when injected.
    pub injected: Option<(usize, usize)>,
    /// The regime's own summary (the run log's line).
    pub summary: String,
}

/// Prove the Rust statement partition equals the TS one and return the
/// TS's bytes per statement (lesson 16's injection).
pub fn inject_ts_statement_hashes(
    input: &SplitInput,
    ts_partitions: &PartitionsFile,
) -> Result<(Vec<String>, usize), String> {
    let family = ts_partitions
        .families
        .iter()
        .find(|f| f.family == "statementHash")
        .ok_or("partitions.json has no statementHash family")?;
    let by_span: HashMap<(i64, i64), &str> = family
        .members
        .iter()
        .map(|m| ((m.member.start, m.member.end), m.hash.as_str()))
        .collect();
    if by_span.len() != input.spans.len() {
        return Err(format!(
            "statementHash family has {} members for {} statements",
            by_span.len(),
            input.spans.len()
        ));
    }
    let mut rust_to_ts: HashMap<&str, &str> = HashMap::new();
    let mut ts_to_rust: HashMap<&str, &str> = HashMap::new();
    let mut out = Vec::with_capacity(input.spans.len());
    for (i, &(start, end)) in input.spans.iter().enumerate() {
        let ts = *by_span
            .get(&(i64::from(start), i64::from(end)))
            .ok_or_else(|| format!("statement {i} [{start}..{end}) has no TS hash"))?;
        let rust = input.hashes[i].as_str();
        if *rust_to_ts.entry(rust).or_insert(ts) != ts
            || *ts_to_rust.entry(ts).or_insert(rust) != rust
        {
            return Err(format!(
                "statement {i} [{start}..{end}): the hash partitions differ (not a bijection)"
            ));
        }
        out.push(ts.to_string());
    }
    Ok((out, rust_to_ts.len()))
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Run placement over a TS dump and write `<out>/meta.json` +
/// `<out>/placement.json`.
pub fn dump_placement(
    ts_dump_dir: &Path,
    out_dir: &Path,
    gate: PlacementGate,
) -> Result<PlacementReport, String> {
    let meta: Value = read_json(&ts_dump_dir.join("meta.json"))?;
    let shipped = fs::read_to_string(ts_dump_dir.join("text").join("shipped.js"))
        .map_err(|e| format!("shipped text: {e}"))?;
    let mut input = split_input(&shipped)?;
    let mut report = PlacementReport::default();
    if gate.inject_ts_hashes {
        let partitions: PartitionsFile = read_json(&ts_dump_dir.join("partitions.json"))?;
        let (ts_hashes, classes) = inject_ts_statement_hashes(&input, &partitions)?;
        report.injected = Some((ts_hashes.len(), classes));
        input.hashes = ts_hashes;
    }
    let prior: Option<StableSplitLedger> =
        gate.prior_ledger.as_deref().map(read_ledger).transpose()?;
    let mut trail = PlacementTrail::default();
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    let Placed {
        assignment,
        summary,
        ..
    } = assign_regime(
        &input,
        &shipped,
        gate,
        prior.as_ref(),
        &mut trail,
        Some(out_dir),
    )?;
    report.summary = summary;
    let file = trail.to_placement_file();
    report.rows = file.placements.len();
    let mut files: Vec<&str> = assignment.iter().map(String::as_str).collect();
    files.sort_unstable();
    files.dedup();
    report.files = files.len();
    fs::write(
        out_dir.join("meta.json"),
        serde_json::to_string(&meta).expect("json"),
    )
    .map_err(|e| format!("write meta: {e}"))?;
    fs::write(
        out_dir.join("placement.json"),
        serde_json::to_string(&file).expect("json"),
    )
    .map_err(|e| format!("write placement: {e}"))?;
    Ok(report)
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

/// Compare the namer calls the Rust dispatched with the TS dump's
/// `prompts.jsonl` rows for `function_id` (in dispatch order): system
/// prompt, user prompt, identifiers and the cache key must be the TS's
/// bytes. Returns one line per divergence.
pub fn check_dispatched_prompts(
    dispatched: &[LlmCall],
    prompts_jsonl: &Path,
    function_id: &str,
    params: &CacheKeyParams,
) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(prompts_jsonl).map_err(|e| format!("prompts.jsonl: {e}"))?;
    let mut expected: Vec<PromptRecord> = Vec::new();
    for line in text.lines().filter(|l| l.contains(function_id)) {
        let row: PromptRecord =
            serde_json::from_str(line).map_err(|e| format!("prompts.jsonl row: {e}"))?;
        if row.function_id == function_id {
            expected.push(row);
        }
    }
    let mut out = Vec::new();
    if expected.len() != dispatched.len() {
        out.push(format!(
            "{function_id}: {} TS dispatch(es), {} Rust",
            expected.len(),
            dispatched.len()
        ));
    }
    for (k, (ts, rust)) in expected.iter().zip(dispatched).enumerate() {
        let key = cache_key_of(&rust.request, params);
        let checks = [
            ("systemPrompt", ts.system_prompt == rust.system_prompt),
            ("userPrompt", ts.user_prompt == rust.user_prompt),
            ("identifiers", ts.identifiers == rust.request.identifiers),
            ("cacheKey", ts.cache_key == key),
        ];
        for (field, ok) in checks {
            if !ok {
                out.push(format!("{function_id} dispatch {k}: {field} differs"));
            }
        }
    }
    Ok(out)
}
