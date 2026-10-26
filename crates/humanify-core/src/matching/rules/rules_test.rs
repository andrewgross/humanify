//! The rule lists (docs/matching-architecture.md §3.5): the library
//! profile refuses INTO, the cross-version list is the cascade's order,
//! and every decision is made by a listed rule.

use crate::graph::edges::Scope;
use crate::matching::rules::{
    Evidence, Profile, RESOLVE, RuleList, RuleSpec, Stage, cross, cross_version, library,
};

const READS_INTO: RuleSpec = RuleSpec::new(
    "readsInto",
    Stage::Propagate,
    &[Evidence::Graph(Scope::Inside), Evidence::Graph(Scope::Into)],
);
const READS_ALL: RuleSpec = RuleSpec::new("readsAll", Stage::Resolve, &[Evidence::GraphAll]);
const CHECKS_INTO: RuleSpec =
    RuleSpec::new("checksInto", Stage::Check, &[Evidence::Graph(Scope::Into)]);

#[test]
fn the_library_profile_refuses_into_and_every_edge_except_in_a_check() {
    let mut list = RuleList::new(Profile::Library);
    let refused = list.push(READS_INTO).expect_err("INTO is not identity");
    assert_eq!(
        (refused.profile, refused.rule),
        (Profile::Library, "readsInto")
    );
    assert!(refused.why.contains("graph:into"), "{}", refused.why);
    let refused = list.push(READS_ALL).expect_err("every edge includes INTO");
    assert!(refused.why.contains("graph:all"), "{}", refused.why);
    list.push(CHECKS_INTO)
        .expect("a post-claim check may read INTO");
    assert_eq!(list.rules().len(), 1);
}

#[test]
fn the_cross_version_profile_reads_every_edge_as_today() {
    let mut list = RuleList::new(Profile::CrossVersion);
    list.push(READS_INTO)
        .expect("cross-version reads every edge");
    list.push(READS_ALL)
        .expect("cross-version reads every edge");
    assert!(
        cross_version()
            .rules()
            .iter()
            .any(|r| r.may_read(Evidence::GraphAll))
    );
}

#[test]
fn no_built_in_library_rule_reads_into_but_the_boundary_check() {
    let into: Vec<&str> = library()
        .rules()
        .iter()
        .filter(|r| r.reads.iter().any(|e| e.reads_into()))
        .map(|r| r.name)
        .collect();
    assert_eq!(into, vec!["boundary"]);
    assert!(
        library()
            .rules()
            .iter()
            .filter(|r| r.name == "boundary")
            .all(|r| r.stage == Stage::Check)
    );
}

#[test]
fn the_cascade_executes_the_listed_resolve_rungs_in_the_listed_order() {
    let names: Vec<&str> = RESOLVE.iter().map(|r| r.name).collect();
    assert_eq!(crate::matching::cascade::resolve_rung_names(), names);
    assert_eq!(
        names,
        vec![
            "identity",
            "memberKey",
            "enclosingStatement",
            "depthGate",
            "calleeShapes",
            "callerShapes",
            "calleeHashes",
            "shingleSimilarity",
        ],
        "TS resolveMatch + resolveDeepStages, in order"
    );
    // The same order inside the profile's whole list.
    let profile: Vec<&str> = cross_version().rules().iter().map(|r| r.name).collect();
    let at: Vec<usize> = names
        .iter()
        .map(|n| profile.iter().position(|p| p == n).expect("listed"))
        .collect();
    assert!(at.windows(2).all(|w| w[0] < w[1]), "{at:?}");
}

#[test]
fn every_tier_a_pair_row_can_carry_is_a_cross_version_rule() {
    use crate::matching::cascade::MatchedResolution as M;
    let list = cross_version();
    let tiers = [
        M::StructuralHashUnique,
        M::Identity,
        M::MemberKey,
        M::EnclosingStatement,
        M::CalleeShapes,
        M::CallerShapes,
        M::CalleeHashes,
        M::TwoHopShapes,
        M::ShingleSimilarity,
    ]
    .map(M::as_str);
    for tier in tiers
        .into_iter()
        .chain(["propagation", "ordinal", "interchangeable"])
    {
        let d = list.decide("fresh".into(), "prior".into(), tier);
        assert_eq!(d.rule, tier);
    }
    let d = list.decide(
        "input.js:1:0".into(),
        "input.js:2:0".into(),
        cross::MEMBER_KEY.name,
    );
    assert_eq!(d.reads, vec!["member-key".to_string()]);
}

#[test]
#[should_panic(expected = "not on the Library list")]
fn a_rule_off_the_list_cannot_decide() {
    library().decide("unit@0-1".into(), "index:P".into(), "memberKey");
}

#[test]
fn the_call_context_rule_reads_only_inside_and_out_of_and_the_library_profile_admits_it() {
    use crate::matching::rules::lib;
    let graph: Vec<Evidence> = lib::CALL_CONTEXT
        .reads
        .iter()
        .copied()
        .filter(|e| matches!(e, Evidence::Graph(_) | Evidence::GraphAll))
        .collect();
    assert_eq!(
        graph,
        vec![
            Evidence::Graph(Scope::Inside),
            Evidence::Graph(Scope::OutOf)
        ]
    );
    let mut list = RuleList::new(Profile::Library);
    list.push(lib::CALL_CONTEXT)
        .expect("INSIDE and OUT-OF are identity evidence");
}
