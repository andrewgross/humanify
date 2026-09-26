//! The wave processor — TS `RenameProcessor.processUnified` and its wave
//! loop (processor.ts), with the scheduler mechanics of wave-scheduler.ts.
//!
//! Wave N = every pending node whose dependencies settled in waves < N (the
//! deadlock-break tiers as deterministic steps). A step runs in ROUNDS:
//!
//! - round A: every function's main pass (phase 0), every module-binding
//!   group, every retry seed from the previous step — all reading the
//!   FROZEN pre-step state and COLLECTING renames;
//! - barrier A: the collected entries apply in (node, phase, binding, seq)
//!   order through validated rename; losers become next-step retry seeds;
//!   then, per function in node order, the shadowed-binding second pass is
//!   computed and uniquified (it mutates, so it runs inside the barrier);
//! - round B: the shadowed pass (phase 1) collects;
//! - barrier B: applies it.
//!
//! The TS dispatches every round's requests concurrently (32 in flight) and
//! the dispatch ORDER depends on promise timing — but nothing a decision
//! reads can move within a round, so each lane's request sequence and the
//! barrier's apply order are fixed: those are reproduced exactly; the
//! dispatch order is not a decision input (the dump's `seq`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use humanify_model::llm::{
    BatchRenameRequest, CacheKeyParams, CalleeSignature, LlmCall, LlmErrorKind, NameProvider,
    RenameFailures, Renames, StrMap, cache_key_of,
};
use oxc_semantic::Semantic;
use oxc_span::Span;

use super::batch::{
    Lane, LaneCall, LaneEffect, LaneEnv, LaneReport, Transform, WaveTunables, compute_lane_count,
    split_by_position,
};
use super::generate::TextView;
use super::graph_ext::{NamingGraph, NodeRef};
use super::jsset::{JsRecord, JsSet};
use super::nodes::FnNode;
use super::render::{FnPrinter, Occurrences};
use super::used_set::{NameLayer, UsedSet};
use crate::fast::Lever;
use crate::graph::UnifiedGraph;
use crate::naming::code_window::{FunctionCodeSelection, cap_context_code, select_function_code};
use crate::naming::context::{ContextView, DeclView, ParentBinding, build_context};
use crate::naming::prompts::{
    ArrayRecord, MODULE_LEVEL_RENAME_SYSTEM_PROMPT, ModuleLevelInput, RetryInput,
    build_batch_rename_retry_body, build_module_level_rename_body,
    build_module_level_rename_prompt, build_module_level_retry_prefix, render_system_prompt,
    render_user_prompt,
};
use crate::naming::report::{
    ContentionEvent, IdentifierOutcome, Outcomes, ProcessorReport, RenameReport, ReportStrategy,
    ReportType, Status,
};
use crate::naming::snap::{build_prior_stem_index, snap_suggestion_to_prior, snap_to_known_prior};
use crate::naming::validation::resolve_conflict;
use crate::rename::eligibility::Eligibility;
use crate::rename::transfer::lifecycle::Lifecycle;
use crate::rename::transfer::owned::{
    BindingInfo, collect_owned_binding_infos, collect_shadowed_block_bindings,
};
use crate::rename::transfer::rows::Rows;
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::target::is_valid_rename_target;
use crate::rename::validated::{RejectionReason, RenameRequest, RenameState, TrailSpec};
use crate::rename::votes::proximity::{ProximityBinding, get_proximate_used_names};
use crate::trail::{Attempt, Outcome, Tier};

/// A close-matched function's prior-version context (`fn.priorVersion*`).
#[derive(Clone, Debug, Default)]
pub struct CloseContext {
    /// `generate(priorFn.path.node)` — the PRIOR function's code.
    pub prior_code: String,
    /// `collectPriorNames` (placeholder names, unique, first 40).
    pub prior_names: Vec<String>,
    pub hints: Option<StrMap>,
    pub snaps: Option<Vec<(String, String)>>,
}

/// Everything the waves read that the run does not mutate.
pub struct WaveInputs<'a, 's> {
    pub semantic: &'a Semantic<'s>,
    pub graph: &'a UnifiedGraph,
    pub ng: &'a NamingGraph,
    pub view: &'a TextView<'s>,
    pub occ: &'a Occurrences,
    pub fns: &'a [Option<FnNode>],
    pub rows: &'a Rows,
    pub eligible: &'a Eligibility,
    /// Per function row: `fn.priorVersionTransferred`.
    pub transferred: &'a [HashSet<String>],
    /// Per function row: `fn.priorVersionTransferredPairs`.
    pub transferred_pairs: &'a [Option<Vec<(String, String)>>],
    /// Per function row: the close context (a pending close match).
    pub close: &'a [Option<CloseContext>],
    /// Per module-binding row: `suggestedName`.
    pub suggested: &'a [Option<String>],
    /// `options.bundlerType === "esbuild"` (module groups of 15).
    pub esbuild: bool,
    pub params: CacheKeyParams,
    /// ONE scope epoch: no prior version, so `clearBabelCacheAfterPriorMatch`
    /// never ran — every traversal reuses the graph build's cached paths
    /// and scopes (no fresh-era re-crawl; a context reads the live names).
    pub single_epoch: bool,
    /// `--batch-size` / `--max-retries` / `--max-free-retries` /
    /// `--lane-threshold`.
    pub tunables: WaveTunables,
    /// `--fast [tier]`: any tier pipelines each round's LLM calls (a lane's
    /// follow-up starts as soon as ITS answer lands, not when the round's
    /// slowest does).
    pub fast: crate::fast::FastTier,
}

/// One recorded dispatch (a prompts.jsonl row + its cache-key material).
#[derive(Clone, Debug)]
pub struct DispatchRecord {
    pub seq: u64,
    pub function_id: String,
    pub round: u64,
    pub wave: u64,
    pub request: BatchRenameRequest,
    pub cache_key: String,
    pub system_prompt: String,
    pub user_prompt: String,
    /// (sessionId, span) per target.
    pub targets: Vec<(String, Span)>,
}

/// One recorded name (the dump's `recordName`): the declaration identifier
/// span, old/new names, kind and functionId.
#[derive(Clone, Debug)]
pub struct NameRecord {
    pub span: Span,
    pub old_name: String,
    pub new_name: String,
    pub module: bool,
    pub function_id: String,
}

/// What the waves hand on.
pub struct WaveOutcome {
    pub state: RenameState,
    pub fn_state: Vec<Lifecycle>,
    pub binding_state: Vec<Lifecycle>,
    pub dispatches: Vec<DispatchRecord>,
    pub names: Vec<NameRecord>,
    pub misses: usize,
    pub errors: usize,
    pub waves: u64,
    /// The processor's reports and counters (coverage + diagnostics).
    pub processor: ProcessorReport,
    /// Name strings the function contexts' used-identifier Sets hold at
    /// the end of the run — the memory bound's observable (finding #56).
    pub context_set_names: usize,
}

impl WaveOutcome {
    /// `graph.nodes.size === 0`: the plugin never builds a processor.
    pub fn idle(
        state: RenameState,
        fn_state: Vec<Lifecycle>,
        binding_state: Vec<Lifecycle>,
    ) -> WaveOutcome {
        WaveOutcome {
            state,
            fn_state,
            binding_state,
            dispatches: Vec::new(),
            names: Vec::new(),
            misses: 0,
            errors: 0,
            waves: 0,
            processor: ProcessorReport::default(),
            context_set_names: 0,
        }
    }
}

/// The per-node wave bookkeeping (`WaveNodeCtx`).
struct NodeCtx {
    node_index: usize,
    wave: u64,
    kind: CtxKind,
    /// oldName → binding (grows per registered phase).
    binding_map: HashMap<String, BindingInfo>,
    /// `${phase}:${oldName}` → index within the phase's identifiers.
    order: HashMap<(u8, String), usize>,
    /// Applied renames (the llm-done names map).
    names: JsRecord,
    /// `fn.renameReport` / `ctx.report`: the node's report, patched by
    /// the barrier and the retries until it settles.
    report: Option<RenameReport>,
}

#[derive(Clone)]
enum CtxKind {
    Fn(usize),
    Module(Vec<usize>),
}

/// How an entry reads its barrier-time used names.
#[derive(Clone, Copy)]
enum Live {
    /// A function strategy: that context's used identifiers + usedNames.
    Fn(usize),
    /// A module strategy: usedNames.
    Module,
}

/// What an entry's apply renames.
#[derive(Clone)]
enum ApplyTarget {
    Fn {
        binding: Option<BindingInfo>,
        /// The context set the apply updates.
        set: usize,
    },
    Module {
        mb: Option<usize>,
    },
}

/// One deferred wave entry (`WaveEntry`).
#[derive(Clone)]
struct Entry {
    node_index: usize,
    phase: u8,
    binding_index: usize,
    seq: usize,
    old: String,
    new: String,
    identity: bool,
    suffix_on_reject: bool,
    ctx: usize,
    binding: Option<BindingInfo>,
    target: ApplyTarget,
    live: Live,
    /// A retry entry's previous (collided) suggestion.
    prev_name: Option<String>,
}

/// A barrier rejection seeding a retry.
struct Rejection {
    entry: Entry,
    winner_old: Option<String>,
}

struct RetryItem {
    id: String,
    index: usize,
    prev_name: String,
    target: ApplyTarget,
    binding: Option<BindingInfo>,
}

/// Barrier rejections of one node+phase (`WaveRetrySeed`).
struct RetrySeed {
    ctx: usize,
    phase: u8,
    items: Vec<RetryItem>,
    winners: JsRecord,
}

/// A deferred lifecycle settlement (with the node's wave context).
enum Settle {
    FnDone(usize, usize),
    FnSkipped(usize, &'static str),
    Module(Vec<usize>, usize),
}

/// The request-building strategy of one lane group.
#[derive(Clone)]
enum Strategy {
    Fn {
        f: usize,
        /// Name → binding for this phase's identifiers.
        bindings: Vec<BindingInfo>,
        /// The context built for this phase.
        context: FnContext,
        set: usize,
    },
    Module {
        batch: Vec<usize>,
        windowed: Vec<String>,
    },
}

/// `buildContext`'s output, kept for the phase.
#[derive(Clone)]
struct FnContext {
    callee_signatures: Vec<CalleeSignature>,
    callsites: Vec<String>,
    context_vars: Option<Vec<String>>,
}

/// One lane in a round.
struct LaneRun {
    lane: Lane,
    function_id: String,
    ctx: usize,
    phase: u8,
    strategy: usize,
}

/// A provider result, as the waves receive it.
type LlmResult = Result<humanify_model::llm::BatchRenameResponse, humanify_model::llm::LlmError>;

/// Call outcomes counted while a round runs (commutative: added to the run
/// in one step, whatever the completion order).
#[derive(Default)]
struct Tally {
    completed: usize,
    misses: usize,
    errors: usize,
}

impl Tally {
    /// A provider result as a lane reads it, counted.
    fn map(&mut self, r: LlmResult) -> Result<(Renames, Option<String>), ()> {
        match r {
            Ok(resp) => {
                self.completed += 1;
                Ok((resp.renames, resp.finish_reason))
            }
            Err(e) => {
                if e.kind == LlmErrorKind::CacheMiss {
                    self.misses += 1;
                } else {
                    self.errors += 1;
                }
                Err(())
            }
        }
    }
}

/// A retry task in a round.
struct RetryRun {
    seed: RetrySeed,
    strategy: usize,
    function_id: String,
}

/// The mutable run.
struct Run<'a, 's, 'p, P: NameProvider> {
    inp: &'a WaveInputs<'a, 's>,
    provider: &'p P,
    state: RenameState,
    fn_state: Vec<Lifecycle>,
    binding_state: Vec<Lifecycle>,
    used: JsSet,
    winners: HashMap<String, String>,
    settle: BTreeMap<usize, Settle>,
    ctxs: Vec<NodeCtx>,
    /// Each function context's `usedIdentifiers` Set, over shared layers.
    sets: Vec<UsedSet>,
    /// The latest snapshot of each scope table a context read, with the
    /// table version it was taken at (a context built while the table is
    /// unchanged shares it).
    layers: HashMap<BScopeId, (u64, Arc<NameLayer>)>,
    /// The file's free names (`scope.globals`) — fixed for the run.
    globals_layer: Arc<NameLayer>,
    strategies: Vec<Strategy>,
    entries: Vec<Entry>,
    entry_seq: usize,
    target_scope: BScopeId,
    program_scope: BScopeId,
    /// The graph-era tables of the fresh-era scopes (read by the context
    /// chain; written only by renames made through graph-era objects).
    graph_era: HashMap<BScopeId, JsSet>,
    /// The fresh-era scopes by span (start, end, id), sorted.
    fresh_scopes: Vec<(u32, u32, BScopeId)>,
    wave: u64,
    // dump
    dispatches: Vec<DispatchRecord>,
    rounds: HashMap<String, u64>,
    names: Vec<NameRecord>,
    misses: usize,
    errors: usize,
    // reports
    processor: ProcessorReport,
    /// Function row → its wave context (a function is dispatched once).
    fn_ctx: HashMap<usize, usize>,
    /// `--fast relaxed:defer-shadowed`: the previous wave's round-B lanes,
    /// riding with this wave's round A.
    deferred: Vec<LaneRun>,
}

/// Run the LLM naming waves over the transfer stage's state
/// (`processUnified`).
pub fn run_waves<P: NameProvider>(
    inp: &WaveInputs<'_, '_>,
    provider: &P,
    state: RenameState,
    fn_state: Vec<Lifecycle>,
    binding_state: Vec<Lifecycle>,
) -> WaveOutcome {
    let mut state = state;
    let program_scope = state.view().program_scope();
    // The prior-match cache clear: every scope the waves reach through a
    // NEW path (all but the graph functions' retained `fn.path.scope`s and
    // the program) is a fresh crawl — registration order, current names.
    // Without a prior there was no clear: ONE epoch, nothing is fresh-era.
    let retained: HashSet<BScopeId> = inp.rows.fns.iter().map(|f| f.scope).collect();
    let fresh_era = |s: BScopeId| !inp.single_epoch && s != program_scope && !retained.contains(&s);
    // The RETAINED (graph-era) tables of those scopes, as the clear left
    // them: a function's context walks `fn.path.scope.parent` — graph-era
    // objects — and a rename made through a fresh-era scope object never
    // reaches them (the TS's two scope epochs, exp059).
    let n_scopes = state.view().scopes.len();
    let graph_era: HashMap<BScopeId, JsSet> = (0..n_scopes)
        .map(|i| BScopeId(i as u32))
        .filter(|&sid| fresh_era(sid))
        .map(|sid| {
            let names = state.bindings_in(sid).into_iter().map(|(n, _)| n);
            let mut set = JsSet::new();
            for n in names {
                set.add(n);
            }
            (sid, set)
        })
        .collect();
    let mut fresh_scopes: Vec<(u32, u32, BScopeId)> = (0..n_scopes)
        .map(|i| BScopeId(i as u32))
        .filter(|&sid| fresh_era(sid))
        .map(|sid| {
            let span = state.view().scope(sid).span;
            (span.start, span.end, sid)
        })
        .collect();
    fresh_scopes.sort_unstable();
    state.recrawl_order(fresh_era);
    let target_scope = inp
        .rows
        .modules
        .first()
        .map(|m| m.scope)
        .unwrap_or(program_scope);
    let globals_layer = Arc::new(NameLayer::new(state.view().globals_order.iter().cloned()));
    let mut run = Run {
        inp,
        provider,
        state,
        fn_state,
        binding_state,
        used: JsSet::new(),
        winners: HashMap::new(),
        settle: BTreeMap::new(),
        ctxs: Vec::new(),
        sets: Vec::new(),
        layers: HashMap::new(),
        globals_layer,
        strategies: Vec::new(),
        entries: Vec::new(),
        entry_seq: 0,
        target_scope,
        program_scope,
        graph_era,
        fresh_scopes,
        wave: 0,
        dispatches: Vec::new(),
        rounds: HashMap::new(),
        names: Vec::new(),
        misses: 0,
        errors: 0,
        processor: ProcessorReport::default(),
        fn_ctx: HashMap::new(),
        deferred: Vec::new(),
    };
    run.used = run.module_used_names();
    run.wave_loop();
    // `processUnified`'s tail: after the module reports (pushed at
    // settle), every function's report in graph node order.
    let mut processor = std::mem::take(&mut run.processor);
    for node in &inp.ng.order {
        if let NodeRef::Fn(f) = *node
            && let Some(&ctx) = run.fn_ctx.get(&f)
            && let Some(report) = run.ctxs[ctx].report.take()
        {
            processor.reports.push(report);
        }
    }
    let context_set_names = run.context_set_names();
    WaveOutcome {
        context_set_names,
        state: run.state,
        fn_state: run.fn_state,
        binding_state: run.binding_state,
        dispatches: run.dispatches,
        names: run.names,
        misses: run.misses,
        errors: run.errors,
        waves: run.wave,
        processor,
    }
}

impl<'a, 's, 'p, P: NameProvider> Run<'a, 's, 'p, P> {
    fn context_set_names(&self) -> usize {
        let mut seen: HashSet<*const NameLayer> = HashSet::new();
        let shared: usize = self
            .sets
            .iter()
            .flat_map(UsedSet::layers)
            .filter(|l| seen.insert(Arc::as_ptr(l)))
            .map(|l| l.len())
            .sum();
        shared + self.sets.iter().map(UsedSet::owned_names).sum::<usize>()
    }

    fn printer(&self) -> FnPrinter<'_, 's> {
        FnPrinter {
            semantic: self.inp.semantic,
            view: self.inp.view,
            graph: self.inp.graph,
            state: &self.state,
            occ: self.inp.occ,
            fns: self.inp.fns,
        }
    }

    fn is_eligible(&self, name: &str) -> bool {
        self.inp.eligible.is_eligible(name)
    }

    fn node_settled(&self, node: usize) -> bool {
        match self.inp.ng.order[node] {
            NodeRef::Fn(f) => self.fn_state[f].is_settled(),
            NodeRef::Mb(j) => self.binding_state[j].is_settled(),
        }
    }

    /// `collectModuleUsedNames(targetScope)`.
    fn module_used_names(&self) -> JsSet {
        let mut s = JsSet::new();
        for (name, _) in self.state.bindings_in(self.target_scope) {
            s.add(name);
        }
        for g in &self.state.view().globals_order {
            s.add(g.clone());
        }
        s
    }

    // ------------------------------------------------------------------
    // the wave loop
    // ------------------------------------------------------------------

    fn wave_loop(&mut self) {
        let n = self.inp.ng.order.len();
        let mut done: Vec<bool> = (0..n).map(|i| self.node_settled(i)).collect();
        let mut pending: Vec<usize> = (0..n).filter(|&i| !done[i]).collect();
        let mut seeds: Vec<RetrySeed> = Vec::new();
        while !pending.is_empty() || !seeds.is_empty() || !self.deferred.is_empty() {
            let members = self.wave_members(&pending, &done);
            for &m in &members {
                done[m] = true;
            }
            let member_set: HashSet<usize> = members.iter().copied().collect();
            pending.retain(|p| !member_set.contains(p));
            seeds = self.wave_step(&members, seeds);
            self.settle_nodes(&seeds);
            self.wave += 1;
        }
    }

    /// `computeWaveMembers`: ready nodes in graph order; else ready when
    /// scope-parent edges are ignored; else every pending node.
    fn wave_members(&self, pending: &[usize], done: &[bool]) -> Vec<usize> {
        let ng = self.inp.ng;
        let ready = |id: usize, ignore_sp: bool| {
            ng.deps[id]
                .iter()
                .all(|&d| done[d] || (ignore_sp && ng.scope_parent_edges.contains(&(id, d))))
        };
        let tier0: Vec<usize> = pending
            .iter()
            .copied()
            .filter(|&i| ready(i, false))
            .collect();
        if !tier0.is_empty() {
            return tier0;
        }
        let tier1: Vec<usize> = pending
            .iter()
            .copied()
            .filter(|&i| ready(i, true))
            .collect();
        if !tier1.is_empty() {
            return tier1;
        }
        pending.to_vec()
    }

    /// `settleWaveNodes`: nodes whose work fully resolved, in node order.
    fn settle_nodes(&mut self, live: &[RetrySeed]) {
        // A node settles once ALL its work is done: no live retry seed and
        // no deferred shadowed pass.
        let held: HashSet<usize> = live
            .iter()
            .map(|s| s.ctx)
            .chain(self.deferred.iter().map(|l| l.ctx))
            .map(|ctx| self.ctxs[ctx].node_index)
            .collect();
        let due: Vec<usize> = self
            .settle
            .keys()
            .copied()
            .filter(|k| !held.contains(k))
            .collect();
        for key in due {
            let record = self.settle.remove(&key).expect("due");
            match record {
                Settle::FnDone(f, ctx) => {
                    if let Some(r) = self.ctxs[ctx].report.as_mut() {
                        r.fixup_renamed_count();
                    }
                    let who = self.inp.graph.functions[f].session_id.clone();
                    self.fn_state[f].mark_llm_done(&who);
                }
                Settle::FnSkipped(f, reason) => {
                    let who = self.inp.graph.functions[f].session_id.clone();
                    self.fn_state[f].mark_skipped(reason, &who);
                }
                Settle::Module(batch, ctx) => {
                    if let Some(mut r) = self.ctxs[ctx].report.take() {
                        r.fixup_renamed_count();
                        self.processor.reports.push(r);
                    }
                    for j in batch {
                        if self.binding_state[j].is_pending() {
                            let who = self.inp.graph.module_bindings[j].session_id.clone();
                            self.binding_state[j].mark_llm_done(&who);
                        }
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // one wave step
    // ------------------------------------------------------------------

    fn wave_step(&mut self, members: &[usize], seeds: Vec<RetrySeed>) -> Vec<RetrySeed> {
        let ng = self.inp.ng;
        let mut fn_nodes: Vec<usize> = Vec::new();
        let mut mb_nodes: Vec<usize> = Vec::new();
        for &m in members {
            match ng.order[m] {
                NodeRef::Fn(f) => fn_nodes.push(f),
                NodeRef::Mb(j) => mb_nodes.push(j),
            }
        }
        let groups = self.group_by_proximity(&mb_nodes);
        let setup_phase = crate::profiling::phase("waves:setup");

        // ---- round A ----------------------------------------------------
        // (`defer-shadowed`: the previous wave's round-B lanes first — they
        // are earlier nodes, and their requests read the same state they
        // would have read in their own round.)
        let mut lanes: Vec<LaneRun> = std::mem::take(&mut self.deferred);
        // Functions with a main pass: (fn, ctx, all bindings) — the gate
        // waiters, in node order.
        let mut waiters: Vec<(usize, usize, Vec<BindingInfo>)> = Vec::new();
        for &f in &fn_nodes {
            let ctx = self.new_ctx(ng.node_of_fn[f], CtxKind::Fn(f));
            self.fn_ctx.insert(f, ctx);
            // The task's traversal: a fresh path for every node under
            // the function, so every fresh-era scope inside it is
            // (re)crawled NOW — registration order, current names.
            let ph = crate::profiling::phase("setup:recrawl");
            self.recrawl_inside(self.inp.graph.functions[f].span);
            drop(ph);
            let row = &self.inp.rows.fns[f];
            let ph = crate::profiling::phase("setup:owned-bindings");
            let all = collect_owned_binding_infos(&self.state, row);
            drop(ph);
            match self.select_llm_bindings(f, &all) {
                Err(reason) => {
                    self.settle
                        .insert(ng.node_of_fn[f], Settle::FnSkipped(f, reason));
                }
                Ok(bindings) => {
                    self.start_fn_phase(f, ctx, 0, bindings, &mut lanes);
                    waiters.push((f, ctx, all));
                }
            }
        }
        for group in groups {
            let ctx = self.new_ctx(ng.node_of_mb[group[0]], CtxKind::Module(group.clone()));
            self.start_module(ctx, group.clone(), &mut lanes);
            self.settle
                .insert(ng.node_of_mb[group[0]], Settle::Module(group, ctx));
        }
        let retries: Vec<RetryRun> = seeds
            .into_iter()
            .map(|seed| self.start_retry(seed))
            .collect();
        drop(setup_phase);
        self.drive_round(lanes, retries);
        let mut rejections = self.barrier();

        // ---- the gate release: shadowed bindings, in node order ---------
        let gate_phase = crate::profiling::phase("waves:gate-release");
        let mut lanes: Vec<LaneRun> = Vec::new();
        waiters.sort_by_key(|(_, ctx, _)| self.ctxs[*ctx].node_index);
        for (f, ctx, all) in waiters {
            let shadowed = self.compute_shadowed_uniquified(f, &all);
            if !shadowed.is_empty() {
                self.start_fn_phase(f, ctx, 1, shadowed, &mut lanes);
            }
            self.settle
                .insert(self.ctxs[ctx].node_index, Settle::FnDone(f, ctx));
        }
        drop(gate_phase);
        // ---- round B -----------------------------------------------------
        if self.inp.fast.lever(Lever::DeferShadowed) {
            self.deferred = lanes;
        } else {
            self.drive_round(lanes, Vec::new());
            rejections.extend(self.barrier());
        }
        self.build_retry_seeds(rejections)
    }

    fn new_ctx(&mut self, node_index: usize, kind: CtxKind) -> usize {
        self.ctxs.push(NodeCtx {
            node_index,
            wave: self.wave,
            kind,
            binding_map: HashMap::new(),
            order: HashMap::new(),
            names: JsRecord::default(),
            report: None,
        });
        self.ctxs.len() - 1
    }

    /// The references and writes of the binding `scope.getBinding(old)`
    /// resolves, each counted once (finding #35: the TS's two scope epochs
    /// re-registered a retained binding's references inside every fresh-era
    /// crawl, so the count read up to 2x; the diagnostics count the binding,
    /// not the crawls).
    fn ref_count(&self, b: BindingId) -> u32 {
        let bb = self.state.view().binding(b);
        (bb.refs.len() + bb.violations.len()) as u32
    }

    /// Re-crawl every fresh-era scope inside `span`.
    fn recrawl_inside(&mut self, span: Span) {
        let lo = self.fresh_scopes.partition_point(|s| s.0 < span.start);
        let scopes: Vec<BScopeId> = self.fresh_scopes[lo..]
            .iter()
            .take_while(|s| s.0 < span.end)
            .filter(|s| s.1 <= span.end)
            .map(|s| s.2)
            .collect();
        self.state.recrawl_scopes(&scopes);
    }

    /// `selectLlmBindings`.
    fn select_llm_bindings(
        &mut self,
        f: usize,
        all: &[BindingInfo],
    ) -> Result<Vec<BindingInfo>, &'static str> {
        if all.is_empty() {
            self.processor.skip_reasons.zero_bindings += 1;
            return Err("zero-bindings");
        }
        let transferred = &self.inp.transferred[f];
        let bindings: Vec<BindingInfo> = all
            .iter()
            .filter(|b| self.is_eligible(&b.name) && !transferred.contains(&b.name))
            .cloned()
            .collect();
        self.processor.skipped_by_skip_list += all.len() - bindings.len();
        if bindings.is_empty() {
            self.processor.skip_reasons.all_preserved += 1;
            return Err("all-preserved");
        }
        Ok(bindings)
    }

    /// `groupByProximity(mbNodes, 50, max)`.
    fn group_by_proximity(&self, mbs: &[usize]) -> Vec<Vec<usize>> {
        let max = if self.inp.esbuild { 15 } else { 10 };
        let line = |j: usize| self.inp.ng.mb_text[j].declaration_line;
        let mut sorted = mbs.to_vec();
        sorted.sort_by_key(|&j| line(j));
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut current: Vec<usize> = Vec::new();
        for j in sorted {
            if current.is_empty() || (line(j) - line(current[0]) <= 100 && current.len() < max) {
                current.push(j);
            } else {
                groups.push(std::mem::take(&mut current));
                current.push(j);
            }
        }
        if !current.is_empty() {
            groups.push(current);
        }
        groups
    }

    // ------------------------------------------------------------------
    // function passes
    // ------------------------------------------------------------------

    /// `processFunctionBatched` up to dispatch: build the context,
    /// register the phase, make the lanes.
    fn start_fn_phase(
        &mut self,
        f: usize,
        ctx: usize,
        phase: u8,
        bindings: Vec<BindingInfo>,
        lanes: &mut Vec<LaneRun>,
    ) {
        let (context, set) = self.build_context(f);
        for (i, b) in bindings.iter().enumerate() {
            self.ctxs[ctx].order.insert((phase, b.name.clone()), i);
            self.ctxs[ctx].binding_map.insert(b.name.clone(), b.clone());
        }
        let names: Vec<String> = bindings.iter().map(|b| b.name.clone()).collect();
        self.strategies.push(Strategy::Fn {
            f,
            bindings,
            context,
            set,
        });
        let strategy = self.strategies.len() - 1;
        let session = self.inp.graph.functions[f].session_id.clone();
        let batch = self.inp.tunables.batch_size.max(1);
        let n_lanes = if self.inp.fast.lever(Lever::WindowLanes) && names.len() > batch {
            names.len().div_ceil(batch)
        } else {
            compute_lane_count(names.len(), self.inp.tunables.lane_threshold)
        };
        if n_lanes > 0 {
            for (i, lane) in split_by_position(&names, n_lanes).into_iter().enumerate() {
                lanes.push(LaneRun {
                    lane: Lane::new(lane, true).tuned(&self.inp.tunables),
                    function_id: format!("{session}:lane{i}"),
                    ctx,
                    phase,
                    strategy,
                });
            }
        } else {
            lanes.push(LaneRun {
                lane: Lane::new(names, true).tuned(&self.inp.tunables),
                function_id: session,
                ctx,
                phase,
                strategy,
            });
        }
    }

    /// `buildContext(fn)` at the current state, plus its used-identifiers
    /// Set registered for barrier-time reads.
    fn build_context(&mut self, f: usize) -> (FnContext, usize) {
        let ph = crate::profiling::phase("setup:context-view");
        let view = self.context_view(f);
        drop(ph);
        let _ph = crate::profiling::phase("setup:build-context");
        let eligible = self.inp.eligible;
        let ctx = build_context(&view, &self.inp.ng.fn_call_sites[f], |n: &str| {
            eligible.is_eligible(n)
        });
        let layers = self.used_layers(f);
        self.sets.push(UsedSet::new(layers));
        (
            FnContext {
                callee_signatures: ctx.callee_signatures,
                callsites: ctx.callsites,
                context_vars: ctx.context_vars,
            },
            self.sets.len() - 1,
        )
    }

    fn context_view(&self, f: usize) -> ContextView {
        let printer = self.printer();
        let callees = self.inp.ng.fn_callees[f]
            .iter()
            .map(|&c| printer.callee_view(c))
            .collect();
        let parent_bindings = self.inp.ng.fn_scope_parent[f]
            .filter(|&p| self.fn_state[p].is_pending())
            .map(|p| {
                self.state
                    .bindings_in(self.inp.rows.fns[p].scope)
                    .into_iter()
                    .map(|(name, b)| {
                        let decl = if self.is_eligible(&name) {
                            printer.decl_view(b)
                        } else {
                            DeclView::FunctionOrClass
                        };
                        ParentBinding { name, decl }
                    })
                    .collect()
            });
        // The used-identifier names are layered in the context's UsedSet
        // (`used_layers`), shared across contexts — not copied here.
        ContextView {
            callees,
            scope_chain: Vec::new(),
            program_bindings: Vec::new(),
            program_globals: Vec::new(),
            parent_bindings,
        }
    }

    /// The `usedIdentifiers` layers of function `f`'s context: each
    /// non-program scope from its own outward (a graph-era scope reads its
    /// retained table), the program's bindings, the file's free names.
    fn used_layers(&mut self, f: usize) -> Vec<Arc<NameLayer>> {
        let mut layers = Vec::new();
        let mut cur = Some(self.inp.rows.fns[f].scope);
        while let Some(s) = cur {
            if s == self.program_scope {
                break;
            }
            layers.push(match self.graph_era.get(&s) {
                Some(era) => Arc::new(NameLayer::new(era.to_vec())),
                None => self.table_layer(s),
            });
            cur = self.state.view().scope(s).parent;
        }
        layers.push(self.table_layer(self.program_scope));
        layers.push(self.globals_layer.clone());
        layers
    }

    /// The current names of `scope`'s table, as a snapshot shared while
    /// the table is unchanged.
    fn table_layer(&mut self, scope: BScopeId) -> Arc<NameLayer> {
        let version = self.state.table_version(scope);
        if let Some((v, layer)) = self.layers.get(&scope)
            && *v == version
        {
            return layer.clone();
        }
        let names = self.state.bindings_in(scope).into_iter().map(|(n, _)| n);
        let layer = Arc::new(NameLayer::new(names));
        self.layers.insert(scope, (version, layer.clone()));
        layer
    }

    /// `computeShadowedUniquified` (inside the barrier: it renames).
    fn compute_shadowed_uniquified(&mut self, f: usize, all: &[BindingInfo]) -> Vec<BindingInfo> {
        let phase0: HashSet<BindingId> = all.iter().map(|b| b.binding).collect();
        let eligible = self.inp.eligible;
        let shadowed: Vec<BindingInfo> =
            collect_shadowed_block_bindings(&self.state, &self.inp.rows.fns[f], |n: &str| {
                eligible.is_eligible(n)
            })
            .into_iter()
            .filter(|b| !phase0.contains(&b.binding))
            .collect();
        if shadowed.is_empty() {
            return shadowed;
        }
        let session = self.inp.graph.functions[f].session_id.clone();
        let mut seen: HashMap<String, usize> = HashMap::new();
        shadowed
            .into_iter()
            .map(|b| {
                let count = seen.get(&b.name).copied().unwrap_or(0) + 1;
                seen.insert(b.name.clone(), count);
                if count == 1 {
                    return b;
                }
                self.apply_uniquify(&b, count, &session).unwrap_or(b)
            })
            .collect()
    }

    /// `applyUniquifyRename`: `<name>_<k>`, bumping past name collisions.
    fn apply_uniquify(
        &mut self,
        b: &BindingInfo,
        ordinal: usize,
        fn_id: &str,
    ) -> Option<BindingInfo> {
        for suffix in ordinal..ordinal + 20 {
            let candidate = format!("{}_{suffix}", b.name);
            let attempt = self.state.attempt_validated_rename(
                RenameRequest {
                    scope: b.scope,
                    old_name: &b.name,
                    new_name: &candidate,
                    expected: None,
                },
                TrailSpec::Untrailed { why: "uniquify" },
            );
            if attempt.applied {
                let span = self.state.view().binding(b.binding).id_span;
                self.names.push(NameRecord {
                    span,
                    old_name: b.name.clone(),
                    new_name: candidate.clone(),
                    module: false,
                    function_id: fn_id.to_string(),
                });
                return Some(BindingInfo {
                    name: candidate,
                    binding: b.binding,
                    scope: b.scope,
                });
            }
            if !matches!(
                attempt.reason,
                Some(
                    RejectionReason::TargetInScope
                        | RejectionReason::TargetVisible
                        | RejectionReason::ShadowsChild
                )
            ) {
                return None;
            }
        }
        None
    }

    /// The function strategy's first-round / retry request
    /// (`buildFunctionCallbacks.buildRequest`).
    fn fn_request(&self, strategy: usize, ctx: usize, call: &LaneCall) -> BatchRenameRequest {
        let Strategy::Fn {
            f,
            bindings,
            context,
            set,
        } = &self.strategies[strategy]
        else {
            unreachable!("a function strategy");
        };
        let f = *f;
        let binding_map: HashMap<&str, &BindingInfo> =
            bindings.iter().map(|b| (b.name.as_str(), b)).collect();
        let remaining = &call.batch;
        let is_retry = call.round > 1;
        let full_code = self.select_request_code(f, remaining, &binding_map);
        let close = self.inp.close[f].as_ref();
        let prior_context = close
            .filter(|c| !c.prior_code.is_empty())
            .map(|c| cap_context_code(&c.prior_code, &self.inp.graph.functions[f].session_id));
        let code = if is_retry {
            extract_retry_snippet(&full_code, remaining)
        } else {
            full_code
        };
        let windowed = self.windowed_used_names(f, remaining, &binding_map, &self.sets[*set]);
        let used_for_prompt = if is_retry {
            build_retry_used_names(&windowed, &call.prev)
        } else {
            windowed
        };
        let already = self.already_renamed(f, ctx, is_retry);
        let prev = StrMap(call.prev.0.clone());
        let prompt_body = is_retry.then(|| {
            build_batch_rename_retry_body(&RetryInput {
                code: &code,
                identifiers: remaining,
                used_names: &used_for_prompt,
                previous_attempt: &prev,
                failures: &call.failures,
                prior_version_code: prior_context.as_deref(),
                already_renamed: already.as_ref(),
            })
        });
        BatchRenameRequest {
            code,
            identifiers: remaining.clone(),
            used_names: used_for_prompt,
            callee_signatures: context.callee_signatures.clone(),
            callsites: context.callsites.clone(),
            is_retry: Some(is_retry),
            previous_attempt: is_retry.then(|| prev.clone()),
            failures: is_retry.then(|| call.failures.clone()),
            system_prompt: None,
            user_prompt: None,
            prompt_body,
            context_vars: context.context_vars.clone(),
            prior_version_code: prior_context,
            prior_version_names: close.map(|c| c.prior_names.clone()),
            already_renamed: already,
            prior_name_hints: close.and_then(|c| c.hints.clone()),
        }
    }

    /// `selectRequestCode`.
    fn select_request_code(
        &self,
        f: usize,
        remaining: &[String],
        binding_map: &HashMap<&str, &BindingInfo>,
    ) -> String {
        let span = self.inp.graph.functions[f].span;
        let code = self.printer().function_code(f);
        let anchors: Vec<Option<i64>> = remaining
            .iter()
            .map(|n| {
                binding_map
                    .get(n.as_str())
                    .map(|b| i64::from(self.decl_line(b.binding)))
            })
            .collect();
        select_function_code(&FunctionCodeSelection {
            code: &code,
            session_id: &self.inp.graph.functions[f].session_id,
            fn_start_line: Some(i64::from(self.inp.view.line_of(span.start))),
            fn_end_line: Some(i64::from(self.inp.view.line_of(span.end))),
            anchor_start_lines: Some(&anchors),
            identifier_names: Some(remaining),
        })
    }

    fn decl_line(&self, b: BindingId) -> u32 {
        self.inp
            .view
            .line_of(self.state.view().binding(b).id_span.start)
    }

    /// The proximity view of a binding (`identifier.loc` + reference lines).
    fn proximity_binding(&self, b: BindingId) -> ProximityBinding {
        let bb = self.state.view().binding(b);
        ProximityBinding {
            decl_line: Some(self.inp.view.line_of(bb.id_span.start)),
            ref_lines: bb
                .refs
                .iter()
                .map(|r| self.inp.view.line_of(r.span.start))
                .collect(),
        }
    }

    /// `computeWindowedUsedNames`.
    fn windowed_used_names(
        &self,
        f: usize,
        remaining: &[String],
        binding_map: &HashMap<&str, &BindingInfo>,
        set: &UsedSet,
    ) -> Vec<String> {
        let batch_lines: Vec<u32> = remaining
            .iter()
            .filter_map(|n| {
                binding_map
                    .get(n.as_str())
                    .map(|b| self.decl_line(b.binding))
            })
            .collect();
        if batch_lines.is_empty() {
            return set.order().map(str::to_string).collect();
        }
        let scope = self.inp.rows.fns[f].scope;
        let total = self.state.bindings_in(scope).len();
        let order: Vec<&str> = set.order().collect();
        get_proximate_used_names(
            &order,
            &batch_lines,
            |name| {
                self.state
                    .binding_in(scope, name)
                    .map(|b| self.proximity_binding(b))
            },
            total,
            |n| self.is_eligible(n),
        )
    }

    /// `computeAlreadyRenamed`.
    fn already_renamed(&self, f: usize, ctx: usize, is_retry: bool) -> Option<StrMap> {
        let mut out: Option<JsRecord> = None;
        if let Some(pairs) = &self.inp.transferred_pairs[f]
            && !pairs.is_empty()
        {
            let mut r = JsRecord::default();
            for (k, v) in pairs {
                r.set(k, v);
            }
            out = Some(r);
        }
        let names = &self.ctxs[ctx].names;
        if is_retry && !names.is_empty() {
            let mut r = out.unwrap_or_default();
            r.spread(names);
            out = Some(r);
        }
        out.map(|r| StrMap(r.0))
    }

    /// The function strategy's `transformSuggestion`, when it has one.
    fn fn_transform(&self, f: usize) -> Option<Box<Transform<'_>>> {
        let close = self.inp.close[f].as_ref()?;
        let index = if close.prior_names.is_empty() {
            HashMap::new()
        } else {
            build_prior_stem_index(&close.prior_names)
        };
        let snaps = close.snaps.clone();
        if index.is_empty() && snaps.is_none() {
            return None;
        }
        Some(Box::new(move |old: &str, suggestion: &str| {
            snap_suggestion_to_prior(suggestion, &index, Some(old), snaps.as_deref())
        }))
    }

    // ------------------------------------------------------------------
    // module-binding groups
    // ------------------------------------------------------------------

    fn module_function_id(&self, batch: &[usize]) -> String {
        let names: Vec<&str> = batch
            .iter()
            .map(|&j| self.inp.graph.module_bindings[j].name.as_str())
            .collect();
        format!("module-binding-batch:{}", names.join(","))
    }

    /// `buildModuleBindingBatchCallbacks`: the windowed names are computed
    /// ONCE, at construction, from the live usedNames.
    fn module_strategy(&mut self, batch: &[usize]) -> usize {
        let lines: Vec<u32> = batch
            .iter()
            .map(|&j| self.inp.ng.mb_text[j].declaration_line)
            .collect();
        let total = self.state.bindings_in(self.target_scope).len();
        let used = self.used.to_vec();
        let target = self.target_scope;
        let windowed = get_proximate_used_names(
            &used,
            &lines,
            |name| {
                self.state
                    .binding_in(target, name)
                    .map(|b| self.proximity_binding(b))
            },
            total,
            |n| self.is_eligible(n),
        );
        self.strategies.push(Strategy::Module {
            batch: batch.to_vec(),
            windowed,
        });
        self.strategies.len() - 1
    }

    fn start_module(&mut self, ctx: usize, batch: Vec<usize>, lanes: &mut Vec<LaneRun>) {
        for (i, &j) in batch.iter().enumerate() {
            let name = self.inp.graph.module_bindings[j].name.clone();
            self.ctxs[ctx].order.insert((0, name), i);
        }
        let strategy = self.module_strategy(&batch);
        let names: Vec<String> = batch
            .iter()
            .map(|&j| self.inp.graph.module_bindings[j].name.clone())
            .collect();
        lanes.push(LaneRun {
            lane: Lane::new(names, false).tuned(&self.inp.tunables),
            function_id: self.module_function_id(&batch),
            ctx,
            phase: 0,
            strategy,
        });
    }

    /// The module strategy's request.
    fn module_request(&self, strategy: usize, call: &LaneCall) -> BatchRenameRequest {
        let Strategy::Module { batch, windowed } = &self.strategies[strategy] else {
            unreachable!("a module strategy");
        };
        let graph = self.inp.graph;
        let by_name: HashMap<&str, usize> = batch
            .iter()
            .map(|&j| (graph.module_bindings[j].name.as_str(), j))
            .collect();
        let remaining = &call.batch;
        let mut declarations: Vec<String> = Vec::new();
        for id in remaining {
            if let Some(&j) = by_name.get(id.as_str()) {
                let d = &self.inp.ng.mb_text[j].declaration;
                if !declarations.contains(d) {
                    declarations.push(d.clone());
                }
            }
        }
        let is_retry = call.round > 1;
        let prompt_names = if is_retry {
            build_retry_used_names(windowed, &call.prev)
        } else {
            windowed.clone()
        };
        let mut assignment = Vec::new();
        let mut usage = Vec::new();
        let mut suggested = Vec::new();
        for &j in batch {
            let name = graph.module_bindings[j].name.clone();
            let t = &self.inp.ng.mb_text[j];
            assignment.push((name.clone(), t.assignments.clone()));
            usage.push((name.clone(), t.usages.clone()));
            if let Some(s) = &self.inp.suggested[j]
                && !s.is_empty()
            {
                suggested.push((name, s.clone()));
            }
        }
        let input = ModuleLevelInput {
            declarations,
            assignment_context: ArrayRecord(dedup_record(assignment)),
            usage_examples: ArrayRecord(dedup_record(usage)),
            identifiers: remaining.clone(),
            used_names: prompt_names.clone(),
            suggested_names: Some(StrMap(dedup_record(suggested))),
        };
        let eligible = |n: &str| self.is_eligible(n);
        let mut user = build_module_level_rename_prompt(&input, eligible);
        let prev = StrMap(call.prev.0.clone());
        let mut body = None;
        if is_retry {
            let prefix = build_module_level_retry_prefix(&prev, &call.failures);
            user = format!("{prefix}\n{user}");
            body = Some(format!(
                "{prefix}\n{}",
                build_module_level_rename_body(&input, eligible)
            ));
        }
        BatchRenameRequest {
            code: String::new(),
            identifiers: remaining.clone(),
            used_names: prompt_names,
            callee_signatures: Vec::new(),
            callsites: Vec::new(),
            is_retry: Some(is_retry),
            previous_attempt: is_retry.then_some(prev),
            failures: is_retry.then(|| call.failures.clone()),
            system_prompt: Some(MODULE_LEVEL_RENAME_SYSTEM_PROMPT.to_string()),
            user_prompt: Some(user),
            prompt_body: body,
            ..BatchRenameRequest::default()
        }
    }

    /// The module strategy's `transformSuggestion`.
    fn module_transform(&self, strategy: usize) -> Box<Transform<'_>> {
        let Strategy::Module { batch, .. } = &self.strategies[strategy] else {
            unreachable!("a module strategy");
        };
        let suggested: HashMap<String, String> = batch
            .iter()
            .filter_map(|&j| {
                self.inp.suggested[j]
                    .clone()
                    .map(|s| (self.inp.graph.module_bindings[j].name.clone(), s))
            })
            .collect();
        Box::new(
            move |old: &str, suggestion: &str| match suggested.get(old) {
                Some(prior) if !prior.is_empty() && prior != suggestion => {
                    snap_to_known_prior(prior, suggestion)
                }
                _ => suggestion.to_string(),
            },
        )
    }

    // ------------------------------------------------------------------
    // retries
    // ------------------------------------------------------------------

    /// `buildWaveRetryCallbacks` (at round-A time of the next step).
    fn start_retry(&mut self, seed: RetrySeed) -> RetryRun {
        let ctx = seed.ctx;
        match self.ctxs[ctx].kind.clone() {
            CtxKind::Module(batch) => {
                let strategy = self.module_strategy(&batch);
                RetryRun {
                    function_id: self.module_function_id(&batch),
                    seed,
                    strategy,
                }
            }
            CtxKind::Fn(f) => {
                let (context, set) = self.build_context(f);
                let bindings: Vec<BindingInfo> = seed
                    .items
                    .iter()
                    .filter_map(|i| i.binding.clone())
                    .collect();
                self.strategies.push(Strategy::Fn {
                    f,
                    bindings,
                    context,
                    set,
                });
                RetryRun {
                    function_id: self.inp.graph.functions[f].session_id.clone(),
                    seed,
                    strategy: self.strategies.len() - 1,
                }
            }
        }
    }

    /// `executeWaveRetry`'s request.
    fn retry_request(&self, r: &RetryRun) -> BatchRenameRequest {
        let ids: Vec<String> = r.seed.items.iter().map(|i| i.id.clone()).collect();
        let mut prev = JsRecord::default();
        for item in &r.seed.items {
            prev.set(&item.id, &item.prev_name);
        }
        let call = LaneCall {
            batch: ids.clone(),
            round: 2,
            prev,
            failures: RenameFailures {
                duplicates: ids,
                ..RenameFailures::default()
            },
        };
        let mut request = match &self.strategies[r.strategy] {
            Strategy::Fn { .. } => self.fn_request(r.strategy, r.seed.ctx, &call),
            Strategy::Module { .. } => self.module_request(r.strategy, &call),
        };
        let mut already = JsRecord(
            request
                .already_renamed
                .take()
                .map(|m| m.0)
                .unwrap_or_default(),
        );
        already.spread(&r.seed.winners);
        request.already_renamed = Some(StrMap(already.0));
        request.prompt_body = None;
        request
    }

    /// `collectWaveRetryEntries` from the retry's response.
    fn collect_retry_entries(&mut self, r: &RetryRun, renames: &Renames) {
        let transform: Option<Box<Transform<'_>>> = match &self.strategies[r.strategy] {
            Strategy::Fn { f, .. } => self.fn_transform(*f),
            Strategy::Module { .. } => Some(self.module_transform(r.strategy)),
        };
        let mut picked: Vec<String> = Vec::new();
        for item in &r.seed.items {
            let raw = renames.get(&item.id);
            let transformed = raw.map(|raw| match &transform {
                Some(t) => t(&item.id, raw),
                None => raw.to_string(),
            });
            let candidate = match transformed {
                Some(t) if !t.is_empty() && t != item.id && is_valid_rename_target(&t) => t,
                _ => item.prev_name.clone(),
            };
            picked.push(candidate);
        }
        drop(transform);
        let live = match &self.strategies[r.strategy] {
            Strategy::Fn { set, .. } => Live::Fn(*set),
            Strategy::Module { .. } => Live::Module,
        };
        let node_index = self.ctxs[r.seed.ctx].node_index;
        for (item, candidate) in r.seed.items.iter().zip(picked) {
            let seq = self.next_seq();
            self.entries.push(Entry {
                node_index,
                phase: r.seed.phase,
                binding_index: item.index,
                seq,
                old: item.id.clone(),
                new: candidate,
                identity: false,
                suffix_on_reject: true,
                ctx: r.seed.ctx,
                binding: item.binding.clone(),
                target: item.target.clone(),
                live,
                prev_name: Some(item.prev_name.clone()),
            });
        }
    }

    /// `buildWaveRetrySeeds`: per (node, phase), in rejection order.
    fn build_retry_seeds(&self, rejections: Vec<Rejection>) -> Vec<RetrySeed> {
        let mut seeds: Vec<RetrySeed> = Vec::new();
        let mut index: HashMap<(usize, u8), usize> = HashMap::new();
        for Rejection { entry, winner_old } in rejections {
            let key = (entry.ctx, entry.phase);
            let i = *index.entry(key).or_insert_with(|| {
                seeds.push(RetrySeed {
                    ctx: entry.ctx,
                    phase: entry.phase,
                    items: Vec::new(),
                    winners: JsRecord::default(),
                });
                seeds.len() - 1
            });
            if let Some(w) = &winner_old {
                seeds[i].winners.set(w, &entry.new);
            }
            seeds[i].items.push(RetryItem {
                id: entry.old,
                index: entry.binding_index,
                prev_name: entry.new,
                target: entry.target,
                binding: entry.binding,
            });
        }
        seeds
    }

    // ------------------------------------------------------------------
    // driving a round
    // ------------------------------------------------------------------

    fn next_seq(&mut self) -> usize {
        self.entry_seq += 1;
        self.entry_seq - 1
    }

    /// Run every lane and retry of a round to completion, dispatching one
    /// call per active lane per turn (a lane's sequence is independent of
    /// the others: its reads are frozen for the round).
    fn drive_round(&mut self, lanes: Vec<LaneRun>, retries: Vec<RetryRun>) {
        if self.inp.fast.on() {
            self.drive_round_pipelined(lanes, retries);
        } else {
            self.drive_round_turns(lanes, retries);
        }
    }

    /// The parity-faithful driver: turn by turn, every active lane's next
    /// call dispatched together and the turn waiting for its slowest.
    fn drive_round_turns(&mut self, mut lanes: Vec<LaneRun>, retries: Vec<RetryRun>) {
        self.run_retries(retries);
        loop {
            let mut active: Vec<(usize, LaneCall)> = Vec::new();
            for (i, lr) in lanes.iter_mut().enumerate() {
                if lr.lane.is_finished() {
                    continue;
                }
                if let Some(call) = lr.lane.next_call() {
                    active.push((i, call));
                }
            }
            // Lanes with no further call run their tail now.
            let calling: HashSet<usize> = active.iter().map(|(a, _)| *a).collect();
            for (i, lr) in lanes.iter_mut().enumerate() {
                if !lr.lane.is_finished() && !calling.contains(&i) {
                    self.finish_lane(lr);
                    // Contention events land as the lanes finish.
                    let events = std::mem::take(&mut lr.lane.report.contention);
                    self.processor.contention.extend(events);
                }
            }
            if active.is_empty() {
                break;
            }
            let requests: Vec<BatchRenameRequest> = active
                .iter()
                .map(|(i, call)| self.lane_request(&lanes[*i], call))
                .collect();
            let targets: Vec<(usize, String)> = active
                .iter()
                .map(|(i, _)| (lanes[*i].ctx, lanes[*i].function_id.clone()))
                .collect();
            let results = self.dispatch(requests, &targets);
            for ((i, _), res) in active.into_iter().zip(results) {
                self.feed_lane(&mut lanes[i], res);
            }
        }
        self.collect_lanes(lanes);
    }

    /// `--fast`: the same round, PIPELINED. Every retry and every lane's
    /// first call start at once; a lane's next call starts as soon as ITS
    /// answer lands. Decision-neutral by construction — a lane reads only
    /// the round's frozen state and its own claims (`batch.rs`), so its
    /// call sequence does not depend on completion order — and everything
    /// order-sensitive is put back into the turn driver's canonical order
    /// before it is recorded: dispatch records by (turn, lane) with the
    /// retries first, contention events by (finishing turn, lane), retry
    /// entries in retry order ahead of the lanes' effects.
    fn drive_round_pipelined(&mut self, mut lanes: Vec<LaneRun>, retries: Vec<RetryRun>) {
        let n_retries = retries.len();
        // (canonical order key, record): retries are turn 0, a lane's k-th
        // call is turn k + 1.
        let mut records: Vec<((usize, usize), DispatchRecord)> = Vec::new();
        let mut initial: Vec<(usize, LlmCall)> = Vec::new();
        for (r, run) in retries.iter().enumerate() {
            let request = self.retry_request(run);
            let (record, call) = self.prepare_dispatch(request, run.seed.ctx, &run.function_id);
            records.push(((0, r), record));
            initial.push((r, call));
        }
        let mut turns = vec![0usize; lanes.len()];
        let mut finished: Vec<(usize, usize, Vec<ContentionEvent>)> = Vec::new();
        for (i, lr) in lanes.iter_mut().enumerate() {
            if let Some((record, call)) = self.step_lane(lr, 0, i, &mut finished) {
                records.push(((1, i), record));
                initial.push((n_retries + i, call));
            }
        }
        let mut retry_results: Vec<Option<LlmResult>> = (0..n_retries).map(|_| None).collect();
        let mut tally = Tally::default();
        {
            let this: &Self = self;
            let provider = this.provider;
            let ph = crate::profiling::phase("waves:llm-pipelined");
            provider.run_pipelined(initial, &mut |id, result| {
                if id < n_retries {
                    retry_results[id] = Some(result);
                    return Vec::new();
                }
                let i = id - n_retries;
                let mapped = tally.map(result);
                this.feed_lane(&mut lanes[i], mapped);
                turns[i] += 1;
                match this.step_lane(&mut lanes[i], turns[i], i, &mut finished) {
                    Some((record, call)) => {
                        records.push(((turns[i] + 1, i), record));
                        vec![(id, call)]
                    }
                    None => Vec::new(),
                }
            });
            // The round's call structure (for the cold-run LLM model):
            // retries are single calls; each lane is a chain.
            if let Some(mut ph) = ph {
                ph.note("retries", n_retries);
                ph.note("chains", turns.clone());
            }
        }
        records.sort_by_key(|(key, _)| *key);
        for (_, record) in records {
            self.commit_record(record);
        }
        finished.sort_by_key(|(turn, lane, _)| (*turn, *lane));
        for (_, _, events) in finished {
            self.processor.contention.extend(events);
        }
        for (run, result) in retries.iter().zip(retry_results) {
            let result = result.expect("every retry call completes");
            let renames = match tally.map(result) {
                Ok((renames, finish)) => {
                    if let Some(report) = self.ctxs[run.seed.ctx].report.as_mut() {
                        report.bump_retry_call(finish);
                    }
                    renames
                }
                Err(()) => Renames::default(),
            };
            self.collect_retry_entries(run, &renames);
        }
        self.apply_tally(tally);
        self.collect_lanes(lanes);
    }

    /// One lane's next move at `turn` (reads frozen state only): its next
    /// request, prepared for dispatch — or, when it has none, its tail
    /// (`finish_lane`), with the contention events it raised filed under
    /// (turn, lane) for the canonical order.
    fn step_lane(
        &self,
        lr: &mut LaneRun,
        turn: usize,
        lane: usize,
        finished: &mut Vec<(usize, usize, Vec<ContentionEvent>)>,
    ) -> Option<(DispatchRecord, LlmCall)> {
        if lr.lane.is_finished() {
            return None;
        }
        match lr.lane.next_call() {
            Some(call) => {
                let request = self.lane_request(lr, &call);
                Some(self.prepare_dispatch(request, lr.ctx, &lr.function_id))
            }
            None => {
                self.finish_lane(lr);
                let events = std::mem::take(&mut lr.lane.report.contention);
                finished.push((turn, lane, events));
                None
            }
        }
    }

    /// A lane call's request, by strategy.
    fn lane_request(&self, lr: &LaneRun, call: &LaneCall) -> BatchRenameRequest {
        match self.strategies[lr.strategy] {
            Strategy::Fn { .. } => self.fn_request(lr.strategy, lr.ctx, call),
            Strategy::Module { .. } => self.module_request(lr.strategy, call),
        }
    }

    /// `processBatch`: one report per (node, phase) over its lanes, in
    /// lane order, attached before the barrier patches it; then each
    /// lane's effects become barrier entries.
    fn collect_lanes(&mut self, lanes: Vec<LaneRun>) {
        let mut group: Option<(usize, u8, usize, Vec<LaneReport>)> = None;
        for mut lr in lanes {
            let lane_report = std::mem::take(&mut lr.lane.report);
            let key = (lr.ctx, lr.phase, lr.strategy);
            match &mut group {
                Some((c, p, s, reps)) if (*c, *p, *s) == key => reps.push(lane_report),
                _ => {
                    if let Some((c, _, s, reps)) = group.take() {
                        self.attach_report(c, s, reps);
                    }
                    group = Some((key.0, key.1, key.2, vec![lane_report]));
                }
            }
            self.collect_lane_effects(lr);
        }
        if let Some((c, _, s, reps)) = group {
            self.attach_report(c, s, reps);
        }
    }

    /// The barrier retries of a round: one call each.
    fn run_retries(&mut self, retries: Vec<RetryRun>) {
        if !retries.is_empty() {
            let requests: Vec<BatchRenameRequest> =
                retries.iter().map(|r| self.retry_request(r)).collect();
            let targets: Vec<(usize, String)> = retries
                .iter()
                .map(|r| (r.seed.ctx, r.function_id.clone()))
                .collect();
            let results = self.dispatch(requests, &targets);
            for (r, res) in retries.iter().zip(results) {
                let renames = match res {
                    Ok((renames, finish)) => {
                        if let Some(report) = self.ctxs[r.seed.ctx].report.as_mut() {
                            report.bump_retry_call(finish);
                        }
                        renames
                    }
                    Err(()) => Renames::default(),
                };
                self.collect_retry_entries(r, &renames);
            }
        }
    }

    /// `processBatch`'s report over one phase's lanes, then
    /// `fn.renameReport = merge(...)` / `ctx.report = report`.
    fn attach_report(&mut self, ctx: usize, strategy: usize, lanes: Vec<LaneReport>) {
        let (ty, target_id, identifiers, hash) = match &self.strategies[strategy] {
            Strategy::Fn { f, bindings, .. } => {
                let row = &self.inp.graph.functions[*f];
                (
                    ReportType::Function,
                    row.session_id.clone(),
                    bindings.len(),
                    Some(row.structural_hash.clone()),
                )
            }
            Strategy::Module { batch, .. } => (
                ReportType::ModuleBinding,
                self.module_function_id(batch),
                batch.len(),
                None,
            ),
        };
        let mut outcomes = Outcomes::default();
        let mut finish_reasons = Vec::new();
        let mut remaining: HashSet<String> = HashSet::new();
        let mut calls = 0u64;
        for lane in lanes {
            outcomes.assign(lane.outcomes);
            calls += lane.finish_reasons.len() as u64;
            finish_reasons.extend(lane.finish_reasons);
            remaining.extend(lane.remaining);
        }
        let report = RenameReport {
            ty,
            strategy: ReportStrategy::Llm,
            target_id,
            total_identifiers: identifiers,
            renamed_count: identifiers - remaining.len(),
            outcomes,
            total_llm_calls: Some(calls),
            finish_reasons,
            structural_hash: hash.clone(),
        };
        let slot = &mut self.ctxs[ctx].report;
        match (slot.as_mut(), ty) {
            (Some(existing), ReportType::Function) => {
                existing.merge(report);
                existing.structural_hash = hash;
            }
            _ => *slot = Some(report),
        }
    }

    /// `recordWaveRejectionOutcome`: a barrier-rejected entry reads as a
    /// duplicate until its retry overwrites it (keyed by the OLD name —
    /// a shadowed-pass `name#2` entry's rejection lands on `name`).
    fn record_rejection_outcome(&mut self, ctx: usize, old: &str, new: &str) {
        if let Some(r) = self.ctxs[ctx].report.as_mut() {
            r.outcomes.set(
                old,
                IdentifierOutcome {
                    status: Status::Duplicate {
                        conflicted_with: new.to_string(),
                        attempts: 1,
                        suggestion: Some(new.to_string()),
                    },
                    trail: None,
                },
            );
        }
    }

    /// `recordWaveRetryOutcome`.
    fn record_retry_outcome(&mut self, ctx: usize, id: &str, final_name: &str) {
        if let Some(r) = self.ctxs[ctx].report.as_mut() {
            let round = r.total_llm_calls.unwrap_or(1);
            r.outcomes
                .set(id, IdentifierOutcome::renamed(final_name, round, None));
        }
    }

    /// `recordWaveRetryGiveUp`'s outcome half.
    fn record_retry_give_up(&mut self, ctx: usize, id: &str, prev: &str) {
        if let Some(r) = self.ctxs[ctx].report.as_mut() {
            r.outcomes.set(
                id,
                IdentifierOutcome {
                    status: Status::Duplicate {
                        conflicted_with: prev.to_string(),
                        attempts: 2,
                        suggestion: Some(prev.to_string()),
                    },
                    trail: None,
                },
            );
        }
    }

    fn with_lane_env<R>(&self, lr: &LaneRun, f: impl FnOnce(&LaneEnv<'_>) -> R) -> R {
        match &self.strategies[lr.strategy] {
            Strategy::Fn {
                f: fi,
                bindings,
                set,
                ..
            } => {
                let set = &self.sets[*set];
                let used = |n: &str| set.contains(n) || self.used.has(n);
                let scopes: HashMap<&str, BScopeId> = bindings
                    .iter()
                    .map(|b| (b.name.as_str(), b.scope))
                    .collect();
                let reject = |old: &str, new: &str| {
                    scopes
                        .get(old)
                        .is_some_and(|&s| self.state.get_rename_rejection(s, old, new).is_some())
                };
                let transform = self.fn_transform(*fi);
                let env = LaneEnv {
                    used: &used,
                    would_reject: &reject,
                    transform: transform.as_deref(),
                };
                f(&env)
            }
            Strategy::Module { batch, .. } => {
                let used = |n: &str| self.used.has(n);
                let names: HashSet<&str> = batch
                    .iter()
                    .map(|&j| self.inp.graph.module_bindings[j].name.as_str())
                    .collect();
                let target = self.target_scope;
                let reject = |old: &str, new: &str| {
                    names.contains(old)
                        && self.state.get_rename_rejection(target, old, new).is_some()
                };
                let transform = self.module_transform(lr.strategy);
                let env = LaneEnv {
                    used: &used,
                    would_reject: &reject,
                    transform: Some(&*transform),
                };
                f(&env)
            }
        }
    }

    fn feed_lane(&self, lr: &mut LaneRun, res: Result<(Renames, Option<String>), ()>) {
        let mut lane = std::mem::replace(&mut lr.lane, Lane::new(Vec::new(), false));
        self.with_lane_env(lr, |env| lane.feed(res, env));
        lr.lane = lane;
    }

    fn finish_lane(&self, lr: &mut LaneRun) {
        let mut lane = std::mem::replace(&mut lr.lane, Lane::new(Vec::new(), false));
        self.with_lane_env(lr, |env| lane.finish(env));
        lr.lane = lane;
    }

    /// Turn a lane's effects into barrier entries (`collectWaveRename` /
    /// `collectWaveIdentity`).
    fn collect_lane_effects(&mut self, lr: LaneRun) {
        let (live, target_kind) = match &self.strategies[lr.strategy] {
            Strategy::Fn { set, .. } => (Live::Fn(*set), Some(*set)),
            Strategy::Module { .. } => (Live::Module, None),
        };
        for effect in lr.lane.effects {
            let (old, new, identity) = match effect {
                LaneEffect::Rename { old, new } => (old, new, false),
                LaneEffect::Identity { name } => (name.clone(), name, true),
            };
            let ctx = &self.ctxs[lr.ctx];
            let binding_index = ctx
                .order
                .get(&(lr.phase, old.clone()))
                .copied()
                .unwrap_or(usize::MAX);
            let binding = ctx.binding_map.get(&old).cloned();
            let target = match (&ctx.kind, target_kind) {
                (CtxKind::Fn(_), Some(set)) => ApplyTarget::Fn {
                    binding: binding.clone(),
                    set,
                },
                (CtxKind::Module(batch), _) => ApplyTarget::Module {
                    mb: batch
                        .iter()
                        .copied()
                        .find(|&j| self.inp.graph.module_bindings[j].name == old),
                },
                _ => unreachable!("strategy/ctx kinds agree"),
            };
            let node_index = ctx.node_index;
            let seq = self.next_seq();
            self.entries.push(Entry {
                node_index,
                phase: lr.phase,
                binding_index,
                seq,
                old,
                new,
                identity,
                suffix_on_reject: false,
                ctx: lr.ctx,
                binding,
                target,
                live,
                prev_name: None,
            });
        }
    }

    /// Dispatch one turn's requests through the provider, recording each
    /// (prompts.jsonl + cache-keys.jsonl).
    fn dispatch(
        &mut self,
        requests: Vec<BatchRenameRequest>,
        targets: &[(usize, String)],
    ) -> Vec<Result<(Renames, Option<String>), ()>> {
        let mut calls = Vec::with_capacity(requests.len());
        for (request, (ctx, function_id)) in requests.into_iter().zip(targets) {
            let (record, call) = self.prepare_dispatch(request, *ctx, function_id);
            self.commit_record(record);
            calls.push(call);
        }
        let results = {
            let mut ph = crate::profiling::phase("waves:llm-dispatch");
            if let Some(ph) = ph.as_mut() {
                ph.note("calls", calls.len());
            }
            self.provider.run_wave(calls)
        };
        let mut tally = Tally::default();
        let mapped = results.into_iter().map(|r| tally.map(r)).collect();
        self.apply_tally(tally);
        mapped
    }

    /// A request's prompts, cache key and dump record (its `seq` and
    /// `round` are assigned by [`Run::commit_record`], in dispatch order).
    fn prepare_dispatch(
        &self,
        request: BatchRenameRequest,
        ctx: usize,
        function_id: &str,
    ) -> (DispatchRecord, LlmCall) {
        let system_prompt = render_system_prompt(&request);
        let user_prompt = render_user_prompt(&request);
        let cache_key = cache_key_of(&request, &self.inp.params);
        let record = DispatchRecord {
            seq: 0,
            function_id: function_id.to_string(),
            round: 0,
            wave: self.ctxs[ctx].wave,
            request: request.clone(),
            cache_key,
            system_prompt: system_prompt.clone(),
            user_prompt: user_prompt.clone(),
            targets: self.dump_targets(ctx),
        };
        let call = LlmCall {
            request,
            system_prompt,
            user_prompt,
        };
        (record, call)
    }

    /// Record a dispatch in dispatch order: its `seq`, and its `round`
    /// (the function id's call count so far).
    fn commit_record(&mut self, mut record: DispatchRecord) {
        let round = self.rounds.entry(record.function_id.clone()).or_insert(0);
        *round += 1;
        record.round = *round;
        record.seq = self.dispatches.len() as u64;
        self.dispatches.push(record);
    }

    fn apply_tally(&mut self, tally: Tally) {
        self.processor.completed_calls += tally.completed;
        self.misses += tally.misses;
        self.errors += tally.errors;
    }

    /// `dumpTargetForWaveCtx`.
    fn dump_targets(&self, ctx: usize) -> Vec<(String, Span)> {
        let graph = self.inp.graph;
        match &self.ctxs[ctx].kind {
            CtxKind::Fn(f) => vec![(
                graph.functions[*f].session_id.clone(),
                graph.functions[*f].span,
            )],
            CtxKind::Module(batch) => batch
                .iter()
                .map(|&j| {
                    let b = &graph.module_bindings[j];
                    (b.session_id.clone(), b.span)
                })
                .collect(),
        }
    }

    // ------------------------------------------------------------------
    // the barrier
    // ------------------------------------------------------------------

    fn live_has(&self, live: Live, name: &str) -> bool {
        match live {
            Live::Fn(set) => self.sets[set].contains(name) || self.used.has(name),
            Live::Module => self.used.has(name),
        }
    }

    /// `applyWaveBarrier` over every collected entry.
    fn barrier(&mut self) -> Vec<Rejection> {
        let _ph = crate::profiling::phase("waves:barrier");
        let mut entries = std::mem::take(&mut self.entries);
        entries.sort_by(|a, b| {
            (a.node_index, a.phase, a.binding_index, a.seq).cmp(&(
                b.node_index,
                b.phase,
                b.binding_index,
                b.seq,
            ))
        });
        let mut rejections = Vec::new();
        for entry in entries {
            if entry.identity {
                self.record_identity(entry.ctx, &entry.old, entry.binding.as_ref());
                continue;
            }
            let taken = self.live_has(entry.live, &entry.new);
            let applied = !taken && self.apply(&entry, &entry.new.clone());
            if applied {
                self.winners.insert(entry.new.clone(), entry.old.clone());
                if entry.suffix_on_reject {
                    self.record_retry_outcome(entry.ctx, &entry.old, &entry.new);
                }
                continue;
            }
            if entry.suffix_on_reject {
                let variant = resolve_conflict(&entry.new, |n| self.live_has(entry.live, n));
                let ok = variant != entry.new && self.apply(&entry, &variant);
                if ok {
                    self.processor.contention.push(ContentionEvent {
                        requested: entry.new.clone(),
                        resolved_to: variant.clone(),
                        old_name: entry.old.clone(),
                        site: "wave",
                    });
                    self.record_retry_outcome(entry.ctx, &entry.old, &variant);
                    self.winners.insert(variant, entry.old.clone());
                } else {
                    // Terminal give-up: identity bookkeeping.
                    self.record_identity(entry.ctx, &entry.old, entry.binding.as_ref());
                    let prev = entry.prev_name.clone().unwrap_or_default();
                    self.record_retry_give_up(entry.ctx, &entry.old, &prev);
                }
                continue;
            }
            self.record_rejection_outcome(entry.ctx, &entry.old, &entry.new);
            let winner_old = self.winners.get(&entry.new).cloned();
            rejections.push(Rejection { entry, winner_old });
        }
        rejections
    }

    /// `recordWaveIdentity`.
    fn record_identity(&mut self, ctx: usize, name: &str, binding: Option<&BindingInfo>) {
        let CtxKind::Fn(f) = self.ctxs[ctx].kind else {
            return;
        };
        let Some(b) = binding else {
            return;
        };
        let span = self.state.view().binding(b.binding).id_span;
        self.names.push(NameRecord {
            span,
            old_name: name.to_string(),
            new_name: name.to_string(),
            module: false,
            function_id: self.inp.graph.functions[f].session_id.clone(),
        });
        self.ctxs[ctx].names.set(name, name);
    }

    /// The entry's apply closure: `applyFunctionRename` /
    /// `applyModuleRename` through `applyLlmRename`.
    fn apply(&mut self, entry: &Entry, name: &str) -> bool {
        match &entry.target {
            ApplyTarget::Fn { binding, set } => {
                let (Some(b), CtxKind::Fn(f)) = (binding, &self.ctxs[entry.ctx].kind) else {
                    return false;
                };
                let f = *f;
                if !self.llm_rename(b.scope, &entry.old, name) {
                    return false;
                }
                let span = self.state.view().binding(b.binding).id_span;
                self.names.push(NameRecord {
                    span,
                    old_name: entry.old.clone(),
                    new_name: name.to_string(),
                    module: false,
                    function_id: self.inp.graph.functions[f].session_id.clone(),
                });
                let s = &mut self.sets[*set];
                s.remove(&entry.old);
                s.insert(name);
                // A function declaration's own name renames through
                // `fnPath.parentPath.scope` — a graph-era object.
                if self.is_own_name(f, b.binding)
                    && let Some(era) = self.graph_era.get_mut(&b.scope)
                {
                    era.delete(&entry.old);
                    era.add(name);
                }
                self.ctxs[entry.ctx].names.set(&entry.old, name);
                if b.scope == self.target_scope || b.scope == self.program_scope {
                    self.used.delete(&entry.old);
                    self.used.add(name);
                }
                true
            }
            ApplyTarget::Module { mb } => {
                let Some(j) = *mb else {
                    return false;
                };
                let scope = self.inp.rows.modules[j].scope;
                if !self.llm_rename(scope, &entry.old, name) {
                    return false;
                }
                let b = &self.inp.graph.module_bindings[j];
                self.names.push(NameRecord {
                    span: b.span,
                    old_name: entry.old.clone(),
                    new_name: name.to_string(),
                    module: true,
                    function_id: b.session_id.clone(),
                });
                self.used.delete(&entry.old);
                self.used.add(name);
                true
            }
        }
    }

    /// The binding is function row `f`'s own declaration name.
    fn is_own_name(&self, f: usize, b: BindingId) -> bool {
        self.inp.rows.fns[f]
            .id_symbol
            .and_then(|sym| self.state.view().binding_of_symbol(sym))
            == Some(b)
    }

    /// `applyLlmRename`: validated rename + the `llm` trail row on the
    /// binding `scope.getBinding(old)` resolves (captured BEFORE).
    fn llm_rename(&mut self, scope: BScopeId, old: &str, new: &str) -> bool {
        let trail_binding = self.state.get_binding(scope, old);
        // Both captured BEFORE the rename: the count the guards saw
        // (`referencePaths + constantViolations`) and the scope's block.
        let ref_count = trail_binding.map(|b| self.ref_count(b));
        let scope_block = self.state.view().scope(scope).span;
        let attempt = self.state.attempt_validated_rename(
            RenameRequest {
                scope,
                old_name: old,
                new_name: new,
                expected: None,
            },
            TrailSpec::CallerRecords { tier: Tier::Llm },
        );
        if let Some(b) = trail_binding {
            let outcome = if attempt.applied {
                Outcome::Applied
            } else {
                Outcome::Rejected
            };
            let mut row = Attempt::new(Tier::Llm, outcome)
                .proposed(new)
                .scope_block(scope_block);
            if let Some(reason) = attempt.reason {
                row = row.reason(reason.as_str());
            }
            if let Some(n) = ref_count {
                row = row.ref_count(n);
            }
            self.state.record(b, old, row, false);
        }
        attempt.applied
    }
}

/// `{...record}` of `[name, value]` pairs: a later duplicate key keeps the
/// first position with the last value.
fn dedup_record<T: Clone>(pairs: Vec<(String, T)>) -> Vec<(String, T)> {
    let mut out: Vec<(String, T)> = Vec::new();
    for (k, v) in pairs {
        match out.iter_mut().find(|(ok, _)| *ok == k) {
            Some(e) => e.1 = v,
            None => out.push((k, v)),
        }
    }
    out
}

/// Retry snippets keep this many lines around each reference.
const RETRY_SNIPPET_CONTEXT_LINES: usize = 2;
const RETRY_SNIPPET_MIN_LINES: usize = 30;
const RETRY_SNIPPET_MAX_LINES: usize = 80;
const RETRY_USED_NAMES_CAP: usize = 25;

/// `extractRetrySnippet`.
pub fn extract_retry_snippet(code: &str, identifiers: &[String]) -> String {
    let lines: Vec<&str> = code.split('\n').collect();
    if lines.len() <= RETRY_SNIPPET_MIN_LINES {
        return code.to_string();
    }
    let mut keep: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    keep.insert(0);
    for (i, line) in lines.iter().enumerate() {
        if !identifiers
            .iter()
            .any(|id| crate::naming::code_window::line_has_identifier(line, id))
        {
            continue;
        }
        let from = i.saturating_sub(RETRY_SNIPPET_CONTEXT_LINES);
        let to = (i + RETRY_SNIPPET_CONTEXT_LINES).min(lines.len() - 1);
        keep.extend(from..=to);
    }
    let kept: Vec<usize> = keep.into_iter().take(RETRY_SNIPPET_MAX_LINES).collect();
    let mut parts: Vec<&str> = Vec::new();
    let mut prev: Option<usize> = None;
    for &i in &kept {
        if let Some(p) = prev
            && i > p + 1
        {
            parts.push("  // …");
        }
        parts.push(lines[i]);
        prev = Some(i);
    }
    if prev.is_some_and(|p| p < lines.len() - 1) {
        parts.push("  // …");
    }
    parts.join("\n")
}

/// `buildRetryUsedNames`: the previous suggestions, then windowed names up
/// to 25.
pub fn build_retry_used_names(windowed: &[String], prev: &JsRecord) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (_, v) in &prev.0 {
        if seen.insert(v.clone()) {
            out.push(v.clone());
        }
    }
    for n in windowed {
        if out.len() >= RETRY_USED_NAMES_CAP {
            break;
        }
        if seen.insert(n.clone()) {
            out.push(n.clone());
        }
    }
    out
}

#[cfg(test)]
mod processor_test;
