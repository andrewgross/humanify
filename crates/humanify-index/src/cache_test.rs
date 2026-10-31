//! The run-time index cache's tests (exp154, library-names-plan step 3).
//!
//! WHAT IS STUBBED, and what is not, in the run-time plan tests: the Bun
//! binary is a stub `sh` script printing "1.4.0" (the real one is a large
//! npm download the run never makes); the registry is the RECORDED
//! packuments under `testdata/` — no network; `npm install` is never
//! reached (the plan stage only: pin derivation, the cap, the slice
//! records). The install/bundle/fingerprint/merge stages are proven by
//! the lane's manual demo on the real toolchain, not here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use humanify_core::libindex::format::{
    INDEX_FORMAT, Manifest, RegistrySnapshot, SliceRecord, compare_versions, fingerprint_versions,
};
use humanify_core::libindex::recipe::{DetectedRecipe, RecipeBundler, Target};

use crate::build::Trace;
use crate::cache::{
    BUN_FETCH_BYTES, BuildSidecar, PINNED_BUN, STORE_DIR, Seed, SidecarSlice, TOOLS_DIR,
    bun_binary, estimate_build, looks_like_id, plan_runtime, search_store, uncovered,
    valid_package_name, write_build_sidecar,
};
use crate::recipe::RecipeBuild;

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("humanify-ix-cache-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    dir
}

/// A manifest this binary can read, with the slices it is given.
fn manifest(slices: Vec<SliceRecord>) -> Manifest {
    Manifest {
        format: INDEX_FORMAT.to_string(),
        as_of: "2026-10-25T23:59:59.999Z".to_string(),
        fingerprints: fingerprint_versions(),
        registry: RegistrySnapshot::default(),
        slices,
        packages: Vec::new(),
        refused: Vec::new(),
        shards: BTreeMap::new(),
        tables: BTreeMap::new(),
        data: BTreeMap::new(),
    }
}

/// The bundle's detected recipe, as the run passes it (a Bun bundle: the
/// target the `// @bun` banner does or does not record).
fn detected(target: Option<Target>) -> DetectedRecipe {
    DetectedRecipe {
        bundler: Some(RecipeBundler::Bun),
        target,
        target_reason: "test",
        format: None,
        minified: true,
    }
}

/// A bun slice, as the exp152-built index spells it: the slice NAME
/// derives from the recipe's target, the SLICE's `version` is the npm
/// package's.
fn bun_slice(target: &str, version: &str, default: bool) -> SliceRecord {
    SliceRecord {
        slice: format!("bun-{target}-cjs-min"),
        recipe: format!("bun:target={target},format=cjs,minify,production"),
        recipe_version: crate::recipe::RECIPE_VERSION.to_string(),
        default_for_bundler: default,
        bundler: "bun".to_string(),
        version: version.to_string(),
        published: None,
        newest_at_as_of: None,
        bundler_after_as_of: false,
    }
}

/// A whole index directory (as [`search_store`] sees it: a manifest, an
/// optional sidecar), named by its id like the convention lays out.
fn write_index(root: &Path, m: &Manifest, completed: Option<&str>) -> String {
    let bytes = serde_json::to_vec_pretty(m).expect("a manifest");
    let id = crate::shard::hex_sha256(&bytes);
    let dir = root.join(&id);
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("manifest.json"), &bytes).expect("write");
    if let Some(done) = completed {
        write_build_sidecar(
            &dir,
            &BuildSidecar {
                schema: crate::cache::BUILD_SIDECAR_SCHEMA.into(),
                index_id: id.clone(),
                completed: done.into(),
                ..Default::default()
            },
        )
        .expect("sidecar");
    }
    id
}

/// `root`, as search sees it: an empty store, a non-index directory, an
/// id directory with no manifest, a stale-stamp index, an esbuild-only
/// index and a usable bun one — every candidate gets a recorded verdict
/// and the bun one is found with the properties it matched on.
#[test]
fn the_finder_records_why_each_candidate_was_refused() {
    let root = temp("finder");
    let _ = std::fs::create_dir_all(root.join("not-an-index"));
    let _ = std::fs::create_dir_all(root.join("a".repeat(64)));
    let stale = manifest(vec![bun_slice("bun", "1.4.0", true)]);
    let stale = Manifest {
        fingerprints: {
            let mut f = fingerprint_versions();
            f.literals = "literals/OLD".to_string();
            f
        },
        ..stale
    };
    write_index(&root, &stale, None);
    let esbuild = manifest(vec![SliceRecord {
        slice: "esbuild-node-cjs-min".into(),
        recipe: "esbuild:target=node,format=cjs,minify".into(),
        recipe_version: crate::recipe::RECIPE_VERSION.into(),
        default_for_bundler: true,
        bundler: "esbuild".into(),
        version: "0.25.0".into(),
        published: None,
        newest_at_as_of: None,
        bundler_after_as_of: false,
    }]);
    write_index(&root, &esbuild, None);
    let bun = write_index(
        &root,
        &manifest(vec![bun_slice("bun", "1.4.0", true)]),
        None,
    );
    let search = search_store(&root, &detected(Some(Target::Bun)));
    let read = |id: &str| {
        search
            .candidates
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    };
    assert!(
        search
            .candidates
            .iter()
            .any(|(i, v)| { i == "not-an-index" && v.contains("not an index id") }),
        "{:?}",
        search.candidates
    );
    assert!(
        read(&"a".repeat(64)).contains("manifest.json"),
        "{:?}",
        search.candidates
    );
    assert!(
        search
            .candidates
            .iter()
            .any(|(_, v)| v.contains("literals/OLD")),
        "{:?}",
        search.candidates
    );
    assert!(
        search
            .candidates
            .iter()
            .any(|(_, v)| v.contains("no slice for recipe")),
        "{:?}",
        search.candidates
    );
    let found = search.found.as_ref().expect("the bun index");
    assert_eq!(found.id, bun);
    assert_eq!(found.bundler_version, "1.4.0");
    assert_eq!(found.as_of, "2026-10-25T23:59:59.999Z");
    assert_eq!(found.slice, "bun-bun-cjs-min");
    // as-of freshness is deliberately not judged in this lane
    assert!(found.verdict.contains("as-of"));
}

/// Two usable indexes: the newer `build.json` wins; a requested bundler
/// version must match the slice record's.
#[test]
fn the_newest_compatible_index_wins() {
    let root = temp("newest");
    // two DIFFERENT indexes (the id is the manifest's hash, so the older
    // one needs genuinely different content: an older as-of)
    let old = write_index(
        &root,
        &Manifest {
            as_of: "2026-09-01T23:59:59.999Z".into(),
            ..manifest(vec![bun_slice("bun", "1.4.0", true)])
        },
        Some("2026-10-01T00:00:00Z"),
    );
    let new = write_index(
        &root,
        &manifest(vec![bun_slice("bun", "1.4.0", true)]),
        Some("2026-10-26T00:00:00Z"),
    );
    assert_ne!(
        old, new,
        "two different indexes, or the test avoids its ask"
    );
    let search = search_store(&root, &detected(Some(Target::Bun)));
    assert_eq!(
        search.found.as_ref().expect("one").id,
        new,
        "{old} is older"
    );
    // an index whose only bun slice has the WRONG target is refused: the
    // finder asks choose_slice's question, so a found index always loads
    let other_root = temp("newest-mismatch");
    let node_only = write_index(
        &other_root,
        &manifest(vec![bun_slice("node", "1.4.0", true)]),
        None,
    );
    let miss = search_store(&other_root, &detected(Some(Target::Bun)));
    assert!(miss.found.is_none(), "{:?}", miss.candidates);
    assert!(
        miss.candidates
            .iter()
            .any(|(i, v)| i == &node_only && v.contains("no slice for recipe")),
        "{:?}",
        miss.candidates
    );
    // ... and it is the one found when the bundle's recipe IS the node target
    let hit = search_store(&other_root, &detected(Some(Target::Node)));
    assert_eq!(hit.found.as_ref().expect("the node slice").id, node_only);
}

/// A missing store is a miss with a note, not an error — the first build
/// creates it.
#[test]
fn an_empty_store_is_a_miss_with_a_note() {
    let root = temp("empty").join("no-such-store");
    let search = search_store(&root, &detected(Some(Target::Bun)));
    assert!(search.found.is_none());
    assert!(search.note.as_deref().is_some_and(|n| !n.is_empty()));
    assert!(search.candidates.is_empty());
}

/// The sidecar round trips, and is EXCLUDED from the index id: the same
/// directory with a sidecar answers the same search.
#[test]
fn the_sidecar_round_trips() {
    let root = temp("sidecar");
    let id = write_index(
        &root,
        &manifest(vec![bun_slice("bun", "1.4.0", true)]),
        None,
    );
    let dir = root.join(&id);
    let side = BuildSidecar {
        schema: crate::cache::BUILD_SIDECAR_SCHEMA.into(),
        index_id: id.clone(),
        built_from: "run-time build".into(),
        as_of: "2026-10-25T23:59:59.999Z".into(),
        format: INDEX_FORMAT.into(),
        slices: vec![SidecarSlice {
            slice: "bun-bun-cjs-min".into(),
            recipe: "bun:target=bun,format=cjs,minify,production".into(),
            bundler: "bun".into(),
            bundler_version: "1.4.0".into(),
        }],
        seeds: vec![crate::cache::PlannedSeed {
            name: "ms".into(),
            version: "2.1.0".into(),
            how: "declared".into(),
            evidence: "package.json".into(),
        }],
        dropped: vec![crate::cache::DroppedSeed {
            name: "http://mths.be/fromcodepoint".into(),
            reason: "not an npm package name".into(),
            evidence: "banner comment at byte 7".into(),
        }],
        failed: vec![crate::cache::DroppedSeed {
            name: "@typescript-eslint/eslint-plugin@8.68.0".into(),
            reason: "install/bundle failed: npm error ERESOLVE".into(),
            evidence: String::new(),
        }],
        versions_in_index: 1,
        packages: 1,
        refused: 0,
        built: 1,
        reused: 0,
        stage_ms: BTreeMap::from([("install".to_string(), 12_345)]),
        completed: "2026-10-26T00:00:00Z".into(),
    };
    write_build_sidecar(&dir, &side).expect("write");
    assert_eq!(
        crate::cache::read_build_sidecar(&dir).expect("read"),
        Some(side)
    );
}

/// The pinned-Bun lookup: the NEWEST installed wins when the run does not
/// know the bundle's own version, and a cache with none says exactly how
/// to fetch one.
#[test]
fn the_bun_lookup_picks_the_newest_and_names_the_fetch_command() {
    let root = temp("bun");
    let install = |v: &str| {
        let bin = root
            .join(TOOLS_DIR)
            .join(format!("bun-{v}"))
            .join("node_modules/.bin");
        std::fs::create_dir_all(&bin).expect("mkdir");
        std::fs::write(bin.join("bun"), "#!/bin/sh\nexit 0\n").expect("stub");
    };
    install("1.4.0");
    install("1.10.0");
    let (bin, version) = bun_binary(&root, None).expect("the newest");
    assert_eq!(version, "1.10.0");
    assert!(bin.ends_with("node_modules/.bin/bun"), "{})", bin.display());
    let (too, v2) = bun_binary(&root, Some("1.4.0")).expect("the exact");
    assert_eq!(v2, "1.4.0");
    assert!(too.ends_with("bun-1.4.0/node_modules/.bin/bun"));
    let bare = temp("bun-none");
    let err = bun_binary(&bare, None).expect_err("nothing installed");
    assert!(
        err.contains(&format!("npm install bun@{PINNED_BUN}")),
        "{err}"
    );
    assert!(
        err.contains(bare.join(TOOLS_DIR).display().to_string().as_str()),
        "{err}"
    );
}

/// The run-time plan: declared versions pin as declared, undeclared ones
/// take the newest at as-of, and the bundle's evidence is recorded.
fn recorded_registry(root: &Path) -> PathBuf {
    let registry = root.join("registry");
    std::fs::create_dir_all(&registry).expect("mkdir");
    for (name, file) in [
        ("bun", include_str!("../testdata/bun.packument.json")),
        ("ms", include_str!("../testdata/ms.packument.json")),
        (
            "leftpad",
            include_str!("../testdata/leftpad.packument.json"),
        ),
    ] {
        std::fs::write(registry.join(format!("{name}.json")), file).expect("fixture");
    }
    registry
}

/// A stub Bun binary (a `sh` script): `--version` answers 1.4.0. The real
/// binary is a large npm download; the run-time build looks it up under
/// `tools/`, never downloads one.
fn stub_bun(root: &Path) -> PathBuf {
    let bin = root
        .join(TOOLS_DIR)
        .join("bun-1.4.0")
        .join("node_modules/.bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    let path = bin.join("bun");
    std::fs::write(&path, "#!/bin/sh\necho 1.4.0\n").expect("stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    path
}

/// The error text of a result (its `Err`), without needing `Debug` on the
/// success type.
fn err_of<T>(r: Result<T, String>) -> String {
    match r {
        Ok(_) => panic!("the plan succeeded where the test refuses it"),
        Err(e) => e,
    }
}

#[test]
fn run_time_pins_are_resolved_and_the_cap_is_enforced() {
    let root = temp("plan");
    recorded_registry(&root);
    let bin = stub_bun(&root);
    let recipe = |target: &str| RecipeBuild {
        recipe: humanify_core::libindex::recipe::Recipe::parse(&format!(
            "bun:target={target},format=cjs,minify,production"
        ))
        .expect("recipe"),
        bin: bin.clone(),
    };
    let seeds = vec![
        Seed {
            name: "ms".into(),
            version: Some("2.1.0".into()),
            evidence: "package.json".into(),
        },
        Seed {
            name: "leftpad".into(),
            version: None,
            evidence: "banner".into(),
        },
    ];
    let (cfg, planned, dropped) = plan_runtime(
        &root,
        seeds.clone(),
        vec![recipe("bun"), recipe("node")],
        "2026-10-25",
        300,
        true,
    )
    .expect("planned");
    assert_eq!(cfg.registry, root.join("registry"));
    assert_eq!(cfg.store, root.join(STORE_DIR));
    assert!(cfg.quiet, "the in-process build prints nothing");
    assert!(
        cfg.names,
        "names mode restores the units' original names (exp171)"
    );
    assert!(dropped.is_empty(), "{dropped:?}");
    assert_eq!(
        cfg.pinned,
        vec!["ms@2.1.0".to_string(), "leftpad@1.0.0".to_string()]
    );
    assert_eq!(planned[0].how, "declared");
    assert_eq!(planned[1].how, "newest-at-as-of");
    assert_eq!(planned[1].evidence, "banner");
    let plan = crate::build::plan(&cfg, Trace::default()).expect("plan");
    assert_eq!(plan.tasks.len(), 2, "one pinned version per seed");
    assert_eq!(plan.slices.len(), 2, "both bun slices");

    // over the cap: refused loudly, before anything is fetched
    let err = err_of(plan_runtime(
        &root,
        seeds,
        Vec::new(),
        "2026-10-25",
        1,
        true,
    ));
    assert!(err.contains("1"), "{err}");
    assert!(err.contains("library.index_build_max_packages"), "{err}");
    assert!(err.contains("refuses"), "{err}");
}

/// A seed the bundle names that the registry has no record of (and the
/// plan runs offline): a loud refusal naming the package — never a
/// silently smaller index.
#[test]
fn an_unknown_seed_fails_the_plan_loudly() {
    let root = temp("unknown-seed");
    recorded_registry(&root);
    let err = err_of(plan_runtime(
        &root,
        vec![Seed {
            name: "no-such-package".into(),
            version: None,
            evidence: "banner".into(),
        }],
        Vec::new(),
        "2026-10-25",
        300,
        true,
    ));
    assert!(err.contains("no-such-package"), "{err}");
}

/// exp171: the seed scan reads banners, so it offers URLs and prose; a name
/// that cannot be an npm package is DROPPED with its reason, before any
/// registry is asked — never a failed run (a bun-dev bundle's banner
/// offered `http://mths.be/fromcodepoint`, and the plan died on it).
#[test]
fn a_seed_that_is_not_a_package_name_is_dropped_with_its_reason() {
    let root = temp("bad-seed");
    recorded_registry(&root);
    let seed = |name: &str| Seed {
        name: name.into(),
        version: None,
        evidence: "banner".into(),
    };
    let (cfg, planned, dropped) = plan_runtime(
        &root,
        vec![
            seed("http://mths.be/fromcodepoint"),
            seed("leftpad"),
            seed("jQuery UI"),
        ],
        Vec::new(),
        "2026-10-25",
        300,
        true,
    )
    .expect("the plan goes on without them");
    assert_eq!(cfg.pinned, vec!["leftpad@1.0.0".to_string()]);
    assert_eq!(planned.len(), 1);
    let names: Vec<&str> = dropped.iter().map(|d| d.name.as_str()).collect();
    assert_eq!(names, ["http://mths.be/fromcodepoint", "jQuery UI"]);
    assert!(
        dropped
            .iter()
            .all(|d| d.reason == "not an npm package name")
    );
    assert!(dropped.iter().all(|d| d.evidence == "banner"));
}

#[test]
fn npm_package_names_are_told_apart_from_prose() {
    for ok in [
        "ms",
        "lodash.merge",
        "@aws-sdk/client-s3",
        "a-b_c~d",
        "jose",
    ] {
        assert!(valid_package_name(ok), "{ok}");
    }
    for bad in [
        "",
        "http://mths.be/fromcodepoint",
        "jQuery",
        "two words",
        ".hidden",
        "_private",
        "@scope",
        "@scope/",
        "@/x",
        &"a".repeat(215),
    ] {
        assert!(!valid_package_name(bad), "{bad:?}");
    }
}

/// The notice's estimate scales with the versions to BUILD (reused ones
/// cost nothing) and a Bun fetch adds its download.
#[test]
fn the_estimate_scales_with_the_versions_to_build() {
    let none = estimate_build(0, false);
    assert_eq!(none.bytes_low, 0);
    assert_eq!(none.bytes_high, 0);
    let ten = estimate_build(10, false);
    let twenty = estimate_build(20, false);
    assert!(
        ten.bytes_low > 0 && ten.bytes_low <= ten.bytes_high,
        "{ten:?}"
    );
    assert!(
        ten.seconds_low > 0 && ten.seconds_low <= ten.seconds_high,
        "{ten:?}"
    );
    assert_eq!(twenty.bytes_low, 2 * ten.bytes_low);
    assert!(twenty.seconds_high > ten.seconds_high);
    let with_bun = estimate_build(10, true);
    assert_eq!(with_bun.bytes_low, ten.bytes_low + BUN_FETCH_BYTES);
}

/// exp171: the store search matches on the bundle's SLICE only, so a store
/// that holds app A's run-time index would serve app B forever — and B's
/// packages would never be indexed. A run-time index covers a bundle when
/// every package name the bundle reveals is one its build planned or
/// dropped; an index with no seed record (an offline full build) covers
/// everything.
#[test]
fn a_run_time_index_covers_only_the_packages_it_was_built_from() {
    let seed = |name: &str| Seed {
        name: name.into(),
        version: None,
        evidence: "banner".into(),
    };
    let side = BuildSidecar {
        seeds: vec![crate::cache::PlannedSeed {
            name: "ms".into(),
            ..Default::default()
        }],
        dropped: vec![crate::cache::DroppedSeed {
            name: "no-such".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    assert_eq!(
        uncovered(Some(&side), &[seed("ms"), seed("no-such")]),
        Vec::<String>::new()
    );
    assert_eq!(
        uncovered(
            Some(&side),
            &[seed("ms"), seed("leftpad"), seed("http://x.y/z")]
        ),
        vec!["leftpad".to_string()],
        "a name that is no package is never a reason to rebuild"
    );
    let full = BuildSidecar::default();
    assert!(
        uncovered(Some(&full), &[seed("leftpad")]).is_empty(),
        "no seed record: a full index"
    );
    assert!(
        uncovered(None, &[seed("leftpad")]).is_empty(),
        "no sidecar: a full index"
    );
}

/// The id-looking names the store's merged indexes carry (64 lowercase
/// hex: the manifest's sha256), everything else is a directory the
/// convention owns (`store/`, `tools/`, ...).
#[test]
fn only_id_named_directories_are_candidates() {
    assert!(looks_like_id(&"a".repeat(64)));
    assert!(!looks_like_id(&"A0".repeat(32)));
    assert!(!looks_like_id("store"));
    assert!(!looks_like_id(&"a".repeat(63)));
    assert!(!looks_like_id(&format!("{}7", "f".repeat(64))));
    assert_eq!(
        compare_versions("1.10.0", "1.4.0"),
        std::cmp::Ordering::Greater,
        "the newest-pick compares semantically"
    );
}
