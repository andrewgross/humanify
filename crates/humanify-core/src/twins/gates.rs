//! The gate ladder + the apply half's OUTPUT shape — TS original:
//! `src/prior-version/statement-twin.ts` :278-1281 (the part downstream of
//! the inventory + unique join that lives in twins.rs).
//!
//! Five gates, in order, per proposal pair (abstain on any failure — a
//! wrong name on the wrong binding is worse than a missed inherit):
//!  1. unique twin (1:1 statementHash join — twins.rs, before this module)
//!  2. candidacy — the statement still contains pending work
//!     ([`needs_bridging`]: pending functions, an unclaimed pending module
//!     binding, or an exact match that cross-paired into a different prior
//!     statement)
//!  3. statement callee gate — [`callee_sets_agree`], the statement-scope
//!     mirror of the role gate's callee veto
//!  4. role gate — [`declared_roles_agree`], pairwise
//!     [`crate::twins::role::binding_roles_agree`] over the declared module
//!     bindings
//!  5. structural gate — [`bridge_twin_slots`]'s canonical hash equality +
//!     slot-mapping alignment
//!  6. per-slot owner gate — [`OwnerContext::owner_allows_transfer`]
//!
//! Three proposal tiers feed the ladder (statement-twin.ts :1196-1262): the
//! UNIQUE tier (1:1 hash join), the exp073 module tier ([`pair_by_module_context`]
//! over [`crate::twins::fossil::extract_fossil_modules`]) and the bucket
//! tier ([`pair_buckets_by_ref_key`] over matched-reference identity keys).
//! Every pair runs the SAME ladder — the tiers widen WHAT gets proposed,
//! never WHAT gets accepted.
//!
//! Parity posture (documented divergences, each strictly in the
//! precision-safe direction):
//!  - the structural gate uses the canonical hash, whose private tokens stay
//!    VERBATIM (the TS placeholder-walk's `P=#name` spelling — the
//!    serializer's per-class slot map is currently clobbered before any
//!    private token can read it), and the TS's masked-stream comparison is
//!    ported as the fallback that reconciles a private re-lettering —
//!    same-order or swapped — by blinding `P=#` tokens
//!    ([`canonical_serialize_privates_blinded`]). The collected swaps then
//!    fail the collision gate (the target id already declared in the fresh
//!    class) exactly as the TS's do.
//!  - private-node walk order is the JSON key order, not babel's
//!    VISITOR_KEYS order — the collected SETS agree; the dump sorts.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxc_ast::AstKind;
use oxc_semantic::{AstNodes, Semantic, SymbolId};
use oxc_span::{GetSpan, Span};
use serde_json::{Value, json};

use crate::graph::{ModuleBindingNode, UnifiedGraph};
use crate::hash::serialize::{
    LiteralPolicy, SymbolTables, canonical_serialize, canonical_serialize_privates_blinded,
};
use crate::matching::alternation::GraphSide;
use crate::twins::SideInventory;
use crate::twins::fossil::{FossilModule, extract_fossil_modules, module_signature};
use crate::twins::role::{RoleSide, binding_roles_agree, compute_binding_role};

// ---------------------------------------------------------------------------
// Inputs — the cascade results as parameters (this module stays decoupled
// from the cascade's internals; WP2.4's orchestration supplies them)
// ---------------------------------------------------------------------------

/// The lifecycle state of one graph row at twins time. The TS reads
/// `fn.state.kind` off the babel-backed nodes (lifecycle.ts); the Rust
/// graph rows carry no state, so the caller supplies the two maps on
/// [`TwinInputs`]. The natural derivation: a fresh fn is
/// [`RowState::ExactMatched`] iff its session id is a VALUE of fnMatches
/// (applyExactMatches marked it before the twins ran); everything else the
/// caller knows (frozen/skipped) is [`RowState::Settled`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    /// `state.kind === "pending"` — work the twin tier should do.
    Pending,
    /// `state.kind === "transferred"` — exact-matched fresh fn; the twin
    /// tier applies BEFORE exact transfers, so a same-statement match just
    /// produces the same names while a cross-paired match gets repaired.
    ExactMatched,
    /// Frozen (wrapper / library / eval-taint) and every other settled
    /// state — never transfer.
    Settled,
}

impl RowState {
    /// TS `isPending` (lifecycle.ts).
    pub fn is_pending(self) -> bool {
        matches!(self, RowState::Pending)
    }
    /// TS `pendingOrExactMatched` (statement-twin.ts :377).
    pub fn is_pending_or_exact(self) -> bool {
        !matches!(self, RowState::Settled)
    }
}

/// The cascade's results, as parameters (statement-twin.ts :648).
pub struct TwinInputs<'a> {
    /// Function matches, PRIOR session id → fresh session id (the
    /// cascade's `MatchResult.matches` direction).
    pub fn_matches: &'a HashMap<String, String>,
    /// Module-binding old names already claimed by the cascade / var-name
    /// transfers — the finer tiers win; twins fill only the residue.
    pub claimed_old_names: &'a HashSet<String>,
    /// The binding cascade's matches INVERTED — (fresh minified oldName,
    /// prior name). Identity evidence for the bucket tier and the conflict
    /// diagnostics' `cascadeNameByOld` (:1173).
    pub binding_identity_pairs: &'a [(String, String)],
    /// Fresh-side lifecycle states: fn session id → state. Missing keys
    /// read as [`RowState::Settled`] (conservative abstention).
    pub fn_states: &'a HashMap<String, RowState>,
    /// Fresh-side lifecycle states: module-binding session id → state.
    pub binding_states: &'a HashMap<String, RowState>,
}

impl TwinInputs<'_> {
    fn fn_state(&self, session_id: &str) -> RowState {
        self.fn_states
            .get(session_id)
            .copied()
            .unwrap_or(RowState::Settled)
    }

    fn binding_state(&self, session_id: &str) -> RowState {
        self.binding_states
            .get(session_id)
            .copied()
            .unwrap_or(RowState::Settled)
    }
}

/// One side's gate inputs, built once per side (statement-twin.ts reads
/// the babel AST through NodePaths; the Rust precomputes the joins).
pub struct GateSide<'a, 's> {
    pub graph: &'a UnifiedGraph,
    pub semantic: &'a Semantic<'s>,
    pub tables: &'a SymbolTables,
    /// The statement inventory (twins.rs), index-aligned with `statements`.
    pub inventory: &'a SideInventory,
    /// The statements' ESTree JSON subtrees — the structural gate's
    /// serialization substrate (the gate re-walks the exact subtree whose
    /// hash the inventory computed).
    pub statements: &'a [Value],
    /// The wrapper FUNCTION's span, when the wrapper gate passed — the
    /// owner gate's module-level test (`fnPath.node === ctx.wrapperNode`).
    pub wrapper_span: Option<Span>,
    /// The wrapper body, else the program — the role evidence's
    /// redeclaration-search bounds (graph.rs's `container_span`).
    pub container_span: Span,
    /// The side's reference-identity inputs (alternation.rs): the holder
    /// map and the resolved-reference table.
    pub side: &'a GraphSide<'a>,
    /// Graph-row span → session id (functions AND module bindings) — the
    /// TS `callee.sessionId` join.
    pub session_join: HashMap<(u32, u32), String>,
}

impl<'a, 's> GateSide<'a, 's> {
    /// Assemble one side. `side` must be `GraphSide::build(graph,
    /// semantic)` on the SAME semantic this carries. (The argument count is
    /// the side's own pieces — grouping them into a struct would just
    /// destructure it back here.)
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        graph: &'a UnifiedGraph,
        semantic: &'a Semantic<'s>,
        tables: &'a SymbolTables,
        inventory: &'a SideInventory,
        statements: &'a [Value],
        side: &'a GraphSide<'a>,
        wrapper_span: Option<Span>,
        container_span: Span,
    ) -> GateSide<'a, 's> {
        GateSide {
            graph,
            semantic,
            tables,
            inventory,
            statements,
            wrapper_span,
            container_span,
            side,
            session_join: crate::matching::alternation::session_join(graph),
        }
    }

    fn fn_session(&self, row: usize) -> &str {
        &self.graph.functions[row].session_id
    }

    fn binding_row(&self, row: usize) -> &ModuleBindingNode {
        &self.graph.module_bindings[row]
    }
}

// ---------------------------------------------------------------------------
// Output shape — what the apply half (WP3.2's transfer port) consumes
// ---------------------------------------------------------------------------

/// Which tier proposed the pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwinTier {
    /// 1:1 statementHash join (twins.rs `unique_twin_proposals`).
    Unique,
    /// exp073: 1:1-matched fossil-module pair, equal-hash positional.
    Module,
    /// Non-unique bucket members paired by matched-reference identity.
    Bucket,
}

impl TwinTier {
    pub fn as_str(self) -> &'static str {
        match self {
            TwinTier::Unique => "unique",
            TwinTier::Module => "module",
            TwinTier::Bucket => "bucket",
        }
    }
}

/// How one proposal pair ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TwinOutcome {
    /// Through the whole ladder — slots bridged.
    Bridged,
    /// Nothing left to bridge (`needs_bridging` false).
    NoCandidacy,
    /// Gate 3: the statement callee sets disagree.
    VetoedCallee,
    /// Gate 4: the declared binding roles disagree.
    VetoedRole,
    /// Gate 5: the canonical hashes / slot mappings disagree.
    VetoedStructural,
}

impl TwinOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            TwinOutcome::Bridged => "bridged",
            TwinOutcome::NoCandidacy => "noCandidacy",
            TwinOutcome::VetoedCallee => "vetoedCallee",
            TwinOutcome::VetoedRole => "vetoedRole",
            TwinOutcome::VetoedStructural => "vetoedStructural",
        }
    }
}

/// One gated transfer pair — TS `TransferPair` ({oldName, newName,
/// binding}) with the binding spelled as the resolved symbol plus the two
/// fields the apply half reads off `binding.path` (the owner registration
/// and the declaration-form branch of applyStatementTwinTransfers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TwinTransferPair {
    /// The fresh binding's resolved symbol (the babel `Binding` OBJECT the
    /// TS carries).
    pub symbol: SymbolId,
    /// The fresh minified name (TS `oldName`).
    pub old_name: String,
    /// The prior name it inherits (TS `newName`).
    pub new_name: String,
    /// The owning fresh function row's session id — None for module-level
    /// bindings (the apply half registers the rename with the owner's
    /// scope when present).
    pub owner_fn_session: Option<String>,
    /// The declaration is a FunctionDeclaration (TS
    /// `binding.path.isFunctionDeclaration()` at apply time).
    pub is_function_declaration: bool,
}

/// One private id's transfer: every fresh PrivateIdentifier node carrying
/// it, spans in source order (statement-twin.ts :98 — TS carries the
/// PrivateName NODES; WP3.2 mutates them directly).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrivateRenameSet {
    pub old_name: String,
    pub new_name: String,
    pub node_spans: Vec<Span>,
}

/// The cascade claimed this head under a DIFFERENT prior name than the
/// gated twin pairing derives — family-rotation diagnostics
/// (statement-twin.ts :642).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CascadeConflict {
    pub old_name: String,
    pub cascade_name: String,
    pub twin_name: String,
}

/// One twin pair's bridged slots (TS `BridgedSlots` :570).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BridgedSlots {
    pub pairs: Vec<TwinTransferPair>,
    pub private_renames: Vec<PrivateRenameSet>,
    pub outer_refs: Vec<TwinTransferPair>,
}

/// One proposal pair's gated row — the dump's unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatedTwin {
    pub tier: TwinTier,
    pub fresh_idx: usize,
    pub prior_idx: usize,
    pub outcome: TwinOutcome,
    /// Set only on [`TwinOutcome::Bridged`].
    pub bridged: Option<BridgedSlots>,
}

/// Cross-version bookkeeping for the counters (TS `StatementTwinStats`
/// :59; field spellings are the TS camelCase — the dump is the diff
/// surface).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatementTwinStats {
    pub fresh_statements: usize,
    pub prior_statements: usize,
    /// statementHash present exactly once on both sides.
    pub unique_twins: usize,
    /// Non-unique-bucket members paired by matched-reference identity.
    pub bucket_twins: usize,
    /// exp073: members paired by fossil-module context.
    pub module_scoped_twins: usize,
    /// Module pairs skipped because their signature was not unique on both
    /// sides.
    pub module_scoped_ambiguous: usize,
    /// Outer-reference vote pairs emitted from bridged statements.
    pub outer_refs: usize,
    /// Private ids transferred from masked-equal twins.
    pub private_renames: usize,
    /// Cascade-claimed heads whose gated twin disagrees on the name.
    pub cascade_conflicts: usize,
    /// Twins containing pending work (the only ones bridged).
    pub candidates: usize,
    /// Candidates rejected by the statement-level callee identity gate.
    pub vetoed_callee: usize,
    /// Candidates rejected by the module-binding role gate.
    pub vetoed_role: usize,
    /// Candidates rejected by the structural gate.
    pub vetoed_structural: usize,
    /// Candidates that produced at least one transfer pair.
    pub transferred_twins: usize,
    pub pairs: usize,
}

/// The full gated output (TS `StatementTwinTransfers` :104 + the per-row
/// provenance the dump section needs).
#[derive(Debug, Default)]
pub struct TwinGateOutput {
    pub stats: StatementTwinStats,
    /// Every proposal pair's gated row, in emission order (unique tier,
    /// then module tier, then bucket tier).
    pub gated: Vec<GatedTwin>,
    /// Gated transfer pairs (TS `result.pairs`).
    pub pairs: Vec<TwinTransferPair>,
    /// Private-name transfers from slot-equal twins.
    pub private_renames: Vec<PrivateRenameSet>,
    /// Prior names observed for OUTER bindings referenced from bridged
    /// statements — never applied directly; they feed the external-
    /// reference vote propagation (WP3.x) with exact-grade testimony.
    pub outer_refs: Vec<TwinTransferPair>,
    pub conflicts: Vec<CascadeConflict>,
}

// ---------------------------------------------------------------------------
// Gate 2 — candidacy
// ---------------------------------------------------------------------------

/// The identity ids of a side's rows: functions `input.js:L:C`, module
/// bindings `module:<name>` — the TS `sessionId` values.
struct CrossPairContext {
    /// fresh fn session id → prior fn session id (inverted fnMatches).
    new_to_prior_fn_id: HashMap<String, String>,
    /// prior fn session id → its top-level statement index.
    prior_stmt_idx_by_fn_id: HashMap<String, usize>,
}

impl CrossPairContext {
    fn new(input: &TwinInputs<'_>, prior: &GateSide<'_, '_>) -> CrossPairContext {
        let new_to_prior_fn_id: HashMap<String, String> = input
            .fn_matches
            .iter()
            .map(|(prior_id, fresh_id)| (fresh_id.clone(), prior_id.clone()))
            .collect();
        let prior_stmt_idx_by_fn_id: HashMap<String, usize> = prior
            .inventory
            .fns_by_statement
            .iter()
            .flat_map(|(&idx, rows)| {
                rows.iter()
                    .map(move |&row| (prior.graph.functions[row].session_id.clone(), idx))
            })
            .collect();
        CrossPairContext {
            new_to_prior_fn_id,
            prior_stmt_idx_by_fn_id,
        }
    }
}

/// An exact-matched fresh function whose matched PRIOR function lives in a
/// DIFFERENT statement than the prior twin — the ordinal/identity tiers
/// cross-paired same-shaped siblings, and the exact transfer is about to
/// write the sibling's names here (statement-twin.ts :689).
fn is_cross_paired(
    fresh: &GateSide<'_, '_>,
    fn_row: usize,
    prior_twin_idx: usize,
    input: &TwinInputs<'_>,
    cross: &CrossPairContext,
) -> bool {
    if input.fn_state(fresh.fn_session(fn_row)) != RowState::ExactMatched {
        return false;
    }
    let Some(prior_id) = cross.new_to_prior_fn_id.get(fresh.fn_session(fn_row)) else {
        return false;
    };
    let Some(&prior_idx) = cross.prior_stmt_idx_by_fn_id.get(prior_id) else {
        return false;
    };
    prior_idx != prior_twin_idx
}

/// True when the statement still contains work this tier should bridge
/// (statement-twin.ts :704).
fn needs_bridging(
    fresh: &GateSide<'_, '_>,
    fresh_idx: usize,
    prior_twin_idx: usize,
    input: &TwinInputs<'_>,
    cross: &CrossPairContext,
) -> bool {
    let fns = fresh.inventory.fns_by_statement.get(&fresh_idx);
    let bindings = fresh.inventory.bindings_by_statement.get(&fresh_idx);
    let pending_fns = fns.is_some_and(|rows| {
        rows.iter()
            .any(|&row| input.fn_state(fresh.fn_session(row)).is_pending())
    });
    let unclaimed_bindings = bindings.is_some_and(|rows| {
        rows.iter().any(|&row| {
            let b = fresh.binding_row(row);
            input.binding_state(&b.session_id).is_pending()
                && !input.claimed_old_names.contains(&b.name)
        })
    });
    let cross_paired = fns.is_some_and(|rows| {
        rows.iter()
            .any(|&row| is_cross_paired(fresh, row, prior_twin_idx, input, cross))
    });
    pending_fns || unclaimed_bindings || cross_paired
}

// ---------------------------------------------------------------------------
// Gate 3 — the statement callee gate
// ---------------------------------------------------------------------------

/// External function-callee evidence of one statement's contained rows
/// (TS `CalleeEvidence` :274 / `statementCalleeEvidence` :279).
struct CalleeEvidence {
    fn_callee_ids: HashSet<String>,
    has_binding_callees: bool,
}

fn statement_callee_evidence(
    side: &GateSide<'_, '_>,
    fn_rows: &[usize],
    binding_rows: &[usize],
) -> CalleeEvidence {
    let mut contained: HashSet<&str> = HashSet::new();
    for &row in fn_rows {
        contained.insert(side.fn_session(row));
    }
    for &row in binding_rows {
        contained.insert(&side.binding_row(row).session_id);
    }
    let mut evidence = CalleeEvidence {
        fn_callee_ids: HashSet::new(),
        has_binding_callees: false,
    };
    let record = |callee_span: &Span, evidence: &mut CalleeEvidence| {
        let Some(session_id) = side.session_join.get(&(callee_span.start, callee_span.end)) else {
            return;
        };
        if contained.contains(session_id.as_str()) {
            return;
        }
        if session_id.starts_with("module:") {
            evidence.has_binding_callees = true;
        } else {
            evidence.fn_callee_ids.insert(session_id.clone());
        }
    };
    for &row in fn_rows {
        for span in &side.graph.functions[row].internal_callees {
            record(span, &mut evidence);
        }
    }
    for &row in binding_rows {
        for span in &side.binding_row(row).internal_callees {
            record(span, &mut evidence);
        }
    }
    evidence
}

/// Statement-level mirror of the role gate's callee veto
/// (statement-twin.ts :309): when both sides' contained rows reference
/// only matched external functions, the prior's callees mapped through the
/// function matches must equal the fresh side's. Module-binding callees or
/// unmatched functions are inconclusive (no veto) — matching is
/// incomplete, not contradictory.
fn callee_sets_agree(
    prior: &CalleeEvidence,
    fresh: &CalleeEvidence,
    fn_matches: &HashMap<String, String>,
) -> bool {
    if prior.has_binding_callees || fresh.has_binding_callees {
        return true;
    }
    if prior.fn_callee_ids.is_empty() || fresh.fn_callee_ids.is_empty() {
        return true;
    }
    // Sorted iteration: the set is unordered (clippy's
    // arbitrary-source-ordering lint) and the comparison below normalizes
    // through its own BTreeSet anyway.
    let mut prior_ids: Vec<&str> = prior.fn_callee_ids.iter().map(String::as_str).collect();
    prior_ids.sort_unstable();
    let mut mapped: Vec<&str> = Vec::with_capacity(prior_ids.len());
    for id in prior_ids {
        let Some(matched) = fn_matches.get(id) else {
            return true; // unmatched prior callee — inconclusive
        };
        mapped.push(matched.as_str());
    }
    fn key<'id>(ids: impl IntoIterator<Item = &'id str>) -> String {
        let unique: BTreeSet<&str> = ids.into_iter().collect();
        unique.into_iter().collect::<Vec<_>>().join("|")
    }
    key(mapped) == key(fresh.fn_callee_ids.iter().map(String::as_str))
}

// ---------------------------------------------------------------------------
// Gate 4 — the role gate
// ---------------------------------------------------------------------------

/// Pairwise role corroboration for the module bindings the twin statements
/// declare (TS `declaredRolesAgree` :340). Counts must line up and every
/// pair must positively agree; `allowContentFreeElimination` is FALSE
/// (role.rs's license note).
fn declared_roles_agree(
    prior: &GateSide<'_, '_>,
    prior_rows: &[usize],
    fresh: &GateSide<'_, '_>,
    fresh_rows: &[usize],
    fn_matches: &HashMap<String, String>,
    prior_role_side: &RoleSide<'_, '_>,
    fresh_role_side: &RoleSide<'_, '_>,
) -> bool {
    if prior_rows.len() != fresh_rows.len() {
        return false;
    }
    if prior_rows.is_empty() {
        return true;
    }
    // byDeclarationOrder (:329) — sort by the binding identifier's start.
    let mut prior_sorted: Vec<usize> = prior_rows.to_vec();
    prior_sorted.sort_by_key(|&row| prior.binding_row(row).span.start);
    let mut fresh_sorted: Vec<usize> = fresh_rows.to_vec();
    fresh_sorted.sort_by_key(|&row| fresh.binding_row(row).span.start);
    for (&prior_row, &fresh_row) in prior_sorted.iter().zip(&fresh_sorted) {
        let prior_role = compute_binding_role(prior.binding_row(prior_row), prior_role_side);
        let fresh_role = compute_binding_role(fresh.binding_row(fresh_row), fresh_role_side);
        if !binding_roles_agree(&prior_role, &fresh_role, fn_matches, false).agrees {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// Gates 5 + 6 — the structural gate and the per-slot owner gate
// ---------------------------------------------------------------------------

/// The owner gate's per-slot context (TS `OwnerGateContext` :361), built on
/// the fresh side only.
struct OwnerContext<'a, 'i> {
    input: &'i TwinInputs<'a>,
    /// The declaration node's span → the graph function row (TS
    /// `fnByNode`, keyed by the fn NODE).
    fn_idx_by_span: HashMap<(u32, u32), usize>,
    /// Binding name → module binding row (TS `moduleNodeByName`).
    binding_row_by_name: HashMap<String, usize>,
    /// The wrapper FUNCTION's span (TS `wrapperNode`).
    wrapper_span: Option<Span>,
    /// oldName (fresh) → the cascade's chosen prior name, for conflict
    /// diagnostics.
    cascade_name_by_old: HashMap<String, String>,
    conflicts: Vec<CascadeConflict>,
}

impl<'a, 'i> OwnerContext<'a, 'i> {
    fn new(fresh: &GateSide<'_, '_>, input: &'i TwinInputs<'a>) -> OwnerContext<'a, 'i> {
        let fn_idx_by_span: HashMap<(u32, u32), usize> = fresh
            .graph
            .functions
            .iter()
            .enumerate()
            .map(|(row, f)| (f.span.start, f.span.end, row))
            .map(|(start, end, row)| ((start, end), row))
            .collect();
        let binding_row_by_name: HashMap<String, usize> = fresh
            .graph
            .module_bindings
            .iter()
            .enumerate()
            .map(|(row, b)| (b.name.clone(), row))
            .collect();
        let cascade_name_by_old: HashMap<String, String> = input
            .binding_identity_pairs
            .iter()
            .map(|(old_name, new_name)| (old_name.clone(), new_name.clone()))
            .collect();
        OwnerContext {
            input,
            fn_idx_by_span,
            binding_row_by_name,
            wrapper_span: fresh.wrapper_span,
            cascade_name_by_old,
            conflicts: Vec::new(),
        }
    }

    /// Note when the cascade claimed this head under a DIFFERENT prior name
    /// than the gated twin pairing derives (statement-twin.ts :554).
    /// Identity-guarded: minified names are reused across scopes, so the
    /// slot must resolve to the SAME binding the cascade's module node
    /// holds — a name-string match alone would count unrelated
    /// statement-locals.
    fn record_cascade_conflict(
        &mut self,
        fresh: &GateSide<'_, '_>,
        symbol: SymbolId,
        old_name: &str,
        twin_name: &str,
    ) -> bool {
        let Some(cascade_name) = self.cascade_name_by_old.get(old_name) else {
            return false;
        };
        if cascade_name == twin_name {
            return false;
        }
        let identity_confirmed = self
            .binding_row_by_name
            .get(old_name)
            .is_some_and(|&row| fresh.binding_row(row).symbol == symbol);
        if !identity_confirmed {
            return false;
        }
        self.conflicts.push(CascadeConflict {
            old_name: old_name.to_string(),
            cascade_name: cascade_name.clone(),
            twin_name: twin_name.to_string(),
        });
        true
    }

    /// The per-slot owner gate (TS `ownerAllowsTransfer` :381). `decl_id`
    /// is the slot symbol's declaration node.
    fn owner_allows_transfer(
        &self,
        fresh: &GateSide<'_, '_>,
        decl_id: oxc_semantic::NodeId,
        old_name: &str,
    ) -> bool {
        let nodes = fresh.semantic.nodes();
        // The nearest function-ish ancestor of the declaration: the TS
        // `scopePath.isFunction() ? scopePath : scopePath.getFunctionParent()`
        // (babel isFunction = FunctionDeclaration | FunctionExpression |
        // ArrowFunctionExpression — oxc spells the first two as one
        // AstKind::Function; methods are NOT function-ISH for this walk,
        // matching babel's t.isFunction).
        let mut cur = decl_id;
        let mut fn_span: Option<Span> = None;
        loop {
            let parent = nodes.parent_id(cur);
            if parent == cur {
                break;
            }
            cur = parent;
            if matches!(
                nodes.get_node(cur).kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            ) {
                fn_span = Some(row_span_of_fn_node(nodes, cur));
                break;
            }
        }
        match fn_span {
            // Module level: no function ancestor, or the wrapper itself.
            None => self.module_level_allows(fresh, decl_id, old_name),
            Some(span) if Some(span) == self.wrapper_span => {
                self.module_level_allows(fresh, decl_id, old_name)
            }
            Some(span) => {
                let Some(&row) = self.fn_idx_by_span.get(&(span.start, span.end)) else {
                    return false; // owner fn is not a graph row
                };
                self.input
                    .fn_state(fresh.fn_session(row))
                    .is_pending_or_exact()
            }
        }
    }

    /// The module-level half of [`OwnerContext::owner_allows_transfer`]
    /// (statement-twin.ts :395-402): claimed names never transfer; a
    /// module binding row transfers when pending; a function DECLARATION
    /// without a row transfers when its fn row is pending or exact-matched.
    fn module_level_allows(
        &self,
        fresh: &GateSide<'_, '_>,
        decl_id: oxc_semantic::NodeId,
        old_name: &str,
    ) -> bool {
        if self.input.claimed_old_names.contains(old_name) {
            return false;
        }
        let nodes = fresh.semantic.nodes();
        match self.binding_row_by_name.get(old_name) {
            Some(&row) => self
                .input
                .binding_state(&fresh.binding_row(row).session_id)
                .is_pending(),
            None => {
                // TS `binding.path.isFunctionDeclaration()` — the oxc
                // declaration-vs-expression split is the PARENT's shape
                // (see [`is_function_declaration`]).
                if !is_function_declaration(nodes, decl_id) {
                    return false;
                }
                let decl_span = nodes.get_node(decl_id).span();
                self.fn_idx_by_span
                    .get(&(decl_span.start, decl_span.end))
                    .is_some_and(|&row| {
                        self.input
                            .fn_state(fresh.fn_session(row))
                            .is_pending_or_exact()
                    })
            }
        }
    }
}

/// The declaration-vs-expression split for an oxc `AstKind::Function`
/// node: the PARENT's shape decides (program, export wrappers, statement
/// lists). DUPLICATION NOTICE: the same predicate is inline in
/// alternation.rs `holding_session_ids` (which owns the holder question;
/// this answers the declaration question) — keep the two in sync.
fn is_function_declaration(nodes: &AstNodes<'_>, decl_id: oxc_semantic::NodeId) -> bool {
    if !matches!(nodes.get_node(decl_id).kind(), AstKind::Function(_)) {
        return false;
    }
    let parent_id = nodes.parent_id(decl_id);
    parent_id != decl_id
        && matches!(
            nodes.get_node(parent_id).kind(),
            AstKind::Program(_)
                | AstKind::ExportNamedDeclaration(_)
                | AstKind::ExportDefaultDeclaration(_)
                | AstKind::BlockStatement(_)
                | AstKind::FunctionBody(_)
                | AstKind::SwitchCase(_)
                | AstKind::StaticBlock(_)
        )
}

/// Bridge one twin pair's slots into gated transfer pairs plus outer-
/// reference vote material (TS `bridgeTwinSlots` :578). Returns None on
/// the structural gate's failure (the caller counts vetoedStructural).
#[allow(clippy::too_many_arguments)]
fn bridge_twin_slots(
    fresh: &GateSide<'_, '_>,
    fresh_idx: usize,
    prior: &GateSide<'_, '_>,
    prior_idx: usize,
    owner: &mut OwnerContext<'_, '_>,
    pairs: &mut Vec<TwinTransferPair>,
    outer_refs: &mut Vec<TwinTransferPair>,
    private_renames: &mut Vec<PrivateRenameSet>,
) -> bool {
    let fresh_stmt = &fresh.statements[fresh_idx];
    let prior_stmt = &prior.statements[prior_idx];
    // Property names and free identifiers are verbatim in this hash;
    // equal hashes also guarantee the slot walks align (same
    // serialization). Privates stay VERBATIM in the canonical stream (the
    // serializer's per-class slot map is clobbered before any private
    // token — see the report), so a private re-lettering — same order or
    // swapped — hashes DIFFERENT here, and the TS's masked-stream
    // comparison (statement-twin.ts :591-597) is the fallback that
    // reconciles it: privates blind to `P=#`, walks still align, and the
    // private ids themselves become transfers.
    let mut fresh_out = canonical_serialize(fresh_stmt, fresh.tables, LiteralPolicy::Blurred);
    let mut prior_out = canonical_serialize(prior_stmt, prior.tables, LiteralPolicy::Blurred);
    if fresh_out.hash != prior_out.hash {
        let fresh_masked =
            canonical_serialize_privates_blinded(fresh_stmt, fresh.tables, LiteralPolicy::Blurred);
        let prior_masked =
            canonical_serialize_privates_blinded(prior_stmt, prior.tables, LiteralPolicy::Blurred);
        if fresh_masked.hash != prior_masked.hash {
            return false;
        }
        // Private pairs — the TS collects them ONLY on this masked branch
        // (on the equal branch the privates are byte-identical, so there is
        // nothing to transfer).
        let mut raw_private: Vec<(String, Span, String)> = Vec::new();
        collect_private_pairs(fresh_stmt, prior_stmt, &mut raw_private);
        private_renames.extend(gate_private_renames(raw_private, fresh_stmt));
        fresh_out = fresh_masked;
        prior_out = prior_masked;
    }
    if fresh_out.mapping.len() != prior_out.mapping.len() {
        return false;
    }

    let prior_names: HashMap<&str, &str> = prior_out
        .mapping
        .iter()
        .map(|(slot, _, name)| (slot.as_str(), name.as_str()))
        .collect();

    let stmt_span = fresh.inventory.statements[fresh_idx].span;
    for (slot, symbol, fresh_name) in &fresh_out.mapping {
        let Some(prior_name) = prior_names.get(slot.as_str()) else {
            continue;
        };
        if *prior_name == fresh_name {
            continue;
        }
        let Some(symbol) = *symbol else {
            continue; // TS `if (!binding) continue`
        };
        bridge_one_slot(
            fresh, stmt_span, symbol, fresh_name, prior_name, owner, pairs, outer_refs,
        );
    }
    true
}

/// Route one aligned slot: outer-reference vote, gated transfer pair, or
/// abstention (TS `bridgeOneSlot` :620). A conflicting cascade claim on an
/// identity-confirmed head emits the twin's pair anyway — the twin pairing
/// sees the literals the cascade cannot, and this tier applies first so
/// the crossed cascade rename drops stale. An AGREEING claim still defers
/// to the cascade.
#[allow(clippy::too_many_arguments)]
fn bridge_one_slot(
    fresh: &GateSide<'_, '_>,
    stmt_span: Span,
    symbol: SymbolId,
    fresh_name: &str,
    prior_name: &str,
    owner: &mut OwnerContext<'_, '_>,
    pairs: &mut Vec<TwinTransferPair>,
    outer_refs: &mut Vec<TwinTransferPair>,
) {
    let nodes = fresh.semantic.nodes();
    let scoping = fresh.semantic.scoping();
    let decl_id = scoping.symbol_declaration(symbol);
    let decl_span = nodes.get_node(decl_id).span();
    // TS `declPath.node === freshStmt.node || declPath.isDescendant(freshStmt)`
    // — span containment (the statements are well-nested siblings).
    let anchored = decl_span.start >= stmt_span.start && decl_span.end <= stmt_span.end;
    if !anchored {
        // Declared elsewhere — this statement's testimony about its name
        // goes to vote propagation, never applied directly.
        outer_refs.push(TwinTransferPair {
            symbol,
            old_name: fresh_name.to_string(),
            new_name: prior_name.to_string(),
            owner_fn_session: None,
            is_function_declaration: false,
        });
        return;
    }
    let conflicted = owner.record_cascade_conflict(fresh, symbol, fresh_name, prior_name);
    if !conflicted && !owner.owner_allows_transfer(fresh, decl_id, fresh_name) {
        return;
    }
    // The two fields the apply half reads off `binding.path`: the owner fn
    // (the module-level case has none) and the declaration form. The owner
    // gate already proved the slot is either module-level (no non-wrapper
    // fn ancestor) or owned by a graph row.
    let owner_fn_session = owner_session_of(fresh, decl_id);
    pairs.push(TwinTransferPair {
        symbol,
        old_name: fresh_name.to_string(),
        new_name: prior_name.to_string(),
        owner_fn_session,
        is_function_declaration: is_function_declaration(nodes, decl_id),
    });
}

/// The owning function row's session id for a binding declared inside the
/// wrapper body: the nearest function-ish ancestor that is NOT the wrapper
/// (the owner gate proved exactly one of those two cases holds). None for
/// module-level bindings.
fn owner_session_of(fresh: &GateSide<'_, '_>, decl_id: oxc_semantic::NodeId) -> Option<String> {
    let nodes = fresh.semantic.nodes();
    let mut cur = decl_id;
    loop {
        let parent = nodes.parent_id(cur);
        if parent == cur {
            return None;
        }
        cur = parent;
        let kind = nodes.get_node(cur).kind();
        if matches!(
            kind,
            AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
        ) {
            let span = row_span_of_fn_node(nodes, cur);
            if Some(span) == fresh.wrapper_span {
                continue; // the wrapper itself is module level
            }
            let row = fresh.graph.functions.iter().position(|f| f.span == span)?;
            return Some(fresh.graph.functions[row].session_id.clone());
        }
    }
}

/// The graph-row span of a function-ish AST node. oxc SPLITS the method
/// forms (graph.rs `collect_function_entries` owns the split): a class /
/// object method's inner `Function` node is NOT the row — the
/// MethodDefinition / method-valued ObjectProperty is, with the span from
/// the KEY. The owner walk above lands on the inner Function node, so the
/// row lookup must go through the parent's span or a method-local slot
/// would abstain with "owner fn is not a graph row".
fn row_span_of_fn_node(nodes: &AstNodes<'_>, fn_id: oxc_semantic::NodeId) -> Span {
    use oxc_ast::ast::PropertyKind;
    let parent_id = nodes.parent_id(fn_id);
    let row_node = match nodes.get_node(parent_id).kind() {
        AstKind::MethodDefinition(_) => parent_id,
        AstKind::ObjectProperty(p) if p.method || p.kind != PropertyKind::Init => parent_id,
        _ => fn_id,
    };
    nodes.get_node(row_node).span()
}

// ---------------------------------------------------------------------------
// Private-name machinery (TS :405-547)
// ---------------------------------------------------------------------------

/// Positional private pairs from two isomorphic subtrees — guaranteed by
/// the equal canonical hash (same serialization, same structure). Walked
/// in JSON key order on both sides (the trees are isomorphic, so the key
/// sets agree); returns (fresh name, fresh node span, prior name).
fn collect_private_pairs(fresh: &Value, prior: &Value, out: &mut Vec<(String, Span, String)>) {
    match (fresh, prior) {
        (Value::Object(f), Value::Object(p)) => {
            let f_type = f.get("type").and_then(Value::as_str);
            let p_type = p.get("type").and_then(Value::as_str);
            if f_type == Some("PrivateIdentifier") && p_type == Some("PrivateIdentifier") {
                let name = f.get("name").and_then(Value::as_str).unwrap_or("");
                let prior_name = p.get("name").and_then(Value::as_str).unwrap_or("");
                let span = Span::new(
                    f.get("start").and_then(Value::as_u64).unwrap_or(0) as u32,
                    f.get("end").and_then(Value::as_u64).unwrap_or(0) as u32,
                );
                out.push((name.to_string(), span, prior_name.to_string()));
                return;
            }
            if f_type != p_type {
                return; // not isomorphic — unreachable after a hash pass
            }
            for (key, f_value) in f {
                if crate::twins::role::SKIP_KEYS.contains(&key.as_str()) {
                    continue;
                }
                if let Some(p_value) = p.get(key) {
                    collect_private_pairs(f_value, p_value, out);
                }
            }
        }
        (Value::Array(fa), Value::Array(pa)) => {
            for (f, p) in fa.iter().zip(pa.iter()) {
                collect_private_pairs(f, p, out);
            }
        }
        _ => {}
    }
}

/// Private ids declared by class members inside the statement, mapped to
/// the set of declaring class spans (an id in >1 class is ambiguous)
/// (TS `declaredPrivateClasses` :467).
fn declared_private_classes(root: &Value) -> HashMap<String, HashSet<(u32, u32)>> {
    let mut by_id: HashMap<String, HashSet<(u32, u32)>> = HashMap::new();
    walk_classes(root, None, &mut by_id);
    by_id
}

fn walk_classes(
    node: &Value,
    class_span: Option<(u32, u32)>,
    by_id: &mut HashMap<String, HashSet<(u32, u32)>>,
) {
    let Some(map) = node.as_object() else {
        return;
    };
    let node_type = map.get("type").and_then(Value::as_str).unwrap_or("");
    let class_span = match node_type {
        "ClassExpression" | "ClassDeclaration" => {
            let span = (
                map.get("start").and_then(Value::as_u64).unwrap_or(0) as u32,
                map.get("end").and_then(Value::as_u64).unwrap_or(0) as u32,
            );
            if let Some(body) = map
                .get("body")
                .and_then(|b| b.get("body"))
                .and_then(Value::as_array)
            {
                for member in body {
                    if let Some(key) = member.get("key")
                        && key.get("type").and_then(Value::as_str) == Some("PrivateIdentifier")
                        && let Some(name) = key.get("name").and_then(Value::as_str)
                    {
                        by_id.entry(name.to_string()).or_default().insert(span);
                    }
                }
            }
            Some(span)
        }
        _ => class_span,
    };
    for (key, value) in map {
        if crate::twins::role::SKIP_KEYS.contains(&key.as_str()) {
            continue;
        }
        walk_classes(value, class_span, by_id);
    }
}

/// Gate positional private pairs into safe rename sets: consistent mapping
/// per id, id declared by exactly ONE class in the statement, target not
/// colliding with any surviving fresh id, and 1:1 (no two ids sharing a
/// target). Everything else abstains (TS `gatePrivateRenames` :527).
/// Output sorted by old name (deterministic; the TS output was in walk
/// order — the dump is the comparison surface).
fn gate_private_renames(
    raw: Vec<(String, Span, String)>,
    fresh_root: &Value,
) -> Vec<PrivateRenameSet> {
    if raw.is_empty() {
        return Vec::new();
    }
    // Per fresh id: the prior names seen (must end size 1) + the node spans.
    let mut by_id: Vec<(String, BTreeSet<String>, Vec<Span>)> = Vec::new();
    let mut index_by_id: HashMap<String, usize> = HashMap::new();
    for (name, span, prior_name) in raw {
        let idx = *index_by_id.entry(name.clone()).or_insert_with(|| {
            by_id.push((name.clone(), BTreeSet::new(), Vec::new()));
            by_id.len() - 1
        });
        by_id[idx].1.insert(prior_name);
        by_id[idx].2.push(span);
    }
    let declared = declared_private_classes(fresh_root);
    let declared_ids: HashSet<&String> = declared.keys().collect();
    // How many fresh ids map (consistently) onto each target id
    // (TS `privateTargetCounts` :515).
    let mut target_counts: HashMap<String, usize> = HashMap::new();
    for (_, prior_names, _) in &by_id {
        if prior_names.len() != 1 {
            continue;
        }
        let target = prior_names.iter().next().unwrap();
        *target_counts.entry(target.clone()).or_default() += 1;
    }
    let mut sets: Vec<PrivateRenameSet> = Vec::new();
    for (old_name, prior_names, mut nodes) in by_id {
        if prior_names.len() != 1 {
            continue; // inconsistent — abstain
        }
        let new_name = prior_names.into_iter().next().unwrap();
        if new_name == old_name {
            continue;
        }
        if declared
            .get(&old_name)
            .is_none_or(|classes| classes.len() != 1)
        {
            continue; // multi/un-declared
        }
        if declared_ids.contains(&new_name) {
            continue; // collision (incl. swaps)
        }
        if target_counts.get(&new_name) != Some(&1) {
            continue; // two ids → one
        }
        nodes.sort_by_key(|s| (s.start, s.end));
        sets.push(PrivateRenameSet {
            old_name,
            new_name,
            node_spans: nodes,
        });
    }
    sets.sort_by(|a, b| a.old_name.cmp(&b.old_name));
    sets
}

// ---------------------------------------------------------------------------
// The ladder
// ---------------------------------------------------------------------------

/// Run one proposal pair through the candidacy check and the gates
/// (TS `gateAndBridgeTwin` :932), appending the bridged slots to the
/// caller's accumulators.
#[allow(clippy::too_many_arguments)]
fn gate_and_bridge_twin(
    tier: TwinTier,
    fresh: &GateSide<'_, '_>,
    fresh_idx: usize,
    prior: &GateSide<'_, '_>,
    prior_idx: usize,
    input: &TwinInputs<'_>,
    owner: &mut OwnerContext<'_, '_>,
    cross: &CrossPairContext,
    stats: &mut StatementTwinStats,
    prior_role_side: &RoleSide<'_, '_>,
    fresh_role_side: &RoleSide<'_, '_>,
) -> GatedTwin {
    let fresh_fns = fresh
        .inventory
        .fns_by_statement
        .get(&fresh_idx)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let fresh_bindings = fresh
        .inventory
        .bindings_by_statement
        .get(&fresh_idx)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !needs_bridging(fresh, fresh_idx, prior_idx, input, cross) {
        return GatedTwin {
            tier,
            fresh_idx,
            prior_idx,
            outcome: TwinOutcome::NoCandidacy,
            bridged: None,
        };
    }
    stats.candidates += 1;

    let prior_fns = prior
        .inventory
        .fns_by_statement
        .get(&prior_idx)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let prior_bindings = prior
        .inventory
        .bindings_by_statement
        .get(&prior_idx)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if !callee_sets_agree(
        &statement_callee_evidence(prior, prior_fns, prior_bindings),
        &statement_callee_evidence(fresh, fresh_fns, fresh_bindings),
        input.fn_matches,
    ) {
        stats.vetoed_callee += 1;
        return GatedTwin {
            tier,
            fresh_idx,
            prior_idx,
            outcome: TwinOutcome::VetoedCallee,
            bridged: None,
        };
    }
    if !declared_roles_agree(
        prior,
        prior_bindings,
        fresh,
        fresh_bindings,
        input.fn_matches,
        prior_role_side,
        fresh_role_side,
    ) {
        stats.vetoed_role += 1;
        return GatedTwin {
            tier,
            fresh_idx,
            prior_idx,
            outcome: TwinOutcome::VetoedRole,
            bridged: None,
        };
    }
    let mut pairs = Vec::new();
    let mut outer_refs = Vec::new();
    let mut private_renames = Vec::new();
    let bridged = bridge_twin_slots(
        fresh,
        fresh_idx,
        prior,
        prior_idx,
        owner,
        &mut pairs,
        &mut outer_refs,
        &mut private_renames,
    );
    if !bridged {
        stats.vetoed_structural += 1;
        return GatedTwin {
            tier,
            fresh_idx,
            prior_idx,
            outcome: TwinOutcome::VetoedStructural,
            bridged: None,
        };
    }
    GatedTwin {
        tier,
        fresh_idx,
        prior_idx,
        outcome: TwinOutcome::Bridged,
        bridged: Some(BridgedSlots {
            pairs,
            private_renames,
            outer_refs,
        }),
    }
}

/// Fold one gated row into the output (TS `takeBridged` :1180).
fn take_bridged(mut gated: GatedTwin, output: &mut TwinGateOutput, stats: &mut StatementTwinStats) {
    if let Some(bridged) = gated.bridged.take() {
        if !bridged.pairs.is_empty() {
            stats.transferred_twins += 1;
            stats.pairs += bridged.pairs.len();
            output.pairs.extend(bridged.pairs);
        }
        if !bridged.private_renames.is_empty() {
            stats.private_renames += bridged.private_renames.len();
            output.private_renames.extend(bridged.private_renames);
        }
        // TS `EMIT_OUTER_REF_VOTES` is a constant true (:990).
        if !bridged.outer_refs.is_empty() {
            stats.outer_refs += bridged.outer_refs.len();
            output.outer_refs.extend(bridged.outer_refs);
        }
    }
    output.gated.push(gated);
}

// ---------------------------------------------------------------------------
// The module tier (exp073)
// ---------------------------------------------------------------------------

/// Signatures occurring exactly once, mapped to their module INDEX
/// (TS `uniqueModulesBySignature` :1020; the identity comparison the TS
/// does with `===` is an index equality here).
fn unique_modules_by_signature(modules: &[FossilModule]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in modules {
        *counts.entry(module_signature(m)).or_default() += 1;
    }
    let mut unique: HashMap<String, usize> = HashMap::new();
    for (idx, m) in modules.iter().enumerate() {
        let key = module_signature(m);
        if counts.get(&key) == Some(&1) {
            unique.insert(key, idx);
        }
    }
    unique
}

/// exp073 — statement pairs licensed by fossil-module identity
/// (TS `pairByModuleContext` :1036).
fn pair_by_module_context(
    fresh: &GateSide<'_, '_>,
    prior: &GateSide<'_, '_>,
    stats: &mut StatementTwinStats,
) -> Result<Vec<(usize, usize)>, String> {
    let fresh_hashes: Vec<String> = fresh
        .inventory
        .statements
        .iter()
        .map(|s| s.hash.clone())
        .collect();
    let prior_hashes: Vec<String> = prior
        .inventory
        .statements
        .iter()
        .map(|s| s.hash.clone())
        .collect();
    let fresh_ex = extract_fossil_modules(fresh.statements, &fresh_hashes)?;
    let prior_ex = extract_fossil_modules(prior.statements, &prior_hashes)?;
    if fresh_ex.modules.is_empty() || prior_ex.modules.is_empty() {
        return Ok(Vec::new());
    }

    let unique_fresh = unique_modules_by_signature(&fresh_ex.modules);
    let unique_prior = unique_modules_by_signature(&prior_ex.modules);

    let mut out: Vec<(usize, usize)> = Vec::new();
    for (fm_idx, fm) in fresh_ex.modules.iter().enumerate() {
        let key = module_signature(fm);
        let pm_idx = if unique_fresh.get(&key) == Some(&fm_idx) {
            unique_prior.get(&key).copied()
        } else {
            None
        };
        let Some(pm_idx) = pm_idx else {
            stats.module_scoped_ambiguous += 1;
            continue;
        };
        let pm = &prior_ex.modules[pm_idx];
        if pm.statements.len() != fm.statements.len() {
            continue;
        }
        for k in 0..fm.statements.len() {
            if let Some(pair) = pair_statement(fresh, prior, fm.statements[k], pm.statements[k]) {
                out.push(pair);
            }
        }
    }
    Ok(out)
}

/// One positional statement pair, kept only when the tree-wide tier did
/// NOT already own it and the per-statement hashes agree
/// (TS `pairStatement` :1079).
fn pair_statement(
    fresh: &GateSide<'_, '_>,
    prior: &GateSide<'_, '_>,
    fresh_idx: usize,
    prior_idx: usize,
) -> Option<(usize, usize)> {
    let hash = &fresh.inventory.statements[fresh_idx].hash;
    if fresh.inventory.hash_counts.get(hash) == Some(&1) {
        return None;
    }
    if *hash != prior.inventory.statements[prior_idx].hash {
        return None;
    }
    Some((fresh_idx, prior_idx))
}

// ---------------------------------------------------------------------------
// The bucket tier
// ---------------------------------------------------------------------------

/// Identity ids for bindings on one side (TS `freshIdentityByBinding` :745
/// / `priorIdentityByBinding` :779), keyed by the binding's SYMBOL (the
/// babel `Binding` OBJECT the TS keys its maps by). Matched functions'
/// holder bindings → `fn:<priorId>`; matched module bindings →
/// `bind:<name>`. Unmatched things are absent — a reference to them
/// contributes no identity. SYMMETRY IS LOAD-BEARING (the TS comment on
/// :769): both sides keep only their MATCHED subset, so a fresh statement
/// whose unmatched refs are invisible cannot uniquely claim the wrong
/// prior member.
fn fresh_identity_by_symbol(
    fresh: &GateSide<'_, '_>,
    input: &TwinInputs<'_>,
) -> HashMap<SymbolId, String> {
    let inverse: HashMap<String, String> = input
        .fn_matches
        .iter()
        .map(|(prior_id, fresh_id)| (fresh_id.clone(), prior_id.clone()))
        .collect();
    let mut map: HashMap<SymbolId, String> = HashMap::new();
    // Matched functions' holder bindings → `fn:<priorId>`.
    for (symbol, fresh_session) in fresh.side.holders() {
        if let Some(prior_id) = inverse.get(fresh_session) {
            map.insert(*symbol, format!("fn:{prior_id}"));
        }
    }
    // Cascade-matched module bindings → `bind:<priorName>`.
    let by_name: HashMap<&str, usize> = fresh
        .graph
        .module_bindings
        .iter()
        .enumerate()
        .map(|(row, b)| (b.name.as_str(), row))
        .collect();
    for (old_name, new_name) in input.binding_identity_pairs {
        if let Some(&row) = by_name.get(old_name.as_str()) {
            map.insert(fresh.binding_row(row).symbol, format!("bind:{new_name}"));
        }
    }
    map
}

fn prior_identity_by_symbol(
    prior: &GateSide<'_, '_>,
    input: &TwinInputs<'_>,
) -> HashMap<SymbolId, String> {
    let mut map: HashMap<SymbolId, String> = HashMap::new();
    // Matched functions' holder bindings → `fn:<priorId>` — restricted to
    // nodes that matched (fnMatches is keyed by the PRIOR session id).
    for (symbol, prior_session) in prior.side.holders() {
        if input.fn_matches.contains_key(prior_session) {
            map.insert(*symbol, format!("fn:{prior_session}"));
        }
    }
    // Matched module bindings → `bind:<name>`.
    let matched_names: HashSet<&str> = input
        .binding_identity_pairs
        .iter()
        .map(|(_, new_name)| new_name.as_str())
        .collect();
    for b in &prior.graph.module_bindings {
        if matched_names.contains(b.name.as_str()) {
            map.insert(b.symbol, format!("bind:{}", b.name));
        }
    }
    map
}

/// A statement's reference-identity key: the sorted set of identity ids of
/// OUTER bindings it references that have a cross-version identity. Null
/// when it references none — no evidence, no pairing (TS
/// `statementRefKey` :806). References to unmatched bindings are
/// tolerated (absent from the key).
fn statement_ref_key(
    side: &GateSide<'_, '_>,
    stmt_span: Span,
    identity: &HashMap<SymbolId, String>,
) -> Option<String> {
    let nodes = side.semantic.nodes();
    let scoping = side.semantic.scoping();
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for (_start, _end, symbol) in side.side.raw_refs(stmt_span) {
        let Some(id) = identity.get(&symbol) else {
            continue;
        };
        // Outer = the declaration is NOT the statement itself and NOT a
        // descendant of it (span containment — the statements are
        // well-nested siblings).
        let decl_id = scoping.symbol_declaration(symbol);
        let decl_span = nodes.get_node(decl_id).span();
        let outer = !(decl_span.start >= stmt_span.start && decl_span.end <= stmt_span.end);
        if outer {
            ids.insert(id.clone());
        }
    }
    if ids.is_empty() {
        None
    } else {
        Some(ids.into_iter().collect::<Vec<_>>().join("|"))
    }
}

/// Hashes eligible for bucket pairing: present on both sides with EQUAL
/// counts above one (TS `sharedNonUniqueHashes` :843). Unequal counts mean
/// an insertion or removal landed in the bucket — a prior member whose
/// true successor changed could then be claimed by a genuinely-new
/// same-shape statement whose matched-ref key coincides.
fn shared_non_unique_hashes(fresh: &GateSide<'_, '_>, prior: &GateSide<'_, '_>) -> HashSet<String> {
    let mut shared: HashSet<String> = HashSet::new();
    #[allow(clippy::iter_over_hash_type)]
    for (hash, count) in &fresh.inventory.hash_counts {
        if *count == 1 {
            continue; // unique-twin path (or absent from prior)
        }
        if prior.inventory.hash_counts.get(hash) != Some(count) {
            continue;
        }
        shared.insert(hash.clone());
    }
    shared
}

/// hash → refKey → member indices, for one side's shared bucket members
/// (TS `collectBucketKeys` :857) — insertion order preserved (the TS
/// Maps iterate in insertion order; the pair ORDER the bucket tier emits
/// follows it). `index` is the hash → position lookup.
///
/// The nested pair type factors out (clippy's type-complexity lint).
type RefKeyMembers = Vec<(String, Vec<usize>)>;

#[derive(Debug, Default)]
struct BucketKeyIndex {
    entries: Vec<(String, RefKeyMembers)>,
    by_hash: HashMap<String, usize>,
}

impl BucketKeyIndex {
    fn collect(
        side: &GateSide<'_, '_>,
        shared: &HashSet<String>,
        identity: &HashMap<SymbolId, String>,
    ) -> BucketKeyIndex {
        let mut out = BucketKeyIndex::default();
        for i in 0..side.inventory.statements.len() {
            let hash = &side.inventory.statements[i].hash;
            if !shared.contains(hash) {
                continue;
            }
            let Some(key) = statement_ref_key(side, side.inventory.statements[i].span, identity)
            else {
                continue;
            };
            let entry_idx = *out.by_hash.entry(hash.clone()).or_insert_with(|| {
                out.entries.push((hash.clone(), Vec::new()));
                out.entries.len() - 1
            });
            let by_key = &mut out.entries[entry_idx].1;
            let key_idx = by_key
                .iter()
                .position(|(k, _)| *k == key)
                .unwrap_or_else(|| {
                    by_key.push((key.clone(), Vec::new()));
                    by_key.len() - 1
                });
            by_key[key_idx].1.push(i);
        }
        out
    }

    fn get(&self, hash: &str) -> Option<&Vec<(String, Vec<usize>)>> {
        self.by_hash.get(hash).map(|&idx| &self.entries[idx].1)
    }
}

/// Pair non-unique bucket members across sides by reference-identity key:
/// a key claimed by exactly ONE member on each side is an unambiguous
/// correspondence. Everything else abstains (TS `pairBucketsByRefKey`
/// :888).
fn pair_buckets_by_ref_key(
    fresh: &GateSide<'_, '_>,
    prior: &GateSide<'_, '_>,
    fresh_identity: &HashMap<SymbolId, String>,
    prior_identity: &HashMap<SymbolId, String>,
) -> Vec<(usize, usize)> {
    let shared = shared_non_unique_hashes(fresh, prior);
    if shared.is_empty() {
        return Vec::new();
    }
    let fresh_by = BucketKeyIndex::collect(fresh, &shared, fresh_identity);
    let prior_by = BucketKeyIndex::collect(prior, &shared, prior_identity);

    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (hash, fresh_keys) in &fresh_by.entries {
        let Some(prior_keys) = prior_by.get(hash) else {
            continue;
        };
        for (key, fresh_idxs) in fresh_keys {
            let Some(prior_idxs) = prior_keys.iter().find(|(k, _)| k == key).map(|(_, v)| v) else {
                continue;
            };
            if fresh_idxs.len() != 1 || prior_idxs.len() != 1 {
                continue;
            }
            pairs.push((fresh_idxs[0], prior_idxs[0]));
        }
    }
    pairs
}

// ---------------------------------------------------------------------------
// The entry point
// ---------------------------------------------------------------------------

/// Computes the gated statement-twin transfers (TS
/// `computeStatementTwinTransfers` :1146). The two sides' inventories must
/// already be built (twins.rs) and the cascade's results supplied on
/// `input`; the output holds only fresh-side symbols and plain strings
/// (WP3.2's transfer port consumes it). Errors: a fossil-map anomaly on
/// either side (the extraction refuses to guess) — nothing is proposed.
pub fn compute_gated_statement_twins(
    prior: &GateSide<'_, '_>,
    fresh: &GateSide<'_, '_>,
    input: &TwinInputs<'_>,
) -> Result<TwinGateOutput, String> {
    let mut output = TwinGateOutput::default();
    let mut stats = StatementTwinStats {
        fresh_statements: fresh.inventory.statements.len(),
        prior_statements: prior.inventory.statements.len(),
        ..StatementTwinStats::default()
    };
    if stats.fresh_statements == 0 || stats.prior_statements == 0 {
        return Ok(TwinGateOutput {
            stats,
            ..TwinGateOutput::default()
        });
    }

    let mut owner_ctx = OwnerContext::new(fresh, input);
    let cross = CrossPairContext::new(input, prior);
    let prior_role_side = RoleSide {
        semantic: prior.semantic,
        tables: prior.tables,
        container_span: prior.container_span,
        session_join: &prior.session_join,
    };
    let fresh_role_side = RoleSide {
        semantic: fresh.semantic,
        tables: fresh.tables,
        container_span: fresh.container_span,
        session_join: &fresh.session_join,
    };

    // ---- the unique tier (TS :1196) ----
    for i in 0..fresh.inventory.statements.len() {
        let hash = fresh.inventory.statements[i].hash.clone();
        if fresh.inventory.hash_counts.get(&hash) != Some(&1) {
            continue;
        }
        let Some(&prior_idx) = prior.inventory.unique_index.get(&hash) else {
            continue;
        };
        stats.unique_twins += 1;
        let gated = gate_and_bridge_twin(
            TwinTier::Unique,
            fresh,
            i,
            prior,
            prior_idx,
            input,
            &mut owner_ctx,
            &cross,
            &mut stats,
            &prior_role_side,
            &fresh_role_side,
        );
        take_bridged(gated, &mut output, &mut stats);
    }

    // ---- the module tier (exp073, TS :1215) ----
    let module_pairs = pair_by_module_context(fresh, prior, &mut stats)?;
    for (fresh_idx, prior_idx) in module_pairs {
        stats.module_scoped_twins += 1;
        let gated = gate_and_bridge_twin(
            TwinTier::Module,
            fresh,
            fresh_idx,
            prior,
            prior_idx,
            input,
            &mut owner_ctx,
            &cross,
            &mut stats,
            &prior_role_side,
            &fresh_role_side,
        );
        take_bridged(gated, &mut output, &mut stats);
    }

    // ---- the bucket tier (TS :1235) ----
    let bucket_pairs = pair_buckets_by_ref_key(
        fresh,
        prior,
        &fresh_identity_by_symbol(fresh, input),
        &prior_identity_by_symbol(prior, input),
    );
    for (fresh_idx, prior_idx) in bucket_pairs {
        stats.bucket_twins += 1;
        let gated = gate_and_bridge_twin(
            TwinTier::Bucket,
            fresh,
            fresh_idx,
            prior,
            prior_idx,
            input,
            &mut owner_ctx,
            &cross,
            &mut stats,
            &prior_role_side,
            &fresh_role_side,
        );
        take_bridged(gated, &mut output, &mut stats);
    }

    let OwnerContext { conflicts, .. } = owner_ctx;
    stats.cascade_conflicts = conflicts.len();
    Ok(TwinGateOutput {
        stats,
        conflicts,
        ..output
    })
}

// ---------------------------------------------------------------------------
// The dump
// ---------------------------------------------------------------------------

/// The gate's dump section: per-proposal rows + the stats — the shape the
/// TS twins.json's gated sections compare against (the parent session
/// implements the TS side). Spans are raw UTF-8 byte offsets (oxc; the TS
/// dump converts its UTF-16 spans at write time).
pub fn gate_dump(
    output: &TwinGateOutput,
    prior: &GateSide<'_, '_>,
    fresh: &GateSide<'_, '_>,
) -> Value {
    let rows: Vec<Value> = output
        .gated
        .iter()
        .map(|g| {
            let fresh_stmt = &fresh.inventory.statements[g.fresh_idx];
            let prior_stmt = &prior.inventory.statements[g.prior_idx];
            let mut row = json!({
                "tier": g.tier.as_str(),
                "fresh": {"start": fresh_stmt.span.start, "end": fresh_stmt.span.end},
                "prior": {"start": prior_stmt.span.start, "end": prior_stmt.span.end},
                "outcome": g.outcome.as_str(),
            });
            if let Some(bridged) = &g.bridged {
                row["pairs"] = json!(
                    bridged
                        .pairs
                        .iter()
                        .map(|p| json!({
                            "oldName": p.old_name,
                            "newName": p.new_name,
                            "ownerFnSession": p.owner_fn_session,
                            "isFunctionDeclaration": p.is_function_declaration,
                        }))
                        .collect::<Vec<_>>()
                );
                row["outerRefs"] = json!(
                    bridged
                        .outer_refs
                        .iter()
                        .map(|p| json!({"oldName": p.old_name, "newName": p.new_name}))
                        .collect::<Vec<_>>()
                );
                row["privateRenames"] = json!(
                    bridged
                        .private_renames
                        .iter()
                        .map(|p| json!({
                            "oldName": p.old_name,
                            "newName": p.new_name,
                            "nodes": p.node_spans.iter()
                                .map(|s| [s.start, s.end])
                                .collect::<Vec<_>>(),
                        }))
                        .collect::<Vec<_>>()
                );
            }
            row
        })
        .collect();
    let stats = &output.stats;
    json!({
        "stats": {
            "freshStatements": stats.fresh_statements,
            "priorStatements": stats.prior_statements,
            "uniqueTwins": stats.unique_twins,
            "bucketTwins": stats.bucket_twins,
            "moduleScopedTwins": stats.module_scoped_twins,
            "moduleScopedAmbiguous": stats.module_scoped_ambiguous,
            "outerRefs": stats.outer_refs,
            "privateRenames": stats.private_renames,
            "cascadeConflicts": stats.cascade_conflicts,
            "candidates": stats.candidates,
            "vetoedCallee": stats.vetoed_callee,
            "vetoedRole": stats.vetoed_role,
            "vetoedStructural": stats.vetoed_structural,
            "transferredTwins": stats.transferred_twins,
            "pairs": stats.pairs,
        },
        "rows": rows,
        "conflicts": output.conflicts.iter()
            .map(|c| json!({"oldName": c.old_name, "cascadeName": c.cascade_name, "twinName": c.twin_name}))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod gates_test;
