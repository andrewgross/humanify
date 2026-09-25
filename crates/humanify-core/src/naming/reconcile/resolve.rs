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
//!   value, so the key twin is synthesized here.
//!
//! A position resolves as an occurrence only through the binding's own
//! bookkeeping (declaration identifier, identifier reference paths,
//! constant-violation write targets) AND only when `path.scope.getBinding`
//! from the identifier's own Babel scope names that binding. Key twins,
//! member properties, labels and private names never resolve.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_semantic::Semantic;

use super::hunks::PositionCandidate;
use crate::babel_view::BabelLines;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BScopeId, BindingId, SiteType};

/// How a Babel `Identifier` node relates to scope bookkeeping.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SiteKind {
    /// An oxc `IdentifierReference` — may be a reference or a write.
    Reference,
    /// An oxc `BindingIdentifier` — a declaration (or a redeclaring write).
    Binding,
    /// Property keys, member properties, labels, private names, key twins.
    Other,
}

/// One Babel `Identifier` node.
#[derive(Clone, Copy, Debug)]
struct IdentSite<'a> {
    /// Byte offset where babel's Identifier starts.
    start: u32,
    name: &'a str,
    kind: SiteKind,
    scope: BScopeId,
}

/// Every Babel `Identifier` node of the program, in source order (babel's
/// traversal order for identifiers; a twin pair keeps key-first order).
fn identifier_sites<'a>(semantic: &'a Semantic<'_>, state: &RenameState) -> Vec<IdentSite<'a>> {
    let view = state.view();
    let mut out: Vec<(u8, IdentSite<'a>)> = Vec::new();
    for node in semantic.nodes().iter() {
        let scope = || view.scope_of_node(node.id());
        let (rank, site) = match node.kind() {
            AstKind::IdentifierReference(r) => (
                1,
                IdentSite {
                    start: r.span.start,
                    name: r.name.as_str(),
                    kind: SiteKind::Reference,
                    scope: scope(),
                },
            ),
            AstKind::BindingIdentifier(b) => (
                1,
                IdentSite {
                    start: b.span.start,
                    name: b.name.as_str(),
                    kind: SiteKind::Binding,
                    scope: scope(),
                },
            ),
            AstKind::IdentifierName(n) => (
                0,
                IdentSite {
                    start: n.span.start,
                    name: n.name.as_str(),
                    kind: SiteKind::Other,
                    scope: scope(),
                },
            ),
            AstKind::LabelIdentifier(l) => (
                0,
                IdentSite {
                    start: l.span.start,
                    name: l.name.as_str(),
                    kind: SiteKind::Other,
                    scope: scope(),
                },
            ),
            AstKind::PrivateIdentifier(p) => (
                0,
                IdentSite {
                    start: p.span.start + 1,
                    name: p.name.as_str(),
                    kind: SiteKind::Other,
                    scope: scope(),
                },
            ),
            AstKind::AssignmentTargetPropertyIdentifier(p) => (
                0,
                IdentSite {
                    start: p.binding.span.start,
                    name: p.binding.name.as_str(),
                    kind: SiteKind::Other,
                    scope: scope(),
                },
            ),
            _ => continue,
        };
        out.push((rank, site));
    }
    out.sort_by_key(|(rank, s)| (s.start, *rank));
    out.into_iter().map(|(_, s)| s).collect()
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
