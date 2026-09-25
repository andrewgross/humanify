//! Post-split prior-diff reconciliation (exp054) in its TEXT form — TS
//! `src/split/post-split-reconcile.ts` (12 §5: the plan-based form flips
//! post-cutover behind its own proof).
//!
//! Each emitted split file is reconciled against the prior release's file
//! at the same path: the reconcile tiers ([`crate::naming::reconcile`])
//! scoped to one file (~20 candidates instead of the bundle's ~60,000).
//! Renames land in the file's rename overlay; the TEXT is then rewritten at
//! every identifier position whose name changed (shorthand-aware), never
//! re-generated, and the rewrite must re-parse to the same structural
//! signature or the file ships as emitted. Top-level renames patch the
//! ledger's `emitNames` / `nameToFiles` (never `emitHashes`: the statement
//! hash masks names).
//!
//! Best-effort throughout, as the TS: any failure is a DISCARD of that
//! file, never a failed run.

use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, ForStatementLeft, Program, Statement};
use oxc_span::GetSpan;
use oxc_syntax::identifier::{is_identifier_part, is_identifier_start};

use humanify_model::js::{JsObject, JsValue, cmp_utf16};

use crate::babel_view::BabelLines;
use crate::emit::substitutions::{Substitution, apply_substitutions};
use crate::naming::reconcile::hunks::compute_normal_diff;
use crate::naming::reconcile::resolve::{IdentSite, identifier_sites, shorthand_key};
use crate::naming::reconcile::{ReconcileOptions, collect_word_tokens, reconcile_diff_noise};
use crate::rename::eligibility::Eligibility;
use crate::rename::validated::RenameState;
use crate::rename::validated::scopes::{BabelScopes, SiteType};
use crate::trail::Anchor;

use super::relink::parse_or_err;
use super::vendor_inherit::file_signature;

/// One rename the pass shipped (`PostSplitRename`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostSplitRename {
    pub file: String,
    pub from_name: String,
    pub to_name: String,
    pub kind: &'static str,
    pub votes: usize,
    pub top_level: bool,
    /// (bodyOrdinal, nameOrdinal).
    pub locator: Option<(usize, usize)>,
}

/// `PostSplitReconcileStats`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PostSplitStats {
    pub considered: usize,
    pub changed: usize,
    pub corpus_gated: usize,
    pub discarded: usize,
    pub incoherent: usize,
}

/// `PostSplitReconcileResult`.
#[derive(Clone, Debug, Default)]
pub struct PostSplitResult {
    /// Only the files whose text changed, in ledger order.
    pub changed: Vec<(String, String)>,
    pub renames: Vec<PostSplitRename>,
    pub stats: PostSplitStats,
    /// What the pass recorded into the run's strategy trail, per file in
    /// visit order: the file's text (the rows' spans index it) and its
    /// rows. The TS records into the ONE run-wide trail with the reconcile
    /// pass's `"generated"` label (diff-reconcile.ts), one entry per
    /// binding NODE — never merged with the naming era's rows.
    pub trail: Vec<(String, Vec<crate::trail::TrailEntry>)>,
    /// The validated renames' claim counters (the run-wide
    /// `renameClaimStats`).
    pub claims: crate::rename::validated::RenameClaimStats,
}

// ---------------------------------------------------------------------------
// The ledger (`StableSplitLedger`, as the JSON object the TS mutates)
// ---------------------------------------------------------------------------

fn strings(v: Option<&JsValue>) -> Vec<String> {
    match v {
        Some(JsValue::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn ledger_obj(ledger: &mut JsValue) -> &mut JsObject {
    match ledger {
        JsValue::Object(o) => o,
        _ => panic!("a split ledger is a JSON object"),
    }
}

/// `patchEmitNames`: re-point this file's slots at the shipped names; a
/// slot's key is every declared name, sorted and comma-joined.
fn patch_emit_names(ledger: &mut JsObject, file: &str, by_from: &HashMap<String, String>) {
    let order = strings(ledger.get("order"));
    let Some(JsValue::Array(emit_names)) = ledger.get_mut("emitNames") else {
        return;
    };
    for (slot, entry) in emit_names.iter_mut().enumerate() {
        if order.get(slot).map(String::as_str) != Some(file) {
            continue;
        }
        let JsValue::String(current) = entry else {
            continue;
        };
        let mut changed = false;
        let mut parts: Vec<String> = current
            .split(',')
            .map(|name| match by_from.get(name) {
                Some(next) => {
                    changed = true;
                    next.clone()
                }
                None => name.to_string(),
            })
            .collect();
        if changed {
            parts.sort_by(|a, b| cmp_utf16(a, b));
            *current = parts.join(",");
        }
    }
}

/// `patchNameHomes`: move each renamed name's home entries, all removals
/// first, counts preserved.
fn patch_name_homes(ledger: &mut JsObject, file: &str, applicable: &[&PostSplitRename]) {
    let Some(JsValue::Object(homes)) = ledger.get_mut("nameToFiles") else {
        return;
    };
    let mut to_add: Vec<(String, usize)> = Vec::new();
    for rename in applicable {
        let list = strings(homes.get(&rename.from_name));
        if homes.get(&rename.from_name).is_none() {
            continue;
        }
        let kept: Vec<String> = list.iter().filter(|f| *f != file).cloned().collect();
        let removed = list.len() - kept.len();
        if removed == 0 {
            continue;
        }
        if kept.is_empty() {
            homes.remove(&rename.from_name);
        } else {
            homes.insert(
                rename.from_name.clone(),
                JsValue::Array(kept.into_iter().map(JsValue::String).collect()),
            );
        }
        match to_add.iter_mut().find(|(n, _)| *n == rename.to_name) {
            Some((_, c)) => *c += removed,
            None => to_add.push((rename.to_name.clone(), removed)),
        }
    }
    for (to_name, count) in to_add {
        let mut list = strings(homes.get(&to_name));
        list.extend(std::iter::repeat_n(file.to_string(), count));
        homes.insert(
            to_name,
            JsValue::Array(list.into_iter().map(JsValue::String).collect()),
        );
    }
}

/// `patchLedger`: only TOP-LEVEL renames describe declared statements.
fn patch_ledger(ledger: &mut JsObject, file: &str, renames: &[PostSplitRename]) {
    let applicable: Vec<&PostSplitRename> = renames.iter().filter(|r| r.top_level).collect();
    if applicable.is_empty() {
        return;
    }
    let by_from: HashMap<String, String> = applicable
        .iter()
        .map(|r| (r.from_name.clone(), r.to_name.clone()))
        .collect();
    patch_emit_names(ledger, file, &by_from);
    patch_name_homes(ledger, file, &applicable);
}

/// `countStaleLedgerEntries` (a stat: must be 0).
fn count_stale(ledger: &JsObject, file: &str, renames: &[PostSplitRename]) -> usize {
    let top: Vec<&PostSplitRename> = renames.iter().filter(|r| r.top_level).collect();
    let targets: HashSet<&str> = top.iter().map(|r| r.to_name.as_str()).collect();
    let order = strings(ledger.get("order"));
    let emit_names: Vec<Option<String>> = match ledger.get("emitNames") {
        Some(JsValue::Array(items)) => items
            .iter()
            .map(|i| i.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    };
    let homes = match ledger.get("nameToFiles") {
        Some(JsValue::Object(o)) => Some(o),
        _ => None,
    };
    let mut stale = 0;
    for r in top
        .iter()
        .filter(|r| !targets.contains(r.from_name.as_str()))
    {
        if homes
            .and_then(|h| h.get(&r.from_name))
            .is_some_and(|v| strings(Some(v)).iter().any(|f| f == file))
        {
            stale += 1;
        }
        for (slot, entry) in emit_names.iter().enumerate() {
            if order.get(slot).map(String::as_str) == Some(file)
                && entry
                    .as_deref()
                    .is_some_and(|e| e.split(',').any(|n| n == r.from_name))
            {
                stale += 1;
            }
        }
    }
    stale
}

// ---------------------------------------------------------------------------
// One file
// ---------------------------------------------------------------------------

/// `identifierTokenAt(line, col)`: the ECMAScript IdentifierName standing
/// at UTF-16 column `col` (ID_Start/`$`/`_`, then ID_Continue/`$`/ZWNJ/ZWJ).
/// Not ASCII-only — that read mangled `café` into `caféé` (finding #28).
pub(crate) fn ident_at(line: &str, col: usize) -> Option<String> {
    let units: Vec<u16> = line.encode_utf16().collect();
    let rest = String::from_utf16_lossy(units.get(col..)?);
    let mut chars = rest.chars();
    let first = chars.next().filter(|&c| is_identifier_start(c))?;
    let mut token = String::from(first);
    token.extend(chars.take_while(|&c| is_identifier_part(c)));
    Some(token)
}

/// `renameSubstitutionText(path, newName)`.
pub(crate) fn substitution_text(shorthand_key: Option<&str>, new_name: &str) -> String {
    match shorthand_key {
        Some(key) => format!("{key}: {new_name}"),
        None => new_name.to_string(),
    }
}

/// Every occurrence span of a renamed binding → its final name (the
/// `fastRenameBinding` set: declaration, identifier references, write
/// targets), keyed by span start.
fn renamed_occurrences(state: &RenameState, source: &str) -> HashMap<u32, String> {
    let view = state.view();
    let mut out = HashMap::new();
    let mut seen = HashSet::new();
    for applied in state.applied_renames() {
        if !seen.insert(applied.binding) {
            continue;
        }
        let b = view.binding(applied.binding);
        let final_name = state.name_of(applied.binding).to_string();
        let original = &source[b.id_span.start as usize..b.id_span.end as usize];
        if original == final_name {
            continue;
        }
        let spans = std::iter::once(b.id_span)
            .chain(
                b.refs
                    .iter()
                    .filter(|r| r.ty == SiteType::Identifier)
                    .map(|r| r.span),
            )
            .chain(b.violation_targets.iter().flatten().copied());
        for span in spans {
            out.insert(span.start, final_name.clone());
        }
    }
    out
}

/// `collectSubstitutions`: every Identifier node whose current name is not
/// the token standing at its `loc` in the text.
fn collect_substitutions(
    semantic: &oxc_semantic::Semantic<'_>,
    sites: &[IdentSite<'_>],
    lines: &BabelLines<'_>,
    renamed: &HashMap<u32, String>,
    text_lines: &[&str],
) -> Vec<Substitution> {
    let mut subs = Vec::new();
    for site in sites {
        let (line, col) = lines.loc(site.start);
        let Some(text) = text_lines.get(line - 1) else {
            continue;
        };
        let current = match renamed.get(&site.start) {
            Some(name) if site.resolvable() => name.as_str(),
            _ => site.name,
        };
        let Some(token) = ident_at(text, col) else {
            continue;
        };
        if token == current {
            continue;
        }
        subs.push(Substitution {
            line,
            col,
            from: token,
            to: substitution_text(shorthand_key(semantic, site).as_deref(), current),
        });
    }
    subs
}

/// Names a binding pattern declares (`getBindingIdentifiers` over
/// `id`/`params`/`left`/`elements`/`properties`/`value`/`argument`).
fn pattern_names(p: &BindingPattern<'_>, out: &mut HashSet<String>) {
    match p {
        BindingPattern::BindingIdentifier(id) => {
            out.insert(id.name.to_string());
        }
        BindingPattern::AssignmentPattern(ap) => pattern_names(&ap.left, out),
        BindingPattern::ObjectPattern(op) => {
            for prop in &op.properties {
                pattern_names(&prop.value, out);
            }
            if let Some(rest) = &op.rest {
                pattern_names(&rest.argument, out);
            }
        }
        BindingPattern::ArrayPattern(ap) => {
            for el in ap.elements.iter().flatten() {
                pattern_names(el, out);
            }
            if let Some(rest) = &ap.rest {
                pattern_names(&rest.argument, out);
            }
        }
    }
}

/// `topLevelNames(ast)`: `t.getBindingIdentifiers` of each top-level
/// statement — a function declaration's PARAMS included (Babel's keys
/// table walks `params`), and a label.
fn top_level_names(program: &Program<'_>) -> HashSet<String> {
    let mut out = HashSet::new();
    for stmt in &program.body {
        match stmt {
            Statement::VariableDeclaration(d) => {
                for decl in &d.declarations {
                    pattern_names(&decl.id, &mut out);
                }
            }
            Statement::FunctionDeclaration(f) => {
                if let Some(id) = &f.id {
                    out.insert(id.name.to_string());
                }
                for p in &f.params.items {
                    pattern_names(&p.pattern, &mut out);
                }
                if let Some(rest) = &f.params.rest {
                    pattern_names(&rest.rest.argument, &mut out);
                }
            }
            Statement::ClassDeclaration(c) => {
                if let Some(id) = &c.id {
                    out.insert(id.name.to_string());
                }
            }
            Statement::LabeledStatement(l) => {
                out.insert(l.label.name.to_string());
            }
            Statement::ForInStatement(f) => left_names(&f.left, &mut out),
            Statement::ForOfStatement(f) => left_names(&f.left, &mut out),
            _ => {}
        }
    }
    out
}

fn left_names(left: &ForStatementLeft<'_>, out: &mut HashSet<String>) {
    match left {
        ForStatementLeft::VariableDeclaration(d) => {
            for decl in &d.declarations {
                pattern_names(&decl.id, out);
            }
        }
        other => target_names(other.to_assignment_target(), out),
    }
}

fn target_names(t: &oxc_ast::ast::AssignmentTarget<'_>, out: &mut HashSet<String>) {
    use oxc_ast::ast::{AssignmentTarget, AssignmentTargetMaybeDefault, AssignmentTargetProperty};
    let maybe = |m: &AssignmentTargetMaybeDefault<'_>, out: &mut HashSet<String>| match m {
        AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(d) => {
            target_names(&d.binding, out)
        }
        other => target_names(other.to_assignment_target(), out),
    };
    match t {
        AssignmentTarget::AssignmentTargetIdentifier(id) => {
            out.insert(id.name.to_string());
        }
        AssignmentTarget::ArrayAssignmentTarget(a) => {
            for el in a.elements.iter().flatten() {
                maybe(el, out);
            }
            if let Some(rest) = &a.rest {
                target_names(&rest.target, out);
            }
        }
        AssignmentTarget::ObjectAssignmentTarget(o) => {
            for prop in &o.properties {
                match prop {
                    AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(p) => {
                        out.insert(p.binding.name.to_string());
                    }
                    AssignmentTargetProperty::AssignmentTargetPropertyProperty(p) => {
                        maybe(&p.binding, out)
                    }
                }
            }
            if let Some(rest) = &o.rest {
                target_names(&rest.target, out);
            }
        }
        _ => {}
    }
}

/// One binding declaration: (top-level statement index, name, start, line).
struct Decl {
    stmt: usize,
    name: String,
    start: u32,
    line: usize,
}

/// The statement index holding `pos` (spans inclusive at both ends, as
/// `statementOf`'s binary search).
pub(crate) fn statement_of(spans: &[(u32, u32)], pos: u32) -> Option<usize> {
    let (mut lo, mut hi) = (0i64, spans.len() as i64 - 1);
    while lo <= hi {
        let mid = ((lo + hi) >> 1) as usize;
        if pos < spans[mid].0 {
            hi = mid as i64 - 1;
        } else if pos > spans[mid].1 {
            lo = mid as i64 + 1;
        } else {
            return Some(mid);
        }
    }
    None
}

/// `declarationIndex(ast)`: every binding declaration, scopes in
/// traversal order, each scope's bindings in `Object.keys` order.
fn declaration_index(
    view: &BabelScopes,
    program: &Program<'_>,
    lines: &BabelLines<'_>,
) -> Vec<Decl> {
    let spans: Vec<(u32, u32)> = program
        .body
        .iter()
        .map(|s| (s.span().start, s.span().end))
        .collect();
    let mut out = Vec::new();
    for (si, map) in view.initial_maps.iter().enumerate() {
        for (name, binding) in map {
            let b = view.binding(*binding);
            if b.owner.0 as usize != si {
                continue;
            }
            let start = b.id_span.start;
            if let Some(stmt) = statement_of(&spans, start) {
                out.push(Decl {
                    stmt,
                    name: name.clone(),
                    start,
                    line: lines.line(start),
                });
            }
        }
    }
    out
}

/// `locateRenames`: each rename's (bodyOrdinal, nameOrdinal) against the
/// file's ledger statements (the LAST `ledger_statements` of the body).
fn locate_renames(
    view: &BabelScopes,
    program: &Program<'_>,
    lines: &BabelLines<'_>,
    renames: &mut [PostSplitRename],
    decl_lines: &[usize],
    ledger_statements: usize,
) {
    let Some(header) = program.body.len().checked_sub(ledger_statements) else {
        return;
    };
    let decls = declaration_index(view, program, lines);
    for (i, rename) in renames.iter_mut().enumerate() {
        let Some(me) = decls
            .iter()
            .position(|d| d.name == rename.to_name && d.line == decl_lines[i] && d.stmt >= header)
        else {
            continue;
        };
        let mut siblings: Vec<usize> = (0..decls.len())
            .filter(|&k| {
                decls[k].stmt == decls[me].stmt && (decls[k].name == rename.from_name || k == me)
            })
            .collect();
        siblings.sort_by_key(|&k| decls[k].start);
        if let Some(name_ordinal) = siblings.iter().position(|&k| k == me) {
            rename.locator = Some((decls[me].stmt - header, name_ordinal));
        }
    }
}

/// What one file's reconciliation did (`FileOutcome`).
#[derive(Default)]
struct FileOutcome {
    text: Option<String>,
    renames: Vec<PostSplitRename>,
    corpus_gated: bool,
    discarded: bool,
}

fn discarded() -> FileOutcome {
    FileOutcome {
        discarded: true,
        ..FileOutcome::default()
    }
}

/// `reconcileOneFile`.
fn reconcile_one_file(
    file: &str,
    fresh: &str,
    prior: &str,
    eligible: &Eligibility,
    ledger_statements: usize,
    sink: &mut PostSplitResult,
) -> Result<FileOutcome, String> {
    let diff_text = compute_normal_diff(prior, fresh)?;
    if diff_text.is_empty() {
        return Ok(FileOutcome::default());
    }
    let allocator = Allocator::default();
    let ingest = parse_or_err(&allocator, fresh)?;
    let baseline = file_signature(fresh).ok_or("the fresh text does not parse")?;
    let lines = BabelLines::new(fresh);
    let mut state = RenameState::with_trail(
        ingest.semantic(),
        Anchor::Generated,
        crate::trail::StrategyTrail::enabled(),
    );
    let prior_names = collect_word_tokens(prior);
    let opts = ReconcileOptions {
        apply: true,
        descriptive_tier: true,
        max_hunk_lines: 10,
        mixed_hunk_tier: true,
        prior_line_count: Some(prior.split('\n').count()),
        consumer_tier: true,
        prior_names: Some(prior_names),
        last_resort_tier: true,
        skip_import_declarations: true,
        skeleton_vote_tier: true,
        plant: None,
    };
    let result = reconcile_diff_noise(ingest.semantic(), &mut state, &diff_text, eligible, &opts);
    // Recorded as the pass ran — whatever the file's fate below.
    let rows = state.trail().entries().to_vec();
    if !rows.is_empty() {
        sink.trail.push((fresh.to_string(), rows));
    }
    crate::naming::driver::add_claims(&mut sink.claims, &state.claim_stats());
    if result.prior_too_dissimilar {
        return Ok(FileOutcome {
            corpus_gated: true,
            ..FileOutcome::default()
        });
    }
    if result.renames.is_empty() {
        return Ok(FileOutcome::default());
    }
    let renamed = renamed_occurrences(&state, fresh);
    let text_lines: Vec<&str> = fresh.split('\n').collect();
    let sites = identifier_sites(ingest.semantic(), &state);
    let subs = collect_substitutions(ingest.semantic(), &sites, &lines, &renamed, &text_lines);
    let owned: Vec<String> = text_lines.iter().map(|l| l.to_string()).collect();
    let rewritten = apply_substitutions(&owned, &subs);
    // The saving is only real if the rewritten TEXT is the same program.
    if file_signature(&rewritten).as_deref() != Some(baseline.as_str()) {
        return Ok(discarded());
    }
    let re_alloc = Allocator::default();
    let reparsed = parse_or_err(&re_alloc, &rewritten)?;
    let declared = top_level_names(reparsed.program);
    let mut renames: Vec<PostSplitRename> = result
        .renames
        .iter()
        .map(|r| PostSplitRename {
            file: file.to_string(),
            from_name: r.from_name.clone(),
            to_name: r.to_name.clone(),
            kind: r.kind.as_str(),
            votes: r.votes,
            top_level: declared.contains(&r.to_name),
            locator: None,
        })
        .collect();
    let decl_lines: Vec<usize> = result.renames.iter().map(|r| r.decl_line).collect();
    let re_lines = BabelLines::new(&rewritten);
    let re_view = BabelScopes::build(reparsed.semantic());
    locate_renames(
        &re_view,
        reparsed.program,
        &re_lines,
        &mut renames,
        &decl_lines,
        ledger_statements,
    );
    Ok(FileOutcome {
        text: Some(rewritten),
        renames,
        corpus_gated: false,
        discarded: false,
    })
}

/// What the pass is given (`PostSplitReconcileInput`).
pub struct PostSplitInput<'i> {
    /// The split ledger (patched in place).
    pub ledger: &'i mut JsValue,
    pub read_fresh: &'i dyn Fn(&str) -> Option<String>,
    pub read_prior: &'i dyn Fn(&str) -> Option<String>,
    pub eligible: &'i Eligibility,
    /// `--disable post-split-reconcile`.
    pub disabled: bool,
}

/// `postSplitReconcile(input)`.
pub fn post_split_reconcile(input: PostSplitInput<'_>) -> PostSplitResult {
    let mut result = PostSplitResult::default();
    if input.disabled {
        return result;
    }
    let ledger = ledger_obj(input.ledger);
    let mut ledger_statements: HashMap<String, usize> = HashMap::new();
    for f in strings(ledger.get("order")) {
        *ledger_statements.entry(f).or_insert(0) += 1;
    }
    for file in strings(ledger.get("files")) {
        let (Some(fresh), Some(prior)) = ((input.read_fresh)(&file), (input.read_prior)(&file))
        else {
            continue;
        };
        result.stats.considered += 1;
        let statements = ledger_statements.get(&file).copied().unwrap_or(0);
        // "An optional pass must never lose a completed run": any error is
        // a discard.
        let outcome = reconcile_one_file(
            &file,
            &fresh,
            &prior,
            input.eligible,
            statements,
            &mut result,
        )
        .unwrap_or_else(|_| discarded());
        if outcome.corpus_gated {
            result.stats.corpus_gated += 1;
        }
        if outcome.discarded {
            result.stats.discarded += 1;
        }
        let Some(text) = outcome.text else {
            continue;
        };
        result.changed.push((file.clone(), text));
        result.stats.changed += 1;
        patch_ledger(ledger, &file, &outcome.renames);
        result.stats.incoherent += count_stale(ledger, &file, &outcome.renames);
        result.renames.extend(outcome.renames);
    }
    result
}

#[cfg(test)]
mod reconcile_test;
