//! The library freeze's classification — which functions of a mixed file
//! are library code (findings #32/#33). TS originals:
//! `src/library-detection/function-carry.ts`, `comment-regions.ts`
//! (`libraryAtOffset`), `src/rename/plugin.ts` (`detectAndMarkLibraries` /
//! `markLibraryFunctionsPreDone`), the dump's `libraryFunctions`.
//!
//! **The rule.** A function is library code iff its RAW start (an offset
//! into the file text the banners were found in, pre-beautify) lies in a
//! banner region `[banner, nextBanner)`, the last one open to EOF. A
//! beautified offset is NEVER compared with a region: beautify expands
//! minified text and drops every comment, so the two coordinate spaces
//! disagree (the TS bug #32).
//!
//! ONE owner, [`LibraryClassification`], with the two sources the port
//! needs, both reduced to the same (fresh span → library) join onto the
//! graph's function rows:
//!
//! - [`LibraryClassification::Carried`] — the pipeline (since WP5.6d, the
//!   Rust owns stage 6): [`carry_format_tree`] classifies the native
//!   formatter's OUTPUT tree (its function nodes still carry raw spans) by
//!   raw start, per ordinal of ONE shared pre-order walk (the JSON twin,
//!   [`carry_function_libraries`] / [`functions_in_tree_order`], walks an
//!   ESTree tree in the same order); [`resolve_function_libraries`] replays
//!   the walk over the re-parsed printed text and fails loud on a count or
//!   per-ordinal type mismatch. The ordinal is taken over the OUTPUT tree,
//!   never the raw one: flipComparisons reorders functions (the `reorder`
//!   vector in test/parity/library-carry.json).
//! - [`LibraryClassification::Consumed`] — the dump verbs (`naming`,
//!   `matches`, `transfers`) replaying a TS dump, which has no raw text:
//!   the TS's classification, `regions.json`'s `libraryFunctions` (fresh
//!   span in UTF-8 bytes, sessionId, library), joined by the function
//!   node's fresh span (the sessionId cross-checked). The pipeline's
//!   `--ts-library-functions` consumed it too through phase 5a (deleted at
//!   WP5.6d).
//!
//! The walk is babel's `Function` alias set (FunctionDeclaration,
//! FunctionExpression, ArrowFunctionExpression, ObjectMethod, ClassMethod,
//! ClassPrivateMethod) in pre-order, children in `VISITOR_KEYS` order —
//! [`crate::place::babel_walk`], the one owner of the babel shape of an
//! oxc ESTree tree. A method's span is babel's ONE node's: the Property /
//! MethodDefinition, key included.
//!
//! What classifies nothing (`detectAndMarkLibraries`, unchanged by #32):
//! `skipLibraries` off, or a wrapper IIFE detected — see
//! [`classify_library_functions`].

use humanify_model::js::{JsObject, JsValue};
use oxc_span::Span;
use serde_json::Value;

use super::CommentRegion;
use crate::format::ast::{NodeId as FormatNodeId, Tree as FormatTree};
use crate::graph::UnifiedGraph;
use crate::place::babel_walk::walk;

/// babel's `Function` alias (`t.FLIPPED_ALIAS_KEYS.Function`).
pub const FUNCTION_TYPES: [&str; 6] = [
    "FunctionDeclaration",
    "FunctionExpression",
    "ObjectMethod",
    "ArrowFunctionExpression",
    "ClassMethod",
    "ClassPrivateMethod",
];

/// `libraryAtOffset`: the library whose region contains `offset` (the last
/// region starting at or before it, if `offset` is before that region's
/// end), or None (app code). `offset` MUST be in the regions' coordinate
/// space — the raw text they were found in.
pub fn library_at_offset(regions: &[CommentRegion], offset: usize) -> Option<&str> {
    let i = regions
        .partition_point(|r| r.start <= offset)
        .checked_sub(1)?;
    match regions[i].end {
        Some(end) if offset >= end => None,
        _ => Some(regions[i].library_name.as_str()),
    }
}

/// One function in the shared walk: its babel type and babel node span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeFunction {
    pub babel_type: String,
    pub span: Span,
}

/// `functionsInTreeOrder`: every babel `Function` of a program's ESTree
/// JSON, pre-order, children in VISITOR_KEYS order. The ONE walk both
/// sides of the carry use — the ordinal is only an identity because the
/// walk is shared.
pub fn functions_in_tree_order(program: &Value) -> Vec<TreeFunction> {
    let mut out = Vec::new();
    walk(program, |v| {
        if FUNCTION_TYPES.contains(&v.babel_type)
            && let Some((start, end)) = v.span
        {
            out.push(TreeFunction {
                babel_type: v.babel_type.to_string(),
                span: Span::new(start, end),
            });
        }
    });
    out
}

/// Per-ordinal library (None = app code) plus the node type as a checksum
/// (`FunctionLibraryCarry`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FunctionLibraryCarry {
    pub libraries: Vec<Option<String>>,
    pub types: Vec<String>,
}

/// `carryFunctionLibraries`: classify every function of the beautify
/// transform's OUTPUT tree (`transformed`, spans still RAW) by its raw
/// start. Fails on an empty span: the beautifier synthesizing a function
/// would break the premise the carry rests on (the TS: no raw `start`).
pub fn carry_function_libraries(
    transformed: &Value,
    regions: &[CommentRegion],
) -> Result<FunctionLibraryCarry, String> {
    let fns = functions_in_tree_order(transformed);
    carry_in_walk_order(
        fns.iter().map(|f| {
            let start = (f.span.start != f.span.end).then_some(f.span.start);
            (f.babel_type.as_str(), start)
        }),
        regions,
    )
}

/// `carryFunctionLibraries` over the NATIVE stage 6's output tree
/// ([`crate::format`], WP5.6c): the same pre-order walk (children in
/// `VISITOR_KEYS` order, from the `File` root — the TS's `file.ast`), each
/// function classified by its RAW start ([`crate::format::ast::Node::span`], the
/// converter's offset into the unformatted text; None = a synthesized
/// node, which fails loud as the TS's missing `start` does). A node the
/// transforms SHARE between two parents is visited once per parent, as
/// the TS's object-graph walk visits it — and as the printed text holds it.
pub fn carry_format_tree(
    tree: &FormatTree,
    root: FormatNodeId,
    regions: &[CommentRegion],
) -> Result<FunctionLibraryCarry, String> {
    let mut fns: Vec<(&str, Option<u32>)> = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        let node = tree.node(id);
        if node.kind.is_function() {
            fns.push((node.kind.type_name(), node.span.map(|(start, _)| start)));
        }
        stack.extend(tree.children(id).into_iter().rev());
    }
    carry_in_walk_order(fns, regions)
}

/// The carry's one rule over a walk: per function (babel type, raw start)
/// in walk order, its library by raw start.
fn carry_in_walk_order<'a>(
    fns: impl IntoIterator<Item = (&'a str, Option<u32>)>,
    regions: &[CommentRegion],
) -> Result<FunctionLibraryCarry, String> {
    let mut carry = FunctionLibraryCarry::default();
    for (babel_type, start) in fns {
        let Some(start) = start else {
            return Err(format!(
                "library carry: a {babel_type} in the beautified tree has no raw start — beautify synthesized a function, so raw offsets no longer identify it"
            ));
        };
        carry
            .libraries
            .push(library_at_offset(regions, start as usize).map(str::to_string));
        carry.types.push(babel_type.to_string());
    }
    Ok(carry)
}

/// `resolveFunctionLibraries`: the (span, library) of every library
/// function of the RE-PARSED beautified tree, in walk order. Fails if the
/// walk does not line up with the carry (count or per-ordinal type).
pub fn resolve_function_libraries(
    reparsed: &Value,
    carry: &FunctionLibraryCarry,
) -> Result<Vec<(Span, String)>, String> {
    let fns = functions_in_tree_order(reparsed);
    if fns.len() != carry.libraries.len() {
        return Err(format!(
            "library carry: {} functions carried across beautify, {} found in the re-parsed text",
            carry.libraries.len(),
            fns.len()
        ));
    }
    let mut out = Vec::new();
    for (i, f) in fns.into_iter().enumerate() {
        if f.babel_type != carry.types[i] {
            return Err(format!(
                "library carry: function #{i} is a {} before re-parse and a {} after",
                carry.types[i], f.babel_type
            ));
        }
        if let Some(library) = &carry.libraries[i] {
            out.push((f.span, library.clone()));
        }
    }
    Ok(out)
}

/// One `regions.json` `libraryFunctions` row: a library function keyed by
/// its FRESH span (UTF-8 bytes of the beautified text).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryFunctionKey {
    pub span: Span,
    pub session_id: String,
    pub library: String,
}

/// Where a file's library classification comes from — the ONE owner the
/// naming driver, the transfer stage's freeze and the dump verbs consult.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryClassification {
    /// A TS dump's classification (the dump verbs).
    Consumed(Vec<LibraryFunctionKey>),
    /// The native stage 6's ordinal carry (the pipeline, WP5.6c/d).
    Carried(FunctionLibraryCarry),
    /// The file has banner regions but no classification reached the
    /// naming stage (a pre-#33 TS dump) — consulting it fails loud (the TS
    /// throws the same: raw offsets cannot classify beautified functions).
    Missing,
}

impl LibraryClassification {
    /// A TS dump's `regions.json`: `libraryFunctions` when present; a
    /// pre-#33 dump (no key) with comment regions is [`Self::Missing`];
    /// no regions at all is None.
    pub fn from_regions_json(regions: &Value) -> Result<Option<Self>, String> {
        if let Some(rows) = regions.get("libraryFunctions") {
            let rows = rows
                .as_array()
                .ok_or("regions.json: libraryFunctions is not an array")?;
            return rows
                .iter()
                .map(|r| {
                    let n = |k: &str| {
                        r["key"][k]
                            .as_u64()
                            .and_then(|v| u32::try_from(v).ok())
                            .ok_or_else(|| format!("regions.json: libraryFunctions key.{k}"))
                    };
                    let s = |k: &str| {
                        r[k].as_str()
                            .map(str::to_string)
                            .ok_or_else(|| format!("regions.json: libraryFunctions {k}"))
                    };
                    if r["key"]["text"].as_str() != Some("fresh") {
                        return Err(
                            "regions.json: a libraryFunctions key is not fresh-anchored".into()
                        );
                    }
                    Ok(LibraryFunctionKey {
                        span: Span::new(n("start")?, n("end")?),
                        session_id: s("sessionId")?,
                        library: s("library")?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()
                .map(|keys| Some(Self::Consumed(keys)));
        }
        let has_regions = regions
            .get("commentRegions")
            .and_then(Value::as_array)
            .is_some_and(|r| !r.is_empty());
        Ok(has_regions.then_some(Self::Missing))
    }

    /// Read `<dir>/regions.json` ([`Self::from_regions_json`]); an absent
    /// file classifies nothing.
    pub fn from_dump_dir(dir: &std::path::Path) -> Result<Option<Self>, String> {
        let path = dir.join("regions.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(None);
        };
        let v: Value =
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_regions_json(&v)
    }

    /// The (graph function row, library) of every library function, in
    /// graph row order (`collectAllFunctions` order — the prefix pass's).
    /// `fresh` is the fresh program's ESTree JSON (read by the carry).
    pub fn classify(
        &self,
        fresh: &Value,
        graph: &UnifiedGraph,
    ) -> Result<Vec<(usize, String)>, String> {
        let keyed: Vec<(Span, Option<&str>, String)> = match self {
            Self::Missing => {
                return Err("library regions reached the naming stage without a function classification — raw offsets cannot classify beautified functions (#32); a TS dump written before #33 (regions.json without libraryFunctions) cannot be replayed".into());
            }
            Self::Consumed(keys) => keys
                .iter()
                .map(|k| (k.span, Some(k.session_id.as_str()), k.library.clone()))
                .collect(),
            Self::Carried(carry) => resolve_function_libraries(fresh, carry)?
                .into_iter()
                .map(|(span, library)| (span, None, library))
                .collect(),
        };
        join_rows(&keyed, graph)
    }
}

/// Join (fresh span → library) onto the graph's function rows; every key
/// must name exactly one row (and agree on its sessionId when it carries
/// one), or the classification and the graph disagree about the text.
fn join_rows(
    keyed: &[(Span, Option<&str>, String)],
    graph: &UnifiedGraph,
) -> Result<Vec<(usize, String)>, String> {
    let mut by_row: Vec<Option<String>> = vec![None; graph.functions.len()];
    for (span, session_id, library) in keyed {
        let row = graph
            .functions
            .iter()
            .position(|f| f.span == *span)
            .ok_or_else(|| {
                format!(
                    "library classification: no function at fresh span {}..{}{}",
                    span.start,
                    span.end,
                    session_id.map_or(String::new(), |s| format!(" ({s})"))
                )
            })?;
        if let Some(id) = session_id
            && graph.functions[row].session_id != *id
        {
            return Err(format!(
                "library classification: fresh span {}..{} is {} in the graph, {id} in the classification",
                span.start, span.end, graph.functions[row].session_id
            ));
        }
        by_row[row] = Some(library.clone());
    }
    Ok(by_row
        .into_iter()
        .enumerate()
        .filter_map(|(row, lib)| lib.map(|l| (row, l)))
        .collect())
}

/// `detectAndMarkLibraries`: nothing is classified when `skipLibraries` is
/// off or a wrapper IIFE was detected; otherwise the file's
/// classification (None = the file has no banner regions).
pub fn classify_library_functions(
    fresh: &Value,
    graph: &UnifiedGraph,
    has_wrapper: bool,
    skip_libraries: bool,
    source: Option<&LibraryClassification>,
) -> Result<Vec<(usize, String)>, String> {
    match source {
        Some(c) if skip_libraries && !has_wrapper => c.classify(fresh, graph),
        _ => Ok(Vec::new()),
    }
}

/// The `libraryFunctions` rows the dump writes (`captureRegionsDump`):
/// each classified function's fresh span, sessionId and library, sorted by
/// span start.
pub fn library_function_rows(
    classified: &[(usize, String)],
    graph: &UnifiedGraph,
) -> Vec<LibraryFunctionKey> {
    let mut rows: Vec<LibraryFunctionKey> = classified
        .iter()
        .map(|(row, library)| LibraryFunctionKey {
            span: graph.functions[*row].span,
            session_id: graph.functions[*row].session_id.clone(),
            library: library.clone(),
        })
        .collect();
    rows.sort_by_key(|r| r.span.start);
    rows
}

/// `regions.json` in the TS schema (#33), in the TS's key order (the file
/// is byte-comparable): `commentRegions` spans in the MINIFIED text (UTF-8
/// bytes; an open end is `null`, never -1), `libraryFunctions` keyed in the
/// FRESH text, `bannerClassifications` (the Bun per-factory records; the
/// caller supplies them serialized).
pub fn regions_json(
    comment_regions: &[CommentRegion],
    library_functions: &[LibraryFunctionKey],
    banner_classifications: Vec<JsValue>,
) -> JsValue {
    let num = |n: usize| JsValue::Number(n as f64);
    let mut sorted: Vec<&CommentRegion> = comment_regions.iter().collect();
    sorted.sort_by_key(|r| r.start);
    let regions = sorted
        .into_iter()
        .map(|r| {
            let mut span = JsObject::new();
            span.insert("start", num(r.start));
            span.insert("end", r.end.map_or(JsValue::Null, num));
            let mut o = JsObject::new();
            o.insert("span", JsValue::Object(span));
            o.insert("library", JsValue::str(&r.library_name));
            JsValue::Object(o)
        })
        .collect();
    let functions = library_functions
        .iter()
        .map(|f| {
            let mut key = JsObject::new();
            key.insert("text", JsValue::str("fresh"));
            key.insert("start", num(f.span.start as usize));
            key.insert("end", num(f.span.end as usize));
            let mut o = JsObject::new();
            o.insert("key", JsValue::Object(key));
            o.insert("sessionId", JsValue::str(&f.session_id));
            o.insert("library", JsValue::str(&f.library));
            JsValue::Object(o)
        })
        .collect();
    let mut out = JsObject::new();
    out.insert("schemaVersion", num(1));
    out.insert("commentRegions", JsValue::Array(regions));
    out.insert("libraryFunctions", JsValue::Array(functions));
    out.insert(
        "bannerClassifications",
        JsValue::Array(banner_classifications),
    );
    JsValue::Object(out)
}

#[cfg(test)]
mod function_carry_test;
