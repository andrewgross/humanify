//! exp095 "misses", Task 2: unit fingerprints under candidate matching
//! variants, for BOTH sides with one unit model, so an index variant and a
//! query variant are always computed by the same code.
//!
//! Usage: exp095-variant-rows <jobs.tsv>
//!   each line: side<TAB>bundle.js<TAB>out.jsonl<TAB>bundler
//!   side = index (factory bodies split, owner by the bundle's map, the
//!   builder's 90% package purity) or query (discovery's pieces).
//!
//! Per unit (index side: owned units only) one JSON row:
//!   s, e, k, b, mb (query: module bytes), owner/version/file (index),
//!   base: [[s,e,hash,nodes,content], …]  whole first, then functions —
//!         EXACTLY libindex::units' prints
//!   norm: the same prints with the canonical text normalised (N below)
//!   bind: normalised prints of every binding's right-hand side: each
//!         declarator init of a `var/let/const`, each `x = e` of an
//!         expression statement (or a comma sequence of them), and a
//!         function/class declaration itself
//! N: `const`/`let` → `var`; FunctionDeclaration/FunctionExpression →
//!    Function; ClassDeclaration/ClassExpression → Class; template raw
//!    spellings blurred (`Q=<n>` → `Q=`).
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use humanify_core::graph::references::{UnitKind, UnitNode};
use humanify_core::hash::serialize::{
    LiteralPolicy, SymbolTables, canonical_serialize_privates_blinded,
};
use humanify_core::libindex::units::statement_prints;
use humanify_core::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn sha16(s: &str) -> String {
    Sha256::digest(s.as_bytes())[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn normalise(parts: &str) -> String {
    let mut s = parts
        .replace("kind:\"const\"", "kind:\"var\"")
        .replace("kind:\"let\"", "kind:\"var\"")
        .replace("FunctionDeclaration{", "Function{")
        .replace("FunctionExpression{", "Function{")
        .replace("ClassDeclaration{", "Class{")
        .replace("ClassExpression{", "Class{");
    // Q=<digits> → Q=
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'Q' && i + 1 < b.len() && b[i + 1] == b'=' {
            out.push_str("Q=");
            i += 2;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            continue;
        }
        out.push(b[i] as char);
        i += 1;
    }
    s = out;
    s
}

fn count_nodes(node: &Value) -> u32 {
    match node {
        Value::Object(map) => {
            u32::from(map.contains_key("type")) + map.values().map(count_nodes).sum::<u32>()
        }
        Value::Array(items) => items.iter().map(count_nodes).sum(),
        _ => 0,
    }
}

fn ty(n: &Value) -> &str {
    n.get("type").and_then(Value::as_str).unwrap_or("")
}

fn span(n: &Value) -> (u64, u64) {
    let at = |k: &str| n.get(k).and_then(Value::as_u64).unwrap_or(0);
    (at("start"), at("end"))
}

/// The right-hand sides a statement binds (module doc, `bind`).
fn bindings<'v>(stmt: &'v Value, out: &mut Vec<&'v Value>) {
    match ty(stmt) {
        "VariableDeclaration" => {
            for d in stmt["declarations"].as_array().into_iter().flatten() {
                if let Some(init) = d.get("init").filter(|v| !v.is_null()) {
                    out.push(init);
                }
            }
        }
        "VariableDeclarator" => {
            if let Some(init) = stmt.get("init").filter(|v| !v.is_null()) {
                out.push(init);
            }
        }
        "FunctionDeclaration" | "ClassDeclaration" => out.push(stmt),
        "ExpressionStatement" => {
            let e = &stmt["expression"];
            let items: Vec<&Value> = if ty(e) == "SequenceExpression" {
                e["expressions"].as_array().into_iter().flatten().collect()
            } else {
                vec![e]
            };
            for it in items {
                if ty(it) == "AssignmentExpression" && it["operator"] == "=" {
                    out.push(&it["right"]);
                }
            }
        }
        _ => {}
    }
}

fn functions<'v>(node: &'v Value, out: &mut Vec<&'v Value>, root: bool) {
    match node {
        Value::Object(map) => {
            if !root
                && matches!(
                    ty(node),
                    "FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression"
                )
            {
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

fn module_bytes(units: &[humanify_core::graph::references::Unit]) -> Vec<u64> {
    let n = units.len();
    let mut of: Vec<Option<usize>> = vec![None; n];
    let mut size: Vec<u64> = Vec::new();
    let mut pending: Vec<usize> = Vec::new();
    for u in 0..n {
        match units[u].kind {
            UnitKind::Statement => pending.push(u),
            UnitKind::Factory | UnitKind::LazyModule => {
                let g = size.len();
                for &p in &pending {
                    of[p] = Some(g);
                }
                of[u] = Some(g);
                pending.clear();
                size.push(0);
            }
            UnitKind::LazyBody { module } | UnitKind::FactoryBody { module } => {
                of[u] = of[module as usize];
            }
        }
    }
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

type Cache = HashMap<PathBuf, Option<(String, String)>>;

/// The builder's `unit_owner` (private there): the package holding >= 90%
/// of the unit's segments, named by its majority file.
fn owner(
    map: &humanify_index::sourcemap::SourceMap,
    out: &Path,
    tally: &[(u32, usize)],
    cache: &mut Cache,
) -> Option<humanify_index::sourcemap::Owner> {
    let total: usize = tally.iter().map(|t| t.1).sum();
    let mut by: BTreeMap<String, (usize, usize, Option<humanify_index::sourcemap::Owner>)> =
        BTreeMap::new();
    for &(src, n) in tally {
        let Some(o) =
            humanify_index::sourcemap::owner_cached(out, &map.sources[src as usize], cache)
        else {
            continue;
        };
        let e = by.entry(o.package.clone()).or_insert((0, 0, None));
        e.0 += n;
        if n > e.1 {
            e.1 = n;
            e.2 = Some(o);
        }
    }
    let (_, (n, _, o)) = by.into_iter().max_by_key(|(_, (n, _, _))| *n)?;
    (n as f64 >= 0.9 * total as f64).then_some(o?)
}

fn run(side: &str, bundle: &str, out_path: &str, bundler: &str) -> Result<(), String> {
    let code = std::fs::read_to_string(bundle).map_err(|e| e.to_string())?;
    let index_side = side == "index";
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, &code);
    if !ingest.errors.is_empty() {
        return Err("parse errors".into());
    }
    let semantic = ingest.semantic();
    let (layout, grammar, interop) = if index_side {
        let slice = if bundler == "esbuild" {
            humanify_core::libindex::IndexSlice::Esbuild
        } else {
            humanify_core::libindex::IndexSlice::Bun
        };
        let tc = humanify_core::toolchain::resolve_toolchain(
            &humanify_core::detect::detect_bundle(&code),
            Some(slice.bundler()),
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
    let mb = module_bytes(&units.units);
    // index side: owners through the map
    let owners: Vec<Option<humanify_index::sourcemap::Owner>> = if index_side {
        let map_text =
            std::fs::read_to_string(format!("{bundle}.map")).map_err(|e| e.to_string())?;
        let map = humanify_index::sourcemap::SourceMap::parse(&map_text)?;
        let spans: Vec<(u32, u32)> = units
            .units
            .iter()
            .map(|u| (u.span.start, u.span.end))
            .collect();
        let out_dir = Path::new(bundle).parent().unwrap().to_path_buf();
        let mut cache = Cache::new();
        map.tally(&code, &spans)
            .iter()
            .map(|t| owner(&map, &out_dir, t, &mut cache))
            .collect()
    } else {
        Vec::new()
    };
    // index side: module bytes = the owner file's owned statement bytes (the builder's)
    let mut file_bytes: HashMap<(String, String), u64> = HashMap::new();
    if index_side {
        for ((u, o), node) in units.units.iter().zip(&owners).zip(&units.nodes) {
            if let (Some(o), UnitNode::Statement(_) | UnitNode::Declarator(_)) = (o, node) {
                *file_bytes
                    .entry((o.package.clone(), o.file.clone()))
                    .or_default() += u64::from(u.bytes);
            }
        }
    }
    let mut out = String::new();
    for (i, (u, node)) in units.units.iter().zip(&units.nodes).enumerate() {
        let text = {
            use oxc_estree::{CompactSerializer, ESTree};
            let mut ser = CompactSerializer::new(false, false);
            match node {
                UnitNode::Statement(s) => s.serialize(&mut ser),
                UnitNode::Declarator(d) => d.serialize(&mut ser),
                UnitNode::Container => continue,
            }
            ser.into_string()
        };
        let own = if index_side {
            match &owners[i] {
                Some(o) => Some(o),
                None => continue,
            }
        } else {
            None
        };
        if !index_side && (runtime.get(i).copied().unwrap_or(false)) {
            continue;
        }
        let jv = humanify_core::ingest::parse_estree_json(&text);
        let p = statement_prints(&jv, &tables);
        let canon = |n: &Value| {
            canonical_serialize_privates_blinded(n, &tables, LiteralPolicy::MatchKey).parts
        };
        let base: Vec<Value> = std::iter::once(&p.whole)
            .chain(&p.functions)
            .map(|f| json!([f.span.start, f.span.end, f.hash, f.nodes, f.content]))
            .collect();
        // norm: whole + functions, same spans/nodes/content, normalised hash
        let mut fnodes = Vec::new();
        functions(&jv, &mut fnodes, true);
        let fhash: BTreeMap<(u64, u64), String> = fnodes
            .iter()
            .map(|f| (span(f), sha16(&normalise(&canon(f)))))
            .collect();
        let norm: Vec<Value> = std::iter::once((&p.whole, sha16(&normalise(&canon(&jv)))))
            .chain(p.functions.iter().map(|f| {
                (
                    f,
                    fhash
                        .get(&(u64::from(f.span.start), u64::from(f.span.end)))
                        .cloned()
                        .unwrap_or_default(),
                )
            }))
            .map(|(f, h)| json!([f.span.start, f.span.end, h, f.nodes, f.content]))
            .collect();
        let mut bs = Vec::new();
        bindings(&jv, &mut bs);
        let mut seen = BTreeSet::new();
        let bind: Vec<Value> = bs
            .into_iter()
            .filter(|b| seen.insert(span(b)))
            .map(|b| {
                let (s, e) = span(b);
                json!([s, e, sha16(&normalise(&canon(b))), count_nodes(b), 0])
            })
            .collect();
        let (kname, _) = match u.kind {
            UnitKind::Statement => ("Statement", 0),
            UnitKind::LazyBody { .. } => ("LazyBody", 0),
            UnitKind::FactoryBody { .. } => ("FactoryBody", 0),
            _ => ("Other", 0),
        };
        let keep = |v: Vec<Value>| -> Vec<Value> {
            if !index_side {
                return v;
            }
            // the builder's UNIT_ROW_MIN_NODES; spans are not needed index-side
            v.into_iter()
                .filter(|x| x[3].as_u64().unwrap_or(0) >= 20)
                .map(|x| json!([x[2], x[3], x[4]]))
                .collect()
        };
        let (base, norm, bind) = (keep(base), keep(norm), keep(bind));
        let mut row = json!({"i": i, "s": u.span.start, "e": u.span.end, "k": kname, "b": u.bytes,
            "mb": mb[i], "base": base, "norm": norm, "bind": bind});
        if let Some(o) = own {
            row["owner"] = json!(o.package);
            row["version"] = json!(o.version);
            row["file"] = json!(o.file);
            row["mb"] = json!(
                file_bytes
                    .get(&(o.package.clone(), o.file.clone()))
                    .copied()
                    .unwrap_or(0)
            );
        }
        out.push_str(&row.to_string());
        out.push('\n');
    }
    std::fs::write(out_path, out).map_err(|e| e.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let jobs = std::fs::read_to_string(&args[1]).expect("jobs");
    let jobs: Vec<Vec<String>> = jobs
        .lines()
        .map(|l| l.split('\t').map(String::from).collect())
        .filter(|f: &Vec<String>| f.len() >= 4)
        .collect();
    use rayon::prelude::*;
    jobs.par_iter().for_each(|f| {
        if std::path::Path::new(&f[2]).exists() {
            return;
        }
        if let Err(e) = run(&f[0], &f[1], &f[2], &f[3]) {
            eprintln!("{}: {e}", f[1]);
        }
    });
}
