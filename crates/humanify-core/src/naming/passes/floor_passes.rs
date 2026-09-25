//! The naming floor's two DETERMINISTIC passes over the naming-era state —
//! TS: `src/rename/class-id-floor.ts` (class/function-expression inner-id
//! derivation) and `src/rename/decoration-retry.ts` (restore a decorated
//! name's bare stem). No LLM; every apply goes through validated rename,
//! every gate skips; eval/with-frozen bindings are never touched.

use oxc_ast::AstKind;
use oxc_ast::ast::{AssignmentTarget, BindingPattern};
use oxc_semantic::Semantic;

use super::census::{MintedFamily, babel_parent, collect_minted_bindings};
use crate::modules::soundness::EvalWithTaint;
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::is_decorated_descriptive;
use crate::rename::validated::scopes::BindingId;
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Attempt, Outcome, Tier};

/// One class-id-floor skip (`ClassIdFloorSkip`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassIdFloorSkip {
    pub name: String,
    pub to_name: Option<String>,
    pub reason: String,
}

/// `ClassIdFloorResult`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassIdFloorResult {
    pub derived: usize,
    pub skipped: Vec<ClassIdFloorSkip>,
}

/// `ownerBindingFor`: the outer binding the derivation copies when the
/// expression is a declarator's init or a plain-identifier assignment's
/// value (the deliberate shadow); None for property/member targets.
fn owner_binding_for(
    semantic: &Semantic<'_>,
    state: &RenameState,
    binding: BindingId,
    name: &str,
) -> Option<BindingId> {
    let expr = state.view().binding(binding).path_node;
    let parent = babel_parent(semantic, expr)?;
    let is_ident_target = match semantic.nodes().kind(parent) {
        AstKind::VariableDeclarator(d) => matches!(d.id, BindingPattern::BindingIdentifier(_)),
        AstKind::AssignmentExpression(a) => {
            matches!(a.left, AssignmentTarget::AssignmentTargetIdentifier(_))
        }
        _ => false,
    };
    if !is_ident_target {
        return None;
    }
    state.get_binding(state.view().scope_of_node(parent), name)
}

/// `deriveExpressionInnerNames`.
pub fn derive_expression_inner_names(
    semantic: &Semantic<'_>,
    state: &mut RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
) -> ClassIdFloorResult {
    let candidates: Vec<_> = collect_minted_bindings(semantic, state, eligible)
        .entries
        .into_iter()
        .filter(|b| matches!(b.family, MintedFamily::ClassExprId | MintedFamily::FnExprId))
        .collect();
    let mut result = ClassIdFloorResult::default();
    for cand in candidates {
        let to_name = cand.derived_from.clone();
        let mut skip = |state: &mut RenameState, reason: String| {
            let mut attempt =
                Attempt::new(Tier::ClassIdFloor, Outcome::Abstained).reason(reason.clone());
            if let Some(t) = &to_name {
                attempt = attempt.proposed(t.clone());
            }
            state.record(cand.binding, &cand.name, attempt, true);
            result.skipped.push(ClassIdFloorSkip {
                name: cand.name.clone(),
                to_name: to_name.clone(),
                reason,
            });
        };
        let Some(target) = to_name.clone() else {
            skip(state, "no-derivation-source".to_string());
            continue;
        };
        if state.is_eval_taint_frozen(cand.binding, taint) {
            skip(state, "eval-taint-frozen".to_string());
            continue;
        }
        let spec = TrailSpec::CallerRecords {
            tier: Tier::ClassIdFloor,
        };
        let attempt = match owner_binding_for(semantic, state, cand.binding, &target) {
            Some(owner) => state.attempt_shadowing_rename(cand.binding, owner, &target, spec),
            None => state.attempt_validated_rename(
                RenameRequest {
                    scope: state.scope_of_binding(cand.binding),
                    old_name: &cand.name,
                    new_name: &target,
                    expected: None,
                },
                spec,
            ),
        };
        if attempt.applied {
            result.derived += 1;
            let row = Attempt::new(Tier::ClassIdFloor, Outcome::Applied).proposed(target);
            state.record(cand.binding, &cand.name, row, true);
        } else {
            let reason = match attempt.reason.map(|r| r.as_str()) {
                Some("capture-in-subtree") => "capture-in-subtree".to_string(),
                r => format!("rename-rejected:{}", r.unwrap_or("unknown")),
            };
            skip(state, reason);
        }
    }
    result
}

/// `DecorationRetryResult`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecorationRetryResult {
    pub undecorated: usize,
    pub skipped: usize,
}

/// `retryDecoratedNames`: retry each decorated-descriptive name's bare stem
/// (`initializeApp_` → `initializeApp`) through validated rename.
pub fn retry_decorated_names(
    semantic: &Semantic<'_>,
    state: &mut RenameState,
    eligible: &Eligibility,
    taint: &EvalWithTaint,
) -> DecorationRetryResult {
    let mut result = DecorationRetryResult::default();
    for entry in collect_minted_bindings(semantic, state, eligible).entries {
        if !is_decorated_descriptive(&entry.name) {
            continue;
        }
        if state.is_eval_taint_frozen(entry.binding, taint) {
            result.skipped += 1;
            let row =
                Attempt::new(Tier::DecorationRetry, Outcome::Abstained).reason("eval-taint-frozen");
            state.record(entry.binding, &entry.name, row, true);
            continue;
        }
        let stem = entry.name.trim_end_matches('_').to_string();
        let attempt = state.attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(entry.binding),
                old_name: &entry.name,
                new_name: &stem,
                expected: None,
            },
            TrailSpec::CallerRecords {
                tier: Tier::DecorationRetry,
            },
        );
        if attempt.applied {
            result.undecorated += 1;
            let row = Attempt::new(Tier::DecorationRetry, Outcome::Applied).proposed(stem);
            state.record(entry.binding, &entry.name, row, true);
        } else {
            result.skipped += 1;
            let reason = attempt.reason.map_or("still-blocked", |r| r.as_str());
            let row = Attempt::new(Tier::DecorationRetry, Outcome::Abstained)
                .reason(reason)
                .proposed(stem);
            state.record(entry.binding, &entry.name, row, true);
        }
    }
    result
}
