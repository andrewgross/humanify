//! The post-render family permutation (exp036 idea 8b) — TS:
//! `src/rename/family-permute.ts` (the pure bucket assignment) and
//! `src/rename/family-permute-step.ts` (members, buckets, the atomic
//! vacate-then-fill apply).
//!
//! Top-level bindings (the wrapper function's scope, then the program's)
//! bucket by the statement hash of their declaring statement; within a
//! bucket, a fresh member takes the prior name whose MASKED usage contexts
//! (reference lines with the member's own name blanked) it shares most —
//! only when that strictly beats staying on its own name's counterpart.
//! Greedy by (support desc, fresh index asc, prior index asc) over members
//! in declaration order: deterministic, so the pass is self-hop stable.
//!
//! Statement-hash BYTES differ between the TS and the Rust hash (02 §4a);
//! only the partition is read here (bucket equality across the two texts),
//! plus the first 8 hex digits in the swap temporaries' names, which never
//! ship and cannot collide with a program name.

use std::collections::{HashMap, HashSet};

use oxc_allocator::Allocator;
use oxc_ast::AstKind;
use oxc_semantic::Semantic;
use oxc_span::{GetSpan, Span};
use serde_json::Value;

use super::sweep::{is_statement, statement_parent};
use crate::babel_view::BabelLines;
use crate::hash::statement_hash::statement_hash;
use crate::ingest::{Ingest, program_estree_json};
use crate::modules::wrapper::find_wrapper_function;
use crate::naming::waves::render::render_program;
use crate::rename::eligibility::Eligibility;
use crate::rename::floor::{is_bun_token, is_decorated_descriptive};
use crate::rename::validated::scopes::{BScopeId, BindingId};
use crate::rename::validated::{RenameRequest, RenameState, TrailSpec};
use crate::trail::{Anchor, StrategyTrail};

/// A bucket member (`BucketMember`): its name and masked usage contexts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BucketMember {
    pub name: String,
    pub contexts: Vec<String>,
}

/// A planned move (`ContextAssignment`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextAssignment {
    pub from_name: String,
    pub to_name: String,
    pub support: usize,
}

/// `isRestorableTarget`: a real descriptive word, never a mint.
fn is_restorable_target(name: &str) -> bool {
    if name.encode_utf16().count() <= 2 || name.starts_with("__") {
        return false;
    }
    if is_bun_token(name) && !is_decorated_descriptive(name) {
        return false;
    }
    let b = name.as_bytes();
    b.windows(3).any(|w| w.iter().all(u8::is_ascii_lowercase))
        || b.windows(2)
            .any(|w| w[0].is_ascii_uppercase() && w[1].is_ascii_lowercase())
}

/// `assignBucket`.
pub fn assign_bucket(
    fresh: &[BucketMember],
    prior: &[BucketMember],
    is_eligible: &dyn Fn(&str) -> bool,
) -> Vec<ContextAssignment> {
    assign_bucket_ordered(fresh, prior, is_eligible, false)
}

/// [`assign_bucket`] with the fresh-index tie-break optionally REVERSED
/// (the gate's planted reorder; never set in the pipeline).
fn assign_bucket_ordered(
    fresh: &[BucketMember],
    prior: &[BucketMember],
    is_eligible: &dyn Fn(&str) -> bool,
    reversed_ties: bool,
) -> Vec<ContextAssignment> {
    let fresh_ctx: Vec<HashSet<&str>> = fresh
        .iter()
        .map(|m| m.contexts.iter().map(String::as_str).collect())
        .collect();
    let prior_ctx: Vec<HashSet<&str>> = prior
        .iter()
        .map(|m| m.contexts.iter().map(String::as_str).collect())
        .collect();
    let support = |fi: usize, pi: usize| {
        fresh_ctx[fi]
            .iter()
            .filter(|c| prior_ctx[pi].contains(*c))
            .count()
    };
    // `new Map(prior.map((m, i) => [m.name, i]))` — the LAST index wins.
    let mut prior_index: HashMap<&str, usize> = HashMap::new();
    for (i, m) in prior.iter().enumerate() {
        prior_index.insert(m.name.as_str(), i);
    }
    let mut candidates: Vec<(usize, usize, usize)> = Vec::new();
    for (fi, f) in fresh.iter().enumerate() {
        if !is_eligible(&f.name) {
            continue;
        }
        let bar = prior_index
            .get(f.name.as_str())
            .map_or(0, |&own| support(fi, own));
        for (pi, p) in prior.iter().enumerate() {
            if !is_restorable_target(&p.name) {
                continue;
            }
            let w = support(fi, pi);
            if w >= 1 && w > bar {
                candidates.push((fi, pi, w));
            }
        }
    }
    candidates.sort_by(|a, b| {
        let fresh_order = if reversed_ties {
            b.0.cmp(&a.0)
        } else {
            a.0.cmp(&b.0)
        };
        b.2.cmp(&a.2).then(fresh_order).then(a.1.cmp(&b.1))
    });
    let mut used_fresh = HashSet::new();
    let mut used_prior = HashSet::new();
    let mut out = Vec::new();
    for (fi, pi, w) in candidates {
        if used_fresh.contains(&fi) || used_prior.contains(&pi) {
            continue;
        }
        used_fresh.insert(fi);
        used_prior.insert(pi);
        if fresh[fi].name != prior[pi].name {
            out.push(ContextAssignment {
                from_name: fresh[fi].name.clone(),
                to_name: prior[pi].name.clone(),
                support: w,
            });
        }
    }
    out
}

/// One top-level binding as a member (`MemberInfo`).
#[derive(Clone, Debug)]
struct MemberInfo {
    member: BucketMember,
    hash: String,
    decl_start: u32,
    binding: BindingId,
}

/// `maskName`: every whole-word occurrence of `name` blanked to `\x00`.
fn mask_name(line: &str, name: &str) -> String {
    let b = line.as_bytes();
    let start = |c: u8| c.is_ascii_alphabetic() || c == b'_' || c == b'$';
    let cont = |c: u8| start(c) || c.is_ascii_digit();
    let mut out = String::with_capacity(line.len());
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        if start(b[i]) {
            let s = i;
            i += 1;
            while i < b.len() && cont(b[i]) {
                i += 1;
            }
            if &line[s..i] == name {
                out.push_str(&line[last..s]);
                out.push('\0');
                last = i;
            }
        } else {
            i += 1;
        }
    }
    out.push_str(&line[last..]);
    out
}

/// The statement node types the ESTree index accepts.
fn is_statement_type(ty: &str) -> bool {
    ty.ends_with("Statement") || ty.ends_with("Declaration")
}

/// Statement JSON by span (the outermost statement node with that span).
fn statement_json_index<'v>(
    root: &'v Value,
    wanted: &HashSet<(u64, u64)>,
) -> HashMap<(u64, u64), &'v Value> {
    let mut out = HashMap::new();
    let mut stack: Vec<&Value> = vec![root];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(map) => {
                if let (Some(ty), Some(s), Some(e)) = (
                    map.get("type").and_then(Value::as_str),
                    map.get("start").and_then(Value::as_u64),
                    map.get("end").and_then(Value::as_u64),
                ) && is_statement_type(ty)
                    && wanted.contains(&(s, e))
                {
                    out.entry((s, e)).or_insert(v);
                }
                for c in map.values() {
                    if c.is_object() || c.is_array() {
                        stack.push(c);
                    }
                }
            }
            Value::Array(items) => stack.extend(items.iter()),
            _ => {}
        }
    }
    out
}

/// The wrapper function's Babel scope when its body is a block.
fn wrapper_scope(semantic: &Semantic<'_>, state: &RenameState) -> Option<BScopeId> {
    let program = semantic.nodes().program();
    let wrapper = find_wrapper_function(program, semantic)?;
    let view = state.view();
    let node = semantic.nodes().iter().find(|n| {
        n.kind().span() == wrapper.span
            && matches!(
                n.kind(),
                AstKind::Function(_) | AstKind::ArrowFunctionExpression(_)
            )
    })?;
    if let AstKind::ArrowFunctionExpression(a) = node.kind()
        && !matches!(a.body, oxc_ast::ast::ArrowFunctionBody::FunctionBody(_))
    {
        return None;
    }
    (0..view.scopes.len())
        .map(|i| BScopeId(i as u32))
        .find(|&s| view.scope(s).node == node.id())
}

/// `collectMembers`: the wrapper scope's bindings, then the program's, each
/// once, with their declaring statement's hash and masked contexts.
fn collect_members(
    semantic: &Semantic<'_>,
    state: &RenameState,
) -> Result<Vec<MemberInfo>, String> {
    let text = semantic.source_text();
    let lines: Vec<&str> = text.split('\n').collect();
    let babel_lines = BabelLines::new(text);
    let view = state.view();
    let nodes = semantic.nodes();
    let mut scopes = Vec::new();
    if let Some(w) = wrapper_scope(semantic, state) {
        scopes.push(w);
    }
    scopes.push(view.program_scope());
    let mut seen = HashSet::new();
    let mut raw: Vec<(String, BindingId, Span)> = Vec::new();
    for scope in scopes {
        for (name, binding) in state.bindings_in(scope) {
            if !seen.insert(binding) {
                continue;
            }
            let path = view.binding(binding).path_node;
            let stmt = if is_statement(&nodes.kind(path)) {
                Some(path)
            } else {
                statement_parent(semantic, path)
            };
            let Some(stmt) = stmt else { continue };
            raw.push((name, binding, nodes.get_node(stmt).span()));
        }
    }
    let wanted: HashSet<(u64, u64)> = raw
        .iter()
        .map(|(_, _, s)| (u64::from(s.start), u64::from(s.end)))
        .collect();
    let program_json = program_estree_json(semantic.nodes().program());
    let index = statement_json_index(&program_json, &wanted);
    let mut spans: Vec<(u64, u64)> = wanted.into_iter().collect();
    spans.sort_unstable();
    if let Some(missing) = spans.iter().find(|k| !index.contains_key(*k)) {
        return Err(format!(
            "declaring statement {}..{} is absent from the estree json",
            missing.0, missing.1
        ));
    }
    let hashes = crate::par::map_ordered(&spans, |k| statement_hash(index[k]));
    let hash_of: HashMap<(u64, u64), String> = spans.into_iter().zip(hashes).collect();
    Ok(raw
        .into_iter()
        .map(|(name, binding, stmt)| {
            let contexts = view
                .binding(binding)
                .refs
                .iter()
                .filter_map(|r| {
                    let ln = babel_lines.line(r.span.start);
                    lines.get(ln - 1).map(|l| mask_name(l, &name))
                })
                .collect();
            MemberInfo {
                hash: hash_of[&(u64::from(stmt.start), u64::from(stmt.end))].clone(),
                decl_start: stmt.start,
                binding,
                member: BucketMember { name, contexts },
            }
        })
        .collect())
}

/// `byHash`: members grouped by hash, in first-appearance order.
fn by_hash(members: Vec<MemberInfo>) -> Vec<(String, Vec<MemberInfo>)> {
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut out: Vec<(String, Vec<MemberInfo>)> = Vec::new();
    for m in members {
        let i = *index.entry(m.hash.clone()).or_insert_with(|| {
            out.push((m.hash.clone(), Vec::new()));
            out.len() - 1
        });
        out[i].1.push(m);
    }
    out
}

/// `byDeclOrder` (a stable sort by declaration start).
fn by_decl_order(members: &[MemberInfo]) -> Vec<BucketMember> {
    let mut sorted: Vec<&MemberInfo> = members.iter().collect();
    sorted.sort_by_key(|m| m.decl_start);
    sorted.into_iter().map(|m| m.member.clone()).collect()
}

/// One applied (or planned) move with its evidence (`AppliedMove`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppliedMove {
    pub from: String,
    pub to: String,
    pub support: usize,
    /// The bucket's hash (Rust bytes — a partition label only).
    pub bucket: String,
}

#[derive(Clone)]
struct PlannedMove {
    binding: BindingId,
    mv: AppliedMove,
}

/// A planted order bug (the gate's red runs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PermutePlant {
    /// Take equal-support candidates in DESCENDING fresh-index order.
    TieBreakReversed,
}

fn plan_bucket_moves(
    fresh: &[(String, Vec<MemberInfo>)],
    prior: &HashMap<String, Vec<MemberInfo>>,
    eligible: &Eligibility,
    plant: Option<PermutePlant>,
) -> (Vec<PlannedMove>, usize) {
    let mut to_apply = Vec::new();
    let mut buckets = 0;
    let is_eligible = |n: &str| eligible.is_eligible(n);
    for (hash, fresh_members) in fresh {
        let Some(prior_members) = prior.get(hash) else {
            continue;
        };
        if prior_members.len() < 2 {
            continue;
        }
        let moves = assign_bucket_ordered(
            &by_decl_order(fresh_members),
            &by_decl_order(prior_members),
            &is_eligible,
            plant == Some(PermutePlant::TieBreakReversed),
        );
        if moves.is_empty() {
            continue;
        }
        buckets += 1;
        // `new Map(freshMembers.map(m => [m.name, m.binding]))` — last wins.
        let mut by_name: HashMap<&str, BindingId> = HashMap::new();
        for m in fresh_members {
            by_name.insert(m.member.name.as_str(), m.binding);
        }
        for mv in moves {
            if let Some(&binding) = by_name.get(mv.from_name.as_str()) {
                to_apply.push(PlannedMove {
                    binding,
                    mv: AppliedMove {
                        from: mv.from_name,
                        to: mv.to_name,
                        support: mv.support,
                        bucket: hash.clone(),
                    },
                });
            }
        }
    }
    (to_apply, buckets)
}

/// `landableMoves`: drop moves whose target is held by a binding that is
/// not itself moving away, to a fixpoint.
fn landable_moves(state: &RenameState, plan: &[PlannedMove]) -> Vec<PlannedMove> {
    let mut live: Vec<PlannedMove> = plan.to_vec();
    loop {
        let moving: HashSet<BindingId> = live.iter().map(|m| m.binding).collect();
        let next: Vec<PlannedMove> = live
            .iter()
            .filter(|m| {
                let scope = state.scope_of_binding(m.binding);
                state
                    .get_binding(scope, &m.mv.to)
                    .is_none_or(|holder| moving.contains(&holder))
            })
            .cloned()
            .collect();
        if next.len() == live.len() {
            return live;
        }
        live = next;
    }
}

fn rename(state: &mut RenameState, binding: BindingId, from: &str, to: &str) -> bool {
    state
        .attempt_validated_rename(
            RenameRequest {
                scope: state.scope_of_binding(binding),
                old_name: from,
                new_name: to,
                expected: None,
            },
            TrailSpec::Untrailed {
                why: "family-permute",
            },
        )
        .applied
}

/// `applyBucketPlan`: vacate every source to a unique temporary, then fill
/// each target; a failing fill reverts the whole bucket.
fn apply_bucket_plan(state: &mut RenameState, plan: &[PlannedMove]) -> Vec<AppliedMove> {
    let mut staged: Vec<(PlannedMove, String)> = Vec::new();
    for (i, m) in landable_moves(state, plan).into_iter().enumerate() {
        let prefix: String = m.mv.bucket.chars().take(8).collect();
        let temp = format!("__familyPermuteSwap{i}${prefix}");
        if rename(state, m.binding, &m.mv.from, &temp) {
            staged.push((m, temp));
        }
    }
    let mut filled: Vec<usize> = Vec::new();
    for (k, (m, temp)) in staged.iter().enumerate() {
        if rename(state, m.binding, temp, &m.mv.to) {
            filled.push(k);
        } else {
            for &f in &filled {
                let (fm, ft) = &staged[f];
                rename(state, fm.binding, &fm.mv.to, ft);
            }
            for (sm, st) in &staged {
                rename(state, sm.binding, st, &sm.mv.from);
            }
            return Vec::new();
        }
    }
    filled.into_iter().map(|k| staged[k].0.mv.clone()).collect()
}

/// `applyPlan`: buckets in plan order, each independently.
fn apply_plan(state: &mut RenameState, plan: &[PlannedMove]) -> Vec<AppliedMove> {
    let mut order: Vec<String> = Vec::new();
    let mut by_bucket: HashMap<String, Vec<PlannedMove>> = HashMap::new();
    for m in plan {
        by_bucket
            .entry(m.mv.bucket.clone())
            .or_insert_with(|| {
                order.push(m.mv.bucket.clone());
                Vec::new()
            })
            .push(m.clone());
    }
    let mut applied = Vec::new();
    for bucket in order {
        applied.extend(apply_bucket_plan(state, &by_bucket[&bucket]));
    }
    applied
}

/// `FamilyPermuteOutcome`.
#[derive(Clone, Debug, Default)]
pub struct FamilyPermuteOutcome {
    pub applied: usize,
    pub buckets: usize,
    pub skipped: usize,
    pub moves: Vec<AppliedMove>,
    /// The re-rendered text — set only when a move applied.
    pub code: Option<String>,
}

/// The prior side's members by hash (plain data; the prior parse is
/// dropped before the fresh side is built, as the TS releases it).
fn prior_by_hash(prior_text: &str) -> Result<HashMap<String, Vec<MemberInfo>>, String> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, prior_text);
    if !ingest.errors.is_empty() {
        return Err(format!("prior text does not parse: {}", ingest.errors[0]));
    }
    let semantic = ingest.semantic();
    let state = RenameState::with_trail(semantic, Anchor::Shipped, StrategyTrail::default());
    Ok(by_hash(collect_members(semantic, &state)?)
        .into_iter()
        .collect())
}

/// `runFamilyPermute(code, prior)`. Err when either text does not parse
/// (the TS returns undefined and the caller ships its input).
pub fn run_family_permute(
    code: &str,
    prior_text: &str,
    eligible: &Eligibility,
    plant: Option<PermutePlant>,
) -> Result<FamilyPermuteOutcome, String> {
    let prior = prior_by_hash(prior_text)?;
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        return Err(format!("text does not parse: {}", ingest.errors[0]));
    }
    let semantic = ingest.semantic();
    let mut state = RenameState::with_trail(semantic, Anchor::Shipped, StrategyTrail::default());
    let fresh = by_hash(collect_members(semantic, &state)?);
    let (to_apply, buckets) = plan_bucket_moves(&fresh, &prior, eligible, plant);
    if to_apply.is_empty() {
        return Ok(FamilyPermuteOutcome {
            buckets,
            ..FamilyPermuteOutcome::default()
        });
    }
    let moves = apply_plan(&mut state, &to_apply);
    if moves.is_empty() {
        return Ok(FamilyPermuteOutcome {
            buckets,
            skipped: to_apply.len(),
            ..FamilyPermuteOutcome::default()
        });
    }
    Ok(FamilyPermuteOutcome {
        applied: moves.len(),
        buckets,
        skipped: to_apply.len() - moves.len(),
        code: Some(render_program(semantic, &state)),
        moves,
    })
}
