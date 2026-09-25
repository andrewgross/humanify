//! Diff-guided reconciliation of cross-version rename noise (WP4.4) — TS
//! originals: `src/rename/diff-reconcile.ts` (the pass) and
//! `src/rename/reconcile-step.ts` (the pipeline step: parse the generated
//! output, reconcile against the prior version's text, re-render).
//!
//! The plaintext `diff prior new` carries an alignment none of the
//! per-binding matchers see; a change hunk whose sides are identical after
//! blanking identifier tokens is rename noise, and each differing
//! identifier position proposes snapping the new name back to the prior
//! one. The unit of decision is the Babel BINDING (a position must resolve
//! through scope bookkeeping), all of a binding's proposals must agree,
//! every occurrence must sit on a diff-covered line, the declaration must
//! sit in a clean aligned pair (or the consumer tier's testimony must
//! hold), and application goes through validated rename. Tiers by what
//! the rename overwrites: asymmetric (minted → descriptive), descriptive
//! (declaration dependencies already reconciled), consumer (changed
//! declaration, ≥2 distinct witnesses), and the relaxed LAST-RESORT round.
//!
//! Ordering is part of the contract (porting lesson 4): groups keep the
//! first-occurrence order of the TS `Map<Binding, …>`, survivors apply in
//! (declLine, declCol) order, skips are reported in gate order, and the
//! vote maps keep insertion order.
//!
//! Submodules: [`lexer`] (line tokens, skeletons, clean pairs), [`hunks`]
//! (the system diff and hunk analysis), [`resolve`] (positions → bindings),
//! [`step`] (the pipeline step: parse, reconcile, render).

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_ast::ast::{Argument, Expression};
use oxc_semantic::Semantic;

use crate::babel_view::unparen;
use crate::modules::soundness::{EvalWithTaint, collect_eval_with_taint};
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{is_half_mint_head, is_wordless_mint_shape};
use crate::rename::validated::scopes::BindingId;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};

pub mod hunks;
pub mod lexer;
pub mod resolve;
pub mod step;

use crate::babel_view::BabelLines;
use hunks::{HunkAnalysis, NoiseLineInfo, analyze_hunks, parse_normal_diff, prior_too_dissimilar};
use resolve::{Resolution, collect_identifier_names, resolve_candidates};

/// `ReconcileOptions` (defaults = the TS `DEFAULT_OPTIONS`).
#[derive(Clone, Debug)]
pub struct ReconcileOptions {
    pub apply: bool,
    pub descriptive_tier: bool,
    pub max_hunk_lines: usize,
    pub mixed_hunk_tier: bool,
    pub prior_line_count: Option<usize>,
    pub consumer_tier: bool,
    /// Word tokens of the prior text (`collectWordTokens`).
    pub prior_names: Option<HashSet<String>>,
    pub last_resort_tier: bool,
    pub skip_import_declarations: bool,
    pub skeleton_vote_tier: bool,
    /// A planted bug for the gate's red runs (never set in the pipeline).
    pub plant: Option<ReconcilePlant>,
}

/// A planted bug (the gate's red runs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReconcilePlant {
    /// Route the FIRST descriptive survivor as asymmetric — one wrong tier
    /// choice (its trail tier and its rename kind).
    FlipTier,
}

impl Default for ReconcileOptions {
    fn default() -> Self {
        ReconcileOptions {
            apply: false,
            descriptive_tier: false,
            max_hunk_lines: 10,
            mixed_hunk_tier: false,
            prior_line_count: None,
            consumer_tier: false,
            prior_names: None,
            last_resort_tier: false,
            skip_import_declarations: false,
            skeleton_vote_tier: false,
            plant: None,
        }
    }
}

/// `RenameKind`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenameKind {
    Asymmetric,
    Descriptive,
    Consumer,
    LastResort,
}

impl RenameKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RenameKind::Asymmetric => "asymmetric",
            RenameKind::Descriptive => "descriptive",
            RenameKind::Consumer => "consumer",
            RenameKind::LastResort => "last-resort",
        }
    }

    fn tier(self) -> Tier {
        match self {
            RenameKind::Asymmetric => Tier::ReconcileAsymmetric,
            RenameKind::Descriptive => Tier::ReconcileDescriptive,
            RenameKind::Consumer => Tier::ReconcileConsumer,
            RenameKind::LastResort => Tier::ReconcileLastResort,
        }
    }
}

/// `ReconcileRename`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileRename {
    pub from_name: String,
    pub to_name: String,
    pub votes: usize,
    pub kind: RenameKind,
    pub decl_line: usize,
    pub applied: bool,
}

/// `ReconcileSkip`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconcileSkip {
    pub from_name: String,
    pub to_name: String,
    pub reason: String,
    pub votes: usize,
}

/// `ReconcileHunkStats`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconcileHunkStats {
    pub changed: usize,
    pub noise: i64,
    pub genuine: usize,
    pub oversized: usize,
    pub tainted: usize,
    pub mixed: usize,
}

/// `ReconcileResult`.
#[derive(Clone, Debug, Default)]
pub struct ReconcileResult {
    pub renames: Vec<ReconcileRename>,
    pub skipped: Vec<ReconcileSkip>,
    pub prior_too_dissimilar: bool,
    pub hunks: ReconcileHunkStats,
}

/// `collectWordTokens`: every `[A-Za-z_$][\w$]*` run of a text.
pub fn collect_word_tokens(text: &str) -> HashSet<String> {
    let b = text.as_bytes();
    let start = |c: u8| c.is_ascii_alphabetic() || c == b'_' || c == b'$';
    let cont = |c: u8| start(c) || c.is_ascii_digit();
    let mut out = HashSet::new();
    let mut i = 0;
    while i < b.len() {
        if start(b[i]) {
            let s = i;
            i += 1;
            while i < b.len() && cont(b[i]) {
                i += 1;
            }
            out.insert(text[s..i].to_string());
        } else {
            i += 1;
        }
    }
    out
}

/// A witness key: the line skeleton, else `hunk:<index>`.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Witness {
    Skeleton(Vec<u16>),
    Hunk(usize),
}

/// `BindingGroup`.
struct Group {
    binding: BindingId,
    from_name: String,
    /// toName → votes, insertion order.
    votes_by_name: Vec<(String, usize)>,
    witnesses_by_name: HashMap<String, HashSet<Witness>>,
    total_votes: usize,
}

fn group_by_binding(
    resolution: &Resolution,
    candidates: &[hunks::PositionCandidate],
) -> Vec<Group> {
    let mut index: HashMap<BindingId, usize> = HashMap::new();
    let mut groups: Vec<Group> = Vec::new();
    for occ in &resolution.occurrences {
        let c = &candidates[occ.candidate];
        if resolution.tainted_hunks.contains(&c.hunk_index) {
            continue;
        }
        let gi = *index.entry(occ.binding).or_insert_with(|| {
            groups.push(Group {
                binding: occ.binding,
                from_name: c.from_name.clone(),
                votes_by_name: Vec::new(),
                witnesses_by_name: HashMap::new(),
                total_votes: 0,
            });
            groups.len() - 1
        });
        let g = &mut groups[gi];
        match g.votes_by_name.iter_mut().find(|(n, _)| *n == c.to_name) {
            Some((_, v)) => *v += 1,
            None => g.votes_by_name.push((c.to_name.clone(), 1)),
        }
        let witness = match &c.skeleton {
            Some(sk) => Witness::Skeleton(sk.clone()),
            None => Witness::Hunk(c.hunk_index),
        };
        g.witnesses_by_name
            .entry(c.to_name.clone())
            .or_default()
            .insert(witness);
        g.total_votes += 1;
    }
    groups
}

/// `Survivor`.
#[derive(Clone)]
struct Survivor {
    group: usize,
    to_name: String,
    kind: RenameKind,
    decl_line: usize,
    decl_col: usize,
}

enum Gate {
    Survivor(Survivor),
    Skip(ReconcileSkip),
}

/// Everything the gates read (`GateContext`).
struct Ctx<'a, 's> {
    semantic: &'a Semantic<'s>,
    lines: &'a BabelLines<'s>,
    analysis: &'a HunkAnalysis,
    tainted: &'a BTreeSet<usize>,
    /// "line:col" → the binding resolved there (last occurrence wins).
    position_bindings: HashMap<(usize, usize), BindingId>,
    applied_bindings: HashSet<BindingId>,
    eval_taint: EvalWithTaint,
    to_name_claimants: HashMap<String, usize>,
    new_name_census: HashSet<String>,
    eligible: &'a Eligibility,
    opts: &'a ReconcileOptions,
    /// The FlipTier plant fired already.
    flipped: std::cell::Cell<bool>,
}

fn skip_of(g: &Group, to_name: &str, reason: &str) -> Gate {
    Gate::Skip(ReconcileSkip {
        from_name: g.from_name.clone(),
        to_name: to_name.to_string(),
        reason: reason.to_string(),
        votes: g.total_votes,
    })
}

/// `isRequireBinding`: declared `<kind> x = require("<string>")`.
fn is_require_binding(semantic: &Semantic<'_>, state: &RenameState, binding: BindingId) -> bool {
    let path = state.view().binding(binding).path_node;
    let AstKind::VariableDeclarator(decl) = semantic.nodes().kind(path) else {
        return false;
    };
    let Some(init) = decl.init.as_ref() else {
        return false;
    };
    let Expression::CallExpression(call) = unparen(init) else {
        return false;
    };
    let Expression::Identifier(callee) = unparen(&call.callee) else {
        return false;
    };
    callee.name == "require"
        && call.arguments.len() == 1
        && matches!(&call.arguments[0], Argument::StringLiteral(_))
}

impl Ctx<'_, '_> {
    /// `gateGroup`.
    fn gate_group(&self, state: &RenameState, groups: &[Group], gi: usize, relax: bool) -> Gate {
        let g = &groups[gi];
        if g.votes_by_name.len() != 1 {
            let mut names: Vec<&str> = g.votes_by_name.iter().map(|(n, _)| n.as_str()).collect();
            names.sort_unstable();
            return skip_of(g, &names.join("|"), "disagreement");
        }
        let to_name = g.votes_by_name[0].0.clone();
        if state.name_of(g.binding) != g.from_name {
            return skip_of(g, &to_name, "stale-binding");
        }
        if state.is_eval_taint_frozen(g.binding, &self.eval_taint) {
            return skip_of(g, &to_name, "eval-taint-frozen");
        }
        if state.is_export_involved(g.binding) {
            return skip_of(g, &to_name, "export-involved");
        }
        if self.opts.skip_import_declarations && is_require_binding(self.semantic, state, g.binding)
        {
            return skip_of(g, &to_name, "import-declaration");
        }
        if !self.eligible.is_eligible(&g.from_name) {
            return skip_of(g, &to_name, "not-eligible");
        }
        if is_wordless_mint_shape(&to_name) {
            let reason = if is_wordless_mint_shape(&g.from_name) {
                "reroll"
            } else {
                "name-downgrade"
            };
            return skip_of(g, &to_name, reason);
        }
        if is_half_mint_head(&to_name) && !is_wordless_mint_shape(&g.from_name) {
            return skip_of(g, &to_name, "half-mint-restore");
        }
        let mut kind = if is_wordless_mint_shape(&g.from_name) {
            RenameKind::Asymmetric
        } else {
            RenameKind::Descriptive
        };
        if self.opts.plant == Some(ReconcilePlant::FlipTier)
            && kind == RenameKind::Descriptive
            && !self.flipped.get()
        {
            self.flipped.set(true);
            kind = RenameKind::Asymmetric;
        }
        if kind == RenameKind::Descriptive && !self.opts.descriptive_tier {
            return skip_of(g, &to_name, "descriptive-tier-disabled");
        }
        self.gate_locations(state, g, gi, &to_name, kind, relax)
    }

    /// Every line the rename would rewrite (`collectOccurrenceLines`).
    fn occurrence_lines(&self, state: &RenameState, binding: BindingId) -> Vec<usize> {
        let b = state.view().binding(binding);
        let mut lines = vec![self.lines.line(b.id_span.start)];
        lines.extend(b.refs.iter().map(|r| self.lines.line(r.span.start)));
        for targets in &b.violation_targets {
            lines.extend(targets.iter().map(|t| self.lines.line(t.start)));
        }
        lines
    }

    /// `alignedDeclaration`.
    fn aligned_declaration(&self, line: usize, col: usize) -> Option<&NoiseLineInfo> {
        let info = self.analysis.noise_lines.get(&line)?;
        if self.tainted.contains(&info.hunk_index) {
            return None;
        }
        info.diffs.iter().any(|d| d.col == col).then_some(info)
    }

    /// `declarationDependenciesClean`.
    fn declaration_dependencies_clean(
        &self,
        info: &NoiseLineInfo,
        line: usize,
        col: usize,
    ) -> bool {
        info.diffs.iter().all(|d| {
            d.col == col
                || self
                    .position_bindings
                    .get(&(line, d.col))
                    .is_some_and(|b| self.applied_bindings.contains(b))
        })
    }

    /// `gateGroupLocations`.
    fn gate_locations(
        &self,
        state: &RenameState,
        g: &Group,
        gi: usize,
        to_name: &str,
        kind: RenameKind,
        relax: bool,
    ) -> Gate {
        let occurrence_lines = self.occurrence_lines(state, g.binding);
        if !occurrence_lines
            .iter()
            .all(|l| self.analysis.changed_new_lines.contains(l))
        {
            return skip_of(g, to_name, "occurrence-outside-diff");
        }
        let id_start = state.view().binding(g.binding).id_span.start;
        let (line, col) = self.lines.loc(id_start);
        let Some(decl_info) = self.aligned_declaration(line, col) else {
            return self.gate_consumer(g, gi, to_name, line, col);
        };
        if !relax
            && self
                .analysis
                .mixed_hunk_indexes
                .contains(&decl_info.hunk_index)
        {
            let all_clean = occurrence_lines.iter().all(|l| {
                self.analysis
                    .noise_lines
                    .get(l)
                    .is_some_and(|i| !self.tainted.contains(&i.hunk_index))
            });
            if !all_clean {
                return skip_of(g, to_name, "mixed-dirty-occurrence");
            }
        }
        if !relax
            && kind == RenameKind::Descriptive
            && !self.declaration_dependencies_clean(decl_info, line, col)
        {
            return skip_of(g, to_name, "decl-not-clean");
        }
        let kind = if relax { RenameKind::LastResort } else { kind };
        Gate::Survivor(Survivor {
            group: gi,
            to_name: to_name.to_string(),
            kind,
            decl_line: line,
            decl_col: col,
        })
    }

    /// `gateConsumerTier`.
    fn gate_consumer(&self, g: &Group, gi: usize, to_name: &str, line: usize, col: usize) -> Gate {
        let prior_names = match (&self.opts.prior_names, self.opts.consumer_tier) {
            (Some(p), true) => p,
            _ => return skip_of(g, to_name, "decl-not-aligned"),
        };
        let hunk_count = g.witnesses_by_name.get(to_name).map_or(0, HashSet::len);
        if hunk_count < 2 {
            let occurrences = g
                .votes_by_name
                .iter()
                .find(|(n, _)| n == to_name)
                .map_or(0, |(_, v)| *v);
            let bucket = match occurrences {
                0 | 1 => "occ1",
                2 => "occ2",
                _ => "occ3plus",
            };
            return skip_of(g, to_name, &format!("consumer-single-hunk-{bucket}"));
        }
        if self.to_name_claimants.get(to_name) != Some(&1) {
            return skip_of(g, to_name, "consumer-name-conflict");
        }
        if self.new_name_census.contains(to_name) {
            return skip_of(g, to_name, "consumer-to-name-live");
        }
        if prior_names.contains(&g.from_name) && hunk_count < 3 {
            return skip_of(g, to_name, "consumer-from-not-novel");
        }
        Gate::Survivor(Survivor {
            group: gi,
            to_name: to_name.to_string(),
            kind: RenameKind::Consumer,
            decl_line: line,
            decl_col: col,
        })
    }
}

/// The round state (`RoundState`).
struct Rounds {
    renames: Vec<ReconcileRename>,
    skipped: Vec<ReconcileSkip>,
    from_name_groups: HashMap<String, usize>,
    /// Groups held for the relaxed round, with their original skip.
    last_resort: Vec<(usize, ReconcileSkip)>,
}

impl Rounds {
    fn trail(
        &self,
        state: &mut RenameState,
        opts: &ReconcileOptions,
        binding: BindingId,
        from: &str,
        attempt: Attempt,
    ) {
        if opts.apply {
            state.record(binding, from, attempt, true);
        }
    }

    fn record_skip(
        &mut self,
        state: &mut RenameState,
        opts: &ReconcileOptions,
        skip: ReconcileSkip,
        binding: BindingId,
    ) {
        let attempt = Attempt::new(Tier::Reconcile, Outcome::Abstained)
            .reason(skip.reason.clone())
            .proposed(skip.to_name.clone());
        self.trail(state, opts, binding, &skip.from_name, attempt);
        self.skipped.push(skip);
    }
}

fn survivor_rename(groups: &[Group], s: &Survivor, applied: bool) -> ReconcileRename {
    ReconcileRename {
        from_name: groups[s.group].from_name.clone(),
        to_name: s.to_name.clone(),
        votes: groups[s.group].total_votes,
        kind: s.kind,
        decl_line: s.decl_line,
        applied,
    }
}

fn survivor_skip(groups: &[Group], s: &Survivor, reason: String) -> ReconcileSkip {
    ReconcileSkip {
        from_name: groups[s.group].from_name.clone(),
        to_name: s.to_name.clone(),
        reason,
        votes: groups[s.group].total_votes,
    }
}

/// `attemptOne`.
fn attempt_one(
    state: &mut RenameState,
    groups: &[Group],
    s: &Survivor,
    apply: bool,
) -> Option<String> {
    let g = &groups[s.group];
    let scope = state.scope_of_binding(g.binding);
    if state.binding_in(scope, &g.from_name) != Some(g.binding) {
        return Some("stale-binding".to_string());
    }
    if !apply {
        return state
            .get_rename_rejection(scope, &g.from_name, &s.to_name)
            .map(|r| format!("rename-rejected:{}", r.as_str()));
    }
    let attempt = state.attempt_validated_rename(
        RenameRequest {
            scope,
            old_name: &g.from_name,
            new_name: &s.to_name,
            expected: None,
        },
        TrailSpec::CallerRecords {
            tier: s.kind.tier(),
        },
    );
    if attempt.applied {
        None
    } else {
        let reason = attempt.reason.map_or("undefined", |r| r.as_str());
        Some(format!("rename-rejected:{reason}"))
    }
}

/// One gate pass over `remaining` (`gateRound`).
fn gate_round(
    ctx: &Ctx<'_, '_>,
    st: &mut Rounds,
    state: &mut RenameState,
    groups: &[Group],
    remaining: &[usize],
    relax: bool,
) -> (Vec<Survivor>, Vec<(usize, ReconcileSkip)>) {
    let mut survivors = Vec::new();
    let mut deferred = Vec::new();
    for &gi in remaining {
        let g = &groups[gi];
        if relax && st.from_name_groups.get(&g.from_name).copied().unwrap_or(0) > 1 {
            let skip = ReconcileSkip {
                from_name: g.from_name.clone(),
                to_name: g.from_name.clone(),
                reason: "same-name-siblings".to_string(),
                votes: g.total_votes,
            };
            st.record_skip(state, ctx.opts, skip, g.binding);
            continue;
        }
        match ctx.gate_group(state, groups, gi, relax) {
            Gate::Survivor(s) => survivors.push(s),
            Gate::Skip(skip) => {
                let holds =
                    !relax && ctx.opts.last_resort_tier && skip.reason == "mixed-dirty-occurrence";
                if !relax && skip.reason == "decl-not-clean" {
                    deferred.push((gi, skip));
                } else if holds {
                    st.last_resort.push((gi, skip));
                } else {
                    st.record_skip(state, ctx.opts, skip, g.binding);
                }
            }
        }
    }
    // Array.prototype.sort is stable: ties keep gate order.
    survivors.sort_by_key(|s| (s.decl_line, s.decl_col));
    (survivors, deferred)
}

/// `runGateRounds` — the fixpoint at one relaxation level.
fn run_gate_rounds(
    ctx: &mut Ctx<'_, '_>,
    st: &mut Rounds,
    state: &mut RenameState,
    groups: &[Group],
    initial: Vec<usize>,
    relax: bool,
) {
    let mut remaining = initial;
    while !remaining.is_empty() {
        let (survivors, deferred) = gate_round(ctx, st, state, groups, &remaining, relax);
        let mut applied = Vec::new();
        let mut rejected = Vec::new();
        for s in survivors {
            match attempt_one(state, groups, &s, ctx.opts.apply) {
                Some(reason) => rejected.push((s, reason)),
                None => applied.push(s),
            }
        }
        for s in &applied {
            st.renames.push(survivor_rename(groups, s, ctx.opts.apply));
            ctx.applied_bindings.insert(groups[s.group].binding);
            let attempt = Attempt::new(s.kind.tier(), Outcome::Applied).proposed(s.to_name.clone());
            st.trail(
                state,
                ctx.opts,
                groups[s.group].binding,
                &groups[s.group].from_name,
                attempt,
            );
        }
        if applied.is_empty() {
            if !relax && ctx.opts.last_resort_tier {
                st.last_resort.extend(deferred);
            } else {
                st.skipped.extend(deferred.into_iter().map(|(_, s)| s));
            }
            for (s, reason) in &rejected {
                st.skipped.push(survivor_skip(groups, s, reason.clone()));
            }
            for (s, reason) in rejected {
                let attempt = Attempt::new(s.kind.tier(), Outcome::Rejected)
                    .reason(reason)
                    .proposed(s.to_name.clone());
                st.trail(
                    state,
                    ctx.opts,
                    groups[s.group].binding,
                    &groups[s.group].from_name,
                    attempt,
                );
            }
            break;
        }
        remaining = deferred
            .into_iter()
            .map(|(gi, _)| gi)
            .chain(rejected.into_iter().map(|(s, _)| s.group))
            .collect();
    }
}

/// `reconcileDiffNoise`: reconcile rename noise between the text `semantic`
/// parsed (whose positions the diff's new side numbers) and the prior
/// text, given `diff prior new` in normal format. Renames apply through
/// `state` (the text's own rename state) when `opts.apply`.
pub fn reconcile_diff_noise(
    semantic: &Semantic<'_>,
    state: &mut RenameState,
    diff_text: &str,
    eligible: &Eligibility,
    opts: &ReconcileOptions,
) -> ReconcileResult {
    let hunks = parse_normal_diff(diff_text);
    if prior_too_dissimilar(&hunks, opts.prior_line_count) {
        return ReconcileResult {
            prior_too_dissimilar: true,
            ..ReconcileResult::default()
        };
    }
    let analysis = analyze_hunks(
        &hunks,
        opts.max_hunk_lines,
        opts.mixed_hunk_tier,
        opts.skeleton_vote_tier,
    );
    let text = semantic.source_text();
    let lines = BabelLines::new(text);
    let resolution = resolve_candidates(semantic, state, &lines, &analysis.candidates);
    let mut position_bindings = HashMap::new();
    for occ in &resolution.occurrences {
        let c = &analysis.candidates[occ.candidate];
        position_bindings.insert((c.line, c.col), occ.binding);
    }
    let groups = group_by_binding(&resolution, &analysis.candidates);
    let mut to_name_claimants: HashMap<String, usize> = HashMap::new();
    for g in &groups {
        for (name, _) in &g.votes_by_name {
            *to_name_claimants.entry(name.clone()).or_insert(0) += 1;
        }
    }
    let eval_taint = if resolution.occurrences.is_empty() {
        EvalWithTaint {
            tainted_functions: Vec::new(),
            module_tainted: false,
            site_count: 0,
        }
    } else {
        collect_eval_with_taint(semantic)
    };
    let new_name_census = if opts.consumer_tier && opts.prior_names.is_some() && !groups.is_empty()
    {
        collect_identifier_names(semantic, state)
    } else {
        HashSet::new()
    };
    let mut ctx = Ctx {
        semantic,
        lines: &lines,
        analysis: &analysis,
        tainted: &resolution.tainted_hunks,
        position_bindings,
        applied_bindings: HashSet::new(),
        eval_taint,
        to_name_claimants,
        new_name_census,
        eligible,
        opts,
        flipped: std::cell::Cell::new(false),
    };
    let mut from_name_groups: HashMap<String, usize> = HashMap::new();
    for g in &groups {
        *from_name_groups.entry(g.from_name.clone()).or_insert(0) += 1;
    }
    let mut st = Rounds {
        renames: Vec::new(),
        skipped: Vec::new(),
        from_name_groups,
        last_resort: Vec::new(),
    };
    let all: Vec<usize> = (0..groups.len()).collect();
    run_gate_rounds(&mut ctx, &mut st, state, &groups, all, false);
    if !st.last_resort.is_empty() {
        let held: Vec<usize> = st.last_resort.iter().map(|(gi, _)| *gi).collect();
        run_gate_rounds(&mut ctx, &mut st, state, &groups, held, true);
    }
    let tainted = resolution.tainted_hunks.len();
    ReconcileResult {
        renames: st.renames,
        skipped: st.skipped,
        prior_too_dissimilar: false,
        hunks: ReconcileHunkStats {
            changed: analysis.changed,
            // JS number arithmetic: a skeleton-vote candidate can taint a
            // genuine or oversized hunk, so this may go negative.
            noise: analysis.noise_hunks as i64 - tainted as i64,
            genuine: analysis.genuine,
            oversized: analysis.oversized,
            tainted,
            mixed: analysis.mixed_hunks,
        },
    }
}

#[cfg(test)]
mod reconcile_test;

#[cfg(test)]
mod cases_test;
