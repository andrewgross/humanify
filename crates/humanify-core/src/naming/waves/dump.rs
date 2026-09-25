//! The phase-4 step-1 gate's dump (WP4.3): the naming driver
//! (`naming::driver`, WP4.6 — the ONE orchestration of parse → graph →
//! matching → the phase-3 transfer stage → the LLM waves) stopped at the
//! wave boundary over a TS dump's texts, with the LLM answered by WARM
//! REPLAY of a cache, writing what the TS dump writes for the waves —
//! `prompts.jsonl`, `cache-keys.jsonl` and `names.json` — for
//! `humanify-parity compare`. Migration scaffolding — deleted at phase 6
//! with the TS core (02 §9).
//!
//! `names.json` here is the WAVE-BOUNDARY table: the strategy trail's rows
//! (every tier that settled a binding, through the LLM) merged with the
//! recorded rows (LLM applies, uniquify, identity) — the recorded row wins
//! on a span collision, as the TS writer merges. Post-wave passes
//! (reconcile, floor, sweep — WP4.4/4.5) are not run.

use std::fs;
use std::path::Path;

use humanify_model::llm::{CacheKeyParams, NameProvider};
use serde_json::{Value, json};

use crate::matching::matches_dump::read_dump_texts;
use crate::naming::driver::NamingHooks;
use crate::naming::driver::dump::{dispatch_rows, run_dump, wave_boundary_names};
use crate::prior::{PriorMatchInput, match_prior_version};
use crate::rename::validated::RenameState;
use crate::trail::Anchor;

use super::generate::TextView;
use super::graph_ext::build_naming_graph;
use super::probe::graph_probe_lines;
use super::render::{FnPrinter, Occurrences};

/// What the dump verb does.
#[derive(Clone, Debug, Default)]
pub struct WavesDumpOptions {
    /// Write only the naming graph's bisection probe (graph-probe.jsonl),
    /// over the ORIGINAL names — no transfer, no waves.
    pub probe_graph: bool,
    /// Session ids whose full code/body the probe writes.
    pub probe_only: Vec<String>,
    /// The cache the waves replay (opened read-only).
    pub llm_cache: Option<std::path::PathBuf>,
    /// A planted order bug (the gate's red runs).
    pub plant: Option<super::processor::Plant>,
}

/// What the dump reports.
#[derive(Clone, Debug, Default)]
pub struct WavesDumpSummary {
    pub probe_rows: usize,
    pub dispatches: usize,
    pub names: usize,
    pub misses: usize,
    pub errors: usize,
    pub waves: u64,
}

/// The cache-key params of the dump's run (meta.json flags; temperature is
/// the TS's literal 0; maxTokens is not configured on the oracle runs).
pub fn cache_params_of(meta: &Value) -> CacheKeyParams {
    let flags = &meta["flags"];
    CacheKeyParams {
        model: flags["model"].as_str().unwrap_or_default().to_string(),
        temperature: Some(0.0),
        max_tokens: flags["maxTokens"].as_u64(),
        reasoning_effort: flags["reasoningEffort"].as_str().map(str::to_string),
    }
}

/// Run the naming driver to the wave boundary on a TS dump's texts and
/// write the waves' dump files (or, with `probe_graph`, only the naming
/// graph's bisection probe). `provider` builds the LLM seam from the run's
/// cache-key params (the CLI's replay-only client).
pub fn dump_waves<P: NameProvider>(
    ts_dump_dir: &Path,
    out_dir: &Path,
    options: &WavesDumpOptions,
    provider: impl FnOnce(CacheKeyParams) -> P,
) -> Result<WavesDumpSummary, String> {
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    if options.probe_graph {
        return probe_graph(ts_dump_dir, out_dir, options);
    }
    let hooks = NamingHooks {
        stop_after_waves: true,
        wave_plant: options.plant,
        ..NamingHooks::default()
    };
    let (run, out) = run_dump(ts_dump_dir, &hooks, provider)?;
    let params = cache_params_of(&run.meta);
    let (prompts, keys) = dispatch_rows(&out.waves.dispatches, &[], &params);
    fs::write(out_dir.join("prompts.jsonl"), prompts).map_err(|e| format!("prompts: {e}"))?;
    fs::write(out_dir.join("cache-keys.jsonl"), keys).map_err(|e| format!("keys: {e}"))?;
    let rows = out.trail.transfer_rows();
    fs::write(
        out_dir.join("transfers-waves.json"),
        json!({"schemaVersion": 1, "transfers": rows}).to_string(),
    )
    .map_err(|e| format!("transfers: {e}"))?;
    let names = wave_boundary_names(&out.trail, &out.waves.names, &out.library_names, &run.fresh);
    let n = names.len();
    fs::write(
        out_dir.join("names.json"),
        json!({"schemaVersion": 1, "names": names}).to_string(),
    )
    .map_err(|e| format!("names: {e}"))?;
    Ok(WavesDumpSummary {
        probe_rows: 0,
        dispatches: out.waves.dispatches.len(),
        names: n,
        misses: out.waves.misses,
        errors: out.waves.errors,
        waves: out.waves.waves,
    })
}

/// The naming graph's bisection probe over the ORIGINAL names.
fn probe_graph(
    ts_dump_dir: &Path,
    out_dir: &Path,
    options: &WavesDumpOptions,
) -> Result<WavesDumpSummary, String> {
    let (meta, fresh, prior) = read_dump_texts(ts_dump_dir)?;
    let flags = &meta["flags"];
    let input = PriorMatchInput {
        fresh: &fresh,
        prior: &prior,
        bundler: flags["bundler"].as_str(),
        minifier: flags["minifier"].as_str(),
        visit_optional_calls: false,
    };
    match_prior_version(input, |stage| {
        let semantic = stage.fresh.ingest.semantic();
        let graph = stage.fresh.graph;
        let view = TextView::build(semantic);
        let ng = build_naming_graph(semantic, graph, &view);
        let fns = FnPrinter::nodes(semantic, graph);
        let state = RenameState::new(semantic, Anchor::Fresh);
        let occ = Occurrences::build(semantic, &state);
        let printer = FnPrinter {
            semantic,
            view: &view,
            graph,
            state: &state,
            occ: &occ,
            fns: &fns,
        };
        let lines = graph_probe_lines(semantic, graph, &ng, &view, &printer, &options.probe_only);
        let mut text = lines.join("\n");
        text.push('\n');
        fs::write(out_dir.join("graph-probe.jsonl"), text)
            .map_err(|e| format!("write graph-probe.jsonl: {e}"))?;
        Ok(WavesDumpSummary {
            probe_rows: lines.len(),
            ..WavesDumpSummary::default()
        })
    })
}
