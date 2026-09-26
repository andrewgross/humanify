//! The LLM coverage sweep over the minted survivors — TS:
//! `src/rename/coverage-sweep.ts` (targeting, grouping, requests, apply)
//! and `src/rename/sweep-step.ts` (the deferred, prior-aware sweep over the
//! reconciled — else generated — output).
//!
//! Targeting is STRICTER than the census (`is_sweep_target`: short, no
//! embedded word, not CONSTANT_CASE — or a camel half-mint), carried
//! identities are exempt, and eval/with-frozen bindings are never swept.
//! Targets group by the node whose code frames them (their own function /
//! class, else the enclosing function, else the declaring statement); one
//! request per group, every prompt pre-built; responses are applied in
//! group-build order, so completion order never decides a conflict.

use std::collections::HashMap;

use humanify_model::llm::{
    BatchRenameRequest, CacheKeyParams, LlmCall, LlmErrorKind, NameProvider, Renames, cache_key_of,
};
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use super::census::{MintedBinding, collect_minted_bindings};
use crate::ingest::Ingest;
use crate::modules::soundness::{EvalWithTaint, collect_eval_with_taint};
use crate::naming::code_window::MAX_CODE_LINES;
use crate::naming::prompts::{render_system_prompt, render_user_prompt};
use crate::naming::waves::generate::TextView;
use crate::naming::waves::render::{Occurrences, program_edits, render_program};
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{is_bun_token, is_half_mint_head, is_wordless_mint_shape};
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Anchor, Attempt, Outcome, StrategyTrail, Tier};

/// Longest a minted survivor is after stripping trailing `_`/`$`.
const MAX_SWEEP_LENGTH: usize = 4;

/// `isSweepTarget`: a genuine minified survivor worth force-naming.
pub fn is_sweep_target(name: &str) -> bool {
    if !is_bun_token(name) {
        return false;
    }
    if !name.is_empty() && name.bytes().all(|b| b == b'_' || b == b'$') {
        return false;
    }
    if is_half_mint_head(name) {
        return true;
    }
    if !is_wordless_mint_shape(name) {
        return false;
    }
    // `name.replace(/[_$]+$/, "").length` — UTF-16 units.
    name.trim_end_matches(['_', '$']).encode_utf16().count() <= MAX_SWEEP_LENGTH
}

/// `collectSweepTargets`.
pub fn collect_sweep_targets(
    semantic: &Semantic<'_>,
    state: &RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
) -> Vec<MintedBinding> {
    collect_minted_bindings(semantic, state, eligible)
        .entries
        .into_iter()
        .filter(|e| {
            is_sweep_target(&e.name)
                && !state.is_eval_taint_frozen(e.binding, taint)
                && !state.is_carried(e.binding)
        })
        .collect()
}

/// A statement kind — Babel's `Statement` alias over oxc node kinds.
pub fn is_statement(kind: &AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::BlockStatement(_)
            | AstKind::BreakStatement(_)
            | AstKind::ContinueStatement(_)
            | AstKind::DebuggerStatement(_)
            | AstKind::DoWhileStatement(_)
            | AstKind::EmptyStatement(_)
            | AstKind::ExpressionStatement(_)
            | AstKind::ForInStatement(_)
            | AstKind::ForOfStatement(_)
            | AstKind::ForStatement(_)
            | AstKind::IfStatement(_)
            | AstKind::LabeledStatement(_)
            | AstKind::ReturnStatement(_)
            | AstKind::SwitchStatement(_)
            | AstKind::ThrowStatement(_)
            | AstKind::TryStatement(_)
            | AstKind::WhileStatement(_)
            | AstKind::WithStatement(_)
            | AstKind::VariableDeclaration(_)
            | AstKind::ImportDeclaration(_)
            | AstKind::ExportAllDeclaration(_)
            | AstKind::ExportDefaultDeclaration(_)
            | AstKind::ExportDeclaration(_)
            | AstKind::ExportFromDeclaration(_)
            | AstKind::ExportNamedDeclaration(_)
    ) || matches!(kind, AstKind::Function(f) if f.is_declaration())
        || matches!(kind, AstKind::Class(c) if c.is_declaration())
}

/// A statement-list container (Babel's `Array.isArray(path.container)` for
/// a statement): program / block / function bodies, switch cases, static
/// blocks.
fn is_statement_list(kind: &AstKind<'_>) -> bool {
    matches!(
        kind,
        AstKind::Program(_)
            | AstKind::BlockStatement(_)
            | AstKind::FunctionBody(_)
            | AstKind::SwitchCase(_)
            | AstKind::StaticBlock(_)
    )
}

/// `path.getStatementParent()`: the nearest ancestor-or-self statement that
/// sits in a statement list.
pub fn statement_parent(semantic: &Semantic<'_>, node: NodeId) -> Option<NodeId> {
    let nodes = semantic.nodes();
    let mut cur = node;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return None;
        }
        if is_statement(&nodes.kind(cur)) && is_statement_list(&nodes.kind(parent)) {
            return Some(cur);
        }
        cur = parent;
    }
}

/// `groupKeyNode`'s span: the binding's own function/class, else its
/// scope's function parent's block, else its declaring statement, else the
/// program.
fn group_key_span(semantic: &Semantic<'_>, state: &RenameState, binding: BindingId) -> Span {
    let view = state.view();
    let b = view.binding(binding);
    let nodes = semantic.nodes();
    let kind = nodes.kind(b.path_node);
    if matches!(kind, AstKind::Function(_) | AstKind::Class(_)) {
        return kind.span();
    }
    if let Some(fp) = view.function_parent(b.owner) {
        return view.scope(fp).span;
    }
    match statement_parent(semantic, b.path_node) {
        Some(s) => nodes.get_node(s).span(),
        None => view.scope(view.program_scope()).span,
    }
}

/// `capCode`.
fn cap_code(code: String) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    if lines.len() <= MAX_CODE_LINES {
        return code;
    }
    lines[..MAX_CODE_LINES].join("\n")
}

/// `Object.keys(scope.getAllBindings())`.
fn all_binding_names(state: &RenameState, from: BScopeId) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut cur = Some(from);
    while let Some(s) = cur {
        for (name, _) in state.bindings_in(s) {
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
        cur = state.view().scope(s).parent;
    }
    out
}

/// One sweep group (`SweepGroup`).
struct SweepGroup {
    code: String,
    targets: Vec<MintedBinding>,
    used_names: Vec<String>,
}

fn build_groups(
    semantic: &Semantic<'_>,
    state: &RenameState,
    targets: Vec<MintedBinding>,
) -> Vec<SweepGroup> {
    let mut order: Vec<Span> = Vec::new();
    let mut by_key: HashMap<(u32, u32), Vec<MintedBinding>> = HashMap::new();
    for t in targets {
        let key = group_key_span(semantic, state, t.binding);
        by_key
            .entry((key.start, key.end))
            .or_insert_with(|| {
                order.push(key);
                Vec::new()
            })
            .push(t);
    }
    let view = TextView::build(semantic);
    let occ = Occurrences::build(semantic, state);
    order
        .into_iter()
        .map(|span| {
            let bucket = by_key
                .remove(&(span.start, span.end))
                .expect("every key has its bucket");
            let scope = state.scope_of_binding(bucket[0].binding);
            let code = view.pretty(span, &occ.edits(view.text, state, span), true);
            SweepGroup {
                code: cap_code(code),
                used_names: all_binding_names(state, scope),
                targets: bucket,
            }
        })
        .collect()
}

/// One recorded sweep dispatch (the dump's `site: "sweep"` prompt row).
#[derive(Clone, Debug)]
pub struct SweepDispatch {
    pub request: BatchRenameRequest,
    pub system_prompt: String,
    pub user_prompt: String,
    pub cache_key: String,
    /// (target name, declaration span) in the anchored text.
    pub targets: Vec<(String, Span)>,
}

/// `SweepResult` + the dispatch record.
#[derive(Clone, Debug, Default)]
pub struct SweepResult {
    pub named: usize,
    pub skipped: usize,
    pub groups: usize,
    pub dispatches: Vec<SweepDispatch>,
    pub misses: usize,
    pub errors: usize,
}

/// Apply one group's suggestions (`applyGroupResponse`). `renames[name]` is
/// the OWN entry (`Renames::get`): a JS object read would fall through to
/// Object.prototype, unreachable here — no sweep target is shaped like an
/// Object.prototype key (every one has a lowercase word run).
fn apply_group_response(
    state: &mut RenameState,
    group: &SweepGroup,
    renames: &Renames,
) -> (usize, usize) {
    let (mut named, mut skipped) = (0, 0);
    for target in &group.targets {
        let suggestion = renames.get(&target.name).filter(|s| !s.is_empty());
        let Some(new_name) = suggestion.filter(|s| *s != target.name && !is_bun_token(s)) else {
            skipped += 1;
            let reason = match suggestion {
                Some(s) if s != target.name => "still-below-floor",
                _ => "llm-declined",
            };
            let row = Attempt::new(Tier::CoverageSweep, Outcome::Abstained).reason(reason);
            state.record(target.binding, &target.name, row, true);
            continue;
        };
        let new_name = new_name.to_string();
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(target.binding),
                old_name: &target.name,
                new_name: &new_name,
                expected: None,
            },
            TrailSpec::CallerRecords {
                tier: Tier::CoverageSweep,
            },
        );
        if attempt.applied {
            named += 1;
            let row = Attempt::new(Tier::CoverageSweep, Outcome::Applied).proposed(new_name);
            state.record(target.binding, &target.name, row, true);
        } else {
            skipped += 1;
            let mut row = Attempt::new(Tier::CoverageSweep, Outcome::Rejected).proposed(new_name);
            if let Some(r) = attempt.reason {
                row = row.reason(r.as_str());
            }
            state.record(target.binding, &target.name, row, true);
        }
    }
    (named, skipped)
}

/// `sweepMintedNames`: force-name the minted survivors the deterministic
/// floor left, one request per group, applied in group-build order.
pub fn sweep_minted_names<P: NameProvider>(
    semantic: &Semantic<'_>,
    state: &mut RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
    provider: &P,
    params: &CacheKeyParams,
) -> SweepResult {
    let targets = collect_sweep_targets(semantic, state, eligible, taint);
    if targets.is_empty() {
        return SweepResult::default();
    }
    let groups = build_groups(semantic, state, targets);
    let mut result = SweepResult {
        groups: groups.len(),
        ..SweepResult::default()
    };
    let mut calls = Vec::with_capacity(groups.len());
    for g in &groups {
        let request = BatchRenameRequest {
            code: g.code.clone(),
            identifiers: g.targets.iter().map(|t| t.name.clone()).collect(),
            used_names: g.used_names.clone(),
            ..BatchRenameRequest::default()
        };
        let system_prompt = render_system_prompt(&request);
        let user_prompt = render_user_prompt(&request);
        result.dispatches.push(SweepDispatch {
            cache_key: cache_key_of(&request, params),
            system_prompt: system_prompt.clone(),
            user_prompt: user_prompt.clone(),
            targets: g
                .targets
                .iter()
                .map(|t| (t.name.clone(), state.view().binding(t.binding).id_span))
                .collect(),
            request: request.clone(),
        });
        calls.push(LlmCall {
            request,
            system_prompt,
            user_prompt,
        });
    }
    let responses = provider.run_wave(calls);
    for (g, response) in groups.iter().zip(responses) {
        match response {
            Ok(resp) => {
                let (named, skipped) = apply_group_response(state, g, &resp.renames);
                result.named += named;
                result.skipped += skipped;
            }
            Err(e) => {
                if e.kind == LlmErrorKind::CacheMiss {
                    result.misses += 1;
                } else {
                    result.errors += 1;
                }
                result.skipped += g.targets.len();
            }
        }
    }
    result
}

/// `DeferredSweepOutcome` + the sweep record and the continued trail.
pub struct DeferredSweepOutcome {
    pub sweep: SweepResult,
    /// The re-rendered text — set only when a rename applied.
    pub code: Option<String>,
    pub trail: StrategyTrail,
    /// The rename ledger's stage for this pass (`--rename-ledger`, only
    /// when a rename applied): its renames over the text it parsed.
    pub ledger: Option<crate::rename::validated::ledger::RenameLedger>,
}

/// `runDeferredSweep(code)`: the prior-aware sweep over its OWN parse of
/// the shipping text (`anchor` = reconciled when the reconcile produced
/// the text, else generated). Err when the text does not parse.
pub fn run_deferred_sweep<P: NameProvider>(
    code: &str,
    anchor: Anchor,
    eligible: &Eligibility,
    provider: &P,
    params: &CacheKeyParams,
    trail: StrategyTrail,
    ledger: bool,
) -> Result<DeferredSweepOutcome, (String, StrategyTrail)> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        return Err((
            format!("sweep input does not parse: {}", ingest.errors[0]),
            trail,
        ));
    }
    let semantic = ingest.semantic();
    let taint = collect_eval_with_taint(semantic);
    let mut state = RenameState::with_trail(semantic, anchor, trail);
    let sweep = sweep_minted_names(semantic, &mut state, eligible, &taint, provider, params);
    let code = (sweep.named > 0).then(|| render_program(semantic, &state));
    let ledger = (ledger && code.is_some()).then(|| {
        let rendered = program_edits(semantic, &state, &[]);
        crate::rename::validated::ledger::build_rename_ledger(
            semantic.source_text(),
            &state,
            &rendered,
        )
    });
    Ok(DeferredSweepOutcome {
        sweep,
        code,
        trail: state.finish().trail,
        ledger,
    })
}
