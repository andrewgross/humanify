//! The bundle-scale check of the Babel scope view (WP3.1): one JSON line
//! per scope and per binding, byte-comparable with
//! `test/parity/wp31-scope-bundle-probe.mjs` run on the same text. Spans
//! are UTF-16 code units (Babel's `node.start`), converted from oxc's UTF-8
//! offsets here so the two files compare with `cmp`. (Migration
//! scaffolding — deleted at phase 6 with the TS core, 02 §9.)

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use serde_json::json;

use crate::ingest::Ingest;
use crate::rename::validated::scopes::{BScopeId, BabelScopes, BindingId, Site};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Anchor;

use humanify_model::js::Utf16Offsets;

struct Printer<'v> {
    view: &'v BabelScopes,
    offsets: Utf16Offsets,
}

impl Printer<'_> {
    fn key(&self, id: BScopeId) -> String {
        let s = self.view.scope(id);
        format!(
            "{}@{}:{}",
            s.ty.as_str(),
            self.offsets.at(s.span.start),
            self.offsets.at(s.span.end)
        )
    }

    fn site(&self, s: &Site) -> serde_json::Value {
        json!([
            s.ty.as_str(),
            self.offsets.at(s.span.start),
            self.offsets.at(s.span.end),
            self.key(s.scope)
        ])
    }

    fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        for (i, map) in self.view.initial_maps.iter().enumerate() {
            let id = BScopeId(i as u32);
            let names: Vec<&str> = map.iter().map(|(n, _)| n.as_str()).collect();
            lines.push(
                json!({
                    "scope": self.key(id),
                    "parent": self.view.scope(id).parent.map(|p| self.key(p)),
                    "names": names,
                })
                .to_string(),
            );
        }
        for b in &self.view.bindings {
            lines.push(
                json!({
                    "id": [self.offsets.at(b.id_span.start), self.offsets.at(b.id_span.end)],
                    "name": b.name,
                    "kind": b.kind.as_str(),
                    "owner": self.key(b.owner),
                    "refs": b.refs.iter().map(|s| self.site(s)).collect::<Vec<_>>(),
                    "viol": b.violations.iter().map(|s| self.site(s)).collect::<Vec<_>>(),
                })
                .to_string(),
            );
        }
        lines.sort();
        lines
    }
}

/// Parse `text` (as the pipeline ingests it), build the view, return the
/// probe-comparable lines (the last one is the globals).
pub fn scope_view_lines(text: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "input.js");
    if !ingest.errors.is_empty() {
        return Err(format!("{} parse error(s)", ingest.errors.len()));
    }
    let view = BabelScopes::build(ingest.semantic());
    let printer = Printer {
        view: &view,
        offsets: Utf16Offsets::new(text),
    };
    let mut lines = printer.lines();
    let globals: BTreeSet<&String> = view.globals.iter().collect();
    lines.push(json!({ "globals": globals }).to_string());
    Ok(lines)
}

/// The rename-probe candidate for step `i` (see the TS probe's table).
fn probe_candidate(i: usize, crawl: &[String], globals: &[String], first_in_scope: &str) -> String {
    let n = crawl.len();
    match i % 6 {
        0 => crawl[(i + 1) % n].clone(),
        1 => crawl[(i * 7919) % n].clone(),
        2 if !globals.is_empty() => globals[i % globals.len()].clone(),
        2 => format!("g{i}"),
        3 => format!("r{i}"),
        4 => first_in_scope.to_string(),
        _ => format!("q{}", i % 97),
    }
}

/// The bundle-scale check of the validated-rename RULES: the deterministic
/// rename sequence of `test/parity/wp31-rename-bundle-probe.mjs` (every
/// binding, in declaration order, renamed through its own scope from its
/// current name to a rule-exercising candidate), one line per step.
pub fn rename_probe_lines(text: &str) -> Result<Vec<String>, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, text, "input.js");
    if !ingest.errors.is_empty() {
        return Err(format!("{} parse error(s)", ingest.errors.len()));
    }
    let mut state = RenameState::new(ingest.semantic(), Anchor::Fresh);
    let mut order: Vec<BindingId> = (0..state.view().bindings.len() as u32)
        .map(BindingId)
        .collect();
    order.sort_by_key(|b| state.view().binding(*b).id_span.start);
    let crawl: Vec<String> = order
        .iter()
        .map(|b| state.view().binding(*b).name.clone())
        .collect();
    let globals: Vec<String> = state.view().globals.iter().cloned().collect();
    let mut lines = Vec::with_capacity(order.len());
    for (i, &b) in order.iter().enumerate() {
        let owner = state.scope_of_binding(b);
        let old = state.name_of(b).to_string();
        let first = if i % 6 == 4 {
            state
                .bindings_in(owner)
                .first()
                .map(|(n, _)| n.clone())
                .unwrap_or_default()
        } else {
            String::new()
        };
        let to = probe_candidate(i, &crawl, &globals, &first);
        let request = RenameRequest {
            scope: owner,
            old_name: &old,
            new_name: &to,
            expected: None,
        };
        let spec = TrailSpec::Untrailed {
            why: "rename probe",
        };
        let verdict = match state.attempt_validated_rename(request, spec).reason {
            None => "applied",
            Some(r) => r.as_str(),
        };
        lines.push(format!("{i} {old} {to} {verdict}"));
    }
    Ok(lines)
}
