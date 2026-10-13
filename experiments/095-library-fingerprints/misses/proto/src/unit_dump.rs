//! exp095 "misses": every statement unit of a bundle with the fingerprints
//! library discovery (query side) or the index builder (index side)
//! computes for it, plus the module size the specificity rule reads and,
//! on request, the canonical text the hash is taken of.
//!
//! Usage: exp095-unit-dump <bundle.js> <out.jsonl> [--side query|index]
//!            [--bundler bun|esbuild] [--canon all|<starts file>]
//!
//! query side = `exp095-core-discover`'s pieces (SingleWrapperFunction,
//! Bun+esbuild grammar, Bun interop), factory bodies NOT split (a CommonJS
//! module is one external unit). index side = `humanify-index`'s
//! `unit_rows`: the toolchain resolved for the slice's bundler, factory
//! bodies split.
use std::collections::{BTreeMap, HashSet};

use humanify_core::graph::references::{UnitKind, UnitNode};
use humanify_core::hash::serialize::{
    LiteralPolicy, SymbolTables, canonical_serialize_privates_blinded,
};
use humanify_core::libindex::units::statement_prints;
use humanify_core::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
use serde_json::{Value, json};

fn kind_name(k: UnitKind) -> (&'static str, i64) {
    match k {
        UnitKind::Statement => ("Statement", -1),
        UnitKind::LazyModule => ("LazyModule", -1),
        UnitKind::LazyBody { module } => ("LazyBody", i64::from(module)),
        UnitKind::FactoryBody { module } => ("FactoryBody", i64::from(module)),
        UnitKind::Factory => ("Factory", -1),
    }
}

/// discovery.rs `module_groups` + `module_bytes`, replicated (private there).
fn module_bytes(units: &[humanify_core::graph::references::Unit]) -> Vec<u64> {
    let n = units.len();
    let mut of: Vec<Option<usize>> = vec![None; n];
    let mut size: Vec<u64> = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    let mut pending: Vec<usize> = Vec::new();
    for u in 0..n {
        match units[u].kind {
            UnitKind::Statement => pending.push(u),
            UnitKind::Factory | UnitKind::LazyModule => {
                let g = members.len();
                for &p in &pending {
                    of[p] = Some(g);
                }
                of[u] = Some(g);
                members.push(std::mem::take(&mut pending));
                size.push(0);
            }
            UnitKind::LazyBody { module } | UnitKind::FactoryBody { module } => {
                let g = of[module as usize];
                of[u] = g;
                if let Some(g) = g {
                    members[g].push(u);
                }
            }
        }
    }
    // a group's size = its statements' bytes (container included)
    for (u, g) in of.iter().enumerate() {
        if let Some(g) = g {
            size[*g] += u64::from(units[u].bytes);
        }
    }
    of.iter()
        .enumerate()
        .map(|(u, g)| g.map_or(u64::from(units[u].bytes), |g| size[g]))
        .collect()
}

fn span_of(node: &Value) -> (u64, u64) {
    let at = |k: &str| node.get(k).and_then(Value::as_u64).unwrap_or(0);
    (at("start"), at("end"))
}

fn is_function(node: &Value) -> bool {
    matches!(
        node.get("type").and_then(Value::as_str),
        Some("FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression")
    )
}

fn functions<'v>(node: &'v Value, out: &mut Vec<&'v Value>, root: bool) {
    match node {
        Value::Object(map) => {
            if !root && is_function(node) {
                out.push(node);
            }
            for v in map.values() {
                functions(v, out, false);
            }
        }
        Value::Array(items) => {
            for v in items {
                functions(v, out, false);
            }
        }
        _ => {}
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opt = |k: &str| {
        args.iter()
            .position(|a| a == k)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let index_side = opt("--side").as_deref() == Some("index");
    let canon: Option<Option<HashSet<u32>>> = opt("--canon").map(|c| {
        (c != "all").then(|| {
            std::fs::read_to_string(&c)
                .expect("starts file")
                .lines()
                .filter_map(|l| l.trim().parse().ok())
                .collect()
        })
    });
    let code = std::fs::read_to_string(&args[1]).expect("bundle");
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, &code);
    assert!(ingest.errors.is_empty(), "parse errors");
    let semantic = ingest.semantic();
    let (layout, grammar, interop) = if index_side {
        let bundler = match opt("--bundler").as_deref() {
            Some("esbuild") => humanify_core::libindex::IndexSlice::Esbuild,
            _ => humanify_core::libindex::IndexSlice::Bun,
        };
        let tc = humanify_core::toolchain::resolve_toolchain(
            &humanify_core::detect::detect_bundle(&code),
            Some(bundler.bundler()),
            None,
        );
        (tc.layout.piece, tc.module_wrappers.piece, tc.interop.piece)
    } else {
        (
            BundleLayout::SingleWrapperFunction,
            ModuleWrapperGrammar::BunAndEsbuild,
            InteropHelpers::Bun,
        )
    };
    let wrapper = layout.find_wrapper(ingest.program, semantic);
    let body = wrapper.as_ref().map(|w| w.body_span);
    let statements = humanify_core::modules::container_statements(ingest.program, semantic, body);
    let tables = SymbolTables::build(semantic);
    let factories: Vec<oxc_span::Span> = grammar
        .classify_factories(&code, ingest.program, semantic, body, &tables)
        .map(|c| c.factories.iter().map(|f| f.span).collect())
        .unwrap_or_default();
    let facts = humanify_core::graph::references::BundlerFacts::recognise(
        &code, statements, grammar, interop,
    );
    let runtime: Vec<bool> = if index_side {
        Vec::new()
    } else {
        humanify_core::graph::references::build_reference_graph(
            &code, semantic, statements, &factories, &facts,
        )
        .units
        .iter()
        .map(|u| u.runtime)
        .collect()
    };
    let units = humanify_core::graph::references::container_units(
        statements, &factories, &facts, index_side,
    );
    let mbytes = module_bytes(&units.units);
    let mut out = String::new();
    for (i, (u, node)) in units.units.iter().zip(&units.nodes).enumerate() {
        let (kname, module) = kind_name(u.kind);
        let mut row = json!({
            "i": i, "s": u.span.start, "e": u.span.end, "k": kname, "m": module,
            "b": u.bytes, "rt": runtime.get(i).copied().unwrap_or(false), "mb": mbytes[i],
        });
        let text = {
            use oxc_estree::{CompactSerializer, ESTree};
            let mut ser = CompactSerializer::new(false, false);
            match node {
                UnitNode::Statement(s) => s.serialize(&mut ser),
                UnitNode::Declarator(d) => d.serialize(&mut ser),
                UnitNode::Container => {
                    out.push_str(&row.to_string());
                    out.push('\n');
                    continue;
                }
            }
            ser.into_string()
        };
        let jv = humanify_core::ingest::parse_estree_json(&text);
        let p = statement_prints(&jv, &tables);
        row["w"] = json!([p.whole.hash, p.whole.nodes, p.whole.content]);
        row["f"] = Value::Array(
            p.functions
                .iter()
                .map(|f| json!([f.span.start, f.span.end, f.hash, f.nodes, f.content]))
                .collect(),
        );
        let want = match &canon {
            Some(None) => true,
            Some(Some(set)) => set.contains(&u.span.start),
            None => false,
        };
        if want {
            let c = |n: &Value| {
                canonical_serialize_privates_blinded(n, &tables, LiteralPolicy::MatchKey).parts
            };
            row["cw"] = json!(c(&jv));
            let mut fs = Vec::new();
            functions(&jv, &mut fs, true);
            let mut by: BTreeMap<String, String> = BTreeMap::new();
            for f in fs {
                let (s, e) = span_of(f);
                by.insert(format!("{s}-{e}"), c(f));
            }
            row["cf"] = json!(by);
        }
        out.push_str(&row.to_string());
        out.push('\n');
    }
    std::fs::write(&args[2], out).expect("write");
}
