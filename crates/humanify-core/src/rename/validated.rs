//! Validated rename (WP3.1) — TS original: `src/rename/validated-rename.ts`.
//!
//! Every path that renames a binding — prior-version transfers, votes, the
//! LLM, the floor passes — goes through ONE applier with the same legality
//! rules (exp013: unguarded transfer paths shipped `let NH` twice and a
//! parameter named `delete`). The rules, in the TS order:
//!
//! 1. `stale-binding` — the caller's evidence binding no longer holds the
//!    old name (checked first, only when the caller passes one);
//! 2. `invalid-target` — not a legal ASCII identifier, a reserved word, or
//!    a global builtin;
//! 3. `no-binding` — the old name is not bound in the given scope's map;
//! 4. `target-in-scope` — the new name is already bound there;
//! 5. `target-visible` — an ancestor binding of the new name is referenced
//!    or written inside this scope's block (the 2.1.166 capture);
//! 6. `target-free-name` — the file reads the new name as a global;
//! 7. `shadows-child` — a scope between one of the binding's references and
//!    this scope binds the new name.
//!
//! (`capture-in-subtree` is the shadowing variant's own rule.)
//!
//! ## The overlay (02 §2)
//!
//! oxc's `Scoping` cannot be resealed, so names are NOT written to it. This
//! module owns a private per-binding name OVERLAY plus Babel's per-scope
//! binding maps (current names), and [`RenameState::attempt_validated_rename`]
//! / [`RenameState::attempt_shadowing_rename`] are their only writers.
//! Everything else reads names through [`RenameState::name_of`]. The render
//! (`core::emit`, WP5.3) applies the overlay once, from [`RenameState::finish`].
//!
//! ## One scope table, no eras
//!
//! The TS runs two Babel scope trees over one AST (the graph's, retained
//! across `clearBabelCacheAfterPriorMatch`, and the naming pass's fresh
//! crawl), and needs a claim ledger so a rename through one is visible to
//! guards walking the other (exp059). The Rust has ONE table, so every
//! rename is visible everywhere by construction; `RenameClaimStats` keeps
//! the TS counter shape, and `ledger_only_rejections` is structurally zero —
//! matching the oracle (0 on all four oracle-b53b3a8 pairs). The TS's
//! OTHER staleness direction (a post-clear rename through the retained
//! tree leaves the fresh tree's map holding the dead old name) is not
//! reproduced; the prior-version transfer stage runs entirely before that
//! clear, so the phase-3 mechanical gate cannot observe it — the LLM era
//! (WP4.x) is where it would show.

use std::collections::{BTreeMap, BTreeSet};

use oxc_semantic::{NodeId, Semantic, SymbolId};

use crate::modules::soundness::{EvalWithTaint, is_binding_eval_taint_frozen};
use crate::rename::floor::is_below_floor_name;
use crate::trail::{Anchor, Attempt, Outcome, StrategyTrail, Tier, TrailTarget};

pub mod ledger;
pub mod scope_dump;
pub mod scopes;
pub mod target;

#[cfg(test)]
pub(crate) mod test_support;

use scopes::{BScopeId, BabelScopes, BindingId, Site};
use target::is_valid_rename_target;

/// Why a rename was rejected (`RenameRejectionReason`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum RejectionReason {
    InvalidTarget,
    NoBinding,
    TargetInScope,
    TargetVisible,
    CaptureInSubtree,
    TargetFreeName,
    ShadowsChild,
    StaleBinding,
}

impl RejectionReason {
    /// The TS reason code (what the trail records).
    pub fn as_str(self) -> &'static str {
        match self {
            RejectionReason::InvalidTarget => "invalid-target",
            RejectionReason::NoBinding => "no-binding",
            RejectionReason::TargetInScope => "target-in-scope",
            RejectionReason::TargetVisible => "target-visible",
            RejectionReason::CaptureInSubtree => "capture-in-subtree",
            RejectionReason::TargetFreeName => "target-free-name",
            RejectionReason::ShadowsChild => "shadows-child",
            RejectionReason::StaleBinding => "stale-binding",
        }
    }
}

/// `RenameAttempt`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[must_use]
pub struct RenameAttempt {
    pub applied: bool,
    pub reason: Option<RejectionReason>,
}

impl RenameAttempt {
    fn applied() -> RenameAttempt {
        RenameAttempt {
            applied: true,
            reason: None,
        }
    }

    fn rejected(reason: RejectionReason) -> RenameAttempt {
        RenameAttempt {
            applied: false,
            reason: Some(reason),
        }
    }
}

/// `RenameClaimStats` — the TS counter shape. `claims_recorded` counts
/// applied renames; the ledger-only fields are structurally zero here (one
/// scope table; see the module docs).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize)]
pub struct RenameClaimStats {
    #[serde(rename = "ledgerOnlyRejections")]
    pub ledger_only_rejections: u64,
    #[serde(rename = "byGuard")]
    pub by_guard: ClaimGuards,
    #[serde(rename = "claimsRecorded")]
    pub claims_recorded: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize)]
pub struct ClaimGuards {
    #[serde(rename = "targetInScope")]
    pub target_in_scope: u64,
    #[serde(rename = "targetVisible")]
    pub target_visible: u64,
    #[serde(rename = "shadowsChild")]
    pub shadows_child: u64,
}

/// How an applied rename reaches the emitted text (the render's input).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RenameMode {
    /// `fastRenameBinding`: every occurrence renamed in place.
    InPlace,
    /// The binding is export-involved and not an export declaration's own
    /// id: the TS falls back to Babel's `scope.rename`, which renames the
    /// same occurrences and, for a binding declared in `export var/let/const`,
    /// SPLITS the declaration (`const b = 1; export { b as a }`) the first
    /// time — after which the binding is no longer export-involved and later
    /// renames take the in-place path (leaving the synthesized specifier's
    /// local behind — the TS emit carries that; see the WP3.1 report).
    BabelRenamer { splits_export: bool },
}

/// One applied rename, in apply order.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct AppliedRename {
    pub binding: BindingId,
    pub old_name: String,
    pub new_name: String,
    pub mode: RenameMode,
}

/// A validated rename request — the TS call shape
/// `attemptValidatedRename(scope, oldName, newName, expectedBinding?)`.
#[derive(Clone, Copy, Debug)]
pub struct RenameRequest<'n> {
    pub scope: BScopeId,
    pub old_name: &'n str,
    pub new_name: &'n str,
    /// The binding the caller's EVIDENCE was collected on; a different
    /// binding under the old name rejects (`stale-binding`).
    pub expected: Option<BindingId>,
}

/// How the applier records an attempt on the trail.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TrailSpec {
    /// The dominant TS shape (`transferOwnedPair`, the binding cascade, the
    /// LLM apply): when a binding held the old name, record
    /// `{tier, applied|rejected, reason (raw code on rejection), newName}` —
    /// through `record` for a settling tier, `record_post_pass` for a post
    /// pass.
    Standard { tier: Tier, post_pass: bool },
    /// The caller records its own row (a mapped reason, an abstain) through
    /// [`RenameState::record`]. Counted.
    CallerRecords { tier: Tier },
    /// No trail row, as in the TS paths that record elsewhere (uniquify,
    /// identity renames, library prefix → the dump's name records). Counted.
    Untrailed { why: &'static str },
}

impl TrailSpec {
    /// A settling tier's standard row.
    pub fn settling(tier: Tier) -> TrailSpec {
        TrailSpec::Standard {
            tier,
            post_pass: false,
        }
    }

    /// A post pass's standard row.
    pub fn post_pass(tier: Tier) -> TrailSpec {
        TrailSpec::Standard {
            tier,
            post_pass: true,
        }
    }
}

/// The trail-opt-out counters (02 §5: a tier cannot run untrailed unseen).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrailOptOuts {
    pub caller_records: BTreeMap<Tier, u64>,
    pub untrailed: BTreeMap<&'static str, u64>,
}

/// What a finished rename pass hands on: the names to render, the apply
/// log, and the counters/trail for the next pass or the dump.
pub struct RenameOutcome {
    /// Every renamed oxc symbol with its final name, in symbol order.
    pub symbol_names: Vec<(SymbolId, String)>,
    pub applied: Vec<AppliedRename>,
    pub trail: StrategyTrail,
    pub claims: RenameClaimStats,
    pub opt_outs: TrailOptOuts,
}

/// The rename state of one parsed text: the Babel scope view, the name
/// overlay, the current scope maps, the carried-names registry and the
/// trail. The ONLY writer of names.
pub struct RenameState {
    view: BabelScopes,
    anchor: Anchor,
    /// The overlay: per binding, the current name when renamed.
    names: Vec<Option<String>>,
    /// Babel's `scope.bindings`, current names: name → (order, binding).
    maps: Vec<BTreeMap<String, (u64, BindingId)>>,
    next_order: u64,
    /// Export declarations Babel's renamer has split (their bindings are no
    /// longer under an export declaration).
    split_exports: BTreeSet<NodeId>,
    /// `carriedNames`: bindings whose APPLIED name is below the floor.
    carried: BTreeSet<BindingId>,
    claims: RenameClaimStats,
    applied: Vec<AppliedRename>,
    trail: StrategyTrail,
    opt_outs: TrailOptOuts,
}

impl RenameState {
    /// A fresh state over a parsed text, with an armed trail.
    pub fn new(semantic: &Semantic<'_>, anchor: Anchor) -> RenameState {
        RenameState::with_trail(semantic, anchor, StrategyTrail::enabled())
    }

    /// A state that continues an earlier pass's trail (the TS trail is one
    /// recorder across the naming, reconcile and sweep passes).
    pub fn with_trail(
        semantic: &Semantic<'_>,
        anchor: Anchor,
        trail: StrategyTrail,
    ) -> RenameState {
        let view = BabelScopes::build(semantic);
        let maps: Vec<BTreeMap<String, (u64, BindingId)>> = view
            .initial_maps
            .iter()
            .map(|entries| {
                entries
                    .iter()
                    .enumerate()
                    .map(|(i, (name, b))| (name.clone(), (i as u64, *b)))
                    .collect()
            })
            .collect();
        let next_order = maps.iter().map(|m| m.len() as u64).max().unwrap_or(0);
        RenameState {
            names: vec![None; view.bindings.len()],
            view,
            anchor,
            maps,
            next_order,
            split_exports: BTreeSet::new(),
            carried: BTreeSet::new(),
            claims: trail.claims,
            applied: Vec::new(),
            trail,
            opt_outs: TrailOptOuts::default(),
        }
    }

    // -- reads ---------------------------------------------------------------

    /// The Babel scope view (scopes, bindings, sites — crawl-time).
    pub fn view(&self) -> &BabelScopes {
        &self.view
    }

    /// A binding's CURRENT name — the one accessor every module reads.
    pub fn name_of(&self, binding: BindingId) -> &str {
        self.names[binding.0 as usize]
            .as_deref()
            .unwrap_or(&self.view.binding(binding).name)
    }

    /// The current name of the binding an oxc symbol declares.
    pub fn name_of_symbol(&self, symbol: SymbolId) -> Option<&str> {
        self.view.binding_of_symbol(symbol).map(|b| self.name_of(b))
    }

    /// `scope.bindings[name]` (current names).
    pub fn binding_in(&self, scope: BScopeId, name: &str) -> Option<BindingId> {
        self.maps[scope.0 as usize].get(name).map(|&(_, b)| b)
    }

    /// `Object.entries(scope.bindings)` — current names in Babel's map
    /// order (registration order; a rename moves the name to the end).
    pub fn bindings_in(&self, scope: BScopeId) -> Vec<(String, BindingId)> {
        let mut entries: Vec<(u64, &String, BindingId)> = self.maps[scope.0 as usize]
            .iter()
            .map(|(n, &(o, b))| (o, n, b))
            .collect();
        entries.sort();
        entries
            .into_iter()
            .map(|(_, n, b)| (n.clone(), b))
            .collect()
    }

    /// A Babel RE-CRAWL of the scopes `recrawled` selects: each scope's
    /// table goes back to registration (AST) order under the CURRENT names
    /// — the order a freshly crawled Scope object's `Object.keys` shows.
    /// The TS naming waves read block scopes through NEW paths after the
    /// prior-match cache clear (`clearBabelCacheAfterPriorMatch`), so every
    /// rename made before the clear (the transfer stage's) no longer sits
    /// at the end of those tables; renames after it move names to the end
    /// again.
    pub fn recrawl_order(&mut self, recrawled: impl Fn(BScopeId) -> bool) {
        let scopes: Vec<BScopeId> = (0..self.view.initial_maps.len())
            .map(|i| BScopeId(i as u32))
            .filter(|&s| recrawled(s))
            .collect();
        self.recrawl_scopes(&scopes);
    }

    /// [`RenameState::recrawl_order`] over an explicit scope list.
    pub fn recrawl_scopes(&mut self, scopes: &[BScopeId]) {
        for &sid in scopes {
            let i = sid.0 as usize;
            let entries = &self.view.initial_maps[i];
            let map = &mut self.maps[i];
            for (order, (_, b)) in entries.iter().enumerate() {
                let current = self.names[b.0 as usize]
                    .as_deref()
                    .unwrap_or(&self.view.bindings[b.0 as usize].name);
                if let Some(slot) = map.get_mut(current)
                    && slot.1 == *b
                {
                    slot.0 = order as u64;
                }
            }
        }
    }

    /// `scope.getBinding(name)` over the current maps.
    pub fn get_binding(&self, scope: BScopeId, name: &str) -> Option<BindingId> {
        scopes::resolve_in(
            &self.view.scopes,
            name,
            scope,
            |sid, n| self.binding_in(sid, n),
            |b| self.view.binding(b).kind,
        )
    }

    /// `binding.scope` — the scope a binding was registered in.
    pub fn scope_of_binding(&self, binding: BindingId) -> BScopeId {
        self.view.binding(binding).owner
    }

    /// The trail target of a binding in this state's text.
    pub fn trail_target(&self, binding: BindingId) -> TrailTarget {
        TrailTarget {
            anchor: self.anchor,
            decl_span: self.view.binding(binding).id_span,
        }
    }

    pub fn trail(&self) -> &StrategyTrail {
        &self.trail
    }

    /// Every applied rename so far, in apply order.
    pub fn applied(&self) -> &[AppliedRename] {
        &self.applied
    }

    pub fn claim_stats(&self) -> RenameClaimStats {
        self.claims
    }

    /// `carriedNames.isCarried`.
    pub fn is_carried(&self, binding: BindingId) -> bool {
        self.carried.contains(&binding)
    }

    /// `carriedNames.recordedCount`.
    pub fn carried_count(&self) -> usize {
        self.carried.len()
    }

    /// `carriedNames.record` — for tiers that settle a below-floor name
    /// WITHOUT a rename (the binding cascade's same-name settle).
    pub fn record_carried(&mut self, binding: BindingId) {
        self.carried.insert(binding);
    }

    pub fn applied_renames(&self) -> &[AppliedRename] {
        &self.applied
    }

    pub fn opt_outs(&self) -> &TrailOptOuts {
        &self.opt_outs
    }

    // -- export involvement --------------------------------------------------

    /// `isExportInvolved`: the binding's path sits under an export
    /// declaration (not yet split by Babel's renamer), or one of its
    /// reference paths is an export specifier's local.
    pub fn is_export_involved(&self, binding: BindingId) -> bool {
        let b = self.view.binding(binding);
        let under_export = b
            .export_ancestor
            .is_some_and(|node| !self.split_exports.contains(&node));
        under_export || b.specifier_referenced
    }

    /// `isExportDeclarationId`.
    pub fn is_export_declaration_id(&self, binding: BindingId) -> bool {
        self.view.binding(binding).export_declaration_id
    }

    // -- soundness (the rename freeze's consumer) ------------------------------

    /// `isBindingEvalTaintFrozen`: renaming the binding is unsound because
    /// it is visible at a `with` / direct-`eval` site — the binding's
    /// scope's function parent is tainted, or it is module-level and any
    /// site exists.
    pub fn is_eval_taint_frozen(&self, binding: BindingId, taint: &EvalWithTaint) -> bool {
        let owner = self.view.binding(binding).owner;
        let fn_span = self
            .view
            .function_parent(owner)
            .map(|s| self.view.scope(s).span);
        is_binding_eval_taint_frozen(fn_span, taint)
    }

    // -- the rules -------------------------------------------------------------

    /// `getRenameRejection`: the reason a rename must not be applied, or
    /// None when it is safe.
    pub fn get_rename_rejection(
        &self,
        scope: BScopeId,
        old_name: &str,
        new_name: &str,
    ) -> Option<RejectionReason> {
        if !is_valid_rename_target(new_name) {
            return Some(RejectionReason::InvalidTarget);
        }
        if self.binding_in(scope, old_name).is_none() {
            return Some(RejectionReason::NoBinding);
        }
        if self.binding_in(scope, new_name).is_some() {
            return Some(RejectionReason::TargetInScope);
        }
        if self.would_capture_outer_reference(scope, new_name) {
            return Some(RejectionReason::TargetVisible);
        }
        // A rename may never bind a previously-free name (review C1).
        if self.view.globals.contains(new_name) {
            return Some(RejectionReason::TargetFreeName);
        }
        if self.would_rename_shadow_in_child_scope(scope, old_name, new_name) {
            return Some(RejectionReason::ShadowsChild);
        }
        None
    }

    /// `wouldRenameShadowInChildScope`: a scope between one of the
    /// binding's reads or writes and `scope` binds `new_name`. The walk
    /// climbs until it meets `scope` (or runs off the root).
    pub fn would_rename_shadow_in_child_scope(
        &self,
        scope: BScopeId,
        old_name: &str,
        new_name: &str,
    ) -> bool {
        let Some(binding) = self.binding_in(scope, old_name) else {
            return false;
        };
        let b = self.view.binding(binding);
        b.refs
            .iter()
            .chain(&b.violations)
            .any(|site| self.binds_between(site.scope, scope, new_name))
    }

    /// Some scope from `from` up to (excluding) `stop` binds `name`.
    fn binds_between(&self, from: BScopeId, stop: BScopeId, name: &str) -> bool {
        let mut cur = Some(from);
        while let Some(s) = cur.filter(|s| *s != stop) {
            if self.binding_in(s, name).is_some() {
                return true;
            }
            cur = self.view.scope(s).parent;
        }
        false
    }

    /// `wouldCaptureOuterReference`: the binding `new_name` resolves to
    /// from an ancestor of `scope` (a raw map walk) has a read or write
    /// inside `scope`'s block — position containment, inclusive.
    fn would_capture_outer_reference(&self, scope: BScopeId, new_name: &str) -> bool {
        let Some(outer) = self.resolve_outer_binding(scope, new_name) else {
            return false;
        };
        let block = self.view.scope(scope).span;
        let b = self.view.binding(outer);
        let inside = |span: oxc_span::Span| span.start >= block.start && span.end <= block.end;
        // The outer binding's OWN initialized `var` declaration inside the
        // block is a write too: Annex B lets a catch body redeclare `var x`,
        // the declaration hoists but its initializer runs in the catch, so a
        // catch param renamed to `x` swallows it (16-findings-queue #15,
        // fixed TS-first).
        b.refs
            .iter()
            .chain(&b.violations)
            .any(|site| inside(site.span))
            || b.initialized_declarator_span.is_some_and(inside)
    }

    /// `resolveOuterBinding`: the first ancestor map holding the name.
    fn resolve_outer_binding(&self, scope: BScopeId, name: &str) -> Option<BindingId> {
        let mut cur = self.view.scope(scope).parent;
        while let Some(s) = cur {
            if let Some(b) = self.binding_in(s, name) {
                return Some(b);
            }
            cur = self.view.scope(s).parent;
        }
        None
    }

    /// `referencesOwnerInside`: a read or write of `owner` whose node sits
    /// strictly inside `block_scope`'s block (`findParent` excludes the
    /// node itself).
    fn references_owner_inside(&self, owner: BindingId, block_scope: BScopeId) -> bool {
        let scope = self.view.scope(block_scope);
        let b = self.view.binding(owner);
        let inside = |site: &Site| {
            site.node != scope.node
                && site.span.start >= scope.span.start
                && site.span.end <= scope.span.end
        };
        b.refs.iter().chain(&b.violations).any(inside)
    }

    // -- the writers -------------------------------------------------------------

    /// `attemptValidatedRename`: validate and apply, recording the attempt
    /// per `spec`. Callers decide what a rejection means (skip the
    /// transfer, fall back to the LLM, try a conflict-free variant, ...).
    pub fn attempt_validated_rename(
        &mut self,
        request: RenameRequest<'_>,
        spec: TrailSpec,
    ) -> RenameAttempt {
        let trail_binding = self.binding_in(request.scope, request.old_name);
        let attempt = self.validate_and_apply(request);
        self.record_standard(spec, trail_binding, request, attempt);
        attempt
    }

    fn validate_and_apply(&mut self, request: RenameRequest<'_>) -> RenameAttempt {
        let RenameRequest {
            scope,
            old_name,
            new_name,
            expected,
        } = request;
        if expected.is_some() && self.binding_in(scope, old_name) != expected {
            return RenameAttempt::rejected(RejectionReason::StaleBinding);
        }
        if let Some(reason) = self.get_rename_rejection(scope, old_name, new_name) {
            return RenameAttempt::rejected(reason);
        }
        let binding = self
            .binding_in(scope, old_name)
            .expect("the no-binding rule passed");
        let mode = if self.fast_rename_allowed(binding) {
            self.rebind(scope, old_name, new_name, binding);
            RenameMode::InPlace
        } else {
            self.babel_renamer(binding, old_name, new_name)
        };
        self.post_check(scope, old_name, new_name);
        self.claims.claims_recorded += 1;
        // exp066 provenance rule: a below-floor name deliberately APPLIED is
        // carried — the sweep must not re-roll it this run.
        if is_below_floor_name(new_name)
            && let Some(carried) = self.binding_in(scope, new_name)
        {
            self.carried.insert(carried);
        }
        self.log_applied(binding, old_name, new_name, mode);
        RenameAttempt::applied()
    }

    /// `attemptShadowingRename`: rename a class/function EXPRESSION's own
    /// id to the name of the outer binding it is assigned to — the
    /// deliberate shadow `X = class X {}` — keeping every other guard and
    /// adding the subtree-capture check. Never falls back to Babel's renamer.
    pub fn attempt_shadowing_rename(
        &mut self,
        inner: BindingId,
        owner: BindingId,
        new_name: &str,
        spec: TrailSpec,
    ) -> RenameAttempt {
        let scope = self.view.binding(inner).owner;
        let old_name = self.name_of(inner).to_string();
        let request = RenameRequest {
            scope,
            old_name: &old_name,
            new_name,
            expected: None,
        };
        let trail_binding = self.binding_in(scope, &old_name);
        let attempt = self.validate_shadowing(owner, request);
        self.record_standard(spec, trail_binding, request, attempt);
        attempt
    }

    fn validate_shadowing(
        &mut self,
        owner: BindingId,
        request: RenameRequest<'_>,
    ) -> RenameAttempt {
        let RenameRequest {
            scope,
            old_name,
            new_name,
            ..
        } = request;
        if !is_valid_rename_target(new_name) {
            return RenameAttempt::rejected(RejectionReason::InvalidTarget);
        }
        if self.name_of(owner) != new_name || self.binding_in(scope, old_name).is_none() {
            return RenameAttempt::rejected(RejectionReason::NoBinding);
        }
        if self.binding_in(scope, new_name).is_some() {
            return RenameAttempt::rejected(RejectionReason::TargetInScope);
        }
        if self.references_owner_inside(owner, scope) {
            return RenameAttempt::rejected(RejectionReason::CaptureInSubtree);
        }
        if self.would_rename_shadow_in_child_scope(scope, old_name, new_name) {
            return RenameAttempt::rejected(RejectionReason::ShadowsChild);
        }
        let binding = self
            .binding_in(scope, old_name)
            .expect("the no-binding rule passed");
        if !self.fast_rename_allowed(binding) {
            // An export-involved inner id: the TS refuses the fallback.
            return RenameAttempt::rejected(RejectionReason::TargetInScope);
        }
        self.rebind(scope, old_name, new_name, binding);
        self.post_check(scope, old_name, new_name);
        self.claims.claims_recorded += 1;
        self.log_applied(binding, old_name, new_name, RenameMode::InPlace);
        RenameAttempt::applied()
    }

    /// `fastRenameBinding`'s gate: not export-involved, or the export
    /// declaration's own id (which renames in place).
    fn fast_rename_allowed(&self, binding: BindingId) -> bool {
        !self.is_export_involved(binding) || self.is_export_declaration_id(binding)
    }

    /// The map update both rename paths make on ONE scope:
    /// `scope.bindings[new] = binding; delete scope.bindings[old]` — the new
    /// key lands at the END of the map order. The overlay takes the new
    /// name. `fastRenameBinding` throws on an unvalidated target; here the
    /// only callers validated first, and the assertion keeps it that way.
    fn rebind(&mut self, scope: BScopeId, old_name: &str, new_name: &str, binding: BindingId) {
        assert!(
            is_valid_rename_target(new_name),
            "invalid rename target {new_name:?} reached the applier — callers must validate first"
        );
        let map = &mut self.maps[scope.0 as usize];
        map.remove(old_name);
        map.insert(new_name.to_string(), (self.next_order, binding));
        self.next_order += 1;
        self.names[binding.0 as usize] = Some(new_name.to_string());
    }

    /// Babel's `scope.rename` for an export-involved binding: the renamer
    /// updates `binding.scope`'s map and, the first time a binding declared
    /// in `export var/let/const` is renamed, splits that declaration (every
    /// binding it declares then leaves the export).
    fn babel_renamer(&mut self, binding: BindingId, old_name: &str, new_name: &str) -> RenameMode {
        let b = self.view.binding(binding);
        let owner = b.owner;
        let split_node = b
            .export_ancestor
            .filter(|node| b.declared_in_export_var && !self.split_exports.contains(node));
        if let Some(node) = split_node {
            self.split_exports.insert(node);
        }
        self.rebind(owner, old_name, new_name, binding);
        RenameMode::BabelRenamer {
            splits_export: split_node.is_some(),
        }
    }

    /// The post-rename spot check: the binding must now live under the new
    /// name in `scope`, and nothing under the old. A split binding here is
    /// a bug minutes before the output parse gate — fail loud, as the TS
    /// throws.
    fn post_check(&self, scope: BScopeId, old_name: &str, new_name: &str) {
        let new_present = self.binding_in(scope, new_name).is_some();
        let old_present = self.binding_in(scope, old_name).is_some();
        assert!(
            new_present && !old_present,
            "rename {old_name}→{new_name} left scope bindings inconsistent \
             (new present: {new_present}, old present: {old_present})"
        );
    }

    fn log_applied(&mut self, binding: BindingId, old: &str, new: &str, mode: RenameMode) {
        self.applied.push(AppliedRename {
            binding,
            old_name: old.to_string(),
            new_name: new.to_string(),
            mode,
        });
    }

    // -- the trail ---------------------------------------------------------------

    fn record_standard(
        &mut self,
        spec: TrailSpec,
        trail_binding: Option<BindingId>,
        request: RenameRequest<'_>,
        attempt: RenameAttempt,
    ) {
        let (tier, post_pass) = match spec {
            TrailSpec::Standard { tier, post_pass } => (tier, post_pass),
            TrailSpec::CallerRecords { tier } => {
                *self.opt_outs.caller_records.entry(tier).or_insert(0) += 1;
                return;
            }
            TrailSpec::Untrailed { why } => {
                *self.opt_outs.untrailed.entry(why).or_insert(0) += 1;
                return;
            }
        };
        let Some(binding) = trail_binding else {
            return;
        };
        let outcome = if attempt.applied {
            Outcome::Applied
        } else {
            Outcome::Rejected
        };
        let mut row = Attempt::new(tier, outcome).proposed(request.new_name);
        if let Some(reason) = attempt.reason {
            row = row.reason(reason.as_str());
        }
        self.record(binding, request.old_name, row, post_pass);
    }

    /// Record a non-rename outcome (a vote, an abstain, a caller-shaped
    /// rejection) for a binding — the trail's only other writer.
    pub fn record(
        &mut self,
        binding: BindingId,
        old_name: &str,
        attempt: Attempt,
        post_pass: bool,
    ) {
        let target = self.trail_target(binding);
        if post_pass {
            self.trail.record_post_pass(target, old_name, attempt);
        } else {
            self.trail.record(target, old_name, attempt);
        }
    }

    /// Hand the pass's results on: the renamed symbols (for the render),
    /// the apply log, the trail and the counters.
    pub fn finish(self) -> RenameOutcome {
        let mut symbol_names: Vec<(SymbolId, String)> = self
            .names
            .iter()
            .enumerate()
            .filter_map(|(i, n)| {
                n.as_ref()
                    .map(|n| (self.view.binding(BindingId(i as u32)).symbol, n.clone()))
            })
            .collect();
        symbol_names.sort_by_key(|(s, _)| s.index());
        let mut trail = self.trail;
        trail.claims = self.claims;
        RenameOutcome {
            symbol_names,
            applied: self.applied,
            trail,
            claims: self.claims,
            opt_outs: self.opt_outs,
        }
    }
}

#[cfg(test)]
mod validated_test;
