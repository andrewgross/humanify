//! The phase-4 step-1 gate's dump (WP4.3): run parse → graph → matching →
//! the phase-3 transfer stage → the LLM waves over a TS dump's texts, with
//! the LLM answered by WARM REPLAY of a cache, and write what the TS dump
//! writes for the waves — `prompts.jsonl`, `cache-keys.jsonl` and
//! `names.json` — for `humanify-parity compare`. Migration scaffolding —
//! deleted at phase 6 with the TS core (02 §9).

use std::fs;
use std::path::Path;

use crate::matching::matches_dump::read_dump_texts;
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
}

/// What the dump reports.
#[derive(Clone, Debug, Default)]
pub struct WavesDumpSummary {
    pub probe_rows: usize,
}

/// Run the stages on a TS dump's texts and write the waves' dump files.
pub fn dump_waves(
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
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;
    match_prior_version(input, |stage| {
        let semantic = stage.fresh.ingest.semantic();
        let graph = stage.fresh.graph;
        let view = TextView::build(semantic);
        if options.probe_graph {
            let ng = build_naming_graph(semantic, graph, &view);
            let state = RenameState::new(semantic, Anchor::Fresh);
            let occ = Occurrences::build(semantic, &state);
            let printer = FnPrinter::new(semantic, &view, graph, &state, &occ);
            let lines =
                graph_probe_lines(semantic, graph, &ng, &view, &printer, &options.probe_only);
            let mut text = lines.join("\n");
            text.push('\n');
            fs::write(out_dir.join("graph-probe.jsonl"), text)
                .map_err(|e| format!("write graph-probe.jsonl: {e}"))?;
            return Ok(WavesDumpSummary {
                probe_rows: lines.len(),
            });
        }
        Err("the waves run is not wired yet".to_string())
    })
}
