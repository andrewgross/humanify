//! The LLM coverage sweep over the minted survivors — TS:
//! `src/rename/coverage-sweep.ts` (targeting, grouping, requests, apply)
//! and `src/rename/sweep-step.ts` (the deferred, prior-aware sweep over the
//! reconciled — else generated — output).
//!
//! Targeting is STRICTER than the census (`is_sweep_target`: short, no
//! embedded word, not CONSTANT_CASE — or a camel half-mint; single
//! letters are targets since 2026-09-30 — Andrew: never-asked loop
//! counters must be visible to the pass whose job is never-asked names),
//! carried identities are exempt, and eval/with-frozen bindings are never
//! swept. Targets group by the node whose code frames them (their own
//! function / class, else the enclosing function, else the declaring
//! statement); one request per group, every prompt pre-built; responses
//! are applied in group-build order, so completion order never decides a
//! conflict. The ANSWER filter is `rename::floor::is_sweep_answer_acceptable`
//! — junk shapes stay refused, but a single-letter answer may land.

use std::collections::HashMap;

use humanify_model::llm::{
    BatchRenameRequest, CacheKeyParams, LlmCall, LlmErrorKind, NameProvider, Renames, cache_key_of,
};
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use super::census::{MintedBinding, collect_minted_bindings};
use crate::artifact_dump::{Dispatch, DispatchLog, RecordMode};
use crate::ingest::Ingest;
use crate::modules::soundness::{EvalWithTaint, collect_eval_with_taint};
use crate::naming::code_window::MAX_CODE_LINES;
use crate::naming::prompts::{render_system_prompt, render_user_prompt};
use crate::naming::waves::generate::TextView;
use crate::naming::waves::render::{Occurrences, program_edits, render_program};
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{
    is_bun_token, is_half_mint_head, is_sweep_answer_acceptable, is_wordless_mint_shape,
};
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Anchor, Attempt, Outcome, StrategyTrail, Tier};

/// Longest a minted survivor is after stripping trailing `_`/`$`.
const MAX_SWEEP_LENGTH: usize = 4;

/// `isSweepTarget`: a genuine minified survivor worth force-naming — the
/// WHO GETS ASKED question (see `rename::floor`'s module doc for the
/// target-vs-answer split). Single letters qualify since 2026-09-30.
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
    /// The ask-trace site (`--dump-asks`; recording only): phase 0, no
    /// prior context; a re-ask dispatch records the reask class and the
    /// rejection codes its targets were seeded by.
    pub ask: crate::naming::ask_trace::AskSite,
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
    /// Re-ask dispatch targets, summed over the bounded re-ask rounds
    /// (`naming::reask`, `--rename-retries`) — a target re-asked twice
    /// counts twice.
    pub reasked: usize,
    /// Re-asked suggestions that applied.
    pub reask_applied: usize,
    /// Re-asked suggestions rejected again once the budget was spent (or
    /// left unanswered) — the give-up half of the bounded retry.
    pub reask_dropped: usize,
}

/// One pending re-ask's seed: the LAST suggestion the model made, the
/// applier's rejection of it (class + code, for the ask site), and the
/// ACCUMULATED reject history the next prompt must disclose.
type SweepSeed = (
    String,
    crate::naming::reask::ReaskClass,
    &'static str,
    Vec<(String, &'static str)>,
);

/// One pending disclosed re-ask: the target, the LAST suggestion the
/// model made, the applier's rejection of it (`naming::reask` classifies
/// the reason; the ask trace records class + code), and the ACCUMULATED
/// history of every suggestion that already failed — each further re-ask
/// discloses all of it.
struct SweepReask {
    target: MintedBinding,
    suggestion: String,
    /// The rejection as reask.rs classified it (recorded on the re-ask's
    /// ask-trace site; nothing decides on it).
    class: crate::naming::reask::ReaskClass,
    code: &'static str,
    /// Every prior rejected suggestion, oldest first: (name, code).
    rejects: Vec<(String, &'static str)>,
}

/// The re-ask context one apply round runs under: the budget
/// (`--rename-retries`), how many re-asks the round's targets have
/// already had (0 on the sweep's first apply, k during re-ask round k),
/// and each target's accumulated reject history.
struct ReaskCtx<'c> {
    limit: usize,
    spent: usize,
    carried: &'c HashMap<String, Vec<(String, &'static str)>>,
}

/// Apply one group's suggestions (`applyGroupResponse`). `renames[name]` is
/// the OWN entry (`Renames::get`): a JS object read would fall through to
/// Object.prototype, unreachable here — no sweep target is shaped like an
/// Object.prototype key (every one has a lowercase word run).
///
/// The third return is the re-askable set: suggestions the validated
/// applier rejected for a reason a disclosed re-ask can fix (the
/// collision classes — `target-in-scope`, `target-visible`,
/// `shadows-child`, `target-free-name`) while the identifier still has
/// re-ask budget (`naming::reask::reask_again`). Before the 2026-09-28
/// fix these were counted `skipped` and dropped — the minted name kept
/// forever.
fn apply_group_response(
    state: &mut RenameState,
    group: &SweepGroup,
    renames: &Renames,
    reask: &ReaskCtx<'_>,
) -> (usize, usize, Vec<SweepReask>) {
    let (mut named, mut skipped) = (0, 0);
    let mut reasks = Vec::new();
    for target in &group.targets {
        let suggestion = renames.get(&target.name).filter(|s| !s.is_empty());
        let Some(new_name) =
            suggestion.filter(|s| *s != target.name && is_sweep_answer_acceptable(s))
        else {
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
            continue;
        }
        skipped += 1;
        let mut row =
            Attempt::new(Tier::CoverageSweep, Outcome::Rejected).proposed(new_name.clone());
        if let Some(r) = attempt.reason {
            row = row.reason(r.as_str());
        }
        state.record(target.binding, &target.name, row, true);
        if let Some(r) = attempt.reason
            && let class = crate::naming::reask::class_of(r)
            && crate::naming::reask::reask_again(reask.limit, reask.spent, class)
        {
            let mut rejects = reask.carried.get(&target.name).cloned().unwrap_or_default();
            rejects.push((new_name.clone(), r.as_str()));
            reasks.push(SweepReask {
                target: target.clone(),
                suggestion: new_name,
                class,
                code: r.as_str(),
                rejects,
            });
        }
    }
    (named, skipped, reasks)
}

/// `sweepMintedNames`: force-name the minted survivors the deterministic
/// floor left, one request per group, applied in group-build order. A
/// suggestion rejected for a collision class gets the run's disclosed
/// re-asks (default TWO, `--rename-retries`): each one names and
/// blocklists EVERY suggestion that already failed — the same retry
/// prompt shape the wave lanes use. A budget exhausted at a further
/// collision gives up, recorded.
#[allow(clippy::too_many_arguments)]
pub fn sweep_minted_names<P: NameProvider>(
    semantic: &Semantic<'_>,
    state: &mut RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
    provider: &P,
    log: &mut DispatchLog,
    anchor: crate::trail::Anchor,
    params: &CacheKeyParams,
    window: usize,
    reask_limit: usize,
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
    // One bounded window of rendered prompts at a time (finding #65) —
    // the rows are recorded (and streamed) in group-build order either way.
    let mut responses = Vec::with_capacity(groups.len());
    for chunk in groups.chunks(window.max(1)) {
        let mut calls = Vec::with_capacity(chunk.len());
        for g in chunk {
            let request = BatchRenameRequest {
                code: g.code.clone(),
                identifiers: g.targets.iter().map(|t| t.name.clone()).collect(),
                used_names: g.used_names.clone(),
                ..BatchRenameRequest::default()
            };
            calls.push(sweep_call(
                &g.targets,
                state,
                request,
                crate::naming::ask_trace::AskSite::fresh(0),
                anchor,
                log,
                &mut result,
                params,
            ));
        }
        responses.extend(provider.run_wave(calls));
    }
    let none: HashMap<String, Vec<(String, &'static str)>> = HashMap::new();
    let first = ReaskCtx {
        limit: reask_limit,
        spent: 0,
        carried: &none,
    };
    let mut reasks: Vec<SweepReask> = Vec::new();
    for (g, response) in groups.iter().zip(responses) {
        match response {
            Ok(resp) => {
                let (named, skipped, group_reasks) =
                    apply_group_response(state, g, &resp.renames, &first);
                result.named += named;
                // A re-askable rejection is PENDING, not skipped — the
                // re-ask decides, and its dropped half lands in `skipped`.
                result.skipped += skipped - group_reasks.len();
                reasks.extend(group_reasks);
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
    if !reasks.is_empty() {
        sweep_reask(
            semantic,
            state,
            reasks,
            provider,
            log,
            anchor,
            params,
            window,
            reask_limit,
            &mut result,
        );
    }
    result
}

/// One sweep dispatch: its prompts for the provider, its row to the log AT
/// COMMIT (finding #65 — no per-ask string survives the dispatch outside
/// the tests' retaining log). Returns the call.
#[allow(clippy::too_many_arguments)]
fn sweep_call(
    targets: &[MintedBinding],
    state: &RenameState,
    request: BatchRenameRequest,
    ask: crate::naming::ask_trace::AskSite,
    anchor: crate::trail::Anchor,
    log: &mut DispatchLog,
    result: &mut SweepResult,
    params: &CacheKeyParams,
) -> LlmCall {
    let system_prompt = render_system_prompt(&request);
    let user_prompt = render_user_prompt(&request);
    if log.mode() != RecordMode::Off {
        let dispatch = SweepDispatch {
            cache_key: cache_key_of(&request, params),
            system_prompt: system_prompt.clone(),
            user_prompt: user_prompt.clone(),
            targets: targets
                .iter()
                .map(|t| (t.name.clone(), state.view().binding(t.binding).id_span))
                .collect(),
            request: request.clone(),
            ask,
        };
        log.record(&Dispatch::Sweep(anchor, &dispatch));
        if log.retains() {
            result.dispatches.push(dispatch);
        }
    }
    LlmCall {
        request,
        system_prompt,
        user_prompt,
    }
}

/// The sweep's bounded re-ask rounds for the collision-rejected targets —
/// ONE round per loop iteration, until the budget (`--rename-retries`)
/// exhausts or every re-asked suggestion settles. The retry request
/// reuses the wave lanes' round-2 envelope (`is_retry` +
/// `previous_attempt` + `failures.duplicates` + the accumulated
/// `prior_rejects`), so the model sees every suggestion that failed and
/// why, plus a blocklist — the same disclosure the function waves give.
/// The re-asked groups are REBUILT over the current state each round, so
/// the code window and the used-names list show the names the previous
/// applies just landed. A further reaskable rejection once the budget is
/// spent counts as dropped — never a loop.
#[allow(clippy::too_many_arguments)]
fn sweep_reask<P: NameProvider>(
    semantic: &Semantic<'_>,
    state: &mut RenameState,
    mut pending: Vec<SweepReask>,
    provider: &P,
    log: &mut DispatchLog,
    anchor: crate::trail::Anchor,
    params: &CacheKeyParams,
    window: usize,
    limit: usize,
    result: &mut SweepResult,
) {
    // Re-asks each pending target has already had (loop iteration k is
    // re-ask round k, so its applies run at spent = k).
    let mut spent = 0usize;
    while !pending.is_empty() {
        spent += 1;
        // The round's seeds: each target's last suggestion, its latest
        // rejection (class + code, for the ask site), and the accumulated
        // reject history the next prompt must disclose.
        let seed_of: HashMap<String, SweepSeed> = pending
            .iter()
            .map(|r| {
                (
                    r.target.name.clone(),
                    (r.suggestion.clone(), r.class, r.code, r.rejects.clone()),
                )
            })
            .collect();
        let carried: HashMap<String, Vec<(String, &'static str)>> = seed_of
            .iter()
            .map(|(k, (_, _, _, rejects))| (k.clone(), rejects.clone()))
            .collect();
        let targets: Vec<MintedBinding> = pending.into_iter().map(|r| r.target).collect();
        let fresh = build_groups(semantic, state, targets);
        let mut owners = Vec::with_capacity(fresh.len());
        let mut round_responses = Vec::with_capacity(fresh.len());
        let mut round_calls = Vec::with_capacity(fresh.len());
        for g in &fresh {
            let mut prev = crate::naming::waves::jsset::JsRecord::default();
            let mut failures = humanify_model::llm::RenameFailures::default();
            let mut prior = humanify_model::llm::PriorRejects::default();
            for t in &g.targets {
                let (suggestion, _, _, rejects) = seed_of
                    .get(&t.name)
                    .expect("every re-asked target carries its suggestion");
                prev.set(&t.name, suggestion);
                failures.duplicates.push(t.name.clone());
                prior.0.push((
                    t.name.clone(),
                    rejects
                        .iter()
                        .map(|(name, code)| humanify_model::llm::PriorReject {
                            name: name.clone(),
                            invalid: *code == "invalid-target",
                        })
                        .collect(),
                ));
            }
            // The ask site: the group's targets were seeded by applier
            // rejections — the class when they all agree (the usual case:
            // one collision class), their codes in the detail. Recording
            // only.
            let seeded: Vec<(crate::naming::reask::ReaskClass, &'static str)> = g
                .targets
                .iter()
                .map(|t| {
                    let (_, class, code, _) =
                        seed_of.get(&t.name).expect("every target carries its seed");
                    (*class, *code)
                })
                .collect();
            let mut codes: Vec<&str> = seeded.iter().map(|(_, c)| *c).collect();
            codes.sort_unstable();
            codes.dedup();
            let uniform = seeded.iter().map(|(c, _)| *c).all(|c| c == seeded[0].0);
            let ask = crate::naming::ask_trace::AskSite {
                phase: 0,
                prior: false,
                cause: uniform.then_some(seeded[0].0.into()),
                detail: Some(codes.join(",")),
            };
            let request = BatchRenameRequest {
                code: g.code.clone(),
                identifiers: g.targets.iter().map(|t| t.name.clone()).collect(),
                used_names: crate::naming::waves::processor::build_retry_used_names(
                    &g.used_names,
                    &prev,
                ),
                is_retry: Some(true),
                previous_attempt: Some(humanify_model::llm::StrMap(prev.0.clone())),
                failures: Some(failures),
                prior_rejects: Some(prior),
                ..BatchRenameRequest::default()
            };
            round_calls.push(sweep_call(
                &g.targets, state, request, ask, anchor, log, result, params,
            ));
            owners.push(SweepGroup {
                code: g.code.clone(),
                used_names: g.used_names.clone(),
                targets: g.targets.clone(),
            });
            // One bounded window of rendered prompts at a time (finding #65).
            if round_calls.len() == window.max(1) {
                round_responses.extend(provider.run_wave(std::mem::take(&mut round_calls)));
            }
        }
        if !round_calls.is_empty() {
            round_responses.extend(provider.run_wave(std::mem::take(&mut round_calls)));
        }
        let responses = round_responses;
        let ctx = ReaskCtx {
            limit,
            spent,
            carried: &carried,
        };
        let mut next: Vec<SweepReask> = Vec::new();
        for (narrow, response) in owners.into_iter().zip(responses) {
            result.reasked += narrow.targets.len();
            match response {
                Ok(resp) => {
                    // Same validation. A further reaskable rejection with
                    // budget left seeds the next round (PENDING, not
                    // dropped — the main body's rule); one without counts
                    // as dropped (never a loop).
                    let (named, rejected, reasks) =
                        apply_group_response(state, &narrow, &resp.renames, &ctx);
                    let dropped = rejected - reasks.len();
                    result.reask_applied += named;
                    result.reask_dropped += dropped;
                    result.named += named;
                    result.skipped += dropped;
                    next.extend(reasks);
                }
                Err(e) => {
                    if e.kind == LlmErrorKind::CacheMiss {
                        result.misses += 1;
                    } else {
                        result.errors += 1;
                    }
                    result.reask_dropped += narrow.targets.len();
                }
            }
        }
        pending = next;
    }
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
/// `reask_limit` is the run's name-conflict re-ask budget
/// (`--rename-retries`).
#[allow(clippy::too_many_arguments)]
pub fn run_deferred_sweep<P: NameProvider>(
    code: &str,
    anchor: Anchor,
    eligible: &Eligibility,
    provider: &P,
    log: &mut DispatchLog,
    params: &CacheKeyParams,
    prompt_window: usize,
    trail: StrategyTrail,
    ledger: bool,
    reask_limit: usize,
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
    let ph = crate::profiling::phase("sweep:names");
    let taint = collect_eval_with_taint(semantic);
    let mut state = RenameState::with_trail(semantic, anchor, trail);
    let sweep = sweep_minted_names(
        semantic,
        &mut state,
        eligible,
        &taint,
        provider,
        log,
        anchor,
        params,
        prompt_window,
        reask_limit,
    );
    drop(ph);
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
