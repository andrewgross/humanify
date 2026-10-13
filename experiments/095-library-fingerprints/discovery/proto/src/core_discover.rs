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
    BundlePieces, DiscoveryOptions, DiscoveryReport, IndexLookup, VetoSupport, discover_in_bundle,
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
    // INDEX=<dir>: look statement units up in a library index. AGREEMENT=0
    // (structural alone), SPEC=0 (no specificity), TARGETED=0 (no queue
    // round), SUPPORT=<n>.
    let flag = |k: &str| std::env::var(k).ok();
    let index = flag("INDEX").map(|dir| {
        let t = Instant::now();
        let ix = humanify_core::libindex::lookup::LibraryIndex::load(
            std::path::Path::new(&dir),
            humanify_core::libindex::IndexSlice::from_name(&flag("SLICE").unwrap_or("bun".into()))
                .expect("slice"),
        )
        .expect("index");
        eprintln!("index loaded in {:.1}s", t.elapsed().as_secs_f64());
        ix
    });
    let mut rules = humanify_core::libindex::lookup::LookupRules::default();
    if flag("AGREEMENT").as_deref() == Some("0") {
        rules.agreement = false;
    }
    if flag("SPEC").as_deref() == Some("0") {
        rules.specificity = None;
    }
    if let Some(n) = flag("MIN_NODES") {
        rules.min_nodes = n.parse().expect("min nodes");
    }
    if let Some(n) = flag("SUPPORT") {
        rules.support = n.parse().expect("support");
    }
    let lookup = index.as_ref().map(|index| IndexLookup {
        index,
        rules,
        targeted: (flag("TARGETED").as_deref() != Some("0")).then_some(
            humanify_core::libindex::lookup::LookupRules {
                min_nodes: rules.min_nodes.min(20),
                support: 0,
                ..rules
            },
        ),
    });
    // --many <jobs.tsv>: one bundle per line, `bundle<TAB>report<TAB>units dump`,
    // all looked up in the ONE index loaded above (exp095 index-v2 lane).
    if args[1] == "--many" {
        for line in std::fs::read_to_string(&args[2]).expect("jobs").lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() < 3 {
                continue;
            }
            eprintln!("== {}", f[0]);
            run_one(f[0], f[1], Some(f[2]), Vec::new(), lookup, index.as_ref());
        }
        return;
    }
    let claims = match (args.get(3), args.get(4)) {
        (Some(raw), Some(pred)) => index_claims(raw, pred),
        _ => Vec::new(),
    };
    let dump = std::env::var("DUMP_UNITS").ok();
    run_one(
        &args[1],
        &args[2],
        dump.as_deref(),
        claims,
        lookup,
        index.as_ref(),
    );
}

fn run_one(
    bundle: &str,
    report_path: &str,
    dump: Option<&str>,
    claims: Vec<(u32, String)>,
    lookup: Option<IndexLookup>,
    index: Option<&humanify_core::libindex::lookup::LibraryIndex>,
) {
    let flag = |k: &str| std::env::var(k).ok();
    let code = std::fs::read_to_string(bundle).expect("bundle");
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
    if let (Some(at), Some(ix)) = (flag("DEBUG_AT"), index) {
        let at: u32 = at.parse().expect("offset");
        let statements = humanify_core::modules::container_statements(
            ingest.program,
            ingest.semantic(),
            wrapper.as_ref().map(|w| w.body_span),
        );
        let facts = humanify_core::graph::references::BundlerFacts::recognise(
            &code,
            statements,
            grammar,
            InteropHelpers::Bun,
        );
        let units = humanify_core::graph::references::container_units(
            statements, &factories, &facts, false,
        );
        let tables = SymbolTables::build(ingest.semantic());
        let prints = humanify_core::libindex::units::unit_prints(&units, &tables);
        for (u, p) in units.units.iter().zip(&prints) {
            if u.span.start != at {
                continue;
            }
            let p = p.as_ref().expect("statement");
            for (i, f) in std::iter::once(&p.whole).chain(&p.functions).enumerate() {
                let head: String = code[f.span.start as usize..f.span.end as usize]
                    .chars()
                    .take(50)
                    .collect();
                let occ = ix.occurrences(&f.hash);
                eprintln!(
                    "{} {} nodes {} content {} hits {:?} {head:?}",
                    if i == 0 { "WHOLE" } else { "  fn" },
                    f.hash,
                    f.nodes,
                    f.content,
                    &occ[..occ.len().min(3)]
                );
            }
        }
    }
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
        lookup,
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
    if let Some(path) = dump {
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
    let report = DiscoveryReport::new(&graph, &found, true);
    std::fs::write(report_path, serde_json::to_string(&report).unwrap()).expect("write");
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
