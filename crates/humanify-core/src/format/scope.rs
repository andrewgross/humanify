//! The one scope question the stage-6 visitors ask:
//! `path.scope.hasBinding("undefined", { noGlobals: true })` (the
//! beautifier's `void` rewrites; `convertVoidToUndefined` asks nothing —
//! finding #42).
//!
//! The answer comes from Babel's scope model as the port already
//! reconstructs it over oxc's semantic (`rename::validated::scopes`,
//! pinned against Babel's own crawl — WP3.1): the Babel scopes whose OWN
//! bindings include `undefined`, mapped to the arena nodes that own them
//! (same span, same Babel type). The traversal then walks a path's parent
//! chain through those nodes ([`super::traverse::Engine::has_undefined_binding`]).
//! Bindings never move between scopes during the beautify: every visitor
//! that rebuilds a declaration reuses its declarators in the same block,
//! and the blocks it synthesizes hold no declarations of their own.

use std::collections::{HashMap, HashSet};

use super::ast::{NodeId, Tree};
use crate::ingest::Ingest;
use crate::rename::validated::scopes::BabelScopes;

/// The arena nodes whose Babel scope declares `undefined`.
pub fn undefined_binding_scopes(
    tree: &Tree,
    ingest: &Ingest<'_>,
) -> Result<HashSet<NodeId>, String> {
    let scoping = ingest.semantic().scoping();
    let any = scoping
        .symbol_ids()
        .any(|s| scoping.symbol_name(s) == "undefined");
    if !any {
        return Ok(HashSet::new());
    }
    let scopes = BabelScopes::build(ingest.semantic());
    let mut wanted: HashMap<(u32, u32, &'static str), ()> = HashMap::new();
    for (i, map) in scopes.initial_maps.iter().enumerate() {
        if map.iter().any(|(name, _)| name == "undefined") {
            let s = &scopes.scopes[i];
            wanted.insert((s.span.start, s.span.end, s.ty.as_str()), ());
        }
    }
    let mut out = HashSet::new();
    for (i, node) in tree.nodes.iter().enumerate() {
        if let Some((start, end)) = node.span
            && wanted.contains_key(&(start, end, node.kind.type_name()))
        {
            out.insert(NodeId(i as u32));
        }
    }
    if out.len() < wanted.len() {
        return Err(format!(
            "undefined-binding scopes: {} Babel scope(s), {} matched in the tree",
            wanted.len(),
            out.len()
        ));
    }
    Ok(out)
}
