//! The bundle's own package in library discovery (exp183): discovery's
//! view of the bundle's modules, handed to the ONE owner of the question
//! ([`crate::modules::own_package`]), and the fence that sets the own
//! package's claims apart.
//!
//! The fence asks the entry rule ALONE
//! ([`crate::modules::own_package::own_packages_from_entries`]): the
//! front's manifest and mutual rules answer the app-module decision's
//! question, and at discovery scope they are unsound (an unclaimed
//! module is no proof of app code — a library reads its own
//! `package.json` inside the bundle, and unclaimed helpers relay the
//! entry's requires to it).
//!
//! The view: one module per module group ([`super::module_groups`]: a
//! lazy ES module or a bundled CommonJS module, with its hoisted
//! statements and its body), its claims, the inlined `package.json` a
//! declared seed read in it, and the modules its statements READ (a
//! write or an init-time mutation is no dependency). The entries: the
//! modules the statements after the last module head call
//! ([`crate::modules::app_factories::entry_factories`], the one glue
//! grammar) — Bun's and esbuild's trailing `module.exports = {}; X();`.
//!
//! The fence: the own package's claims held their units through the
//! fixpoint (no other package grew into the app's code), then leave the
//! claims for [`Discovery::self_claims`] — app code, named like app code,
//! never vendored, never restored from the index. The queue forgets the
//! package, so the targeted round never looks for more of it.

use std::collections::BTreeSet;

use oxc_ast::AstKind;
use oxc_span::GetSpan;

use super::{Discovery, Evidence, Groups, module_groups};
use crate::cut::units::UnitKind;
use crate::graph::edges::EdgeKind;
use crate::graph::references::ReferenceGraph;
use crate::modules::own_package::{ModuleEvidence, majority, own_packages_from_entries};

/// The reader whose declared seed is an inlined `package.json`.
const PACKAGE_JSON_READER: &str = "package-json";

fn is_head(graph: &ReferenceGraph, u: u32) -> bool {
    matches!(
        graph.units[u as usize].kind,
        UnitKind::Factory | UnitKind::LazyModule
    )
}

/// Discovery's view of the modules (module doc), one per group.
fn module_evidence(
    graph: &ReferenceGraph,
    groups: &Groups,
    found: &Discovery,
) -> Vec<ModuleEvidence> {
    let n = groups.statements.len();
    let mut members: Vec<Vec<u32>> = vec![Vec::new(); n];
    for (u, g) in groups.of.iter().enumerate() {
        if let Some(g) = g {
            members[*g as usize].push(u as u32);
        }
    }
    let mut requires: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for e in graph.edges.iter().filter(|e| e.kind == EdgeKind::Read) {
        if let (Some(a), Some(b)) = (groups.of[e.from as usize], groups.of[e.to as usize])
            && a != b
        {
            requires[a as usize].insert(b as usize);
        }
    }
    members
        .iter()
        .zip(requires)
        .map(|(units, requires)| {
            let claimed: Vec<(u32, &super::Claim)> = units
                .iter()
                .filter_map(|&u| found.claims.get(&u).map(|c| (u, c)))
                .collect();
            // Most of the module's OWN code: its wrapper and body, not the
            // statements hoisted ahead of it (every eager module's code
            // since the previous head joins the group).
            let own_code: Vec<u32> = units
                .iter()
                .copied()
                .filter(|&u| {
                    is_head(graph, u)
                        || matches!(
                            graph.units[u as usize].kind,
                            UnitKind::LazyBody { .. } | UnitKind::FactoryBody { .. }
                        )
                })
                .collect();
            let bytes: u64 = own_code
                .iter()
                .map(|&u| u64::from(graph.units[u as usize].bytes))
                .sum();
            ModuleEvidence {
                packages: claimed.iter().map(|(_, c)| c.package.clone()).collect(),
                majority: majority(
                    own_code.iter().filter_map(|&u| {
                        let c = found.claims.get(&u)?;
                        Some((c.package.as_str(), u64::from(graph.units[u as usize].bytes)))
                    }),
                    bytes,
                ),
                manifest_of: claimed.iter().find_map(|(_, c)| match &c.evidence {
                    Some(Evidence::Declared { reader }) if reader == PACKAGE_JSON_READER => {
                        Some(c.package.clone())
                    }
                    _ => None,
                }),
                requires: requires.into_iter().collect(),
                ..ModuleEvidence::default()
            }
        })
        .collect()
}

/// The modules (group ids) the statements after the last module head run.
fn entry_groups(
    ingest: &crate::ingest::Ingest<'_>,
    graph: &ReferenceGraph,
    groups: &Groups,
    statements: &[oxc_ast::ast::Statement<'_>],
) -> Vec<usize> {
    let Some(last_head_end) = (0..graph.units.len() as u32)
        .filter(|&u| is_head(graph, u))
        .map(|u| graph.units[u as usize].span.end)
        .max()
    else {
        return Vec::new();
    };
    let semantic = ingest.semantic();
    let scoping = semantic.scoping();
    let nodes = semantic.nodes();
    let tail = statements
        .iter()
        .filter(|s| s.span().start >= last_head_end);
    crate::modules::app_factories::entry_factories(
        tail,
        |_| false,
        |id| {
            let symbol = scoping.get_reference(id.reference_id()).symbol_id()?;
            let AstKind::VariableDeclarator(d) = nodes.kind(scoping.symbol_declaration(symbol))
            else {
                return None;
            };
            let unit = graph.unit_at(d.span.start)?;
            if !is_head(graph, unit) {
                return None;
            }
            groups.of[unit as usize].map(|g| g as usize)
        },
        None,
    )
    .unwrap_or_default()
}

/// The bundle's own packages, read from what discovery found.
pub(super) fn own_packages_of(
    ingest: &crate::ingest::Ingest<'_>,
    graph: &ReferenceGraph,
    statements: &[oxc_ast::ast::Statement<'_>],
    found: &Discovery,
) -> BTreeSet<String> {
    let groups = module_groups(graph);
    let entries = entry_groups(ingest, graph, &groups, statements);
    let evidence = module_evidence(graph, &groups, found);
    if entries.is_empty() {
        return BTreeSet::new();
    }
    own_packages_from_entries(&evidence, &entries)
}

/// `found` with the own packages' claims set apart (module doc).
pub(super) fn set_apart(mut found: Discovery, own: &BTreeSet<String>) -> Discovery {
    if own.is_empty() {
        return found;
    }
    let (kept, own_claims): (Vec<_>, Vec<_>) = std::mem::take(&mut found.claims)
        .into_iter()
        .partition(|(_, c)| !own.contains(&c.package));
    found.claims = kept.into_iter().collect();
    found.self_claims.extend(own_claims);
    found.own_packages.extend(own.iter().cloned());
    found.library_class.retain(|_, p| !own.contains(p));
    found.boundary.retain(|p, _| !own.contains(p));
    for q in &mut found.queue {
        q.uses.retain(|p| !own.contains(p));
    }
    found.queue.retain(|q| !q.uses.is_empty());
    found
}
