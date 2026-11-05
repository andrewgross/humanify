//! The planner: a spec resolved against a REGISTRY SNAPSHOT (no network)
//! into the package selections the build indexes — every package@version
//! carrying the rule that chose it.

use std::collections::{BTreeMap, BTreeSet};

use super::asof::{Packument, VersionMeta};
use super::build::PackageSelection;
use super::planner::{self, Loader, ResolvedSpec};
use super::spec::IndexSpec;

const SPEC: &str = r#"
schema = "humanify-index-spec/1"
as_of = "2026-10-25"

[build]
recipes = ["bun:target=bun,format=cjs,minify,production"]
sponge_regions = "origin"
names = true

[tools]
bun = "1.4.0"

[[select]]
id = "impact"
source = "npm-high-impact"
list = "top"
version = "1.13.0"
sha256 = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
limit = 3

[[select]]
id = "dep-trees"
source = "dependencies"
of = ["impact"]

[versions]
policy = "as-of"
dependents = 2
"#;

/// The canned `npm-high-impact` list the tests' fetcher returns.
const TOP: &str = "export const top = [
  'chalk',
  'ms',
  'left-pad',
];
";

/// An in-memory registry: packuments by name; a missing one fails loud
/// (a package absent from the snapshot is an error unless a refusal rule
/// skipped it first). `absent` names the ones the REGISTRY does not know
/// (404) — `get` hands back an absent packument for them (the wave
/// itself records nothing; enter() is the decision point).
/// `absent_files` names the ones whose absence is RECORDED in the
/// snapshot (the fetcher wrote an absent packument — the offline replay
/// path): the same absent packument through `get`.
///
/// The two shapes exist so a test can pin that the LIVE resolution and
/// the OFFLINE replay of the same snapshot produce the same records.
struct TestRegistry {
    packs: BTreeMap<String, Packument>,
    absent: BTreeSet<String>,
    absent_files: BTreeSet<String>,
    /// Every name a prefetch wave was handed (what the resolution ASKED
    /// the loader for — the plan must never ask for an alias's KEY).
    asked: BTreeSet<String>,
}

/// The absent packument a recorded absence re-reads as.
fn absent_packument(name: &str) -> Packument {
    Packument {
        name: name.to_string(),
        fetched_at: None,
        time: BTreeMap::new(),
        versions: Vec::new(),
        deps: BTreeMap::new(),
        deps_fetched: true,
        absent: true,
    }
}

impl Loader for TestRegistry {
    fn prefetch(&mut self, names: &BTreeSet<String>) -> Result<(), String> {
        self.asked.extend(names.iter().cloned());
        Ok(())
    }
    fn peek(&self, name: &str) -> Option<Packument> {
        if self.absent.contains(name) || self.absent_files.contains(name) {
            return Some(absent_packument(name));
        }
        self.packs.get(name).cloned()
    }
    fn get(&mut self, name: &str) -> Result<(Packument, String), String> {
        if self.absent.contains(name) || self.absent_files.contains(name) {
            return Ok((absent_packument(name), "test".to_string()));
        }
        self.packs
            .get(name)
            .cloned()
            .map(|p| (p, "test".to_string()))
            .ok_or(format!("{name}: no packument in the test registry"))
    }
}

/// A packument fixture: times from `time`, the version list from
/// `versions`, per-version dependency metadata from `deps` (the layer the
/// fetch merges in), the snapshot marked as carrying it.
fn packument(
    name: &str,
    time: &str,
    versions: &str,
    deps: &[(&str, &[(&str, &str)])],
) -> Packument {
    let mut p = Packument::from_json(&format!(
        r#"{{"name":"{name}","time":{{{time}}},"versions":[{versions}]}}"#
    ))
    .expect("fixture");
    p.deps_fetched = true;
    for (v, pairs) in deps {
        p.deps.insert(
            (*v).to_string(),
            VersionMeta {
                dependencies: pairs
                    .iter()
                    .map(|(n, r)| ((*n).to_string(), (*r).to_string()))
                    .collect(),
                ..VersionMeta::default()
            },
        );
    }
    p
}

fn registry() -> TestRegistry {
    let mut r = TestRegistry {
        packs: BTreeMap::new(),
        absent: BTreeSet::new(),
        absent_files: BTreeSet::new(),
        asked: BTreeSet::new(),
    };
    r.packs.insert(
        "chalk".into(),
        packument(
            "chalk",
            r#""created":"2013-01-01T00:00:00.000Z","2.4.2":"2019-05-15T10:00:00.000Z","4.1.0":"2020-01-05T10:00:00.000Z","5.0.0":"2021-06-01T10:00:00.000Z","5.3.0":"2022-09-01T10:00:00.000Z""#,
            r#""2.4.2","4.1.0","5.0.0","5.3.0""#,
            &[
                ("4.1.0", &[("ansi-styles", "^4")]),
                ("5.0.0", &[("ansi-styles", "^6")]),
                ("5.3.0", &[("ansi-styles", "^6")]),
            ],
        ),
    );
    r.packs.insert(
        "ms".into(),
        packument(
            "ms",
            r#""created":"2011-12-21T00:00:00.000Z","2.1.3":"2020-12-08T13:54:35.223Z""#,
            r#""2.1.3""#,
            &[("2.1.3", &[("chalk", "^2"), ("ansi-styles", "^3")])],
        ),
    );
    r.packs.insert(
        "left-pad".into(),
        packument(
            "left-pad",
            r#""created":"2016-01-01T00:00:00.000Z","1.3.0":"2016-06-01T00:00:00.000Z""#,
            r#""1.3.0""#,
            &[],
        ),
    );
    r.packs.insert(
        "ansi-styles".into(),
        packument(
            "ansi-styles",
            r#""created":"2013-01-01T00:00:00.000Z","3.4.0":"2019-01-01T00:00:00.000Z","5.2.0":"2021-01-01T00:00:00.000Z","6.1.0":"2022-06-01T00:00:00.000Z","7.0.0":"2023-04-01T00:00:00.000Z""#,
            r#""3.4.0","5.2.0","6.1.0","7.0.0""#,
            &[],
        ),
    );
    r
}

/// The list fetcher: the spec's `top` selection gets TOP.
fn top_list(
    _list: super::spec::HighImpactList,
    _version: &str,
    _sha: &str,
) -> Result<String, String> {
    Ok(TOP.to_string())
}

/// The reordered list the pending test's spec selects with.
const REORDERED: &str = "export const top = [
  'ms',
  'chalk',
];
";

/// left-pad's 1.3.0 gains a dependency on an ABSENT package through a
/// range that is not a range (a git URL): the wave's `want` names it
/// (the wave does not parse ranges), the walk never consults it.
fn git_dep_on_an_absent_ghost(r: &mut TestRegistry) {
    let p = r.packs.get_mut("left-pad").expect("left-pad");
    p.deps.insert(
        "1.3.0".to_string(),
        VersionMeta {
            dependencies: [(
                "ghost-pkg".to_string(),
                "git+https://github.com/x/y.git#abc".to_string(),
            )]
            .into_iter()
            .collect(),
            ..VersionMeta::default()
        },
    );
}

fn reordered_list(
    _list: super::spec::HighImpactList,
    _version: &str,
    _sha: &str,
) -> Result<String, String> {
    Ok(REORDERED.to_string())
}

fn sources(r: &mut TestRegistry) -> planner::Sources<'_> {
    planner::Sources {
        loader: r,
        list: &top_list,
    }
}

fn resolved(spec_text: &str, r: &mut TestRegistry) -> ResolvedSpec {
    let spec = IndexSpec::parse(spec_text).expect("spec");
    let mut src = sources(r);
    planner::resolve(&spec, &mut src).expect("resolves")
}

fn plan_of<'a>(res: &'a ResolvedSpec, name: &str) -> &'a PackageSelection {
    res.selections
        .iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| {
            panic!(
                "no selection for {name}: {:?}",
                res.selections.iter().map(|s| &s.name).collect::<Vec<_>>()
            )
        })
}

fn assert_versions(res: &ResolvedSpec, name: &str, want: &[(&str, &str)]) {
    let got: Vec<(String, String)> = plan_of(res, name)
        .versions
        .iter()
        .map(|s| (s.role.clone(), s.version.clone()))
        .collect();
    let want: Vec<(String, String)> = want
        .iter()
        .map(|(r, v)| ((*r).to_string(), (*v).to_string()))
        .collect();
    assert_eq!(got, want, "the plan for {name}");
}

#[test]
fn the_impact_selection_plans_each_package_under_the_policy() {
    let mut r = registry();
    let res = resolved(SPEC, &mut r);
    assert_eq!(res.selections.len(), 4);
    assert_eq!(
        res.selections
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["chalk", "ms", "left-pad", "ansi-styles"],
        "universe order: the list, then the closure members"
    );
    // as-of = latest, then the highest of each OLDER major line; 5.0.0
    // shares the latest's line, so it is not a pick
    assert_versions(
        &res,
        "chalk",
        &[
            ("latest", "5.3.0"),
            ("major-1", "4.1.0"),
            ("major-2", "2.4.2"),
        ],
    );
    assert_versions(&res, "ms", &[("latest", "2.1.3")]);
    assert_versions(&res, "left-pad", &[("latest", "1.3.0")]);
    // every planned version names the rule that chose it
    let chalk = plan_of(&res, "chalk");
    assert!(
        chalk
            .versions
            .iter()
            .all(|v| v.attribution.as_deref().is_some_and(
                |a| a == "sel=impact rule=as-of" || a.starts_with("sel=impact dependent-of=")
            ))
    );
    assert_eq!(
        plan_of(&res, "ansi-styles").attribution.as_deref(),
        Some("sel=dep-trees rank=1 member-of=chalk@5.3.0"),
        "the closure member names the version that pulled it in"
    );
}

#[test]
fn the_limit_truncates_a_list_selection() {
    let mut r = registry();
    let res = resolved(&SPEC.replace("limit = 3", "limit = 2"), &mut r);
    assert_eq!(
        res.selections
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["chalk", "ms", "ansi-styles"],
        "left-pad never entered the universe (the closure still runs)"
    );
}

#[test]
fn the_closure_adds_the_packages_the_install_trees_hold() {
    let mut r = registry();
    let res = resolved(SPEC, &mut r);
    // chalk (a closure root, an `of` selection's package) depends on
    // ansi-styles — the closure adds it, ranked after the list packages;
    // the plan is the policy's picks plus the version ms's edge resolved
    // to
    assert_eq!(
        plan_of(&res, "ansi-styles").attribution.as_deref(),
        Some("sel=dep-trees rank=1 member-of=chalk@5.3.0"),
        "attribution names the rule and the version that pulled it in"
    );
    assert_versions(
        &res,
        "ansi-styles",
        &[
            ("latest", "7.0.0"),
            ("major-1", "6.1.0"),
            ("major-2", "5.2.0"),
            ("dependent", "3.4.0"),
        ],
    );
    // transitively: ansi-styles has no deps, the closure stops
}

#[test]
fn the_dependents_rule_adds_the_version_the_ranges_resolve_to() {
    let mut r = registry();
    let res = resolved(SPEC, &mut r);
    // ms@2.1.3 (published 2020-12) depends on ansi-styles ^3: npm that
    // day got 3.4.0 — not one of ansi-styles' own policy picks
    assert!(
        plan_of(&res, "ansi-styles")
            .versions
            .iter()
            .find(|s| s.version == "3.4.0")
            .is_some_and(|s| s.role == "dependent"
                && s.attribution.as_deref()
                    == Some("sel=dep-trees dependents=1 first=ms@2.1.3 range=^3")),
        "{:?}",
        plan_of(&res, "ansi-styles").versions
    );
    // ms's other edge, chalk ^2, resolves 2.4.2 — already one of chalk's
    // policy picks, so the pick and its attribution stand
    let pick = plan_of(&res, "chalk")
        .versions
        .iter()
        .find(|s| s.version == "2.4.2")
        .expect("planned");
    assert_eq!(pick.role, "major-2");
    assert_eq!(pick.attribution.as_deref(), Some("sel=impact rule=as-of"));
    // chalk@4.1.0 depends on ansi-styles ^4: nothing of ansi-styles
    // satisfies it as of 2020-01, so no 4.x is added
    assert!(
        !plan_of(&res, "ansi-styles")
            .versions
            .iter()
            .any(|s| s.version.starts_with('4')),
        "{:?}",
        plan_of(&res, "ansi-styles").versions
    );
}

#[test]
fn a_closure_members_entering_edge_resolves_its_range() {
    // ms (rank 1 after the reorder) is processed FIRST: its edge on
    // ansi-styles is what ADDS the package to the universe, and that
    // edge's own range must resolve with it (the version npm would have
    // installed for ms joins the member's policy picks).
    let spec = SPEC
        .replace("limit = 3", "limit = 2")
        .replace("'chalk',\n  'ms',", "'ms',\n  'chalk',");
    let mut r = registry();
    r.packs.get_mut("ms").expect("ms").deps = [(
        "2.1.3".to_string(),
        VersionMeta {
            dependencies: [("ansi-styles".to_string(), "^3".to_string())]
                .into_iter()
                .collect(),
            ..VersionMeta::default()
        },
    )]
    .into_iter()
    .collect();
    let spec = IndexSpec::parse(&spec).expect("spec");
    let mut src = planner::Sources {
        loader: &mut r,
        list: &reordered_list,
    };
    let res = planner::resolve(&spec, &mut src).expect("resolves");
    // the pending range resolved when ansi-styles entered: 3.4.0 is not
    // one of its policy picks, so it joined as a dependent
    assert_versions(
        &res,
        "ansi-styles",
        &[
            ("latest", "7.0.0"),
            ("major-1", "6.1.0"),
            ("major-2", "5.2.0"),
            ("dependent", "3.4.0"),
        ],
    );
    assert!(
        plan_of(&res, "ansi-styles")
            .versions
            .iter()
            .find(|s| s.version == "3.4.0")
            .is_some_and(|s| s.attribution.as_deref()
                == Some("sel=dep-trees dependents=1 first=ms@2.1.3 range=^3")),
        "{:?}",
        plan_of(&res, "ansi-styles").versions
    );
}

#[test]
fn an_override_by_name_applies_its_policy_to_the_packages_it_names() {
    let mut r = registry();
    let spec = format!(
        "{SPEC}\n[[versions.override]]\nid = \"old-ms\"\npolicy = \"latest-1\"\nnames = [\"ms\"]"
    );
    let res = resolved(&spec, &mut r);
    assert_versions(&res, "ms", &[("latest", "2.1.3")]);
    // chalk keeps the default policy plus the dependent ms adds
    assert_eq!(plan_of(&res, "chalk").versions.len(), 3);
    let sel = plan_of(&res, "ms");
    assert!(
        sel.versions.iter().all(|s| s
            .attribution
            .as_deref()
            .is_some_and(|a| a.contains("override=old-ms"))),
        "{:?}",
        sel.versions
    );
}

#[test]
fn an_override_by_selection_and_rank_applies_to_the_list_head() {
    let mut r = registry();
    let spec = format!(
        "{SPEC}\n[[versions.override]]\nid = \"head\"\npolicy = \"year-all+major\"\nselection = \"impact\"\nrank_below = 2"
    );
    let res = resolved(&spec, &mut r);
    // chalk (rank 1): as-of 2026-10-25 is over a year past every chalk
    // release, so the year adds nothing and the major lines are
    // unbounded — every line top, 2.4.2 included
    assert_versions(
        &res,
        "chalk",
        &[("major", "5.3.0"), ("major", "4.1.0"), ("major", "2.4.2")],
    );
    // ms (rank 2, not below the rank cap): the default policy
    assert_versions(&res, "ms", &[("latest", "2.1.3")]);
}

#[test]
fn a_names_refusal_skips_a_package_before_its_packument_is_needed() {
    let mut r = registry();
    // no packument for left-pad at all: the refusal means the planner
    // never asks for it
    r.packs.remove("left-pad").expect("removed");
    let spec = format!(
        "{SPEC}\n[[refuse]]\nid = \"junk\"\nreason = \"a test junk package\"\nnames = [\"left-pad\"]"
    );
    let res = resolved(&spec, &mut r);
    assert_eq!(
        res.skipped.get("junk").map(|v| v.as_slice()),
        Some(["left-pad".to_string()].as_slice())
    );
    assert_eq!(
        res.selections
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["chalk", "ms", "ansi-styles"]
    );
}

#[test]
fn a_version_refusal_drops_one_version_with_its_reason() {
    let mut r = registry();
    let spec = format!(
        "{SPEC}\n[[refuse]]\nid = \"broken\"\nreason = \"its build OOMs\"\nversions = [\"chalk@4.1.0\"]"
    );
    let res = resolved(&spec, &mut r);
    assert_versions(&res, "chalk", &[("latest", "5.3.0"), ("major-2", "2.4.2")]);
    assert_eq!(
        plan_of(&res, "chalk").refused.as_slice(),
        [("4.1.0".to_string(), "its build OOMs".to_string())]
    );
}

#[test]
fn a_platform_refusal_drops_a_platform_package_version() {
    let mut r = registry();
    r.packs
        .get_mut("ms")
        .expect("ms")
        .deps
        .get_mut("2.1.3")
        .expect("meta")
        .platform = true;
    let spec = format!(
        "{SPEC}\n[[refuse]]\nid = \"plat\"\nreason = \"a platform binary\"\nplatform = true"
    );
    let res = resolved(&spec, &mut r);
    assert_versions(&res, "ms", &[]);
    assert_eq!(
        plan_of(&res, "ms").refused.as_slice(),
        [("2.1.3".to_string(), "a platform binary".to_string())]
    );
}

#[test]
fn a_size_refusal_drops_an_oversized_version() {
    let mut r = registry();
    r.packs
        .get_mut("ms")
        .expect("ms")
        .deps
        .get_mut("2.1.3")
        .expect("meta")
        .unpacked_size = Some(1_000);
    let spec =
        format!("{SPEC}\n[[refuse]]\nid = \"big\"\nreason = \"too big\"\nunpacked_size_over = 999");
    let res = resolved(&spec, &mut r);
    assert_versions(&res, "ms", &[]);
    assert_eq!(
        plan_of(&res, "ms").refused.as_slice(),
        [("2.1.3".to_string(), "too big".to_string())]
    );
}

#[test]
fn an_alias_specifier_resolves_its_target() {
    let mut r = registry();
    // ms@2.1.3 depends on `cli-color: npm:chalk@^2` — the ALIAS's target
    // is what resolves, the alias name is never a package of its own
    r.packs
        .get_mut("ms")
        .expect("ms")
        .deps
        .get_mut("2.1.3")
        .expect("meta")
        .dependencies = [("cli-color".to_string(), "npm:chalk@^2".to_string())]
        .into_iter()
        .collect();
    let res = resolved(SPEC, &mut r);
    assert!(
        plan_of(&res, "chalk")
            .versions
            .iter()
            .any(|s| s.version == "2.4.2"),
        "{:?}",
        plan_of(&res, "chalk").versions
    );
    // the prefetch waves resolve the alias too: the ghost KEY's packument
    // is never asked for (the registry would 404 it — the real package is
    // the VALUE's target)
    assert!(
        !r.asked.contains("cli-color"),
        "the alias key was prefetched: {:?}",
        r.asked
    );
    assert!(
        res.selections.iter().all(|s| s.name != "cli-color"),
        "the alias name is not indexed"
    );
}

#[test]
fn a_spec_whose_dependency_rules_meet_an_old_snapshot_fails_loud() {
    let mut r = registry();
    for p in r.packs.values_mut() {
        p.deps_fetched = false;
    }
    let spec = IndexSpec::parse(SPEC).expect("spec");
    let mut src = sources(&mut r);
    let err = planner::resolve(&spec, &mut src).expect_err("loud");
    assert!(
        err.contains("per-version") && err.contains("snapshot"),
        "{err}"
    );
}

#[test]
fn a_missing_packument_fails_loud() {
    let mut r = registry();
    r.packs.remove("chalk").expect("removed");
    let spec = IndexSpec::parse(SPEC).expect("spec");
    let mut src = sources(&mut r);
    let err = planner::resolve(&spec, &mut src).expect_err("loud");
    assert!(err.contains("chalk"), "{err}");
}

/// A LIST member the registry does not know fails loud: the list is a
/// PINNED public artifact, so its member's absence is a fetch-class
/// failure (the bug shape a mis-encoding fetcher produces — 404ing a
/// whole class of names — always reaches the list, which is full of
/// scoped names), not absence to plan around. The failure names the
/// package.
#[test]
fn a_list_members_absence_fails_loud() {
    let mut r = registry();
    r.packs.remove("left-pad").expect("removed");
    r.absent.insert("left-pad".to_string());
    let spec = IndexSpec::parse(SPEC).expect("spec");
    let mut src = sources(&mut r);
    let err = planner::resolve(&spec, &mut src).expect_err("loud");
    assert!(err.contains("left-pad"), "{err}");
    assert!(err.contains("list"), "{err}");
}

/// A CLOSURE member the registry does not know is absence as data: the
/// dependency graph is full of packages that are gone (a backlink-spam
/// package's hundreds of fake dependency names are mostly unpublished —
/// 2,342 of them on the first default-spec fetch, exp184). It is skipped
/// and recorded, never entered, never fetched again — the plan is the
/// rest.
#[test]
fn a_closure_members_absence_is_data() {
    let mut r = registry();
    r.packs.remove("ansi-styles").expect("removed");
    r.absent.insert("ansi-styles".to_string());
    let res = resolved(SPEC, &mut r);
    assert_eq!(
        res.skipped.get(planner::SKIP_ABSENT).map(|v| v.as_slice()),
        Some(["ansi-styles".to_string()].as_slice())
    );
    assert_eq!(
        res.selections
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["chalk", "ms", "left-pad"]
    );
}

/// The outage rule's boundary: more than 2% of a list absent is an
/// outage (a fetch bug 404ing a whole class of names), at or under it a
/// legitimate tail (packages removed since the pin — 213 of 17,338 on
/// the default spec's list, 1.2%).
#[test]
fn the_list_absence_outage_rule_boundaries() {
    // the measured legitimate tail
    assert_eq!(planner::list_absence_outage(213, 17_338), None);
    // a third of a 3-name list is not a tail
    assert!(planner::list_absence_outage(1, 3).is_some());
    // the outage shape the classifier used to catch
    assert!(planner::list_absence_outage(10_000, 17_338).is_some());
    // exact boundary at 2%: 17338 * 1/50 = 346.76 → 347 is the first
    // failing count
    assert_eq!(planner::list_absence_outage(346, 17_338), None);
    assert!(planner::list_absence_outage(347, 17_338).is_some());
    // zero absent is always fine
    assert_eq!(planner::list_absence_outage(0, 17_338), None);
}

/// An absentee the walk never CONSULTS is not recorded. The wave's
/// `want` names every dep of the queued versions WITHOUT parsing
/// ranges, so the wave may fetch a dep whose edge is a git URL — and in
/// the LIVE shape the loader reports it absent. If the resolution
/// recorded THAT, the offline replay of the same snapshot (the absent
/// packument already on disk, the wave reporting nothing) would record
/// a DIFFERENT plan — falsifying "the plan is a function of (spec,
/// snapshot)" (exp184: 505 spam-cluster absentees differed, live vs
/// offline). enter() is the only decision point: a dep the walk skips
/// at the range guard is never recorded.
#[test]
fn a_wave_absentee_the_walk_never_consults_is_not_recorded() {
    // the LIVE shape: the wave fetches the ghost, the loader reports it
    let mut r = registry();
    git_dep_on_an_absent_ghost(&mut r);
    r.absent.insert("ghost-pkg".to_string());
    let spec = IndexSpec::parse(SPEC).expect("spec");
    let mut src = sources(&mut r);
    let res = planner::resolve(&spec, &mut src).expect("resolves");
    assert_eq!(
        res.skipped.get(planner::SKIP_ABSENT).map(|v| v.as_slice()),
        None,
        "the wave's absentee the walk never consults was recorded: {:?}",
        res.skipped.get(planner::SKIP_ABSENT)
    );

    // the OFFLINE replay of the same snapshot: the absent packument is
    // already recorded, the wave reports nothing — the same records
    let mut r = registry();
    git_dep_on_an_absent_ghost(&mut r);
    r.absent_files.insert("ghost-pkg".to_string());
    let spec = IndexSpec::parse(SPEC).expect("spec");
    let mut src = sources(&mut r);
    let res = planner::resolve(&spec, &mut src).expect("resolves");
    assert_eq!(
        res.skipped.get(planner::SKIP_ABSENT).map(|v| v.as_slice()),
        None
    );
}

/// An absence RECORDED in the snapshot (the fetcher wrote an absent
/// packument) replays offline: the loader hands back an absent packument,
/// the resolution skips the package and records it — the offline plan is
/// the live one. (The absent package here is a CLOSURE member — a list
/// member's absence fails loud, see `a_list_members_absence_fails_loud`.)
#[test]
fn an_absent_packument_from_the_loader_is_skipped_and_recorded() {
    let mut r = registry();
    r.packs.remove("ansi-styles").expect("removed");
    r.absent_files.insert("ansi-styles".to_string());
    let res = resolved(SPEC, &mut r);
    assert_eq!(
        res.skipped.get(planner::SKIP_ABSENT).map(|v| v.as_slice()),
        Some(["ansi-styles".to_string()].as_slice())
    );
    assert_eq!(
        res.selections
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["chalk", "ms", "left-pad"]
    );
}

/// A registry where two dependent versions of ansi-styles compete: 3.4.0
/// resolved by two planned versions (ms@2.1.3 and chalk@2.4.2, both
/// `^3`), 2.0.0 by one (left-pad@1.3.0, `~2.0.0`). 3.4.0 itself depends
/// on a package the registry does not hold: walking a DEPENDENT version's
/// own dependencies would fail the resolution.
fn competing_dependents() -> TestRegistry {
    let mut r = registry();
    r.packs.insert(
        "ansi-styles".into(),
        packument(
            "ansi-styles",
            r#""created":"2013-01-01T00:00:00.000Z","2.0.0":"2015-01-01T00:00:00.000Z","3.4.0":"2019-01-01T00:00:00.000Z","5.2.0":"2021-01-01T00:00:00.000Z","6.1.0":"2022-06-01T00:00:00.000Z","7.0.0":"2023-04-01T00:00:00.000Z""#,
            r#""2.0.0","3.4.0","5.2.0","6.1.0","7.0.0""#,
            &[("3.4.0", &[("not-in-the-registry", "^1")])],
        ),
    );
    r.packs.get_mut("chalk").expect("chalk").deps.insert(
        "2.4.2".to_string(),
        VersionMeta {
            dependencies: [("ansi-styles".to_string(), "^3".to_string())]
                .into_iter()
                .collect(),
            ..VersionMeta::default()
        },
    );
    r.packs.get_mut("left-pad").expect("left-pad").deps.insert(
        "1.3.0".to_string(),
        VersionMeta {
            dependencies: [("ansi-styles".to_string(), "~2.0.0".to_string())]
                .into_iter()
                .collect(),
            ..VersionMeta::default()
        },
    );
    r
}

#[test]
fn the_dependents_cap_keeps_the_most_resolved_versions_per_package() {
    let dependents = |res: &ResolvedSpec| -> Vec<String> {
        plan_of(res, "ansi-styles")
            .versions
            .iter()
            .filter(|s| s.role == "dependent")
            .map(|s| s.version.clone())
            .collect()
    };
    // a cap of 1: only the version two dependents resolve to
    let mut r = competing_dependents();
    let res = resolved(&SPEC.replace("dependents = 2", "dependents = 1"), &mut r);
    assert_eq!(dependents(&res), vec!["3.4.0".to_string()]);
    let pick = plan_of(&res, "ansi-styles")
        .versions
        .iter()
        .find(|s| s.version == "3.4.0")
        .expect("planned");
    assert_eq!(
        pick.attribution.as_deref(),
        Some("sel=dep-trees dependents=2 first=chalk@2.4.2 range=^3"),
    );
    // a cap of 2: both, most-resolved first in the record's own order
    let mut r = competing_dependents();
    let res = resolved(SPEC, &mut r);
    assert_eq!(
        dependents(&res),
        vec!["3.4.0".to_string(), "2.0.0".to_string()]
    );
    // 0 turns the rule off
    let mut r = competing_dependents();
    let res = resolved(&SPEC.replace("dependents = 2", "dependents = 0"), &mut r);
    assert!(dependents(&res).is_empty());
}

/// left-pad 1.3.0 (2016-06-01T00:00:00.000Z) pins ansi-styles 2.1.0,
/// published at `sibling` — a monorepo-style release a moment later.
fn late_sibling(sibling: &str) -> TestRegistry {
    let mut r = registry();
    r.packs.insert(
        "ansi-styles".into(),
        packument(
            "ansi-styles",
            &format!(
                r#""created":"2013-01-01T00:00:00.000Z","2.0.0":"2015-01-01T00:00:00.000Z","2.1.0":"{sibling}","2.2.0":"2023-04-01T00:00:00.000Z""#
            ),
            r#""2.0.0","2.1.0","2.2.0""#,
            &[],
        ),
    );
    r.packs.get_mut("left-pad").expect("left-pad").deps.insert(
        "1.3.0".to_string(),
        VersionMeta {
            dependencies: [("ansi-styles".to_string(), "2.1.0".to_string())]
                .into_iter()
                .collect(),
            ..VersionMeta::default()
        },
    );
    r
}

#[test]
fn a_pin_published_within_the_grace_after_its_dependent_still_resolves() {
    let dependents = |res: &ResolvedSpec| -> Vec<String> {
        plan_of(res, "ansi-styles")
            .versions
            .iter()
            .filter(|s| s.role == "dependent")
            .map(|s| s.version.clone())
            .collect()
    };
    // 0.9 s later: what `npm install --before=<own publish time>` misses
    // and the installer's grace retry gets
    let mut r = late_sibling("2016-06-01T00:00:00.900Z");
    let res = resolved(SPEC, &mut r);
    assert_eq!(dependents(&res), vec!["2.1.0".to_string()]);
    // two hours later: outside the grace, nothing resolves
    let mut r = late_sibling("2016-06-01T02:00:00.000Z");
    assert!(dependents(&resolved(SPEC, &mut r)).is_empty());
}
