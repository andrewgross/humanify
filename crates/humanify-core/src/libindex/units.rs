//! Statement-level library fingerprints ("sub-tree units",
//! docs/design/library-index.md §6): the structural hash of every statement
//! unit of a bundle and of every function inside it, with its node count
//! and its content-token count. ONE owner: the index builder
//! (`humanify-index`) computes them on bundled npm packages, and library
//! discovery (`libdetect::discovery`) computes them on the bundle a run
//! processes, so the two can never disagree about what a unit is.

use std::collections::BTreeSet;

use oxc_span::Span;
use serde_json::Value;

use crate::hash::forms::LIBRARY_UNITS;
use crate::hash::serialize::{SymbolTables, canonical_serialize};

/// One unit's fingerprint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitPrint {
    pub span: Span,
    /// The [`LIBRARY_UNITS`] canonical hash (locals slotted, private
    /// names blinded, strings blurred, numbers and globals exact; the
    /// form's version is the index manifest's `units` stamp).
    pub hash: String,
    /// ESTree nodes in the subtree.
    pub nodes: u32,
    /// Distinct content tokens: strings, template text, regexes, static
    /// property names, free (global) identifiers.
    pub content: u32,
}

/// Prints smaller than this are not indexed (the lab's unit votes counted
/// units of >= 20 nodes; the lookup's own floor is higher). The builder
/// writes no row under it, so the index can say nothing about a smaller
/// piece — and a piece at or over it that a package's build HAS is in that
/// package's rows (containment, `libdetect::discovery::Containment`).
pub const UNIT_ROW_MIN_NODES: u32 = 20;

/// A statement unit's fingerprints: the whole statement, and every
/// function inside it (nested ones included), in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatementPrints {
    pub whole: UnitPrint,
    pub functions: Vec<UnitPrint>,
}

/// The fingerprints of one unit's ESTree JSON (the unit's own subtree;
/// `tables` are the bundle's, so locals slot and globals stay verbatim).
pub fn statement_prints(node: &Value, tables: &SymbolTables) -> StatementPrints {
    let named = statement_named_prints(node, tables);
    StatementPrints {
        whole: named.whole.print,
        functions: named.functions.into_iter().map(|f| f.print).collect(),
    }
}

/// A print with the canonical walk's slot mapping beside it (`$slot` ->
/// (symbol id, the name as the walked AST spells it)): the index's names
/// table is built from it (exp159). One entry per slot, in slot order; the
/// slot's first walk occurrence may be a reference — the DECLARATION is
/// found through the symbol id, never this entry's position.
pub type Mapping = Vec<(String, Option<oxc_syntax::symbol::SymbolId>, String)>;

/// One print and its slot mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedPrint {
    pub print: UnitPrint,
    pub mapping: Mapping,
}

/// [`StatementPrints`] with every print's mapping.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedStatementPrints {
    pub whole: NamedPrint,
    pub functions: Vec<NamedPrint>,
}

/// [`statement_prints`] with the mappings kept (the builder's `--names`
/// path; the run side discards them).
pub fn statement_named_prints(node: &Value, tables: &SymbolTables) -> NamedStatementPrints {
    let mut functions = Vec::new();
    collect_functions(node, tables, &mut functions, true);
    NamedStatementPrints {
        whole: named_print(node, tables),
        functions,
    }
}

fn named_print(node: &Value, tables: &SymbolTables) -> NamedPrint {
    let mut content = BTreeSet::new();
    content_tokens(node, tables, &mut content);
    // The mapping is built by the walk regardless; keeping it costs only
    // the Vec the caller drops.
    let out = canonical_serialize(node, tables, &LIBRARY_UNITS);
    NamedPrint {
        print: UnitPrint {
            span: span_of(node),
            // Private names are the minifier's (`#e`), renamed per build: the
            // form blinds them.
            hash: out.hash,
            nodes: count_nodes(node),
            content: content.len() as u32,
        },
        mapping: out.mapping,
    }
}

fn span_of(node: &Value) -> Span {
    let at = |k: &str| node.get(k).and_then(Value::as_u64).unwrap_or(0) as u32;
    Span::new(at("start"), at("end"))
}

fn node_type(node: &Value) -> Option<&str> {
    node.get("type").and_then(Value::as_str)
}

fn is_function(node: &Value) -> bool {
    matches!(
        node_type(node),
        Some("FunctionDeclaration" | "FunctionExpression" | "ArrowFunctionExpression")
    )
}

/// Every function inside `node` (not `node` itself), preorder.
fn collect_functions(node: &Value, tables: &SymbolTables, out: &mut Vec<NamedPrint>, root: bool) {
    match node {
        Value::Object(map) => {
            if !root && is_function(node) {
                out.push(named_print(node, tables));
            }
            for v in map.values() {
                collect_functions(v, tables, out, false);
            }
        }
        Value::Array(items) => {
            for v in items {
                collect_functions(v, tables, out, false);
            }
        }
        _ => {}
    }
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

/// A non-computed key's name (`a.name`, `{name: …}`, `name() {}`).
fn static_key(key: Option<&Value>) -> Option<&str> {
    let key = key?;
    match node_type(key)? {
        "Identifier" => key.get("name")?.as_str(),
        _ => None,
    }
}

/// The content tokens of a subtree: what a minifier cannot rename.
fn content_tokens(node: &Value, tables: &SymbolTables, out: &mut BTreeSet<String>) {
    match node {
        Value::Array(items) => {
            for v in items {
                content_tokens(v, tables, out);
            }
        }
        Value::Object(map) => {
            let computed = map.get("computed").and_then(Value::as_bool) == Some(true);
            let key_field = match node_type(node) {
                Some("MemberExpression") if !computed => Some("property"),
                Some("Property" | "MethodDefinition" | "PropertyDefinition") if !computed => {
                    Some("key")
                }
                _ => None,
            };
            if let Some(name) = key_field.and_then(|f| static_key(map.get(f))) {
                out.insert(format!("P:{name}"));
            }
            match node_type(node) {
                Some("Literal") => {
                    if let Some(re) = map.get("regex").and_then(|r| r.get("pattern")) {
                        out.insert(format!("R:{}", re.as_str().unwrap_or_default()));
                    } else if let Some(s) = map.get("value").and_then(Value::as_str) {
                        out.insert(format!("S:{s}"));
                    }
                }
                Some("TemplateElement") => {
                    if let Some(s) = map
                        .get("value")
                        .and_then(|v| v.get("cooked"))
                        .and_then(Value::as_str)
                        && !s.is_empty()
                    {
                        out.insert(format!("S:{s}"));
                    }
                }
                Some("Identifier") => {
                    let start = span_of(node).start;
                    let bound = tables.decl_by_start.contains_key(&start)
                        || tables.ref_by_start.contains_key(&start);
                    if !bound && let Some(name) = map.get("name").and_then(Value::as_str) {
                        out.insert(format!("G:{name}"));
                    }
                }
                _ => {}
            }
            for (k, v) in map {
                if Some(k.as_str()) != key_field {
                    content_tokens(v, tables, out);
                }
            }
        }
        _ => {}
    }
}

/// Every statement-like unit of a RAW bundle — top-level statements,
/// lazy-module bodies and bundled modules' factory bodies — with its
/// prints (the index side's unit model: `cut::units::container_units`
/// with factory bodies; containers carry no print).
pub fn bundle_unit_prints(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    layout: crate::toolchain::BundleLayout,
    grammar: crate::toolchain::ModuleWrapperGrammar,
    interop: crate::toolchain::InteropHelpers,
) -> Vec<(crate::cut::units::Unit, StatementPrints)> {
    let b = bundle_units(code, ingest, layout, grammar, interop, false, false);
    b.units
        .into_iter()
        .zip(b.prints)
        .filter_map(|(u, p)| Some((u, p?)))
        .collect()
}

/// [`bundle_unit_graph`] with every print's slot mapping kept (the
/// builder's `--names` path, exp159): the graph is still built (the rows
/// carry edges) and `named` is Some wherever `prints` is.
pub fn bundle_unit_named(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    layout: crate::toolchain::BundleLayout,
    grammar: crate::toolchain::ModuleWrapperGrammar,
    interop: crate::toolchain::InteropHelpers,
) -> BundleUnits {
    bundle_units(code, ingest, layout, grammar, interop, true, true)
}

/// A RAW bundle's units (containers included), their prints (None for a
/// container) and the statement reference graph over the SAME units
/// (`graph::references`, factory bodies cut: unit `i` of the graph is unit
/// `i` here) — what the index builder records a unit row's edges from
/// (exp151).
pub struct BundleUnits {
    pub units: Vec<crate::cut::units::Unit>,
    pub prints: Vec<Option<StatementPrints>>,
    /// The mappings of every printed unit (Some exactly where `prints` is
    /// Some) — `None` everywhere on the plain path.
    pub named: Vec<Option<NamedStatementPrints>>,
    pub graph: Option<crate::graph::references::ReferenceGraph>,
}

/// [`bundle_unit_prints`]'s units with every container kept, and the
/// reference graph over them ([`BundleUnits`]).
pub fn bundle_unit_graph(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    layout: crate::toolchain::BundleLayout,
    grammar: crate::toolchain::ModuleWrapperGrammar,
    interop: crate::toolchain::InteropHelpers,
) -> BundleUnits {
    bundle_units(code, ingest, layout, grammar, interop, true, false)
}

fn bundle_units(
    code: &str,
    ingest: &crate::ingest::Ingest<'_>,
    layout: crate::toolchain::BundleLayout,
    grammar: crate::toolchain::ModuleWrapperGrammar,
    interop: crate::toolchain::InteropHelpers,
    with_graph: bool,
    with_named: bool,
) -> BundleUnits {
    let semantic = ingest.semantic();
    let wrapper = layout.find_wrapper(ingest.program, semantic);
    let body = wrapper.as_ref().map(|w| w.body_span);
    let statements = crate::cut::container_statements(ingest.program, semantic, body);
    let tables = SymbolTables::build(semantic);
    let factories: Vec<Span> = grammar
        .classify_factories(code, ingest.program, semantic, body, &tables)
        .map(|c| c.factories.iter().map(|f| f.span).collect())
        .unwrap_or_default();
    let facts = crate::cut::units::BundlerFacts::recognise(code, statements, grammar, interop);
    let units = crate::cut::units::container_units(statements, &factories, &facts, true);
    let blurred = grammar.blurred_literals(statements, semantic);
    let (prints, named) = if with_named {
        let named = unit_named_prints(&units, &tables, &blurred);
        let prints = named.iter().map(|n| n.as_ref().map(plain_of)).collect();
        (prints, named)
    } else {
        (
            unit_prints(&units, &tables, &blurred),
            vec![None; units.units.len()],
        )
    };
    let graph = with_graph.then(|| {
        crate::graph::references::build_reference_graph_with(
            code, semantic, statements, &factories, &facts, true,
        )
    });
    debug_assert!(
        graph
            .as_ref()
            .is_none_or(|g| g.units.len() == units.units.len())
    );
    BundleUnits {
        units: units.units,
        prints,
        named,
        graph,
    }
}

/// [`NamedStatementPrints`] without the mappings.
pub(crate) fn plain_of(named: &NamedStatementPrints) -> StatementPrints {
    StatementPrints {
        whole: named.whole.print.clone(),
        functions: named.functions.iter().map(|f| f.print.clone()).collect(),
    }
}

/// The prints of every unit (None for a container), parallel to `units`.
/// The AST is serialized on this thread; the hashing runs in parallel.
/// `blurred`: the start offsets of number literals read as `0` (the run's
/// grammar's `blurred_literals`: webpack's module ids, exp105; empty for
/// every other bundler, whose prints are unchanged).
pub fn unit_prints(
    units: &crate::cut::units::ContainerUnits<'_>,
    tables: &SymbolTables,
    blurred: &std::collections::HashSet<u32>,
) -> Vec<Option<StatementPrints>> {
    unit_named_prints(units, tables, blurred)
        .into_iter()
        .map(|n| n.map(|n| plain_of(&n)))
        .collect()
}

/// [`unit_prints`] with every print's slot mapping kept.
pub fn unit_named_prints(
    units: &crate::cut::units::ContainerUnits<'_>,
    tables: &SymbolTables,
    blurred: &std::collections::HashSet<u32>,
) -> Vec<Option<NamedStatementPrints>> {
    use crate::cut::units::UnitNode;
    use oxc_estree::{CompactSerializer, ESTree};
    let texts: Vec<Option<String>> = units
        .nodes
        .iter()
        .map(|node| {
            let mut ser = CompactSerializer::new(false, false);
            match node {
                UnitNode::Statement(s) => s.serialize(&mut ser),
                UnitNode::Declarator(d) => d.serialize(&mut ser),
                UnitNode::Container => return None,
            }
            Some(ser.into_string())
        })
        .collect();
    crate::par::map_ordered(&texts, |text| {
        let mut json = crate::ingest::parse_estree_json(text.as_deref()?);
        if !blurred.is_empty() {
            blur_numbers(&mut json, blurred);
        }
        Some(statement_named_prints(&json, tables))
    })
}

/// Every number literal starting at a `blurred` offset reads as `0`.
fn blur_numbers(node: &mut Value, blurred: &std::collections::HashSet<u32>) {
    match node {
        Value::Object(map) => {
            let start = map.get("start").and_then(Value::as_u64);
            let is_number = map.get("type").and_then(Value::as_str) == Some("Literal")
                && map.get("value").is_some_and(Value::is_number);
            if is_number && start.is_some_and(|s| blurred.contains(&(s as u32))) {
                map.insert("value".into(), Value::from(0));
                map.insert("raw".into(), Value::from("0"));
            }
            for v in map.values_mut() {
                blur_numbers(v, blurred);
            }
        }
        Value::Array(items) => {
            for v in items {
                blur_numbers(v, blurred);
            }
        }
        _ => {}
    }
}
