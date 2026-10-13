//! exp095 library discovery — statement-level reference graph of a raw bundle.
//!
//! Usage: exp095-refgraph <bundle.js> <raw.jsonl> <out.tsv>
//!
//! Units come from the exp095 unit model (`raw.jsonl`: depth-0 statements,
//! the depth-1 statements of lazy-ESM init bodies; a CJS factory is ONE unit,
//! its depth-1 rows fold into it). Every resolved reference becomes an edge
//! between the unit that holds the reference and the unit that declares the
//! symbol; a WRITE also links the writer and the declarer both ways, and a
//! READ also points at every unit that writes the symbol (the lazy body
//! statement that assigns a hoisted `var` is that binding's real definer).
//! A DEFINITIONAL member mutation — `x.a=…`, `x.prototype.b=…`,
//! `Object.defineProperty(x,…)` / `Object.assign(x,…)`, or a call to one of
//! the bundler's namespace-populating helpers (`__export`/`__reExport`, names
//! given on the command line) with `x` first — counts as a WRITE of `x` when
//! it runs at module-init time (not inside a function of its unit): it is part
//! of `x`'s definition. The same mutation inside a function is runtime use of
//! `x`'s API (a read edge only): an app configuring a library is not the
//! library depending on the app.
//! Unresolved references (globals) make no edge. Output rows:
//! `src<TAB>dst<TAB>kind` with kind r (read) or w (write), deduplicated.

use std::collections::{BTreeSet, HashMap};

use oxc_ast::AstKind;
use oxc_ast::ast::{Argument, Expression};
use oxc_semantic::{Semantic, SymbolId};

use oxc_allocator::Allocator;
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_span::{GetSpan, SourceType};

struct Units {
    /// (start, end, id), sorted by start, non-overlapping within a level.
    top: Vec<(u32, u32, i64)>,
    inner: Vec<(u32, u32, i64)>,
}

impl Units {
    fn unit_of(&self, pos: u32) -> Option<i64> {
        self.unit_span(pos).map(|u| u.2)
    }
    fn find_span(level: &[(u32, u32, i64)], pos: u32) -> Option<(u32, u32, i64)> {
        let i = level.partition_point(|u| u.0 <= pos);
        if i == 0 {
            return None;
        }
        let u = level[i - 1];
        (pos < u.1).then_some(u)
    }
    fn unit_span(&self, pos: u32) -> Option<(u32, u32, i64)> {
        Units::find_span(&self.inner, pos).or_else(|| Units::find_span(&self.top, pos))
    }
}

fn load_units(path: &str) -> Units {
    let text = std::fs::read_to_string(path).expect("raw.jsonl");
    let mut wrapper_kind: HashMap<i64, String> = HashMap::new();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    for r in &rows {
        if r["d"] == 0 && r["w"] == 1 {
            wrapper_kind.insert(
                r["i"].as_i64().unwrap(),
                r["wk"].as_str().unwrap_or("").into(),
            );
        }
    }
    let mut top = Vec::new();
    let mut inner = Vec::new();
    for r in &rows {
        let span = (
            r["s"].as_u64().unwrap() as u32,
            r["e"].as_u64().unwrap() as u32,
            r["i"].as_i64().unwrap(),
        );
        match r["d"].as_i64().unwrap() {
            0 => top.push(span),
            1 => {
                let parent = r["p"].as_i64().unwrap();
                if wrapper_kind.get(&parent).map(String::as_str) == Some("esm") {
                    inner.push(span);
                }
            }
            _ => {}
        }
    }
    top.sort();
    inner.sort();
    Units { top, inner }
}

/// The identifier at the root of a member chain (`x` in `x.a.b`).
fn chain_root<'a>(mut e: &'a Expression<'a>) -> Option<&'a oxc_ast::ast::IdentifierReference<'a>> {
    loop {
        match e {
            Expression::Identifier(id) => return Some(id),
            _ => match e.as_member_expression() {
                Some(m) => e = m.object(),
                None => return None,
            },
        }
    }
}

/// Definitional member mutations: symbol -> units whose init-time code mutates it.
fn member_mutations(
    semantic: &Semantic<'_>,
    units: &Units,
    helpers: &[String],
) -> HashMap<SymbolId, Vec<i64>> {
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let mut out: HashMap<SymbolId, Vec<i64>> = HashMap::new();
    for node in nodes.iter() {
        let target = match node.kind() {
            AstKind::AssignmentExpression(a) => a
                .left
                .as_member_expression()
                .and_then(|m| chain_root(m.object())),
            AstKind::CallExpression(c) => {
                let callee_ok = match &c.callee {
                    Expression::Identifier(id) => helpers.iter().any(|h| h == id.name.as_str()),
                    e => e.as_member_expression().is_some_and(|m| {
                        matches!(m.object(), Expression::Identifier(o) if o.name == "Object")
                            && matches!(
                                m.static_property_name(),
                                Some("defineProperty" | "defineProperties" | "assign")
                            )
                    }),
                };
                match c.arguments.first() {
                    Some(Argument::Identifier(id)) if callee_ok => Some(&**id),
                    _ => None,
                }
            }
            _ => None,
        };
        let Some(id) = target else { continue };
        let Some(sym) = id
            .reference_id
            .get()
            .and_then(|r| scoping.get_reference(r).symbol_id())
        else {
            continue;
        };
        let Some(unit) = units.unit_span(node.span().start) else {
            continue;
        };
        // Inside a function of this unit (the unit's own function counts) =
        // runtime use, not definition.
        let nested = nodes.ancestors(node.id()).any(|a| {
            matches!(
                a.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            ) && a.span().start >= unit.0
        });
        if !nested {
            out.entry(sym).or_default().push(unit.2);
        }
    }
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let source = std::fs::read_to_string(&args[1]).expect("bundle");
    let units = load_units(&args[2]);
    let allocator = Allocator::default();
    let ret = Parser::new(&allocator, &source, SourceType::cjs()).parse();
    assert!(
        ret.diagnostics.is_empty(),
        "parse errors: {}",
        ret.diagnostics.len()
    );
    let program = allocator.alloc(ret.program);
    let semantic = SemanticBuilder::new()
        .with_build_nodes(true)
        .build(program)
        .semantic;
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();

    let helpers: Vec<String> = args
        .get(4)
        .map(|h| {
            h.split(',')
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default();
    let use_mutations = args.get(5).map(String::as_str) != Some("nomut");
    let mutations = if use_mutations {
        member_mutations(&semantic, &units, &helpers)
    } else {
        HashMap::new()
    };
    let n_mut: usize = mutations.values().map(Vec::len).sum();
    let mut edges: BTreeSet<(i64, i64, u8)> = BTreeSet::new();
    let mut symbols_crossing = 0usize;
    for symbol in scoping.symbol_ids() {
        let Some(decl) = units.unit_of(scoping.symbol_span(symbol).start) else {
            continue;
        };
        let mut readers = Vec::new();
        let mut writers = Vec::new();
        for &rid in scoping.get_resolved_reference_ids(symbol) {
            let reference = scoping.get_reference(rid);
            let pos = nodes.get_node(reference.node_id()).span().start;
            let Some(at) = units.unit_of(pos) else {
                continue;
            };
            if reference.flags().is_write() {
                writers.push(at);
            }
            if reference.flags().is_read() {
                readers.push(at);
            }
        }
        if let Some(m) = mutations.get(&symbol) {
            writers.extend(m.iter().copied());
        }
        let mut owners: BTreeSet<i64> = writers.iter().copied().collect();
        owners.insert(decl);
        if owners.len() > 1 || readers.iter().any(|r| *r != decl) {
            symbols_crossing += 1;
        }
        // Writers and the declarer are one binding: link them both ways.
        for &w in &writers {
            if w != decl {
                edges.insert((w, decl, b'w'));
                edges.insert((decl, w, b'w'));
            }
        }
        for &r in &readers {
            for &o in &owners {
                if r != o {
                    edges.insert((r, o, b'r'));
                }
            }
        }
    }
    let mut out = String::new();
    for (s, d, k) in &edges {
        out.push_str(&format!("{s}\t{d}\t{}\n", *k as char));
    }
    std::fs::write(&args[3], out).expect("write");
    eprintln!(
        "units top={} inner={} symbols={} crossing={} edges={} mutations={}",
        units.top.len(),
        units.inner.len(),
        scoping.symbols_len(),
        symbols_crossing,
        edges.len(),
        n_mut
    );
}
