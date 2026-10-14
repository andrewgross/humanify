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
    let mut functions = Vec::new();
    collect_functions(node, tables, &mut functions, true);
    StatementPrints {
        whole: print(node, tables),
        functions,
    }
}

fn print(node: &Value, tables: &SymbolTables) -> UnitPrint {
    let mut content = BTreeSet::new();
    content_tokens(node, tables, &mut content);
    UnitPrint {
        span: span_of(node),
        // Private names are the minifier's (`#e`), renamed per build: the
        // form blinds them.
        hash: canonical_serialize(node, tables, &LIBRARY_UNITS).hash,
        nodes: count_nodes(node),
        content: content.len() as u32,
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
fn collect_functions(node: &Value, tables: &SymbolTables, out: &mut Vec<UnitPrint>, root: bool) {
    match node {
        Value::Object(map) => {
            if !root && is_function(node) {
                out.push(print(node, tables));
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
    let prints = unit_prints(&units, &tables);
    units
        .units
        .into_iter()
        .zip(prints)
        .filter_map(|(u, p)| Some((u, p?)))
        .collect()
}

/// The prints of every unit (None for a container), parallel to `units`.
/// The AST is serialized on this thread; the hashing runs in parallel.
pub fn unit_prints(
    units: &crate::cut::units::ContainerUnits<'_>,
    tables: &SymbolTables,
) -> Vec<Option<StatementPrints>> {
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
        let json = crate::ingest::parse_estree_json(text.as_deref()?);
        Some(statement_prints(&json, tables))
    })
}
