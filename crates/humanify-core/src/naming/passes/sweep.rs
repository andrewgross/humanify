//! The LLM coverage sweep — TS: `src/rename/coverage-sweep.ts` (targeting,
//! grouping, requests, apply) and `src/rename/sweep-step.ts` (the deferred,
//! prior-aware sweep over the reconciled — else generated — output).
//!
//! TARGETING IS A LEDGER LOOKUP (2026-09-30, Andrew's provenance decision:
//! "move from finding things that look like packed names to just looking
//! up if we already renamed that item"). A binding is a target when it is
//! eligible, not a convention carve-out (`rename::floor::
//! is_convention_carveout`), not eval/with-frozen, NOT renamed this run
//! (applied or carried) and has no recorded LLM-ask decision
//! (`rename::validated`'s decision ledger). There is NO minted-shape gate:
//! a never-asked descriptive name is a target too — missing things that
//! LOOK correct was the point. A binding whose retry budget EXHAUSTED
//! while still unrenamed stays a target (it is exactly the thing that is
//! not properly renamed); a model-kept or renamed one is never re-targeted
//! (the p2sBytes-class re-roll ends).
//!
//! The DEFERRED sweep parses a new text (reconciled — else generated), so
//! per-binding identity from the naming era does not reach it: spans do
//! not survive the generate/reconcile boundary. Its ledger consults
//! [`DecidedNames`] — the run's decisions joined BY NAME, the one key
//! that does survive. That join is APPROXIMATE by construction (a decided
//! record on ANY same-named binding classifies every same-named survivor
//! as decided): the in-era sweep is exact per binding, the deferred half
//! is the declared approximation, and the same split runs through the
//! coverage meter (`naming::report::coverage`).
//!
//! Targets group by the node whose code frames them (their own
//! function / class, else the enclosing function, else the declaring
//! statement); one request per group, every prompt pre-built; responses
//! are applied in group-build order, so completion order never decides a
//! conflict. The ANSWER filter is `rename::floor::is_sweep_answer_acceptable`
//! — junk shapes stay refused, but a single-letter answer may land — and,
//! FIRST, the answer-quality question the wave barrier asks too
//! (`naming::waves::processor::answer_refusal`): an answer that borrows a
//! minified name as a word (`rename::floor::borrowed_minified_stem`,
//! 2026-10-03), or hands a multi-letter minified target back as itself
//! (`rename::floor::is_minified_echo`, round 2), gets a disclosed re-ask
//! and is EXHAUSTED when the budget dies — never applied, never a keep.

use std::collections::{HashMap, HashSet};

use humanify_model::llm::{
    BatchRenameRequest, CacheKeyParams, LlmCall, LlmErrorKind, NameProvider, Renames, cache_key_of,
};
use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_semantic::{NodeId, Semantic};
use oxc_span::{GetSpan, Span};

use super::census::{MintedBinding, collect_eligible_bindings};
use crate::artifact_dump::{Dispatch, DispatchLog, RecordMode};
use crate::ingest::Ingest;
use crate::modules::soundness::{EvalWithTaint, collect_eval_with_taint};
use crate::naming::code_window::MAX_CODE_LINES;
use crate::naming::prompts::{render_system_prompt, render_user_prompt};
use crate::naming::waves::generate::TextView;
use crate::naming::waves::processor::{answer_refusal, disclose_reject};
use crate::naming::waves::render::{Occurrences, program_edits, render_program};
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{MinifiedStems, is_convention_carveout, is_sweep_answer_acceptable};
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Anchor, Attempt, Outcome, StrategyTrail, Tier};

/// The run's rename decisions, keyed by NAME — the join the DEFERRED
/// sweep consults because per-binding identity does not cross the
/// generate/reconcile text boundary (spans are anchored per text; only
/// the name string survives). The declared approximation: a record on ANY
/// same-named binding excludes every same-named candidate, so a
/// never-asked survivor sharing its name with an asked one is missed
/// HERE (never in the in-era sweep, which is per-binding exact).
#[derive(Clone, Debug, Default)]
pub struct DecidedNames {
    /// Names some tier APPLIED this run (the trail's `final_name`s and
    /// the reports' `Renamed{newName}`s) — the bindings wearing them in
    /// the later text are the renamed ones.
    pub renamed_to: HashSet<String>,
    /// Names an LLM ask settled without a rename (declined / same-name /
    /// junk) — the trail's ask-lane rows' `old_name`s plus every report
    /// outcome's ask-time name.
    pub asked: HashSet<String>,
    /// Names whose retry budget died still-unrenamed — they STAY targets.
    pub exhausted: HashSet<String>,
}

impl DecidedNames {
    /// Build the join from the run's records. Only LLM-ask rows count as
    /// asks (`Tier::Llm` / `Tier::CoverageSweep`): deterministic lanes
    /// (floor, transfer, reconcile) leave their leftovers for the sweep.
    pub fn of(
        trail: &StrategyTrail,
        reports: &[crate::naming::report::RenameReport],
        exhausted: &[String],
    ) -> DecidedNames {
        let mut d = DecidedNames {
            exhausted: exhausted.iter().cloned().collect(),
            ..DecidedNames::default()
        };
        for entry in trail.entries() {
            // A prompt-guard refusal (`naming::shown`) is not an ask: the
            // model never saw the identifier.
            let asked = entry.attempts.iter().any(|a| {
                a.outcome != Outcome::Vote
                    && matches!(a.tier, Tier::Llm | Tier::CoverageSweep)
                    && a.reason.as_deref() != Some(crate::naming::shown::NOT_SHOWN)
            });
            if asked {
                d.asked.insert(entry.old_name.clone());
            }
            if let Some(final_name) = entry.final_name.as_ref() {
                d.renamed_to.insert(final_name.clone());
            }
        }
        for report in reports {
            for (name, outcome) in report.outcomes.iter() {
                d.asked.insert(name.clone());
                if let crate::naming::report::Status::Renamed { new_name, .. } = &outcome.status {
                    d.renamed_to.insert(new_name.clone());
                }
            }
        }
        d
    }
}

/// `collectSweepTargets` — the ledger lookup, per binding. `decided` is
/// the cross-text name join; `None` means this state holds the records
/// itself (the in-era sweep — exact per binding).
pub fn collect_sweep_targets(
    semantic: &Semantic<'_>,
    state: &RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
    decided: Option<&DecidedNames>,
) -> Vec<MintedBinding> {
    collect_eligible_bindings(semantic, state, eligible)
        .entries
        .into_iter()
        .filter(|e| is_sweep_candidate(state, taint, decided, e))
        .collect()
}

/// The WHO GETS ASKED question, 2026-09-30 answer (the name is deliberate:
/// the 2026-09-30-morning `is_sweep_target` SHAPE predicate this replaces
/// is gone): not the name's shape — the binding's LEDGER. Never re-ask:
/// renamed (applied or carried), decided (an ask settled it), convention
/// carve-outs, eval/with-frozen soundness. Always ask: never-asked and
/// retry-exhausted-still-unrenamed bindings, whatever their names look
/// like.
fn is_sweep_candidate(
    state: &RenameState,
    taint: &EvalWithTaint,
    decided: Option<&DecidedNames>,
    e: &MintedBinding,
) -> bool {
    if is_convention_carveout(&e.name) || state.is_eval_taint_frozen(e.binding, taint) {
        return false;
    }
    if state.is_renamed(e.binding) || state.is_decided(e.binding) {
        return false;
    }
    if let Some(d) = decided {
        // The cross-text name join: approximate by construction (see
        // `DecidedNames`). Exhausted names fall through on purpose.
        if d.renamed_to.contains(&e.name) || d.asked.contains(&e.name) {
            return false;
        }
    }
    true
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

/// Reference lines (beyond the declaration's own window) each target's
/// window adds: the declaration says what it is, a use says what it does.
const USE_ANCHORS: usize = 2;
/// A use within this many lines after (or half as many before) the
/// declaration already sits in the declaration's window.
const USE_NEAR: i64 = 40;
/// How many refused names the stats keep as examples.
const NOT_SHOWN_EXAMPLES: usize = 20;

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

/// What the sweep shows, and what it could not show.
struct BuiltGroups {
    groups: Vec<SweepGroup>,
    /// Targets the guard refused: the code their ask would show does not
    /// contain them (`naming::shown`). Never asked, never applied.
    not_shown: Vec<MintedBinding>,
}

/// One target's window anchors: 1-based lines of the group's rendered
/// code — its declaration, then up to [`USE_ANCHORS`] uses outside the
/// declaration's window. Lines come from the binding's own spans when the
/// rendered code keeps the source's line structure (the main path's
/// mapping rule, `code_window::resolve_anchors`), else from the target's
/// whole-token occurrences in the rendered code.
fn target_anchors(
    view: &TextView<'_>,
    state: &RenameState,
    span: Span,
    lines: &[&str],
    t: &MintedBinding,
) -> Vec<i64> {
    let start = i64::from(view.line_of(span.start));
    let mapped = i64::from(view.line_of(span.end)) - start + 1 == lines.len() as i64;
    let mut out: Vec<i64> = if mapped {
        let b = state.view().binding(t.binding);
        std::iter::once(b.id_span)
            .chain(b.refs.iter().map(|r| r.span))
            .filter(|s| s.start >= span.start && s.end <= span.end)
            .map(|s| i64::from(view.line_of(s.start)) - start + 1)
            .collect()
    } else {
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| crate::naming::code_window::line_has_identifier(l, &t.name))
            .map(|(i, _)| i as i64 + 1)
            .collect()
    };
    let Some(&decl) = out.first() else {
        return out;
    };
    let mut uses: Vec<i64> = out
        .drain(1..)
        .filter(|l| *l < decl - USE_NEAR / 2 || *l > decl + USE_NEAR)
        .collect();
    uses.sort_unstable();
    uses.dedup();
    out.extend(uses.into_iter().take(USE_ANCHORS));
    out
}

/// One batch being filled: its anchors, the name each anchor belongs to
/// (the selection's positional rescue list), its targets.
type Batch = (Vec<i64>, Vec<String>, Vec<MintedBinding>);

/// Split an oversized group's targets into batches whose windows fit the
/// code budget at the default padding, in target order, and window each
/// batch's code through the main path's selection
/// (`code_window::select_function_code`). A target with no anchor gets no
/// window of its own — the guard refuses it unless a neighbour's window
/// happens to show it.
fn windowed_batches(
    view: &TextView<'_>,
    state: &RenameState,
    span: Span,
    code: &str,
    bucket: Vec<MintedBinding>,
) -> Vec<(String, Vec<MintedBinding>)> {
    use crate::naming::code_window::{
        FunctionCodeSelection, fits_at_default_pads, select_function_code,
    };
    let lines: Vec<&str> = code.split('\n').collect();
    let line_count = lines.len() as i64;
    let mut batches: Vec<Batch> = Vec::new();
    let mut cur: Batch = Default::default();
    for t in bucket {
        let anchors = target_anchors(view, state, span, &lines, &t);
        let mut joined = cur.0.clone();
        joined.extend(&anchors);
        if !cur.2.is_empty() && !fits_at_default_pads(&joined, line_count) {
            batches.push(std::mem::take(&mut cur));
        }
        cur.1
            .extend(std::iter::repeat_n(t.name.clone(), anchors.len()));
        cur.0.extend(anchors);
        cur.2.push(t);
    }
    if !cur.2.is_empty() {
        batches.push(cur);
    }
    batches
        .into_iter()
        .map(|(anchors, names, targets)| {
            let anchor_lines: Vec<Option<i64>> = anchors.into_iter().map(Some).collect();
            let shown = select_function_code(&FunctionCodeSelection {
                code,
                session_id: "",
                fn_start_line: Some(1),
                fn_end_line: Some(line_count),
                anchor_start_lines: Some(&anchor_lines),
                identifier_names: Some(&names),
            });
            (shown, targets)
        })
        .collect()
}

/// Build the sweep's groups. A group's code is its key node's rendered
/// text; over [`MAX_CODE_LINES`] it is no longer the flat HEAD of that
/// text (`capCode`, which asked 300 of 306 real sweep targets blind,
/// 2026-10-04) but a window around each target's declaration and uses,
/// batched to fit the budget. The guard (`naming::shown`) then refuses
/// any target its batch's code still does not contain.
fn build_groups(
    semantic: &Semantic<'_>,
    state: &RenameState,
    targets: Vec<MintedBinding>,
) -> BuiltGroups {
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
    let mut built = BuiltGroups {
        groups: Vec::new(),
        not_shown: Vec::new(),
    };
    for span in order {
        let bucket = by_key
            .remove(&(span.start, span.end))
            .expect("every key has its bucket");
        let scope = state.scope_of_binding(bucket[0].binding);
        let used_names = all_binding_names(state, scope);
        let code = view.pretty(span, &occ.edits(view.text, state, span), true);
        let batches = if code.split('\n').count() <= MAX_CODE_LINES {
            vec![(code, bucket)]
        } else {
            windowed_batches(&view, state, span, &code, bucket)
        };
        for (code, batch) in batches {
            let (shown, refused) = split_shown(&code, batch);
            built.not_shown.extend(refused);
            if !shown.is_empty() {
                built.groups.push(SweepGroup {
                    code,
                    used_names: used_names.clone(),
                    targets: shown,
                });
            }
        }
    }
    built
}

/// The guard's split of one batch: (targets `code` shows, targets it
/// does not).
fn split_shown(code: &str, batch: Vec<MintedBinding>) -> (Vec<MintedBinding>, Vec<MintedBinding>) {
    batch
        .into_iter()
        .partition(|t| crate::naming::shown::shows(code, &t.name))
}

/// The guard's refusal: a target the shown code did not contain is never
/// asked. Its trail row says why, and it stays a target (EXHAUSTED, not
/// decided — it never had an honest ask).
fn refuse_not_shown(
    state: &mut RenameState,
    refused: Vec<MintedBinding>,
    result: &mut SweepResult,
) {
    for t in refused {
        let row = Attempt::new(Tier::CoverageSweep, Outcome::Abstained)
            .reason(crate::naming::shown::NOT_SHOWN);
        state.record(t.binding, &t.name, row, true);
        state.mark_exhausted(t.binding);
        result.not_shown += 1;
        if result.not_shown_examples.len() < NOT_SHOWN_EXAMPLES {
            result.not_shown_examples.push(t.name);
        }
    }
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

/// The per-binding provenance classification of one sweep's state at the
/// END of its run (the exact, in-stage half of the coverage meter —
/// `bindingProvenance` in the stats). Universe: every eligible,
/// non-carve-out, non-frozen binding, because those are exactly the
/// bindings the ledger could have decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BindingProvenance {
    /// True when the classification consulted [`DecidedNames`] for
    /// bindings with no record in the sweep's own state — the deferred
    /// sweep's declared approximation (identity does not cross the text
    /// boundary); false for the in-era sweep's exact per-binding counts.
    pub joined: bool,
    pub total: usize,
    /// Renamed to a descriptive name this run.
    pub renamed: usize,
    /// Carried: a below-floor name deliberately applied (the exp066 rule).
    pub model_chosen: usize,
    /// Asked, terminal keep (declined / same-name / junk).
    pub asked_kept: usize,
    /// Retry budget exhausted still-unrenamed — still targets.
    pub exhausted: usize,
    /// No record anywhere: nothing asked, nothing applied.
    pub never_asked: usize,
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
    /// The per-binding provenance classification of the sweep's state at
    /// the end of its run (the meter's exact, in-stage half).
    pub provenance: Option<BindingProvenance>,
    /// Current names of the state's retry-exhausted still-unrenamed
    /// bindings — feeds the run-level name joins (`DecidedNames`, the
    /// survivor split).
    pub exhausted_names: Vec<String>,
    /// Identifiers the sweep's dispatches asked about (re-asks included,
    /// a target asked twice counts twice) — the prompt guard's universe.
    pub targets_asked: usize,
    /// Of [`Self::targets_asked`], those the ask's shown code contained
    /// (`naming::shown`, measured at dispatch) — should equal it.
    pub targets_shown: usize,
    /// Targets the guard refused before asking: the code their ask would
    /// show did not contain them (never asked, never applied).
    pub not_shown: usize,
    /// The first refused names (examples for the stats).
    pub not_shown_examples: Vec<String>,
}

/// One pending disclosed re-ask: the target, the LAST suggestion the
/// model made, the applier's rejection of it (`naming::reask` classifies
/// the reason; the ask trace records class + code), and the ACCUMULATED
/// history of every suggestion that already failed — each further re-ask
/// discloses all of it. A key-mismatch re-ask (`ReaskClass::AnswerKey`)
/// has no suggestion: it carries the answer's STRAY keys instead.
#[derive(Clone)]
struct SweepReask {
    target: MintedBinding,
    suggestion: String,
    /// The rejection as reask.rs classified it (recorded on the re-ask's
    /// ask-trace site; nothing decides on it).
    class: crate::naming::reask::ReaskClass,
    code: &'static str,
    /// Every prior rejected suggestion, oldest first: (name, code).
    rejects: Vec<(String, &'static str)>,
    /// The answer keys that belonged to no asked identifier — nonempty
    /// only for a key-mismatch re-ask (finding #85).
    stray: Vec<String>,
}

/// The re-ask context one apply round runs under: the budget
/// (`--rename-retries`), how many re-asks the round's targets have
/// already had (0 on the sweep's first apply, k during re-ask round k),
/// and each target's accumulated reject history.
struct ReaskCtx<'c> {
    limit: usize,
    spent: usize,
    carried: &'c HashMap<String, Vec<(String, &'static str)>>,
    /// The program's original minified names (the borrowed-stem refusal).
    stems: &'c MinifiedStems,
}

impl ReaskCtx<'_> {
    /// A rejected suggestion of a reaskable class: seed a disclosed
    /// re-ask while the target has budget, else mark it EXHAUSTED (it is
    /// still not properly renamed — the ledger keeps it targetable).
    fn reask_or_exhaust(
        &self,
        state: &mut RenameState,
        target: &MintedBinding,
        suggestion: String,
        class: crate::naming::reask::ReaskClass,
        code: &'static str,
        reasks: &mut Vec<SweepReask>,
    ) {
        if crate::naming::reask::reask_again(self.limit, self.spent, class) {
            let mut rejects = self.carried.get(&target.name).cloned().unwrap_or_default();
            rejects.push((suggestion.clone(), code));
            reasks.push(SweepReask {
                target: target.clone(),
                suggestion,
                class,
                code,
                rejects,
                stray: Vec::new(),
            });
        } else if crate::naming::reask::should_reask(class) {
            state.mark_exhausted(target.binding);
        }
    }

    /// A target the answer left unanswered while using keys that belong
    /// to no asked identifier (finding #85): seed a re-ask that discloses
    /// those keys while the target has budget, else mark it EXHAUSTED.
    /// Nothing joins the reject history — there was no suggestion.
    fn reask_key_mismatch(
        &self,
        state: &mut RenameState,
        target: &MintedBinding,
        stray: &[String],
        reasks: &mut Vec<SweepReask>,
    ) {
        use crate::naming::reask::{ANSWER_KEY_MISMATCH, ReaskClass, reask_again};
        if reask_again(self.limit, self.spent, ReaskClass::AnswerKey) {
            reasks.push(SweepReask {
                target: target.clone(),
                suggestion: String::new(),
                class: ReaskClass::AnswerKey,
                code: ANSWER_KEY_MISMATCH,
                rejects: self.carried.get(&target.name).cloned().unwrap_or_default(),
                stray: stray.to_vec(),
            });
        } else {
            state.mark_exhausted(target.binding);
        }
    }
}

/// Apply one group's suggestions (`applyGroupResponse`). `renames[name]`
/// reads the OWN entry (`Renames::get`, a plain list — no prototype
/// chain). That matters NOW: under the ledger targeting a target can be
/// named like an Object.prototype key (`var constructor = 1` is a legal
/// binding, and never-asked names are targets whatever they look like);
/// the TS's `renames[name]` would have fallen through to the prototype —
/// the old shape gate protected it only by accident.
///
/// The third return is the re-askable set: suggestions the validated
/// applier rejected for a reason a disclosed re-ask can fix (the
/// collision classes — `target-in-scope`, `target-visible`,
/// `shadows-child`, `target-free-name`) while the identifier still has
/// re-ask budget (`naming::reask::reask_again`). Before the 2026-09-28
/// fix these were counted `skipped` and dropped — the minted name kept
/// forever.
///
/// The answer is read through the ONE answer-key owner
/// (`naming::answer_keys`, finding #85): a key the model mangled (`y$`
/// for the asked `y$_`) lands on its one asked target, the match on the
/// trail row (`answerKey`); a target left unanswered while the answer
/// used keys that belong to no target is a disclosed RE-ASK
/// (`ReaskClass::AnswerKey`), not a decline.
fn apply_group_response(
    state: &mut RenameState,
    group: &SweepGroup,
    renames: &Renames,
    reask: &ReaskCtx<'_>,
) -> (usize, usize, Vec<SweepReask>) {
    let (mut named, mut skipped) = (0, 0);
    let mut reasks = Vec::new();
    let profile = state.name_profile();
    let asked: Vec<String> = group.targets.iter().map(|t| t.name.clone()).collect();
    let keyed = crate::naming::answer_keys::key_answer(renames, &asked, &|k| {
        group.used_names.iter().any(|n| n == k)
    });
    for target in &group.targets {
        let answer_key = keyed.answer_key(&target.name);
        if !keyed.answered(&target.name) && !keyed.stray.is_empty() {
            skipped += 1;
            let row = Attempt::new(Tier::CoverageSweep, Outcome::Abstained)
                .reason(crate::naming::reask::ANSWER_KEY_MISMATCH);
            state.record(target.binding, &target.name, row, true);
            reask.reask_key_mismatch(state, target, &keyed.stray, &mut reasks);
            continue;
        }
        let suggestion = keyed.renames.get(&target.name).filter(|s| !s.is_empty());
        // The ONE answer-quality question the wave barrier asks too
        // (`answer_refusal`): an answer borrowing a minified name as a
        // word (Fix A, 2026-10-03) or handing a multi-letter minified
        // name back as itself (round 2) is refused like an invalid
        // answer — a disclosed re-ask, then EXHAUSTED; never applied.
        if let Some((junk, code)) = suggestion
            .and_then(|s| answer_refusal(&target.name, s, reask.stems).map(|code| (s, code)))
        {
            skipped += 1;
            let row = Attempt::new(Tier::CoverageSweep, Outcome::Rejected)
                .proposed(junk.to_string())
                .reason(code)
                .answer_key(answer_key);
            state.record(target.binding, &target.name, row, true);
            reask.reask_or_exhaust(
                state,
                target,
                junk.to_string(),
                crate::naming::reask::ReaskClass::InvalidSuggestion,
                code,
                &mut reasks,
            );
            continue;
        }
        let Some(new_name) =
            suggestion.filter(|s| *s != target.name && is_sweep_answer_acceptable(profile, s))
        else {
            skipped += 1;
            let reason = match suggestion {
                Some(s) if s != target.name => "still-below-floor",
                _ => "llm-declined",
            };
            let row = Attempt::new(Tier::CoverageSweep, Outcome::Abstained)
                .reason(reason)
                .answer_key(answer_key);
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
            let row = Attempt::new(Tier::CoverageSweep, Outcome::Applied)
                .proposed(new_name)
                .answer_key(answer_key);
            state.record(target.binding, &target.name, row, true);
            continue;
        }
        skipped += 1;
        let mut row = Attempt::new(Tier::CoverageSweep, Outcome::Rejected)
            .proposed(new_name.clone())
            .answer_key(answer_key);
        if let Some(r) = attempt.reason {
            row = row.reason(r.as_str());
        }
        state.record(target.binding, &target.name, row, true);
        if let Some(r) = attempt.reason {
            let class = crate::naming::reask::class_of(r);
            // The budget dying on a class a re-ask could have fixed
            // leaves the identifier EXHAUSTED — the ledger keeps it
            // targetable (later rounds, later runs).
            reask.reask_or_exhaust(state, target, new_name, class, r.as_str(), &mut reasks);
        }
    }
    (named, skipped, reasks)
}

/// Classify the state's bindings by provenance — the meter's per-binding
/// half. `decided` (the deferred sweep's name join) classifies bindings
/// the state itself has no record for; when it is `None` every class
/// comes from this state's own ledger.
fn classify_bindings(
    semantic: &Semantic<'_>,
    state: &RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
    decided: Option<&DecidedNames>,
) -> BindingProvenance {
    let mut p = BindingProvenance {
        joined: decided.is_some(),
        ..BindingProvenance::default()
    };
    for e in collect_eligible_bindings(semantic, state, eligible).entries {
        if is_convention_carveout(&e.name) || state.is_eval_taint_frozen(e.binding, taint) {
            continue;
        }
        p.total += 1;
        if state.is_carried(e.binding) {
            p.model_chosen += 1;
        } else if state.is_renamed(e.binding) {
            p.renamed += 1;
        } else if state.is_decided(e.binding) {
            p.asked_kept += 1;
        } else if state.is_exhausted(e.binding) {
            p.exhausted += 1;
        } else if let Some(d) = decided {
            // Same priority as the name join's exclusions, plus the
            // below-floor read that splits carried (model-chosen) names
            // from descriptively renamed ones.
            if d.renamed_to.contains(&e.name) {
                if crate::rename::floor::is_below_floor_name(state.name_profile(), &e.name) {
                    p.model_chosen += 1;
                } else {
                    p.renamed += 1;
                }
            } else if d.exhausted.contains(&e.name) {
                p.exhausted += 1;
            } else if d.asked.contains(&e.name) {
                p.asked_kept += 1;
            } else {
                p.never_asked += 1;
            }
        } else {
            p.never_asked += 1;
        }
    }
    p
}

/// `sweepMintedNames`: force-name the bindings no pass decided, one
/// request per group, applied in group-build order. A suggestion rejected
/// for a collision class gets the run's disclosed re-asks (default TWO,
/// `--rename-retries`): each one names and blocklists EVERY suggestion
/// that already failed — the same retry prompt shape the wave lanes use.
/// A budget exhausted at a further collision gives up, recorded — and the
/// ledger keeps the identifier targetable (it is not properly renamed).
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
    decided: Option<&DecidedNames>,
    stems: &MinifiedStems,
) -> SweepResult {
    let targets = collect_sweep_targets(semantic, state, eligible, taint, decided);
    if targets.is_empty() {
        // Nothing to ask — but the meter still classifies the state: an
        // empty target set IS the finding (everything has a record).
        return SweepResult {
            provenance: Some(classify_bindings(semantic, state, eligible, taint, decided)),
            exhausted_names: state.exhausted_names(),
            ..SweepResult::default()
        };
    }
    let BuiltGroups { groups, not_shown } = build_groups(semantic, state, targets);
    let mut result = SweepResult {
        groups: groups.len(),
        ..SweepResult::default()
    };
    refuse_not_shown(state, not_shown, &mut result);
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
        stems,
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
            stems,
            &mut result,
        );
    }
    SweepResult {
        provenance: Some(classify_bindings(semantic, state, eligible, taint, decided)),
        exhausted_names: state.exhausted_names(),
        ..result
    }
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
    // The guard's measurement at the dispatch point: every identifier
    // asked, and how many the shown code contains (build_groups refused
    // the rest, so the two counts agree — fail loud when they do not).
    result.targets_asked += request.identifiers.len();
    result.targets_shown += request.identifiers.len()
        - crate::naming::shown::unshown(&request.code, &request.identifiers).len();
    crate::naming::shown::debug_assert_all_shown("sweep", &request.code, &request.identifiers);
    let system_prompt = render_system_prompt(&request);
    let user_prompt = render_user_prompt(&request, state.name_profile());
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

/// A re-ask group's disclosure: per target, its last suggestion
/// (`previous_attempt`), its failure list, and its accumulated reject
/// history. A collision/invalid seed is a `duplicates` entry (the
/// suggestion and every prior reject disclosed); a key-mismatch seed
/// (finding #85) is a `missing` entry, and the answer's stray keys travel
/// in `stray_keys` so the prompt names them.
fn reask_disclosure(
    state: &RenameState,
    targets: &[MintedBinding],
    seed_of: &HashMap<String, SweepReask>,
    stems: &MinifiedStems,
) -> (
    crate::naming::waves::jsset::JsRecord,
    humanify_model::llm::RenameFailures,
    humanify_model::llm::PriorRejects,
) {
    let mut prev = crate::naming::waves::jsset::JsRecord::default();
    let mut failures = humanify_model::llm::RenameFailures::default();
    let mut prior = humanify_model::llm::PriorRejects::default();
    for t in targets {
        let seed = seed_of
            .get(&t.name)
            .expect("every re-asked target carries its seed");
        if seed.class == crate::naming::reask::ReaskClass::AnswerKey {
            failures.missing.push(t.name.clone());
            for key in &seed.stray {
                if !failures.stray_keys.contains(key) {
                    failures.stray_keys.push(key.clone());
                }
            }
            continue;
        }
        prev.set(&t.name, &seed.suggestion);
        failures.duplicates.push(t.name.clone());
        let scope = state.scope_of_binding(t.binding);
        prior.0.push((
            t.name.clone(),
            seed.rejects
                .iter()
                .map(|(name, code)| {
                    // WHO holds a taken name, where the scopes know
                    // (2026-10-06: every retry prompt says it).
                    let held_by = name_taken_reason(code)
                        .and_then(|r| state.name_holder(scope, &t.name, name, Some(r)))
                        .map(crate::naming::prompts::holder_phrase);
                    disclose_reject(name, Some(code), held_by, stems)
                })
                .collect(),
        ));
    }
    (prev, failures, prior)
}

/// The applier rejection a sweep reject code names, when it is a
/// name-taken class (`naming::reask::class_of`).
fn name_taken_reason(code: &str) -> Option<crate::rename::validated::RejectionReason> {
    use crate::rename::validated::RejectionReason as R;
    [
        R::TargetInScope,
        R::TargetVisible,
        R::ShadowsChild,
        R::TargetFreeName,
    ]
    .into_iter()
    .find(|r| r.as_str() == code)
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
    stems: &MinifiedStems,
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
        let seed_of: HashMap<String, SweepReask> = pending
            .iter()
            .map(|r| (r.target.name.clone(), r.clone()))
            .collect();
        let carried: HashMap<String, Vec<(String, &'static str)>> = seed_of
            .iter()
            .map(|(k, seed)| (k.clone(), seed.rejects.clone()))
            .collect();
        let targets: Vec<MintedBinding> = pending.into_iter().map(|r| r.target).collect();
        let BuiltGroups {
            groups: fresh,
            not_shown,
        } = build_groups(semantic, state, targets);
        // A re-asked target its rebuilt window no longer shows is refused
        // like a first-round one (the window follows the current names).
        refuse_not_shown(state, not_shown, result);
        let mut owners = Vec::with_capacity(fresh.len());
        let mut round_responses = Vec::with_capacity(fresh.len());
        let mut round_calls = Vec::with_capacity(fresh.len());
        for g in &fresh {
            let (prev, failures, prior) = reask_disclosure(state, &g.targets, &seed_of, stems);
            // The ask site: the group's targets were seeded by applier
            // rejections — the class when they all agree (the usual case:
            // one collision class), their codes in the detail. Recording
            // only.
            let seeded: Vec<(crate::naming::reask::ReaskClass, &'static str)> = g
                .targets
                .iter()
                .map(|t| {
                    let seed = seed_of.get(&t.name).expect("every target carries its seed");
                    (seed.class, seed.code)
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
            let identifiers: Vec<String> = g.targets.iter().map(|t| t.name.clone()).collect();
            let refused =
                crate::naming::waves::processor::refused_answers(&identifiers, &prev, Some(&prior));
            let request = BatchRenameRequest {
                code: g.code.clone(),
                used_names: crate::naming::waves::processor::build_retry_used_names(
                    &refused,
                    &g.used_names,
                    &g.used_names,
                ),
                identifiers,
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
            stems,
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
/// (`--rename-retries`); `decided` is the run's decision ledger joined BY
/// NAME — the approximation that stands in for per-binding identity
/// across the text boundary (see [`DecidedNames`]).
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
    decided: &DecidedNames,
    stems: &MinifiedStems,
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
    let mut state = RenameState::with_trail(semantic, anchor, trail, stems.profile());
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
        Some(decided),
        stems,
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

#[cfg(test)]
mod answer_key_test;
#[cfg(test)]
mod sweep_test;
