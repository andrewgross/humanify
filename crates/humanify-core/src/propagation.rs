//! The call-graph propagation post-pass — TS original
//! `src/analysis/propagation.ts` (:1-457). PORTING.md tags the file WP3.3
//! (`humanify-core::rename::votes`), but the WP2.1 cascade consumes it
//! DIRECTLY (`fingerprint-index.ts` `matchFunctions` :829-841 runs it as the
//! matching post-pass), so the port is pulled forward into its own module;
//! the WP3-side consumers (votes) will call into here too. The ledger's
//! split confirmation is deferred to checkpoint C1.
//!
//! What it does (:44-58): iteratively propagates CONFIRMED matches through
//! the call graph to resolve AMBIGUOUS functions. Five rungs, in priority
//! order:
//!
//! 1. `matchedCallee` (:235) — the old function calls already-matched
//!    callees; a candidate must call the corresponding new callees.
//! 2. `matchedCaller` (:261) — the old function's callers are matched; a
//!    candidate must be called by the corresponding new callers.
//! 3. `scopeParent` (:281) — the old function's scope parent is matched; a
//!    candidate's parent must be the corresponding new parent.
//! 4. `externalRefs` (:307) — the old function REFERENCES matched module
//!    bindings or functions (references are not calls, so the callee edges
//!    never see them); a candidate must reference the corresponding new
//!    ids. The only discriminating signal for module-scope functions with
//!    no call-graph or parent evidence (structurally identical export
//!    getters differ only in which binding they return).
//! 5. `scopeOrdinal` (:363) — position among same-hash siblings under a
//!    matched parent; fires only when old and new sibling counts are equal.
//!
//! Mutations: `matches` gains a pair and the entry leaves `ambiguous` when
//! a pool closes to one EVIDENCED candidate; a pool that merely shrank is
//! written back (:135-137). A pool that shrank to one purely because the
//! other candidates were claimed elsewhere is NOT evidence (:145-151) —
//! matching on it would be an order-dependent guess.
//!
//! Per-rung attribution (:52-58): the outcome names the rung that CLOSED
//! each resolution. The aggregate alone hid the luck-prone ordinal rung for
//! the whole matching arc (exp065b: 282 ordinal-resolved functions on
//! 85→86, visible only through a side-channel census).
//!
//! Determinism (02 §5): the TS iterates the `ambiguous` Map in INSERTION
//! order and that order is decision input — a resolution claims a new id,
//! and the injectivity filter of later entries in the SAME iteration sees
//! the claim. So the ambiguous map is an ordered structure here
//! ([`AmbiguousMatches`]), never a hash map; the TS's candidate lists are
//! the bucket lists in index (graph-row) order and the port keeps that
//! order. `matches` is keyed lookup only in every decision path; its one
//! iteration (building the claimed-new set) is a bijective membership read
//! whose order cannot matter (allow-annotated).
//!
//! Identity: keys are session-id strings (`input.js:LINE:COL`), the TS
//! key. The index entries carry them (`matching.rs` keys rows by span and
//! rides the session ids along for exactly this parity).
//!
//! Source position: the TS sorts scope children by
//! `position.line * 100000 + position.column` (:426-429), null sorting
//! last. The port sorts by the row's span start — the same loc the TS
//! position is built from (`function-graph.ts` :486), and byte order
//! equals line:col order within the single-file graph, so the ORDER is
//! identical. Every graph row has a span, so the TS's null-position tail
//! cannot arise here.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use oxc_span::Span;

use crate::graph::UnifiedGraph;
use crate::matching::{FingerprintIndex, IndexNode};

// ---------------------------------------------------------------------------
// WP2.1 diagnostics: the propagation trace (migration scaffolding —
// deleted at phase 6)
// ---------------------------------------------------------------------------

/// The propagation trace (migration scaffolding — deleted at phase 6).
/// When CONFIGURED it logs the propagation loop's per-iteration dynamics to
/// stderr: iteration boundaries, resolutions, narrowing writes,
/// contradictions, plus the alternation rounds and the cascade's
/// demote/revoke steps. A watch list (prefix match on the old session id)
/// narrows the PER-ENTRY lines to those ids; without one every entry's
/// narrowing prints.
///
/// The environment is read by the CLI ([`humanify_cli::env`], the ONE
/// reader) which calls [`configure`] — core modules receive
/// environment-derived values as CONFIG, never by reading `std::env`
/// themselves (02 §2; clippy `disallowed_methods` is the static layer).
pub mod trace {
    use std::sync::OnceLock;

    #[derive(Default)]
    struct Config {
        enabled: bool,
        watch: Option<Vec<String>>,
    }

    static CONFIG: OnceLock<Config> = OnceLock::new();

    fn config() -> &'static Config {
        CONFIG.get_or_init(Config::default)
    }

    /// Installs the trace configuration. The FIRST call wins; later calls
    /// are ignored (the CLI reads the env once at startup).
    pub fn configure(enabled: bool, watch: Option<Vec<String>>) {
        let _ = CONFIG.set(Config { enabled, watch });
    }

    pub fn enabled() -> bool {
        config().enabled
    }

    /// The watch prefixes, or `None` when unset (every entry prints).
    fn watch() -> Option<&'static Vec<String>> {
        config().watch.as_ref()
    }

    /// Whether a watch list is set (the PROP-start positions print only
    /// then — without one the watch filter prints every entry anyway).
    pub fn watch_present() -> bool {
        watch().is_some()
    }

    /// Whether per-entry lines for `old_id` should print.
    pub fn wants(old_id: &str) -> bool {
        match watch() {
            None => true,
            Some(ids) => ids.iter().any(|prefix| old_id.starts_with(prefix)),
        }
    }

    /// A global (not per-entry) trace line.
    pub fn line(args: std::fmt::Arguments<'_>) {
        if enabled() {
            eprintln!("{args}");
        }
    }

    /// A per-entry trace line, printed only when the id passes the watch.
    pub fn entry_line(old_id: &str, args: std::fmt::Arguments<'_>) {
        if enabled() && wants(old_id) {
            eprintln!("{args}");
        }
    }
}

// ---------------------------------------------------------------------------
// Types (propagation.ts :17-29, types.ts PropagationRungCounts :693)
// ---------------------------------------------------------------------------

/// TS `PropagationRungCounts` (types.ts :693) — how many resolutions each
/// rung closed. The five fields sum to the outcome's `resolved`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PropagationRungCounts {
    pub matched_callee: usize,
    pub matched_caller: usize,
    pub scope_parent: usize,
    pub external_refs: usize,
    pub scope_ordinal: usize,
}

impl PropagationRungCounts {
    /// The five rungs summed — must equal `resolved` (the exp066 guard).
    pub fn total(&self) -> usize {
        self.matched_callee
            + self.matched_caller
            + self.scope_parent
            + self.external_refs
            + self.scope_ordinal
    }

    fn record(&mut self, rung: PropagationRung) {
        match rung {
            PropagationRung::MatchedCallee => self.matched_callee += 1,
            PropagationRung::MatchedCaller => self.matched_caller += 1,
            PropagationRung::ScopeParent => self.scope_parent += 1,
            PropagationRung::ExternalRefs => self.external_refs += 1,
            PropagationRung::ScopeOrdinal => self.scope_ordinal += 1,
        }
    }
}

/// The rung that closed a resolution — TS `keyof PropagationRungCounts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropagationRung {
    MatchedCallee,
    MatchedCaller,
    ScopeParent,
    ExternalRefs,
    ScopeOrdinal,
}

impl PropagationRung {
    /// The TS counter key's spelling (the artifact dumps carry it).
    pub fn as_str(self) -> &'static str {
        match self {
            PropagationRung::MatchedCallee => "matchedCallee",
            PropagationRung::MatchedCaller => "matchedCaller",
            PropagationRung::ScopeParent => "scopeParent",
            PropagationRung::ExternalRefs => "externalRefs",
            PropagationRung::ScopeOrdinal => "scopeOrdinal",
        }
    }
}

/// TS `ExternalRefEvidence` (:17-24): reference-identity evidence for
/// cracking same-hash buckets whose members have no call-graph or
/// scope-parent evidence. A function's identity is WHICH matched thing it
/// references — a matched module binding or a matched function held in a
/// binding (references are not calls, so callee edges never see them).
/// The ref maps cover the ambiguous functions and their candidates;
/// `ref_matches` is the union of confirmed binding and function matches.
///
/// Keyed by session-id string like the TS. `BTreeMap`/`BTreeSet`: every
/// read is a keyed lookup or an order-insensitive membership check (the
/// TS's Set iteration feeds `.every()`), so sorted order is safe and
/// deterministic.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct ExternalRefEvidence {
    /// old fn sessionId → old-side referenced binding/function sessionIds
    pub old_refs: BTreeMap<String, BTreeSet<String>>,
    /// new fn sessionId → new-side referenced binding/function sessionIds
    pub new_refs: BTreeMap<String, BTreeSet<String>>,
    /// old binding/function sessionId → new sessionId (confirmed matches)
    pub ref_matches: BTreeMap<String, String>,
}

/// TS `PropagationOptions` (:26-29).
#[derive(Debug, Clone, Default)]
pub struct PropagationOptions {
    /// TS `maxIterations` (:27); `None` uses [`DEFAULT_MAX_ITERATIONS`].
    pub max_iterations: Option<usize>,
    pub external_ref_evidence: Option<ExternalRefEvidence>,
}

/// TS `maxIterations` default (:27).
pub const DEFAULT_MAX_ITERATIONS: usize = 10;

/// TS's return shape `{ resolved, iterations, byRung }` (:77).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropagationOutcome {
    /// Newly resolved functions (moved out of `ambiguous` into `matches`).
    pub resolved: usize,
    /// Iterations run: one past the last fruitless iteration, or
    /// `max_iterations` when the cap stopped the loop (:102-109).
    pub iterations: usize,
    pub by_rung: PropagationRungCounts,
}

/// TS `ambiguous: Map<string, string[]>` — the ORDERED ambiguous map.
///
/// The TS iterates it in insertion order and the order is decision input
/// (a resolution's claim affects later entries of the same iteration), so
/// the port carries the order explicitly: entries live in a `Vec` in
/// insertion order with a sorted-key slot index for O(log n) lookups.
/// Removal tombstones the slot (the TS deletes); an update replaces the
/// candidates IN PLACE, keeping the entry's position (the TS `Map.set` on
/// an existing key). No new keys are ever inserted during propagation —
/// the TS only narrows or deletes (:130-137) — but `insert` is provided
/// for the callers that build the map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AmbiguousMatches {
    entries: Vec<Option<AmbiguousEntry>>,
    slot: BTreeMap<String, usize>,
    live: usize,
}

/// One ambiguous function: the old session id and its candidate new ids.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbiguousEntry {
    pub old_id: String,
    pub candidates: Vec<String>,
}

impl AmbiguousMatches {
    /// TS `new Map()`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds from `(oldId, candidates)` pairs in order.
    pub fn from_pairs(pairs: Vec<(String, Vec<String>)>) -> Self {
        let mut ambiguous = Self::new();
        for (old_id, candidates) in pairs {
            ambiguous.insert(old_id, candidates);
        }
        ambiguous
    }

    /// TS `Map.set` on a NEW key (an existing key keeps its position).
    pub fn insert(&mut self, old_id: String, candidates: Vec<String>) {
        if let Some(&index) = self.slot.get(&old_id) {
            self.entries[index] = Some(AmbiguousEntry { old_id, candidates });
        } else {
            self.slot.insert(old_id.clone(), self.entries.len());
            self.entries
                .push(Some(AmbiguousEntry { old_id, candidates }));
            self.live += 1;
        }
    }

    /// TS `Map.has`.
    pub fn contains(&self, old_id: &str) -> bool {
        self.slot.contains_key(old_id)
    }

    /// TS `Map.get` — the candidates of one entry, in candidate order.
    pub fn get(&self, old_id: &str) -> Option<&[String]> {
        let &index = self.slot.get(old_id)?;
        Some(&self.entries[index].as_ref()?.candidates)
    }

    /// TS `Map.set` on an EXISTING key: replaces the candidates, keeps the
    /// entry's position. A no-op for an absent key (the TS would insert;
    /// propagation never does — :130-137).
    pub fn update(&mut self, old_id: &str, candidates: Vec<String>) {
        if let Some(&index) = self.slot.get(old_id)
            && let Some(entry) = &mut self.entries[index]
        {
            entry.candidates = candidates;
        }
    }

    /// TS `Map.delete` — tombstones the slot, keeping the insertion order
    /// of the survivors.
    pub fn remove(&mut self, old_id: &str) {
        if let Some(index) = self.slot.remove(old_id) {
            self.entries[index] = None;
            self.live -= 1;
        }
    }

    /// TS `Map.size`.
    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    /// TS `[...map.entries()]` — the LIVE entries in insertion order. The
    /// snapshot runOneIteration iterates (:118).
    pub fn snapshot(&self) -> Vec<AmbiguousEntry> {
        self.entries.iter().flatten().cloned().collect()
    }

    /// The live `(oldId, candidates)` pairs in insertion order.
    pub fn into_pairs(self) -> Vec<(String, Vec<String>)> {
        self.entries
            .into_iter()
            .flatten()
            .map(|entry| (entry.old_id, entry.candidates))
            .collect()
    }

    /// Iterates the LIVE entries in insertion order, `(oldId, candidates)`
    /// — the shape the TS's `Map` iterators (`keys()`/`entries()`) expose.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Vec<String>)> {
        self.entries
            .iter()
            .flatten()
            .map(|entry| (&entry.old_id, &entry.candidates))
    }
}

// ---------------------------------------------------------------------------
// propagate (propagation.ts :71-110)
// ---------------------------------------------------------------------------

/// TS `propagate` (:71). Iteratively propagates the confirmed matches
/// through the call graph, resolving ambiguous functions rung by rung.
/// MUTATES `matches` (gains pairs) and `ambiguous` (loses resolved
/// entries, narrows open pools in place) and returns how many closed, how
/// many iterations it took, and the per-rung attribution.
///
/// `old_index`/`new_index` are the FUNCTION fingerprint indexes
/// (`matching::build_fingerprint_index`); the graph they were built over
/// carries the call/scope edges the TS read off `index.functions`. The
/// BINDING index carries no function rows (TS `functions` undefined,
/// `fingerprint-index.ts` :121-140), so passing one is the TS's early
/// zero return (:83-85).
pub fn propagate(
    matches: &mut HashMap<String, String>,
    ambiguous: &mut AmbiguousMatches,
    old_index: &FingerprintIndex<'_>,
    new_index: &FingerprintIndex<'_>,
    options: PropagationOptions,
) -> PropagationOutcome {
    let by_rung = PropagationRungCounts::default();
    if ambiguous.is_empty() {
        return PropagationOutcome {
            resolved: 0,
            iterations: 0,
            by_rung,
        };
    }
    if is_binding_index(old_index) || is_binding_index(new_index) {
        // TS :83-85 — no function nodes on an index, nothing to read.
        return PropagationOutcome {
            resolved: 0,
            iterations: 0,
            by_rung,
        };
    }

    let claimed_new = claimed_new_ids(matches);
    let mut state = PropagationState {
        matches,
        claimed_new,
        ambiguous,
        old: Side::build(old_index.graph),
        new: Side::build(new_index.graph),
        external_ref_evidence: options.external_ref_evidence.as_ref(),
        by_rung,
    };

    let max_iterations = options.max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS);
    trace::line(format_args!(
        "PROP start live={} maxIters={}",
        state.ambiguous.len(),
        max_iterations
    ));
    if trace::enabled() && trace::watch_present() {
        for (pos, entry) in state.ambiguous.snapshot().into_iter().enumerate() {
            if trace::wants(&entry.old_id) {
                trace::line(format_args!(
                    "PROP watch pos={} {} cand={}",
                    pos,
                    entry.old_id,
                    entry.candidates.len()
                ));
            }
        }
    }
    let mut total_resolved = 0;
    for i in 0..max_iterations {
        let newly_resolved = run_one_iteration(&mut state, i);
        total_resolved += newly_resolved;
        if newly_resolved == 0 {
            trace::line(format_args!(
                "PROP end at iter {} resolved {}",
                i + 1,
                total_resolved
            ));
            return PropagationOutcome {
                resolved: total_resolved,
                iterations: i + 1,
                by_rung: state.by_rung,
            };
        }
    }
    trace::line(format_args!(
        "PROP cap {} resolved {}",
        max_iterations, total_resolved
    ));
    PropagationOutcome {
        resolved: total_resolved,
        iterations: max_iterations,
        by_rung: state.by_rung,
    }
}

/// Whether the index is the BINDING index (every entry a binding row) —
/// the TS's `functions` map is absent there (`fingerprint-index.ts`
/// :121-140), so propagation has nothing to read (:83-85).
fn is_binding_index(index: &FingerprintIndex<'_>) -> bool {
    index
        .entries
        .iter()
        .any(|entry| matches!(entry.node, IndexNode::Binding(_)))
}

/// TS `buildReverseMatches` (:347) reduced to what its readers use: the
/// reverse map is only ever probed with `.has` (:172, :266 in spirit), so
/// the port carries the claimed NEW ids as a set.
#[allow(clippy::iter_over_hash_type)]
// Membership set of a bijective map — the iteration order cannot matter.
fn claimed_new_ids(matches: &HashMap<String, String>) -> BTreeSet<String> {
    matches.values().cloned().collect()
}

// ---------------------------------------------------------------------------
// The propagation state and one iteration (:31-42, :116-141)
// ---------------------------------------------------------------------------

/// TS `PropagationState` (:31-42). The per-side maps become the [`Side`]
/// tables; `reverseMatches` becomes `claimed_new`.
struct PropagationState<'a> {
    matches: &'a mut HashMap<String, String>,
    /// TS `reverseMatches` (newId → oldId) as a membership set.
    claimed_new: BTreeSet<String>,
    ambiguous: &'a mut AmbiguousMatches,
    old: Side,
    new: Side,
    external_ref_evidence: Option<&'a ExternalRefEvidence>,
    by_rung: PropagationRungCounts,
}

/// TS `runOneIteration` (:116): one pass over the ambiguous entries in
/// insertion order; resolutions claim their new id for the rest of the
/// pass (injectivity), narrowed pools are written back in place.
fn run_one_iteration(state: &mut PropagationState<'_>, iteration: usize) -> usize {
    let mut newly_resolved = 0;
    trace::line(format_args!(
        "ITER {} begin live={}",
        iteration,
        state.ambiguous.len()
    ));
    // TS :118 — the snapshot is the iteration order; entries deleted by an
    // earlier resolution are skipped by the `has` guard (:121).
    for entry in state.ambiguous.snapshot() {
        if !state.ambiguous.contains(&entry.old_id) {
            continue;
        }
        let ctx = Ctx {
            old: &state.old,
            new: &state.new,
            matches: &*state.matches,
            claimed_new: &state.claimed_new,
            evidence: state.external_ref_evidence,
        };
        let narrowing = narrow_candidates(&ctx, &entry.old_id, &entry.candidates);
        let (action, new_id) = if narrowing.pool.len() == 1 && narrowing.evidenced {
            let new_id = narrowing.pool[0].clone();
            state.matches.insert(entry.old_id.clone(), new_id.clone());
            state.claimed_new.insert(new_id.clone());
            state.ambiguous.remove(&entry.old_id);
            if let Some(rung) = narrowing.rung {
                state.by_rung.record(rung);
            }
            newly_resolved += 1;
            ("resolved", Some(new_id))
        } else if narrowing.pool.len() > 1 && narrowing.pool.len() < entry.candidates.len() {
            // TS :135-137 — an open pool that SHRANK is written back.
            state
                .ambiguous
                .update(&entry.old_id, narrowing.pool.clone());
            ("written", None)
        } else {
            ("kept", None)
        };
        trace::entry_line(
            &entry.old_id,
            format_args!(
                "NARROW iter={} {} cand={} pool={} ev={} rung={:?} act={}{}",
                iteration,
                entry.old_id,
                entry.candidates.len(),
                narrowing.pool.len(),
                narrowing.evidenced,
                narrowing.rung,
                action,
                new_id.map(|id| format!(" -> {id}")).unwrap_or_default(),
            ),
        );
        if narrowing.pool.is_empty() {
            trace::entry_line(
                &entry.old_id,
                format_args!(
                    "CONTRA iter={} {} cand={} (empty pool — entry keeps its candidates)",
                    iteration,
                    entry.old_id,
                    entry.candidates.len()
                ),
            );
        }
    }
    newly_resolved
}

// ---------------------------------------------------------------------------
// Narrowing (:143-228)
// ---------------------------------------------------------------------------

/// TS `Narrowing` (:143-155): the surviving candidate pool, whether it is
/// backed by positive evidence, and the rung that closed it.
#[derive(Debug)]
struct Narrowing {
    pool: Vec<String>,
    /// True when at least one strategy positively discriminated or
    /// confirmed the pool. A pool that shrank to one candidate purely
    /// because the others were claimed by other old functions is NOT
    /// evidence — matching on it would be an order-dependent guess.
    evidenced: bool,
    /// The strategy that closed the pool to one (attribution for byRung).
    rung: Option<PropagationRung>,
}

/// TS's `"contradiction"` return (:194): a constraint WITH evidence
/// filtered every candidate out — a weaker strategy must not match what a
/// stronger one rejected.
enum ConstraintOutcome {
    Pool(Narrowing),
    Contradiction,
}

/// The per-narrowing read-only view the strategies share.
struct Ctx<'a> {
    old: &'a Side,
    new: &'a Side,
    matches: &'a HashMap<String, String>,
    claimed_new: &'a BTreeSet<String>,
    evidence: Option<&'a ExternalRefEvidence>,
}

/// TS `narrowCandidates` (:163): injectivity filter, then the constraint
/// rungs, then the ordinal rung.
fn narrow_candidates(ctx: &Ctx<'_>, old_id: &str, candidates: &[String]) -> Narrowing {
    let Some(old_row) = ctx.old.row_by_session.get(old_id).copied() else {
        // TS :169 — an unknown old id passes its candidates through
        // untouched (and can never resolve: no evidence is readable).
        return Narrowing {
            pool: candidates.to_vec(),
            evidenced: false,
            rung: None,
        };
    };
    // TS :171-174 — injectivity: candidates claimed by another old
    // function are excluded up front.
    let available: Vec<String> = candidates
        .iter()
        .filter(|cand| !ctx.claimed_new.contains(*cand))
        .cloned()
        .collect();
    if available.is_empty() {
        return Narrowing {
            pool: available,
            evidenced: false,
            rung: None,
        };
    }

    let narrowed = match apply_constraint_strategies(ctx, old_row, &available) {
        ConstraintOutcome::Contradiction => {
            // TS :177 — the entry keeps its ORIGINAL candidates (the
            // caller's arms never fire on an empty pool).
            return Narrowing {
                pool: Vec::new(),
                evidenced: false,
                rung: None,
            };
        }
        ConstraintOutcome::Pool(narrowed) => narrowed,
    };
    if narrowed.pool.len() == 1 && narrowed.evidenced {
        return narrowed;
    }
    // TS :182-186 — the ordinal rung only fires on an OPEN pool (it is
    // inherently evidenced when it fires).
    if narrowed.pool.len() > 1
        && let Some(matched) = try_scope_ordinal_match(ctx, old_row, &narrowed.pool)
    {
        return Narrowing {
            pool: vec![matched],
            evidenced: true,
            rung: Some(PropagationRung::ScopeOrdinal),
        };
    }
    narrowed
}

/// One constraint rung: narrows the pool, or `None` when it has no
/// evidence for this function (TS :214 `filtered === null`).
type ConstraintStrategy = fn(&Ctx<'_>, usize, &[String]) -> Option<Vec<String>>;

/// TS :205-212 — the constraint rungs in EXACT order.
const CONSTRAINT_STRATEGIES: &[(PropagationRung, ConstraintStrategy)] = &[
    (PropagationRung::MatchedCallee, filter_by_matched_callees),
    (PropagationRung::MatchedCaller, filter_by_matched_callers),
    (PropagationRung::ScopeParent, filter_by_scope_parent),
    (
        PropagationRung::ExternalRefs,
        filter_by_matched_external_refs,
    ),
];

/// TS `applyConstraintStrategies` (:197): the constraint rungs in order.
fn apply_constraint_strategies(
    ctx: &Ctx<'_>,
    old_row: usize,
    available: &[String],
) -> ConstraintOutcome {
    let mut pool: Vec<String> = available.to_vec();
    let mut evidenced = false;
    let mut rung = None;
    for &(name, strategy) in CONSTRAINT_STRATEGIES {
        let Some(filtered) = strategy(ctx, old_row, &pool) else {
            continue;
        };
        if filtered.is_empty() {
            return ConstraintOutcome::Contradiction;
        }
        // TS :217-223 — discriminating (shrank the pool) or confirming
        // (evidence exists and the sole surviving candidate satisfies it)
        // are BOTH evidence, and the strategy providing it is the
        // resolution's attributed rung.
        if filtered.len() < pool.len() || pool.len() == 1 {
            evidenced = true;
            rung = Some(name);
        }
        pool = filtered;
        if pool.len() == 1 && evidenced {
            break;
        }
    }
    ConstraintOutcome::Pool(Narrowing {
        pool,
        evidenced,
        rung,
    })
}

// ---------------------------------------------------------------------------
// The constraint rungs (:235-329)
// ---------------------------------------------------------------------------

/// TS `getMatchedNewIds` (:335): the matched new ids of one side's node
/// set (unmatched nodes skipped, order preserved — every read is a
/// membership check, so order never decides).
fn matched_new_ids(sessions: &[String], matches: &HashMap<String, String>) -> Vec<String> {
    sessions
        .iter()
        .filter_map(|session| matches.get(session).cloned())
        .collect()
}

/// TS `filterByMatchedCallees` (:235): strategy 1 — the old function calls
/// already-matched callees; a candidate must call the corresponding new
/// callees. `None` when no callee is matched (no evidence).
fn filter_by_matched_callees(
    ctx: &Ctx<'_>,
    old_row: usize,
    pool: &[String],
) -> Option<Vec<String>> {
    let matched = matched_new_ids(&ctx.old.callees[old_row], ctx.matches);
    if matched.is_empty() {
        return None;
    }
    Some(
        pool.iter()
            .filter(|cand| {
                // TS :247-252 — a candidate without a node row cannot hold
                // the evidence.
                ctx.new.row_by_session.get(*cand).is_some_and(|&row| {
                    let callees: BTreeSet<&str> =
                        ctx.new.callees[row].iter().map(String::as_str).collect();
                    matched.iter().all(|id| callees.contains(id.as_str()))
                })
            })
            .cloned()
            .collect(),
    )
}

/// TS `filterByMatchedCallers` (:261): strategy 2 — the old function's
/// callers are matched; a candidate must be called by the corresponding
/// new callers. `None` when no caller is matched (no evidence).
fn filter_by_matched_callers(
    ctx: &Ctx<'_>,
    old_row: usize,
    pool: &[String],
) -> Option<Vec<String>> {
    let matched = matched_new_ids(&ctx.old.callers[old_row], ctx.matches);
    if matched.is_empty() {
        return None;
    }
    Some(
        pool.iter()
            .filter(|cand| {
                ctx.new.row_by_session.get(*cand).is_some_and(|&row| {
                    let callers: BTreeSet<&str> =
                        ctx.new.callers[row].iter().map(String::as_str).collect();
                    matched.iter().all(|id| callers.contains(id.as_str()))
                })
            })
            .cloned()
            .collect(),
    )
}

/// TS `filterByScopeParent` (:281): strategy 3 — the old function's scope
/// parent is matched; a candidate's parent must be the corresponding new
/// parent. `None` when there is no parent or it is unmatched (no
/// evidence).
fn filter_by_scope_parent(ctx: &Ctx<'_>, old_row: usize, pool: &[String]) -> Option<Vec<String>> {
    let parent_session = ctx.old.parent[old_row].as_ref()?;
    let matched_parent = ctx.matches.get(parent_session)?;
    Some(
        pool.iter()
            .filter(|cand| {
                ctx.new
                    .row_by_session
                    .get(*cand)
                    .and_then(|&row| ctx.new.parent[row].as_ref())
                    .is_some_and(|parent| parent == matched_parent)
            })
            .cloned()
            .collect(),
    )
}

/// TS `filterByMatchedExternalRefs` (:307): strategy 4 — the old function
/// REFERENCES matched bindings/functions; a candidate must reference the
/// corresponding new ids. `None` without evidence: no ref data for this
/// function, or none of its referenced ids are matched.
fn filter_by_matched_external_refs(
    ctx: &Ctx<'_>,
    old_row: usize,
    pool: &[String],
) -> Option<Vec<String>> {
    let evidence = ctx.evidence?;
    let old_refs = evidence.old_refs.get(&ctx.old.sessions[old_row])?;
    if old_refs.is_empty() {
        return None;
    }
    let expected: Vec<&String> = old_refs
        .iter()
        .filter_map(|old_ref| evidence.ref_matches.get(old_ref))
        .collect();
    if expected.is_empty() {
        return None;
    }
    Some(
        pool.iter()
            .filter(|cand| {
                ctx.new.row_by_session.get(*cand).is_some_and(|&row| {
                    let session = &ctx.new.sessions[row];
                    evidence
                        .new_refs
                        .get(session)
                        .is_some_and(|refs| expected.iter().all(|id| refs.contains(*id)))
                })
            })
            .cloned()
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// The ordinal rung (:363-404)
// ---------------------------------------------------------------------------

/// TS `tryScopeOrdinalMatch` (:363): strategy 5 — position among same-hash
/// siblings under a MATCHED parent. Fires only when the old and new
/// same-hash sibling lists have EQUAL length (no additions/removals), and
/// only pairs within the open pool.
fn try_scope_ordinal_match(ctx: &Ctx<'_>, old_row: usize, pool: &[String]) -> Option<String> {
    let parent_span = ctx.old.parent_span[old_row]?;
    let parent_session = ctx.old.parent[old_row].as_ref()?;
    let matched_parent = ctx.matches.get(parent_session)?;
    let old_hash = &ctx.old.hash[old_row];

    // TS :377-390 — the children lists are in source order (built that
    // way); the same-hash filter preserves it.
    let old_siblings = same_hash_siblings(ctx.old, parent_span, old_hash);
    let new_parent_row = ctx.new.row_by_session.get(matched_parent).copied()?;
    let new_siblings = same_hash_siblings(ctx.new, ctx.new.span[new_parent_row], old_hash);

    // TS :392-394 — equal counts (no additions/removals), non-empty.
    if old_siblings.len() != new_siblings.len() || old_siblings.is_empty() {
        return None;
    }
    let ordinal = old_siblings.iter().position(|&row| row == old_row)?;
    let matched = ctx.new.sessions[new_siblings[ordinal]].clone();
    // TS :400-401 — the paired sibling must actually be a candidate.
    if pool.contains(&matched) {
        Some(matched)
    } else {
        None
    }
}

/// The rows under one parent span whose structural hash equals `hash`, in
/// source order (TS :377-390's filter over the sorted children list).
fn same_hash_siblings(side: &Side, parent_span: Span, hash: &str) -> Vec<usize> {
    side.children
        .get(&(parent_span.start, parent_span.end))
        .map(|children| {
            children
                .iter()
                .copied()
                .filter(|&row| side.hash[row] == hash)
                .collect()
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The per-side tables (TS :406-457, plus the node reads :235-296)
// ---------------------------------------------------------------------------

/// One side's call-graph and scope tables — the TS `PropagationState`'s
/// `oldFunctions`/`newFunctions` maps plus the indexes built from them
/// (`newCallers` :440, `oldScopeChildren`/`newScopeChildren` :409). Built
/// from the graph the fingerprint index was built over, in ROW order (the
/// TS's functions-map iteration order is the build order, matching.rs's
/// module doc).
struct Side {
    /// session id → function row (sorted keys; keyed lookups only).
    row_by_session: BTreeMap<String, usize>,
    /// Per row: the session id (TS `node.sessionId`).
    sessions: Vec<String>,
    /// Per row: the structural hash (TS `fingerprint.structuralHash`).
    hash: Vec<String>,
    /// Per row: the row span — the source-position key of the scope
    /// children sort.
    span: Vec<Span>,
    /// Per row: the internal callees' session ids, in edge order (TS
    /// `internalCallees`; only membership is read).
    callees: Vec<Vec<String>>,
    /// Per row: the callers' session ids — the exact inversion of the
    /// callee edges, deduped (TS `callers`, a Set; `analyzeCallees` pairs
    /// every internal-callee edge).
    callers: Vec<Vec<String>>,
    /// Per row: the scope parent's session id (TS `scopeParent.sessionId`);
    /// `None` at top level, or when the parent span is not a graph row —
    /// an unrowed parent cannot be matched, so it is no evidence either
    /// way.
    parent: Vec<Option<String>>,
    /// Per row: the scope parent's span, for the children lookup.
    parent_span: Vec<Option<Span>>,
    /// Parent span → child rows in source order (TS `buildScopeChildrenIndex`
    /// :409, sorted by source position; the TS's line:col order equals the
    /// span order within the single-file graph — module doc).
    children: BTreeMap<(u32, u32), Vec<usize>>,
}

impl Side {
    fn build(graph: &UnifiedGraph) -> Side {
        let rows = graph.functions.len();
        let mut row_by_span: HashMap<(u32, u32), usize> = HashMap::with_capacity(rows);
        for (row, f) in graph.functions.iter().enumerate() {
            row_by_span.insert((f.span.start, f.span.end), row);
        }
        let sessions: Vec<String> = graph
            .functions
            .iter()
            .map(|f| f.session_id.clone())
            .collect();
        let hash: Vec<String> = graph
            .functions
            .iter()
            .map(|f| f.structural_hash.clone())
            .collect();
        let span: Vec<Span> = graph.functions.iter().map(|f| f.span).collect();
        let parent_span: Vec<Option<Span>> =
            graph.functions.iter().map(|f| f.scope_parent).collect();
        let parent: Vec<Option<String>> = parent_span
            .iter()
            .map(|parent| {
                parent.and_then(|span| {
                    row_by_span
                        .get(&(span.start, span.end))
                        .map(|&row| sessions[row].clone())
                })
            })
            .collect();
        let callees: Vec<Vec<String>> = graph
            .functions
            .iter()
            .map(|f| {
                f.internal_callees
                    .iter()
                    .filter_map(|callee| {
                        row_by_span
                            .get(&(callee.start, callee.end))
                            .map(|&row| sessions[row].clone())
                    })
                    .collect()
            })
            .collect();
        let callers = inverted_callers(graph, &row_by_span, &sessions);
        let children = scope_children(graph);
        let row_by_session: BTreeMap<String, usize> = sessions
            .iter()
            .enumerate()
            .map(|(row, session)| (session.clone(), row))
            .collect();
        Side {
            row_by_session,
            sessions,
            hash,
            span,
            callees,
            callers,
            parent,
            parent_span,
            children,
        }
    }
}

/// TS `buildCallersIndex` (:440): for each function, the set of functions
/// that call it — the exact inversion of the internal-callee edges,
/// deduped (the TS holds a Set).
fn inverted_callers(
    graph: &UnifiedGraph,
    row_by_span: &HashMap<(u32, u32), usize>,
    sessions: &[String],
) -> Vec<Vec<String>> {
    let mut caller_rows: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); graph.functions.len()];
    for (row, f) in graph.functions.iter().enumerate() {
        for callee in &f.internal_callees {
            if let Some(&callee_row) = row_by_span.get(&(callee.start, callee.end)) {
                caller_rows[callee_row].insert(row);
            }
        }
    }
    caller_rows
        .into_iter()
        .map(|rows| rows.into_iter().map(|row| sessions[row].clone()).collect())
        .collect()
}

/// TS `buildScopeChildrenIndex` (:409): parent span → child rows, each
/// list sorted by source position. The TS sorts by
/// `line * 100000 + column` with position-less nodes LAST; every graph row
/// has a span and span order equals line:col order in the single-file
/// graph, so the sort key here is the span start.
fn scope_children(graph: &UnifiedGraph) -> BTreeMap<(u32, u32), Vec<usize>> {
    let mut children: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (row, f) in graph.functions.iter().enumerate() {
        let Some(parent) = f.scope_parent else {
            continue;
        };
        children
            .entry((parent.start, parent.end))
            .or_default()
            .push(row);
    }
    for list in children.values_mut() {
        list.sort_by_key(|&row| graph.functions[row].span.start);
    }
    children
}
