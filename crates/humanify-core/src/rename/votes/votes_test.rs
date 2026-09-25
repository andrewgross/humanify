//! TS originals: `src/rename/single-vote-pin.test.ts` (the ladder + the
//! suggestion ranking) and prior-transfer's `getTopVote` floor.

use std::collections::{BTreeSet, HashMap};

use super::{
    PinRequest, PinResult, VoteCount, Votes, get_top_vote, rank_vote_suggestion,
    try_single_vote_pin,
};
use crate::rename::validated::RenameState;
use crate::rename::validated::test_support::with_semantic;
use crate::trail::{Anchor, Tier};
use crate::twins::role::BindingRole;

fn role(hash: Option<&str>) -> BindingRole {
    BindingRole {
        structural_hash: hash.map(str::to_string),
        content_shingles: None,
        fn_callee_ids: Vec::new(),
        has_binding_callees: false,
    }
}

fn votes(entries: &[(&str, u64, u64)]) -> Votes {
    entries
        .iter()
        .map(|(n, total, exact)| {
            (
                n.to_string(),
                VoteCount {
                    total: *total,
                    exact: *exact,
                },
            )
        })
        .collect()
}

/// One exact vote for `packItem`, hash-equal roles — the happy path; each
/// case overrides one field.
struct Case {
    votes: Votes,
    claimants: HashMap<String, u64>,
    prior_roles: HashMap<String, BindingRole>,
    fn_matches: HashMap<String, String>,
    fresh: BindingRole,
}

impl Case {
    fn base() -> Case {
        Case {
            votes: votes(&[("packItem", 1, 1)]),
            claimants: HashMap::from([("packItem".to_string(), 1)]),
            prior_roles: HashMap::from([("packItem".to_string(), role(Some("h1")))]),
            fn_matches: HashMap::new(),
            fresh: role(Some("h1")),
        }
    }

    fn run(self, code: &str) -> (PinResult, Vec<String>, bool) {
        with_semantic(code, true, |semantic| {
            let mut state = RenameState::new(semantic, Anchor::Fresh);
            let scope = state.view().program_scope();
            let req = PinRequest {
                votes: &self.votes,
                name_claimants: &self.claimants,
                prior_roles: &self.prior_roles,
                fn_matches: &self.fn_matches,
                scope,
                old_name: "q7",
                tier: Tier::ModulePin,
            };
            let fresh = self.fresh.clone();
            let result = try_single_vote_pin(&mut state, &req, || fresh);
            let names: Vec<String> = state
                .bindings_in(scope)
                .into_iter()
                .map(|(n, _)| n)
                .collect();
            let carried = state.carried_count() > 0;
            (result, names, carried)
        })
    }
}

fn blocked(r: &PinResult) -> Option<&str> {
    match r {
        PinResult::NotPinned { blocked } => blocked.as_deref(),
        PinResult::Pinned { .. } => None,
    }
}

#[test]
fn pins_on_one_exact_vote_with_hash_equal_roles() {
    let (r, names, _) = Case::base().run("function q7(v) { return v; } q7(1);");
    assert_eq!(
        r,
        PinResult::Pinned {
            name: "packItem".into(),
            role_reason: "hash-equal"
        }
    );
    assert!(names.contains(&"packItem".to_string()));
    assert!(!names.contains(&"q7".to_string()));
}

#[test]
fn refuses_silently_when_more_than_one_name_has_votes() {
    let mut c = Case::base();
    c.votes = votes(&[("packItem", 1, 1), ("packOther", 1, 1)]);
    let (r, names, _) = c.run("function q7(v) { return v; }");
    assert_eq!(r, PinResult::NotPinned { blocked: None });
    assert!(names.contains(&"q7".to_string()));
}

#[test]
fn refuses_a_vote_without_exact_slot_testimony() {
    let mut c = Case::base();
    c.votes = votes(&[("packItem", 1, 0)]);
    assert_eq!(
        blocked(&c.run("function q7(v) { return v; }").0),
        Some("non-exact-source")
    );
}

#[test]
fn refuses_when_a_second_binding_claims_the_name() {
    let mut c = Case::base();
    c.claimants.insert("packItem".into(), 2);
    assert_eq!(
        blocked(&c.run("function q7(v) { return v; }").0),
        Some("name-conflict")
    );
}

#[test]
fn refuses_without_prior_role_evidence() {
    let mut c = Case::base();
    c.prior_roles.clear();
    assert_eq!(
        blocked(&c.run("function q7(v) { return v; }").0),
        Some("no-prior-role")
    );
}

#[test]
fn refuses_when_roles_do_not_corroborate() {
    let mut c = Case::base();
    c.prior_roles.insert("packItem".into(), role(Some("h2")));
    let r = c.run("function q7(v) { return v; }").0;
    assert!(blocked(&r).is_some_and(|b| b.starts_with("role-mismatch:")));
}

#[test]
fn refuses_via_the_callee_identity_veto() {
    let mut c = Case::base();
    let mut prior = role(Some("h1"));
    prior.fn_callee_ids = vec!["prior:helperA".into()];
    c.prior_roles.insert("packItem".into(), prior);
    c.fn_matches
        .insert("prior:helperA".into(), "new:helperA".into());
    c.fresh.fn_callee_ids = vec!["new:helperB".into()];
    let r = c.run("function q7(v) { return v; }").0;
    assert_eq!(blocked(&r), Some("role-mismatch:callee-mismatch"));
}

#[test]
fn refuses_when_the_target_is_already_held_without_retry() {
    let (r, names, _) =
        Case::base().run("function q7(v) { return v; } function packItem(x) { return x; }");
    assert!(blocked(&r).is_some_and(|b| b == "validation:target-in-scope"));
    assert!(names.contains(&"q7".to_string()));
}

#[test]
fn pins_via_shingle_overlap_when_hashes_are_absent() {
    let mut c = Case::base();
    let shared: BTreeSet<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
    let mut prior = role(None);
    prior.content_shingles = Some(shared.clone());
    c.prior_roles.insert("packItem".into(), prior);
    let mut fresh_set = shared;
    fresh_set.insert("e".into());
    c.fresh = role(Some("different"));
    c.fresh.content_shingles = Some(fresh_set);
    let r = c.run("function q7(v) { return v; }").0;
    assert_eq!(
        r,
        PinResult::Pinned {
            name: "packItem".into(),
            role_reason: "shingle-overlap"
        }
    );
}

#[test]
fn carries_a_below_floor_prior_name_and_registers_it() {
    let mut c = Case::base();
    c.votes = votes(&[("M2_", 1, 1)]);
    c.claimants = HashMap::from([("M2_".to_string(), 1)]);
    c.prior_roles = HashMap::from([("M2_".to_string(), role(Some("h1")))]);
    let (r, names, carried) = c.run("function q7(v) { return v; }");
    assert!(matches!(r, PinResult::Pinned { ref name, .. } if name == "M2_"));
    assert!(names.contains(&"M2_".to_string()));
    assert!(
        carried,
        "a carried below-floor name is registered for the sweep"
    );
}

#[test]
fn pins_a_collision_decorated_descriptive_name() {
    let mut c = Case::base();
    c.votes = votes(&[("initializeApp_", 1, 1)]);
    c.claimants = HashMap::from([("initializeApp_".to_string(), 1)]);
    c.prior_roles = HashMap::from([("initializeApp_".to_string(), role(Some("h1")))]);
    assert!(matches!(
        c.run("function q7(v) { return v; }").0,
        PinResult::Pinned { .. }
    ));
}

#[test]
fn rank_prefers_exact_votes_over_totals() {
    let v = votes(&[("writeConfig", 2, 2), ("persistSettings", 3, 0)]);
    assert_eq!(rank_vote_suggestion(&v).as_deref(), Some("writeConfig"));
}

#[test]
fn rank_abstains_on_a_tie() {
    let v = votes(&[("writeConfig", 1, 1), ("persistSettings", 1, 1)]);
    assert_eq!(rank_vote_suggestion(&v), None);
}

#[test]
fn rank_excludes_below_floor_names() {
    let v = votes(&[("M2_", 3, 3), ("writeConfig", 1, 1)]);
    assert_eq!(rank_vote_suggestion(&v).as_deref(), Some("writeConfig"));
    assert_eq!(rank_vote_suggestion(&votes(&[("M2_", 2, 2)])), None);
}

#[test]
fn rank_breaks_an_exact_tie_by_total() {
    let v = votes(&[("writeConfig", 3, 1), ("persistSettings", 1, 1)]);
    assert_eq!(rank_vote_suggestion(&v).as_deref(), Some("writeConfig"));
}

#[test]
fn top_vote_needs_a_unique_leader_at_the_floor() {
    let t = |pairs: &[(&'static str, u64)], min| get_top_vote(pairs.iter().copied(), min);
    assert_eq!(t(&[("a", 2), ("b", 1)], 2), Some("a"));
    assert_eq!(t(&[("a", 2), ("b", 2)], 2), None);
    assert_eq!(t(&[("a", 1)], 2), None);
    // A later strictly-higher count clears an earlier tie.
    assert_eq!(t(&[("a", 1), ("b", 1), ("c", 3)], 1), Some("c"));
}
