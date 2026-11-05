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

// exp157: the bundler scatters a package between versions, so the same
// module can carry claims in one version and none in the other. When the
// region's whole-module key has a CLEAN index answer — one owner, one
// file — the index alone is enough evidence: the region moves to the
// answer's path, so both versions land the identical vendor path.
#[test]
fn an_unclaimed_region_with_a_clean_index_answer_moves_on_the_index() {
    let answers = BTreeMap::from([(
        "key1".to_string(),
        answer(&["@otel/exporter-metrics"], &["build/src/platform/node.js"]),
    )]);
    let input = LayoutInput {
        regions: vec![module_region(&[None, None, None], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].move_path(),
        Some("vendor/@otel/exporter-metrics/build/src/platform/node.js"),
        "no claims but a clean index answer still moves — the index is the \
         evidence that survives a bundler reshuffle"
    );
}

#[test]
fn an_unclaimed_region_with_a_multi_owner_answer_stays() {
    let answers = BTreeMap::from([("key1".to_string(), answer(&["ms", "other"], &["index.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(&[None, None], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].leave_reason(),
        Some("unclaimed"),
        "two owners and no claim: nobody can say whose module it is"
    );
}

#[test]
fn an_unclaimed_region_with_a_clean_owner_but_many_files_takes_a_synthetic_name() {
    let answers = BTreeMap::from([(
        "key1".to_string(),
        answer(&["ms"], &["lib/a.js", "lib/b.js"]),
    )]);
    let input = LayoutInput {
        regions: vec![module_region(&[None, None], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].move_path(),
        Some("vendor/ms/module-sort-a.js"),
        "the owner is agreed even though the file is not — the synthetic \
         spelling under the index's package keeps the move"
    );
}

#[test]
fn an_unclaimed_region_with_a_no_row_key_stays() {
    let answers = BTreeMap::from([("other".to_string(), answer(&["ms"], &["index.js"]))]);
    let input = LayoutInput {
        regions: vec![module_region(&[None, None], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].leave_reason(),
        Some("unclaimed"),
        "no claim and no index row: nothing says the region is a library's"
    );
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
fn synthetic_names_come_from_the_region_s_own_content_not_the_bundle() {
    // No index answers at all: each group of ms takes a synthetic name
    // derived from its OWN sort key — the module's head statement's
    // rename-invariant hash — so nothing about the other regions or the
    // bundle's order can rename it (exp157: the ordinal scheme renumbered
    // a package's synthetic regions whenever a version's composition
    // added or dropped one, and the shared content landed at a new path).
    let answers = BTreeMap::new();
    let mk = |sort: &str| RegionInput {
        claims: vec![Some("ms".to_string())],
        module: None,
        sort_key: sort.to_string(),
    };
    let forward = plan(&LayoutInput {
        regions: vec![mk("aaaaaaaa11111111"), mk("bbbbbbbb22222222")],
        module_answers: &answers,
    });
    let backwards = plan(&LayoutInput {
        regions: vec![mk("bbbbbbbb22222222"), mk("aaaaaaaa11111111")],
        module_answers: &answers,
    });
    assert_eq!(
        forward.regions[0].move_path(),
        Some("vendor/ms/module-aaaaaaaa.js")
    );
    assert_eq!(
        forward.regions[1].move_path(),
        Some("vendor/ms/module-bbbbbbbb.js")
    );
    assert_eq!(
        backwards.regions[0].move_path(),
        Some("vendor/ms/module-bbbbbbbb.js"),
        "the same region keeps its content-derived name whatever order the \
         regions arrive in"
    );
    assert_eq!(
        backwards.regions[1].move_path(),
        Some("vendor/ms/module-aaaaaaaa.js")
    );
}

// exp157, the brief's composition fixture: the same library region
// discovered with slightly different composition — a sibling region of
// the same package arrives in one version and not the other — must land
// the SAME vendor path. Under the ordinal scheme the sibling shifted
// every rank after it.
#[test]
fn a_composition_change_elsewhere_in_the_package_leaves_the_shared_names() {
    let answers = BTreeMap::new();
    let mk = |sort: &str| RegionInput {
        claims: vec![Some("ms".to_string())],
        module: None,
        sort_key: sort.to_string(),
    };
    let two = plan(&LayoutInput {
        regions: vec![mk("aaaaaaaa11111111"), mk("cccccccc33333333")],
        module_answers: &answers,
    });
    // The next version discovers one more ms region, sorting FIRST.
    let three = plan(&LayoutInput {
        regions: vec![
            mk("0000000044444444"),
            mk("aaaaaaaa11111111"),
            mk("cccccccc33333333"),
        ],
        module_answers: &answers,
    });
    assert_eq!(two.regions[0].move_path(), three.regions[1].move_path());
    assert_eq!(two.regions[1].move_path(), three.regions[2].move_path());
    assert_eq!(
        two.regions[0].move_path(),
        Some("vendor/ms/module-aaaaaaaa.js")
    );
}

#[test]
fn a_synthetic_name_tie_keeps_its_members_distinct_by_arrival_order() {
    // Two regions whose sort keys tie (the same blurred head) cannot share
    // a path: the first by (sort key, arrival) keeps the plain name, the
    // next takes -2. Bundle order decides ONLY inside a content tie.
    let answers = BTreeMap::new();
    let mk = |sort: &str| RegionInput {
        claims: vec![Some("ms".to_string())],
        module: None,
        sort_key: sort.to_string(),
    };
    let out = plan(&LayoutInput {
        regions: vec![mk("aaaaaaaa11111111"), mk("aaaaaaaa11111111")],
        module_answers: &answers,
    });
    let paths: Vec<_> = out.regions.iter().map(|r| r.move_path()).collect();
    assert_eq!(
        paths,
        vec![
            Some("vendor/ms/module-aaaaaaaa.js"),
            Some("vendor/ms/module-aaaaaaaa-2.js")
        ]
    );
}

// D1 (library-names plan step 7): a package's regions ALL coalesce under
// its folder. A file path two regions claim moves the best claimant only;
// the loser re-paths to its OWN synthetic in-package name instead of
// staying put — a `vendor/ms/index.js` beside a stayed region is exactly
// the split Andrew ruled out.
#[test]
fn a_duplicate_file_s_loser_repaths_inside_the_package() {
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
        out.regions[1].move_path(),
        Some("vendor/ms/module-sort-b.js"),
        "the loser of the file race still moves — under the same package, \
         named by its own content key"
    );
}

#[test]
fn an_equal_key_duplicate_takes_the_tie_suffix() {
    // Equal sort keys are identical bodies: whichever region takes the
    // index path and whichever takes the synthetic tie path, the tree is
    // byte-identical — the tie suffix keeps the two files distinct.
    let answers = BTreeMap::from([
        ("key1".to_string(), answer(&["ms"], &["index.js"])),
        ("key2".to_string(), answer(&["ms"], &["index.js"])),
    ]);
    let input = LayoutInput {
        regions: vec![
            module_region(&[Some("ms")], "key1", "sort-a"),
            module_region(&[Some("ms")], "key2", "sort-a"),
        ],
        module_answers: &answers,
    };
    let out = plan(&input);
    let paths: Vec<_> = out.regions.iter().map(|r| r.move_path()).collect();
    assert_eq!(
        paths,
        vec![
            Some("vendor/ms/index.js"),
            Some("vendor/ms/module-sort-a.js")
        ]
    );
}

#[test]
fn a_three_way_race_sets_every_loser_behind_the_winner() {
    let answers = BTreeMap::from([
        ("key1".to_string(), answer(&["ms"], &["index.js"])),
        ("key2".to_string(), answer(&["ms"], &["index.js"])),
        ("key3".to_string(), answer(&["ms"], &["index.js"])),
    ]);
    let input = LayoutInput {
        regions: vec![
            module_region(&[Some("ms")], "key1", "sort-a"),
            module_region(&[Some("ms")], "key2", "sort-a"),
            module_region(&[Some("ms")], "key3", "sort-a"),
        ],
        module_answers: &answers,
    };
    let out = plan(&input);
    let paths: Vec<_> = out.regions.iter().map(|r| r.move_path()).collect();
    assert_eq!(
        paths,
        vec![
            Some("vendor/ms/index.js"),
            Some("vendor/ms/module-sort-a.js"),
            Some("vendor/ms/module-sort-a-2.js")
        ]
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

// D1: a package-level answer with one owner and several files prefers the
// index's MAIN file (basename `index`) when the answer has one; the
// content-hash synthetic form is the else branch.
#[test]
fn a_package_level_answer_prefers_the_index_s_main_file() {
    let answers = BTreeMap::from([(
        "key1".to_string(),
        answer(
            &["@otel/exporter-metrics"],
            &["build/src/hooks.js", "build/src/index.js"],
        ),
    )]);
    let input = LayoutInput {
        regions: vec![module_region(&[None, None], "key1", "sort-a")],
        module_answers: &answers,
    };
    let out = plan(&input);
    assert_eq!(
        out.regions[0].move_path(),
        Some("vendor/@otel/exporter-metrics/build/src/index.js"),
        "the main file is the region's sensible in-package name"
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
    assert_eq!(
        out.regions[0].move_path(),
        Some("vendor/ms/module-sort-a.js")
    );
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
