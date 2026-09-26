//! What a factory body reaches OUTSIDE itself (finding #51).
//!
//! A vendored factory runs as its own CommonJS module, where the only names
//! bound are its own, the CJS wrapper's (`exports`, `require`, `module`,
//! `__filename`, `__dirname`) and the factory references the finish relinks.
//! A body that also references some OTHER bundle-scope binding — Bun's
//! `__toESM` helper, or an app ESM module's init + namespace
//! (`(initM(), ns)`, how Bun compiles `import()` / `require()` of an ESM
//! module) — would reference a FREE name in the runnable tree: a
//! ReferenceError the moment that path runs. Measured on the four eval
//! inputs: 3–8 factories per bundle, over `__toESM`, `__toCommonJS` and one
//! or two ESM init/namespace pairs.
//!
//! Two answers, both decided here before anything is named:
//!
//! - Bun's `__toESM` / `__toCommonJS` are pure runtime helpers, recognised
//!   by SHAPE. A vendor body's reference is rewritten to the canonical name
//!   and the finish binds it from `.humanify/__bun-runtime.js`, which carries
//!   Bun's implementation (`finish::relink`).
//! - Anything else stays in the APP: the factory is not extracted, so its
//!   body keeps resolving against the bundle scope exactly as in the input.
//!   So does every factory that references a kept one (its reference would
//!   otherwise be the kept factory's var, free in the vendor file) — the
//!   closure.
//!
//! If Bun changes the helpers' shape, they fall to the second answer:
//! more app code, never a free name.

use std::collections::{HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, Expression};
use oxc_semantic::SymbolId;
use oxc_span::GetSpan;
use oxc_syntax::reference::ReferenceFlags;

use crate::ingest::Ingest;
use crate::modules::FactoryRecord;

use super::TextEdit;

/// The canonical names the relink shim exports.
pub const TO_ESM: &str = "__toESM";
pub const TO_COMMON_JS: &str = "__toCommonJS";

/// The names Node's CommonJS wrapper binds in every vendor file — a bundle
/// wrapper's parameters of these names resolve there too.
const CJS_WRAPPER_NAMES: [&str; 5] = ["exports", "require", "module", "__filename", "__dirname"];

/// The decision for one bundle.
#[derive(Debug, Default)]
pub struct ScopePlan {
    /// Factories (indices into the classification) that stay in the app.
    pub kept: HashSet<usize>,
    /// Helper references inside VENDORED bodies → the canonical name.
    pub helper_edits: Vec<TextEdit>,
}

/// What one captured (bundle-scope) symbol is to a vendor body.
enum Capture {
    /// Another factory (by index): relinked, but kept together with it.
    Factory(usize),
    /// Resolved in the vendor file anyway (a CJS wrapper name, or the
    /// require alias the unpack rewrites to `require`).
    Resolved,
    /// A Bun runtime helper, bound from the shim under this name.
    Helper(&'static str),
    /// Anything else: only the app can resolve it.
    App,
}

/// Plan every factory body's out-of-body references.
pub fn plan_bundle_scope_refs(
    code: &str,
    ingest: &Ingest<'_>,
    factories: &[FactoryRecord],
    require_var: Option<&str>,
) -> ScopePlan {
    let scoping = ingest.semantic().scoping();
    let nodes = ingest.semantic().nodes();
    let mut bodies: Vec<(u32, u32, usize)> = factories
        .iter()
        .enumerate()
        .map(|(i, f)| (f.body_span.start, f.body_span.end, i))
        .collect();
    bodies.sort_unstable();
    let by_declarator: HashMap<(u32, u32), usize> = factories
        .iter()
        .enumerate()
        .map(|(i, f)| ((f.span.start, f.span.end), i))
        .collect();
    let canonical_taken = canonical_names_taken(ingest);

    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); factories.len()];
    let mut kept = HashSet::new();
    let mut helper_edits = Vec::new();
    let mut captures: HashMap<SymbolId, Capture> = HashMap::new();
    for symbol in scoping.symbol_ids() {
        let declared_at = scoping.symbol_span(symbol).start;
        for &reference_id in scoping.get_resolved_reference_ids(symbol) {
            let span = nodes
                .get_node(scoping.get_reference(reference_id).node_id())
                .span();
            let Some(body) = containing_body(&bodies, span.start) else {
                continue;
            };
            let (start, end, idx) = bodies[body];
            if (start..end).contains(&declared_at) {
                continue; // the body's own binding
            }
            let capture = captures.entry(symbol).or_insert_with(|| {
                classify_capture(code, ingest, symbol, &by_declarator, require_var)
            });
            match capture {
                Capture::Factory(j) => {
                    edges[idx].insert(*j);
                }
                Capture::Resolved => {}
                Capture::Helper(name) if !canonical_taken => helper_edits.push(TextEdit {
                    start: span.start as usize,
                    end: span.end as usize,
                    replacement: (*name).to_string(),
                }),
                Capture::Helper(_) | Capture::App => {
                    kept.insert(idx);
                }
            }
        }
    }
    close_over_dependents(&mut kept, &edges);
    // A kept body is app code: its helper references stay as they are.
    helper_edits.retain(|e| {
        containing_body(&bodies, e.start as u32).is_none_or(|b| !kept.contains(&bodies[b].2))
    });
    ScopePlan { kept, helper_edits }
}

/// The body holding `pos` (the innermost when factories nest).
fn containing_body(bodies: &[(u32, u32, usize)], pos: u32) -> Option<usize> {
    let upto = bodies.partition_point(|b| b.0 <= pos);
    (0..upto).rev().find(|&k| pos < bodies[k].1)
}

/// A factory referencing a kept factory is kept too, to a fixpoint.
fn close_over_dependents(kept: &mut HashSet<usize>, edges: &[HashSet<usize>]) {
    loop {
        let before = kept.len();
        for (i, deps) in edges.iter().enumerate() {
            if !kept.contains(&i) && deps.iter().any(|d| kept.contains(d)) {
                kept.insert(i);
            }
        }
        if kept.len() == before {
            return;
        }
    }
}

/// Does the bundle already use a canonical helper name anywhere (a binding,
/// or a free reference)? Then a rewrite to it could be captured — keep the
/// helper's users in the app instead.
fn canonical_names_taken(ingest: &Ingest<'_>) -> bool {
    let scoping = ingest.semantic().scoping();
    let taken = |name: &str| name == TO_ESM || name == TO_COMMON_JS;
    scoping.symbol_ids().any(|s| taken(scoping.symbol_name(s)))
        || scoping
            .root_unresolved_references()
            .keys()
            .any(|k| taken(k))
}

fn classify_capture(
    code: &str,
    ingest: &Ingest<'_>,
    symbol: SymbolId,
    by_declarator: &HashMap<(u32, u32), usize>,
    require_var: Option<&str>,
) -> Capture {
    let scoping = ingest.semantic().scoping();
    let nodes = ingest.semantic().nodes();
    let name = scoping.symbol_name(symbol);
    let decl = scoping.symbol_declaration(symbol);
    if let AstKind::VariableDeclarator(d) = nodes.kind(decl)
        && let Some(&j) = by_declarator.get(&(d.span.start, d.span.end))
    {
        return Capture::Factory(j);
    }
    let is_param = matches!(nodes.kind(decl), AstKind::FormalParameter(_))
        || matches!(nodes.parent_kind(decl), AstKind::FormalParameter(_));
    if (is_param && CJS_WRAPPER_NAMES.contains(&name)) || Some(name) == require_var {
        return Capture::Resolved;
    }
    let never_written = scoping.symbol_redeclarations(symbol).is_empty()
        && scoping.get_resolved_reference_ids(symbol).iter().all(|&r| {
            !scoping
                .get_reference(r)
                .flags()
                .contains(ReferenceFlags::Write)
        });
    if never_written
        && let AstKind::VariableDeclarator(d) = nodes.kind(decl)
        && let BindingPattern::BindingIdentifier(_) = d.id
        && let Some(helper) = d.init.as_ref().and_then(|init| helper_shape(code, init))
    {
        return Capture::Helper(helper);
    }
    Capture::App
}

/// Bun's `__toESM` / `__toCommonJS`, by shape: an arrow of three / one
/// parameters whose body builds the ES-module interop object.
///
/// `__toESM = (mod, isNodeMode, target) => {…"default"…__esModule…}`;
/// `__toCommonJS = (from) => {…WeakMap…"__esModule"…}`.
fn helper_shape(code: &str, init: &Expression<'_>) -> Option<&'static str> {
    let Expression::ArrowFunctionExpression(arrow) = init else {
        return None;
    };
    let span = arrow.span;
    let text = &code[span.start as usize..span.end as usize];
    let params = arrow.params.items.len();
    if params == 3 && text.contains("\"default\"") && text.contains(".__esModule") {
        return Some(TO_ESM);
    }
    if params == 1 && text.contains("\"__esModule\"") && text.contains("WeakMap") {
        return Some(TO_COMMON_JS);
    }
    None
}
