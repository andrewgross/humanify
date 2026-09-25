//! Carry the post-split reconcile's renames into the bundle (exp054) — TS
//! `src/split/bundle-carry.ts`.
//!
//! `.humanify/humanified.js` is the NEXT release's prior; without the carry
//! the tree and the bundle disagree by exactly the post-split renames.
//! TOP-LEVEL renames are never carried (the export key is a string the
//! tree's rename cannot reach — carrying drifted 238/238 export keys on
//! 85→86). A rename is located by the ledger's `emitIndexes` (file slot →
//! bundle statement) and its `nameOrdinal` among same-named declarations
//! in that statement; everything abstains rather than guesses, and the
//! rewrite must re-parse to the same structural signature.

use std::collections::{BTreeMap, HashMap};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_ast::ast::{ArrowFunctionBody, FunctionBody};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;

use humanify_model::js::JsValue;

use crate::babel_view::BabelLines;
use crate::emit::substitutions::{Substitution, apply_substitutions};
use crate::naming::reconcile::resolve::{identifier_sites, shorthand_key};
use crate::rename::validated::scopes::BScopeId;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::Anchor;

use super::reconcile::{PostSplitRename, ident_at, statement_of, substitution_text};
use super::relink::parse_or_err;
use super::vendor_inherit::file_signature;

/// `BundleCarryResult`.
#[derive(Clone, Debug, Default)]
pub struct CarryResult {
    /// The rewritten bundle, or None when nothing was carried.
    pub code: Option<String>,
    pub carried: usize,
    /// Reason → count, in first-occurrence order (a JS Map).
    pub abstained: Vec<(String, usize)>,
    /// The validated renames' claim counters (the run-wide
    /// `renameClaimStats`).
    pub claims: crate::rename::validated::RenameClaimStats,
}

fn bump(abstained: &mut Vec<(String, usize)>, reason: &str) {
    match abstained.iter_mut().find(|(r, _)| r == reason) {
        Some((_, n)) => *n += 1,
        None => abstained.push((reason.to_string(), 1)),
    }
}

fn set(abstained: &mut Vec<(String, usize)>, reason: &str, n: usize) {
    match abstained.iter_mut().find(|(r, _)| r == reason) {
        Some((_, v)) => *v = n,
        None => abstained.push((reason.to_string(), n)),
    }
}

fn strings(v: Option<&JsValue>) -> Vec<String> {
    match v {
        Some(JsValue::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// `wrapperBody(ast, expected)`: the first Babel function (pre-order)
/// whose block body holds exactly `expected` statements — its statement
/// spans. No fallback to the program body.
fn wrapper_body(semantic: &Semantic<'_>, expected: usize) -> Option<Vec<(u32, u32)>> {
    let spans = |body: &FunctionBody<'_>| -> Vec<(u32, u32)> {
        body.statements
            .iter()
            .map(|s| (s.span().start, s.span().end))
            .collect()
    };
    for node in semantic.nodes().iter() {
        let body = match node.kind() {
            AstKind::Function(f) => f.body.as_deref(),
            AstKind::ArrowFunctionExpression(a) => match &a.body {
                ArrowFunctionBody::FunctionBody(b) => Some(&**b),
                _ => None,
            },
            _ => None,
        };
        if let Some(body) = body
            && body.statements.len() == expected
        {
            return Some(spans(body));
        }
    }
    None
}

/// One located rename: its bundle statement index.
struct Target<'r> {
    rename: &'r PostSplitRename,
    stmt: usize,
}

fn resolve_targets<'r>(
    renames: &'r [PostSplitRename],
    ledger: &JsValue,
    body_len: usize,
    abstained: &mut Vec<(String, usize)>,
) -> Vec<Target<'r>> {
    let obj = match ledger {
        JsValue::Object(o) => o,
        _ => return Vec::new(),
    };
    let order = strings(obj.get("order"));
    let mut slots: HashMap<&str, Vec<usize>> = HashMap::new();
    for (slot, file) in order.iter().enumerate() {
        slots.entry(file.as_str()).or_default().push(slot);
    }
    let indexes: Option<Vec<usize>> = match obj.get("emitIndexes") {
        Some(JsValue::Array(items)) => Some(
            items
                .iter()
                .map(|v| match v {
                    JsValue::Number(n) => *n as usize,
                    _ => usize::MAX,
                })
                .collect(),
        ),
        _ => None,
    };
    let mut targets = Vec::new();
    for rename in renames {
        if rename.top_level {
            bump(abstained, "top-level-would-move-an-export-key");
            continue;
        }
        let Some((body_ordinal, _)) = rename.locator else {
            bump(abstained, "no-locator");
            continue;
        };
        let slot = slots
            .get(rename.file.as_str())
            .and_then(|s| s.get(body_ordinal))
            .copied();
        let (Some(slot), Some(indexes)) = (slot, indexes.as_ref()) else {
            bump(
                abstained,
                if indexes.is_some() {
                    "slot-out-of-range"
                } else {
                    "ledger-has-no-emit-indexes"
                },
            );
            continue;
        };
        match indexes.get(slot).filter(|i| **i < body_len) {
            Some(&stmt) => targets.push(Target { rename, stmt }),
            None => bump(abstained, "bundle-index-out-of-range"),
        }
    }
    targets
}

/// `bundleDeclarations`: every binding declared inside a wrapper-body
/// statement, keyed (statement, name), in start order, with its scope.
fn bundle_declarations(
    state: &RenameState,
    body: &[(u32, u32)],
) -> BTreeMap<(usize, String), Vec<(u32, BScopeId)>> {
    let view = state.view();
    let mut out: BTreeMap<(usize, String), Vec<(u32, BScopeId)>> = BTreeMap::new();
    for (si, map) in view.initial_maps.iter().enumerate() {
        for (name, binding) in map {
            let b = view.binding(*binding);
            if b.owner.0 as usize != si {
                continue;
            }
            if let Some(stmt) = statement_of(body, b.id_span.start) {
                out.entry((stmt, name.clone()))
                    .or_default()
                    .push((b.id_span.start, BScopeId(si as u32)));
            }
        }
    }
    for list in out.values_mut() {
        list.sort_by_key(|(start, _)| *start);
    }
    out
}

/// `occurrencesOf`: the declaration, every reference, every write target —
/// each must hold the old name in the TEXT, else None (abstain).
fn occurrences_of(
    state: &RenameState,
    binding: crate::rename::validated::scopes::BindingId,
    from: &str,
    to: &str,
    lines: &BabelLines<'_>,
    text_lines: &[&str],
    shorthand_keys: &HashMap<u32, Option<String>>,
) -> Option<Vec<Substitution>> {
    let b = state.view().binding(binding);
    let starts = std::iter::once(b.id_span.start)
        .chain(b.refs.iter().map(|r| r.span.start))
        .chain(b.violation_targets.iter().flatten().map(|s| s.start));
    let mut subs = Vec::new();
    for start in starts {
        let (line, col) = lines.loc(start);
        let text = text_lines.get(line - 1)?;
        if ident_at(text, col).as_deref() != Some(from) {
            return None;
        }
        let key = shorthand_keys.get(&start).and_then(Option::as_deref);
        subs.push(Substitution {
            line,
            col,
            from: from.to_string(),
            to: substitution_text(key, to),
        });
    }
    Some(subs)
}

/// `carryRenamesIntoBundle(bundleCode, ledger, renames)`. An `Err` is the
/// TS's throw (the caller logs "bundle carry skipped").
pub fn carry_renames_into_bundle(
    bundle: &str,
    ledger: &JsValue,
    renames: &[PostSplitRename],
) -> Result<CarryResult, String> {
    let mut result = CarryResult::default();
    if renames.is_empty() {
        return Ok(result);
    }
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, bundle)?;
    let order_len = match ledger {
        JsValue::Object(o) => strings(o.get("order")).len(),
        _ => 0,
    };
    let Some(body) = wrapper_body(ingest.semantic(), order_len) else {
        set(
            &mut result.abstained,
            "wrapper-body-not-found",
            renames.len(),
        );
        return Ok(result);
    };
    let targets = resolve_targets(renames, ledger, body.len(), &mut result.abstained);
    let mut state = RenameState::new(ingest.semantic(), Anchor::Shipped);
    let decls = bundle_declarations(&state, &body);
    let lines = BabelLines::new(bundle);
    let text_lines: Vec<&str> = bundle.split('\n').collect();
    let shorthand_keys: HashMap<u32, Option<String>> = identifier_sites(ingest.semantic(), &state)
        .iter()
        .filter(|site| site.resolvable())
        .map(|site| (site.start, shorthand_key(ingest.semantic(), site)))
        .collect();
    let mut subs: Vec<Substitution> = Vec::new();
    for target in &targets {
        let r = target.rename;
        let ordinal = r.locator.map_or(usize::MAX, |(_, n)| n);
        let scope = decls
            .get(&(target.stmt, r.from_name.clone()))
            .and_then(|list| list.get(ordinal))
            .map(|(_, s)| *s);
        let Some(binding) = scope.and_then(|s| state.get_binding(s, &r.from_name)) else {
            bump(&mut result.abstained, "binding-not-found");
            continue;
        };
        let scope = scope.expect("found above");
        let Some(occurrences) = occurrences_of(
            &state,
            binding,
            &r.from_name,
            &r.to_name,
            &lines,
            &text_lines,
            &shorthand_keys,
        ) else {
            bump(&mut result.abstained, "occurrence-not-in-text");
            continue;
        };
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope,
                old_name: &r.from_name,
                new_name: &r.to_name,
                expected: None,
            },
            TrailSpec::Untrailed {
                why: "bundle-carry",
            },
        );
        if !attempt.applied {
            let reason = attempt.reason.map_or("", |x| x.as_str());
            bump(&mut result.abstained, &format!("rename-rejected:{reason}"));
            continue;
        }
        subs.extend(occurrences);
        result.carried += 1;
    }
    result.claims = state.claim_stats();
    if result.carried == 0 {
        return Ok(result);
    }
    let owned: Vec<String> = text_lines.iter().map(|l| l.to_string()).collect();
    let code = apply_substitutions(&owned, &subs);
    let baseline = file_signature(bundle);
    if baseline.is_none() || file_signature(&code) != baseline {
        set(&mut result.abstained, "rewrite-unsound", result.carried);
        result.carried = 0;
        return Ok(result);
    }
    result.code = Some(code);
    Ok(result)
}
