//! Position → binding resolution — diff-reconcile.ts `resolveCandidates`,
//! `resolveOccurrence` and `collectIdentifierNames`, over Babel's
//! `Identifier` nodes as oxc spells them.
//!
//! Babel has ONE node type for every identifier the TS visitor sees; oxc
//! splits them (`IdentifierReference`, `BindingIdentifier`,
//! `IdentifierName`, `LabelIdentifier`, `PrivateIdentifier`). Two shapes
//! differ in more than the name:
//!
//! - a `#name` is babel's `PrivateName { id: Identifier }` — the Identifier
//!   starts AFTER the `#`;
//! - a shorthand in an assignment pattern (`({ a } = y)`) is babel's
//!   `ObjectProperty` with a KEY identifier and a VALUE identifier at the
//!   same position; oxc's `AssignmentTargetPropertyIdentifier` has only the
//!   value, so the key twin is synthesized here;
//! - `import.meta` / `new.target` are babel's `MetaProperty` of two
//!   Identifiers; oxc's `ImportMeta` / `NewTarget` carry none.
//!
//! A position resolves as an occurrence only through the binding's own
//! bookkeeping (declaration identifier, identifier reference paths,
//! constant-violation write targets) AND only when `path.scope.getBinding`
//! from the identifier's own Babel scope names that binding. Key twins,
//! member properties, labels and private names never resolve.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{BindingPattern, PropertyKey};
use oxc_semantic::{AstNodes, NodeId, Semantic};
use oxc_span::GetSpan;

use super::hunks::PositionCandidate;
use crate::babel_view::BabelLines;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, BindingId, SiteType};

/// How a Babel `Identifier` node relates to scope bookkeeping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SiteKind {
    /// An oxc `IdentifierReference` — may be a reference or a write.
    Reference,
    /// An oxc `BindingIdentifier` — a declaration (or a redeclaring write).
    Binding,
    /// Property keys, member properties, labels, private names, meta
    /// property words, key twins.
    Other,
}

/// One Babel `Identifier` node — the ONE view of "which Identifier nodes
/// would a TS `traverse(ast, { Identifier })` visit, and where"
/// (docs/responsibility.md); the diff reconcile, the post-split text
/// rewrite and the bundle carry all read it.
#[derive(Clone, Copy, Debug)]
pub struct IdentSite<'a> {
    /// Byte offset where babel's Identifier starts.
    pub start: u32,
    pub name: &'a str,
    pub kind: SiteKind,
    pub scope: BScopeId,
    /// The oxc node (None for a synthesized key twin).
    pub node: Option<NodeId>,
}

impl IdentSite<'_> {
    /// `scope.getBinding` can make it an occurrence (a reference or binding
    /// identifier); keys, member properties, labels, private names never.
    pub fn resolvable(&self) -> bool {
        self.kind != SiteKind::Other
    }
}

/// Every Babel `Identifier` node of the program, in source order (babel's
/// traversal order for identifiers; a twin pair keeps key-first order).
/// Babel's `MetaProperty` holds two Identifiers (`import`/`meta`,
/// `new`/`target`); oxc's `ImportMeta`/`NewTarget` hold none.
pub fn identifier_sites<'a>(semantic: &'a Semantic<'_>, state: &RenameState) -> Vec<IdentSite<'a>> {
    let view = state.view();
    let mut out: Vec<(u8, IdentSite<'a>)> = Vec::new();
    for node in semantic.nodes().iter() {
        let id = node.id();
        let site = |start: u32, name: &'a str, kind: SiteKind, node: Option<NodeId>| IdentSite {
            start,
            name,
            kind,
            scope: view.scope_of_node(id),
            node,
        };
        let (rank, s) = match node.kind() {
            AstKind::IdentifierReference(r) => (
                1,
                site(r.span.start, r.name.as_str(), SiteKind::Reference, Some(id)),
            ),
            AstKind::BindingIdentifier(b) => (
                1,
                site(b.span.start, b.name.as_str(), SiteKind::Binding, Some(id)),
            ),
            AstKind::IdentifierName(n) => (
                0,
                site(n.span.start, n.name.as_str(), SiteKind::Other, Some(id)),
            ),
            AstKind::LabelIdentifier(l) => (
                0,
                site(l.span.start, l.name.as_str(), SiteKind::Other, Some(id)),
            ),
            AstKind::PrivateIdentifier(p) => (
                0,
                site(p.span.start + 1, p.name.as_str(), SiteKind::Other, Some(id)),
            ),
            // The key twin Babel holds and oxc does not.
            AstKind::AssignmentTargetPropertyIdentifier(p) => (
                0,
                site(
                    p.binding.span.start,
                    p.binding.name.as_str(),
                    SiteKind::Other,
                    None,
                ),
            ),
            AstKind::ImportMeta(m) => {
                out.push((0, site(m.span.start, "import", SiteKind::Other, Some(id))));
                (0, site(m.span.end - 4, "meta", SiteKind::Other, Some(id)))
            }
            AstKind::NewTarget(m) => {
                out.push((0, site(m.span.start, "new", SiteKind::Other, Some(id))));
                (0, site(m.span.end - 6, "target", SiteKind::Other, Some(id)))
            }
            _ => continue,
        };
        out.push((rank, s));
    }
    out.sort_by_key(|(rank, s)| (s.start, *rank));
    out.into_iter().map(|(_, s)| s).collect()
}

fn parent(nodes: &AstNodes<'_>, id: NodeId) -> Option<NodeId> {
    let p = nodes.parent_id(id);
    (p != id).then_some(p)
}

fn key_name(key: &PropertyKey<'_>) -> Option<String> {
    match key {
        PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
        _ => None,
    }
}

/// `shorthandKeyName(path)`: the shorthand property's key name when the
/// site is the VALUE of a shorthand property (directly, or as a defaulted
/// pattern's left) — `renameSubstitutionText`'s `key: newName` expansion.
pub fn shorthand_key(semantic: &Semantic<'_>, site: &IdentSite<'_>) -> Option<String> {
    if !site.resolvable() {
        return None;
    }
    let nodes = semantic.nodes();
    let id = site.node?;
    let span = nodes.kind(id).span();
    let p = parent(nodes, id)?;
    match nodes.kind(p) {
        AstKind::ObjectProperty(prop) if prop.shorthand && prop.value.span() == span => {
            key_name(&prop.key)
        }
        AstKind::AssignmentTargetPropertyIdentifier(prop) => Some(prop.binding.name.to_string()),
        AstKind::BindingProperty(prop)
            if prop.shorthand && matches!(prop.value, BindingPattern::BindingIdentifier(_)) =>
        {
            key_name(&prop.key)
        }
        AstKind::AssignmentPattern(ap) if ap.left.span() == span => {
            let g = parent(nodes, p)?;
            match nodes.kind(g) {
                AstKind::BindingProperty(prop)
                    if prop.shorthand && prop.value.span() == ap.span =>
                {
                    key_name(&prop.key)
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// `collectIdentifierNames`: every Identifier name in the program.
pub fn collect_identifier_names(semantic: &Semantic<'_>, state: &RenameState) -> HashSet<String> {
    identifier_sites(semantic, state)
        .into_iter()
        .map(|s| s.name.to_string())
        .collect()
}

/// `resolveOccurrence`: the binding the identifier is an occurrence of, and
/// whether it is a REFERENCE (vs the declaration or a write target).
fn resolve_occurrence(state: &RenameState, site: &IdentSite<'_>) -> Option<(BindingId, bool)> {
    if site.kind == SiteKind::Other {
        return None;
    }
    let binding = state.get_binding(site.scope, site.name)?;
    let b = state.view().binding(binding);
    if site.kind == SiteKind::Binding && b.id_span.start == site.start {
        return Some((binding, false));
    }
    if site.kind == SiteKind::Reference
        && b.refs
            .iter()
            .any(|r| r.ty == SiteType::Identifier && r.span.start == site.start)
    {
        return Some((binding, true));
    }
    let writes = b
        .violation_targets
        .iter()
        .flatten()
        .any(|t| t.start == site.start);
    writes.then_some((binding, false))
}

/// One resolved candidate occurrence.
#[derive(Clone, Debug)]
pub struct ResolvedOccurrence {
    pub binding: BindingId,
    /// Index into the analysis candidates.
    pub candidate: usize,
}

/// `Resolution`.
pub struct Resolution {
    pub occurrences: Vec<ResolvedOccurrence>,
    pub tainted_hunks: BTreeSet<usize>,
}

/// `resolveCandidates`.
pub fn resolve_candidates(
    semantic: &Semantic<'_>,
    state: &RenameState,
    lines: &BabelLines<'_>,
    candidates: &[PositionCandidate],
) -> Resolution {
    // `byPos` — a Map: the LAST candidate at a position wins.
    let mut by_pos: HashMap<(usize, usize), usize> = HashMap::new();
    let mut pos_order: Vec<(usize, usize)> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        if by_pos.insert((c.line, c.col), i).is_none() {
            pos_order.push((c.line, c.col));
        }
    }
    let wanted_lines: HashSet<usize> = candidates.iter().map(|c| c.line).collect();
    let mut occurrences = Vec::new();
    let mut reference_resolved: HashSet<(usize, usize)> = HashSet::new();
    let mut other_resolved: HashSet<(usize, usize)> = HashSet::new();
    let mut twin_failed: HashSet<(usize, usize)> = HashSet::new();
    for site in identifier_sites(semantic, state) {
        let line = lines.line(site.start);
        if !wanted_lines.contains(&line) {
            continue;
        }
        let key = lines.loc(site.start);
        let Some(&ci) = by_pos.get(&key) else {
            continue;
        };
        if site.name != candidates[ci].from_name {
            continue;
        }
        match resolve_occurrence(state, &site) {
            None => {
                twin_failed.insert(key);
            }
            Some((binding, is_reference)) => {
                occurrences.push(ResolvedOccurrence {
                    binding,
                    candidate: ci,
                });
                if is_reference {
                    reference_resolved.insert(key);
                } else {
                    other_resolved.insert(key);
                }
            }
        }
    }
    let mut tainted_hunks = BTreeSet::new();
    for key in pos_order {
        let clean = reference_resolved.contains(&key)
            || (other_resolved.contains(&key) && !twin_failed.contains(&key));
        if !clean {
            tainted_hunks.insert(candidates[by_pos[&key]].hunk_index);
        }
    }
    Resolution {
        occurrences,
        tainted_hunks,
    }
}
