//! The names-mode placement planner's fixtures (library-names plan step 4,
//! exp156). The planner is PURE: claims in, move/leave decisions out.

use std::collections::BTreeMap;

use super::library_layout::{INNER_CONTESTED, LayoutInput, ModuleAnswer, RegionInput, plan};

fn answer(owners: &[&str], files: &[&str]) -> ModuleAnswer {
    ModuleAnswer {
        owners: owners.iter().map(|s| s.to_string()).collect(),
        files: files.iter().map(|s| s.to_string()).collect(),
    }
}

fn claimed(region_claims: &[Option<&str>]) -> RegionInput {
    RegionInput {
        claims: region_claims
            .iter()
            .map(|c| c.map(|s| s.to_string()))
            .collect(),
        module: None,
        sort_key: String::new(),
    }
}

fn module_region(claims: &[Option<&str>], key: &str, sort: &str) -> RegionInput {
    RegionInput {
        claims: claims.iter().map(|c| c.map(|s| s.to_string())).collect(),
        module: Some(key.to_string()),
        sort_key: sort.to_string(),
    }
}

#[test]
fn a_wholesale_claimed_region_with_one_index_file_moves() {
    let answers = BTreeMap::from([("key1".to_string(), answer(&["ms"], &["index.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(
            &[Some("ms"), Some("ms"), Some("ms")],
            "key1",
            "sort-a",
        )],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(out.regions.len(), 1);
    assert_eq!(
        out.regions[0].move_path(),
        Some("vendor/ms/index.js"),
        "a group every unit of which one package claims moves to that \
         package's real file"
    );
}

#[test]
fn an_interleaved_region_stays_in_place() {
    let answers = BTreeMap::from([("key1".to_string(), answer(&["ms"], &["index.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(
            &[Some("ms"), None, Some("ms")],
            "key1",
            "sort-a",
        )],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].leave_reason(),
        Some("partial"),
        "an unclaimed unit interleaved with claimed ones would split the \
         module group — the region stays"
    );
}

#[test]
fn two_packages_in_one_module_group_stay_in_place() {
    let answers = BTreeMap::from([("key1".to_string(), answer(&["ms"], &["index.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(
            &[Some("ms"), None, Some("lodash")],
            "key1",
            "sort-a",
        )],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].leave_reason(),
        Some("contested"),
        "two packages claiming one module group leave it in place"
    );
}

#[test]
fn an_unclaimed_region_stays() {
    let input = LayoutInput {
        regions: vec![claimed(&[None, None])],
        module_answers: &BTreeMap::new(),
    };
    let out = plan(&input);
    assert_eq!(out.regions[0].leave_reason(), Some("unclaimed"));
}

#[test]
fn the_same_input_twice_gives_the_identical_plan() {
    let answers = BTreeMap::from([
        ("key1".to_string(), answer(&["ms"], &["index.js"])),
        (
            "key2".to_string(),
            answer(&["zod"], &["lib/a.js", "lib/b.js"]),
        ),
    ]);
    let regions = vec![
        module_region(&[Some("ms")], "key1", "sort-a"),
        module_region(&[Some("zod")], "key2", "sort-b"),
    ];
    let a = plan(&LayoutInput {
        regions: regions.clone(),
        module_answers: &answers,
    });
    let b = plan(&LayoutInput {
        regions,
        module_answers: &answers,
    });
    assert_eq!(a.regions, b.regions);
}

#[test]
fn synthetic_names_are_sorted_module_group_ordinals_not_bundle_order() {
    // No index answers at all: both groups of ms take synthetic names,
    // numbered by SORT KEY, so a version that reorders the bundle (or
    // claims one more group ahead of these) cannot rename them.
    let answers = BTreeMap::new();
    let mk = |sort: &str| RegionInput {
        claims: vec![Some("ms".to_string())],
        module: None,
        sort_key: sort.to_string(),
    };
    let forward = plan(&LayoutInput {
        regions: vec![mk("sort-a"), mk("sort-b")],
        module_answers: &answers,
    });
    let backwards = plan(&LayoutInput {
        regions: vec![mk("sort-b"), mk("sort-a")],
        module_answers: &answers,
    });
    assert_eq!(
        forward.regions[0].move_path(),
        Some("vendor/ms/module-1.js")
    );
    assert_eq!(
        forward.regions[1].move_path(),
        Some("vendor/ms/module-2.js")
    );
    assert_eq!(
        backwards.regions[0].move_path(),
        Some("vendor/ms/module-2.js"),
        "the same region keeps its ordinal whatever order the regions arrive in"
    );
    assert_eq!(
        backwards.regions[1].move_path(),
        Some("vendor/ms/module-1.js")
    );
}

#[test]
fn a_duplicate_file_moves_only_once() {
    let answers = BTreeMap::from([
        ("key1".to_string(), answer(&["ms"], &["index.js"])),
        ("key2".to_string(), answer(&["ms"], &["index.js"])),
    ]);
    let input = LayoutInput {
        regions: vec![
            module_region(&[Some("ms")], "key1", "sort-a"),
            module_region(&[Some("ms")], "key2", "sort-b"),
        ],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(out.regions[0].move_path(), Some("vendor/ms/index.js"));
    assert_eq!(
        out.regions[1].leave_reason(),
        Some("duplicate-file"),
        "two claimed groups of one package answering to the same source file: \
         the first by sort key moves, the other stays"
    );
}

#[test]
fn an_index_answer_for_another_package_leaves_the_region() {
    let answers = BTreeMap::from([("key1".to_string(), answer(&["lodash"], &["index.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(&[Some("ms")], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].leave_reason(),
        Some("index-disagrees"),
        "the whole-module key names another package: the claim and the index \
         disagree, the region does not move"
    );
}

#[test]
fn a_multi_file_index_answer_takes_a_synthetic_name() {
    let answers = BTreeMap::from([("key1".to_string(), answer(&["ms"], &["a.js", "b.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(&[Some("ms")], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(out.regions[0].move_path(), Some("vendor/ms/module-1.js"));
}

#[test]
fn an_inner_contest_never_moves_whatever_else_agrees() {
    // The fold sentinel a group's own contents can produce (its wrapper
    // and its body claimed by two packages): no slot of the region may
    // become a Move, whatever the other slots say.
    let answers = BTreeMap::new();
    let input = LayoutInput {
        regions: vec![
            // The contested slot alone:
            claimed(&[Some(INNER_CONTESTED)]),
            // The contested slot beside agreement:
            claimed(&[Some("ms"), Some(INNER_CONTESTED)]),
            // The contested slot beside a gap:
            claimed(&[Some(INNER_CONTESTED), None]),
        ],
        module_answers: &answers,
    };
    let out = plan(&input);
    for (i, region) in out.regions.iter().enumerate() {
        assert_eq!(
            region.leave_reason(),
            Some("contested"),
            "region {i}: an inner contest is a contest, never a move"
        );
    }
}

#[test]
fn a_scoped_package_lays_out_under_its_scope() {
    let answers = BTreeMap::from([(
        "key1".to_string(),
        answer(&["@smithy/core"], &["dist-cjs/index.js"]),
    )]);
    let input = LayoutInput {
        regions: vec![module_region(&[Some("@smithy/core")], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].move_path(),
        Some("vendor/@smithy/core/dist-cjs/index.js")
    );
}
