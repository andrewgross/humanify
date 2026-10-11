//! exp095 discovery lane: the PIPELINE's library discovery
//! (`humanify_core::libdetect::discovery::discover_in_bundle`) on a raw
//! bundle, optionally seeded with the negatives lane's Step B claims (lab
//! unit ids mapped to their span starts through raw.jsonl).
//!
//! Usage: exp095-core-discover <bundle.js> <out.json> [<raw.jsonl> <stepb-pred.json>]
use std::time::Instant;

use humanify_core::hash::serialize::SymbolTables;
use humanify_core::ingest::Ingest;
use humanify_core::libdetect::discovery::{
    BundlePieces, DiscoveryOptions, DiscoveryReport, VetoSupport, discover_in_bundle,
};
use humanify_core::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};

fn index_claims(raw: &str, pred: &str) -> Vec<(u32, String)> {
    let starts: std::collections::HashMap<i64, u32> = std::fs::read_to_string(raw)
        .expect("raw")
        .lines()
        .map(|l| {
            let r: serde_json::Value = serde_json::from_str(l).expect("row");
            (r["i"].as_i64().unwrap(), r["s"].as_u64().unwrap() as u32)
        })
        .collect();
    let pred: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pred).expect("pred")).expect("json");
    let mut out: Vec<(u32, String)> = pred
        .as_object()
        .unwrap()
        .iter()
        .filter_map(|(k, v)| {
            let start = *starts.get(&k.parse::<i64>().ok()?)?;
            Some((start, v.as_str()?.to_string()))
        })
        .collect();
    out.sort();
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = std::fs::read_to_string(&args[1]).expect("bundle");
    let claims = match (args.get(3), args.get(4)) {
        (Some(raw), Some(pred)) => index_claims(raw, pred),
        _ => Vec::new(),
    };
    let t0 = Instant::now();
    let allocator = oxc_allocator::Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, &code);
    assert!(ingest.errors.is_empty(), "parse errors");
    let t_parse = t0.elapsed();
    let layout = BundleLayout::SingleWrapperFunction;
    let grammar = ModuleWrapperGrammar::BunAndEsbuild;
    let wrapper = layout.find_wrapper(ingest.program, ingest.semantic());
    let tables = SymbolTables::build(ingest.semantic());
    let factories: Vec<oxc_span::Span> = grammar
        .classify_factories(
            &code,
            ingest.program,
            ingest.semantic(),
            wrapper.as_ref().map(|w| w.body_span),
            &tables,
        )
        .map(|c| c.factories.iter().map(|f| f.span).collect())
        .unwrap_or_default();
    let t1 = Instant::now();
    let (graph, found) = discover_in_bundle(
        &code,
        &ingest,
        BundlePieces {
            layout,
            grammar,
            interop: InteropHelpers::Bun,
        },
        &factories,
        &claims,
        DiscoveryOptions {
            // VETO_SUPPORT=all-rules: the prototype's veto (reproduction).
            veto_support: match std::env::var("VETO_SUPPORT").as_deref() {
                Ok("all-rules") => VetoSupport::AllRules,
                _ => VetoSupport::WithoutModule,
            },
            ..DiscoveryOptions::default()
        },
    );
    let t_discover = t1.elapsed();
    if let Ok(path) = std::env::var("DUMP_UNITS") {
        // start, end, owned bytes, kind, runtime — every unit (lab scoring).
        let rows: String = graph
            .units
            .iter()
            .map(|u| {
                format!(
                    "{}\t{}\t{}\t{:?}\t{}\n",
                    u.span.start, u.span.end, u.bytes, u.kind, u.runtime
                )
            })
            .collect();
        std::fs::write(path, rows).expect("units dump");
    }
    let report = DiscoveryReport::new(&graph, &found);
    std::fs::write(&args[2], serde_json::to_string(&report).unwrap()).expect("write");
    eprintln!(
        "parse {:.1}s, discover {:.1}s; units {} edges {} runtime {} factories {} lazy {}; index claims in {}; claims {} vetoed {} class {} queue {}",
        t_parse.as_secs_f64(),
        t_discover.as_secs_f64(),
        report.graph.units,
        report.graph.edges,
        report.graph.runtime_units,
        report.graph.factories,
        report.graph.lazy_modules,
        claims.len(),
        report.claims.len(),
        report.vetoed.len(),
        report.library_class.len(),
        report.queue.len()
    );
}
