//! INSIDE call edges in the index (exp151, format `/4`): the builder
//! records, per unit row, the prints of the units its statement references
//! inside its own package (and, apart, in other packages), from the same
//! reference graph the pipeline builds — never who references it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use humanify_core::graph::edges::Scope;
use humanify_core::libdetect::discovery::CallContextRow;
use humanify_core::libindex::format::{Callee, EdgeRow, SliceRecord};
use humanify_core::libindex::lookup::LibraryIndex;
use humanify_core::libindex::recipe::{TerserSlices, detect_recipe};
use humanify_core::matching::store::{EdgeRead, ReferenceStore, StoreEdges};
use humanify_model::detection::{BundlerType, MinifierType};

use super::build::{SpongeRegions, unit_rows};
use super::merge::{MergeMeta, merge};
use super::shard::{Shard, ShardInputs, ShardKind, ShardNotes, ShardUnit};
use super::sourcemap::SourceMap;
use crate::bun_node;

/// A fake build: `node_modules/<pkg>/<file>` for every (pkg, file) of
/// `lines`, and a bundle whose line `i` is `lines[i].2`, mapped wholly to
/// `lines[i]`'s file (`None`: the app entry).
fn bundle(tag: &str, lines: &[(Option<&str>, &str, &str)]) -> (PathBuf, String, SourceMap) {
    let root =
        std::env::temp_dir().join(format!("humanify-index-edges-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    let mut sources: Vec<String> = Vec::new();
    let mut segments = Vec::new();
    for (line, (pkg, file, text)) in lines.iter().enumerate() {
        let source = match pkg {
            Some(pkg) => {
                write(
                    &format!("node_modules/{pkg}/package.json"),
                    &format!(r#"{{"name":"{pkg}","version":"1.0.0"}}"#),
                );
                write(&format!("node_modules/{pkg}/{file}"), "");
                format!("../node_modules/{pkg}/{file}")
            }
            None => "../entries/req.cjs".to_string(),
        };
        let src = match sources.iter().position(|s| *s == source) {
            Some(i) => i,
            None => {
                sources.push(source);
                sources.len() - 1
            }
        } as u32;
        segments.extend(
            (0..text.len() as u32)
                .step_by(4)
                .map(|c| (line as u32, c, src, 0)),
        );
    }
    let code: String = lines.iter().map(|(_, _, t)| format!("{t}\n")).collect();
    (root, code, SourceMap { sources, segments })
}

fn rows_of(root: &Path, code: &str, map: &SourceMap) -> (Vec<ShardUnit>, ShardNotes) {
    unit_rows(
        bun_node(),
        code,
        map,
        &root.join("out"),
        SpongeRegions::Drop,
        false,
    )
}

/// The row of `file`'s WHOLE statement (its largest print).
fn whole<'r>(rows: &'r [ShardUnit], owner: &str, file: &str) -> &'r ShardUnit {
    rows.iter()
        .filter(|r| r.owner == owner && r.file == file)
        .max_by_key(|r| r.nodes)
        .unwrap_or_else(|| panic!("a row of {owner}/{file}: {rows:?}"))
}

const SPLIT: &str =
    "function splitter(t){return t.split(\",\").map(function(e){return e.trim().toLowerCase()})}";
const JOIN: &str =
    "function joiner(t){return t.join(\"|\").toUpperCase().trim().padStart(3,\"-\").slice(1)}";
const USE_A: &str = "function useA(x){var r=splitter(x);return r.length+r.concat([x]).length+1}";
const USE_B: &str = "function useB(x){var r=joiner(x);return r.length+r.concat([x]).length+1}";

/// The two-callers library: `useA` and `useB` print the same (their
/// callees are slots) and call different functions.
fn twin_callers(tag: &str) -> (PathBuf, Vec<ShardUnit>) {
    let (root, code, map) = bundle(
        tag,
        &[
            (Some("dep"), "split.js", SPLIT),
            (Some("dep"), "join.js", JOIN),
            (Some("dep"), "a.js", USE_A),
            (Some("dep"), "b.js", USE_B),
        ],
    );
    let (rows, _) = rows_of(&root, &code, &map);
    (root, rows)
}

#[test]
fn twin_bodies_with_different_callees_record_different_edges() {
    let (root, rows) = twin_callers("twins");
    let (a, b) = (whole(&rows, "dep", "a.js"), whole(&rows, "dep", "b.js"));
    assert_eq!(a.hash, b.hash, "the fixture's premise: one print");
    let (split, join) = (
        whole(&rows, "dep", "split.js"),
        whole(&rows, "dep", "join.js"),
    );
    assert_ne!(split.hash, join.hash);
    assert_eq!(a.inside, vec![split.hash.clone()]);
    assert_eq!(b.inside, vec![join.hash.clone()]);
    assert!(a.out_of.is_empty() && b.out_of.is_empty());
    assert!(split.inside.is_empty(), "a leaf calls nothing: {split:?}");
    std::fs::remove_dir_all(&root).ok();
}

/// A shard of `rows` (the bun-node slice of `<first row's owner>@1.0.0`).
fn shard_of(rows: Vec<ShardUnit>) -> Shard {
    let package = rows.first().map_or("dep".to_string(), |r| r.owner.clone());
    let mut s = Shard::new(&ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package,
        version: "1.0.0".into(),
        bundler: Some("bun@1.4.0".into()),
        recipe: "recipe/4 bun:target=node,format=cjs,minify".into(),
        fingerprint: "test".into(),
    });
    s.units = rows;
    s
}

/// The merged index of `shards`, on disk at `dir`, loaded for a Bun
/// target-node bundle.
fn load_merged(dir: &Path, shards: Vec<Shard>) -> (LibraryIndex, BTreeMap<String, Vec<u8>>) {
    let recipe = bun_node();
    let meta = MergeMeta {
        as_of: "2026-10-11T23:59:59.999Z".into(),
        slices: vec![SliceRecord {
            slice: recipe.slice_name(),
            recipe: recipe.spelling(),
            recipe_version: humanify_core::libindex::format::RECIPE_VERSION.into(),
            default_for_bundler: true,
            bundler: "bun".into(),
            version: "1.4.0".into(),
            published: None,
            newest_at_as_of: None,
            bundler_after_as_of: false,
        }],
        ..MergeMeta::default()
    };
    let files = merge(meta, shards).expect("merges").files;
    std::fs::create_dir_all(dir).expect("mkdir");
    for (name, bytes) in &files {
        std::fs::write(dir.join(name), bytes).expect("write");
    }
    let target_node = detect_recipe("var a;", BundlerType::Bun, MinifierType::Bun);
    let ix = LibraryIndex::load(dir, &target_node, TerserSlices::Off).expect("loads");
    (ix, files)
}

#[test]
fn the_index_tells_twin_bodies_apart_through_their_edges() {
    let (root, rows) = twin_callers("twins-index");
    let hash = whole(&rows, "dep", "a.js").hash.clone();
    let split = whole(&rows, "dep", "split.js").hash.clone();
    let join = whole(&rows, "dep", "join.js").hash.clone();
    let (ix, _) = load_merged(&root.join("ix"), vec![shard_of(rows)]);
    assert!(ix.has_edges());
    let hits = ix.candidates(&hash);
    assert_eq!(hits.len(), 2, "one print, two rows (a.js, b.js)");
    let callees: BTreeSet<Vec<String>> = hits
        .iter()
        .map(|&h| match ix.edges(h, EdgeRead::Scoped(Scope::Inside)) {
            Ok(StoreEdges::Recorded(c)) => c.iter().map(|c| format!("{:016x}", c.key)).collect(),
            other => panic!("a /4 index records edges: {other:?}"),
        })
        .collect();
    assert_eq!(
        callees,
        BTreeSet::from([vec![split], vec![join]]),
        "the two rows differ by their callees"
    );
    for &h in hits.iter() {
        assert_eq!(
            ix.edges(h, EdgeRead::Scoped(Scope::OutOf)),
            Ok(StoreEdges::Recorded(vec![]))
        );
    }
    std::fs::remove_dir_all(&root).ok();
}

const ALPHA_HELPER: &str =
    "function trimAll(t){return t.map(function(e){return e.trim().toLowerCase().normalize()})}";
const ALPHA_MAIN: &str = "function parseList(t){var r=trimAll(t.split(\",\"));return quote(r.join(\";\")).length+r.length}";
const BETA_HELPER: &str = "function esc(s){return String(s).split(\"<\").join(\"&lt;\").split(\">\").join(\"&gt;\").trim()}";
const BETA_MAIN: &str =
    "function quote(s){var q=esc(String(s));return \"[\"+q+\"]\"+(q.length>80?\"...\":\"\")}";
const APP: &str =
    "var main=function(){return parseList(process.argv.slice(2)).toString()+quote(\"x\").length};";

#[test]
fn a_two_package_builds_edges_stay_with_each_caller_and_never_record_who_calls_in() {
    let (root, code, map) = bundle(
        "two",
        &[
            (Some("alpha"), "trim.js", ALPHA_HELPER),
            (Some("alpha"), "index.js", ALPHA_MAIN),
            (Some("beta"), "esc.js", BETA_HELPER),
            (Some("beta"), "index.js", BETA_MAIN),
            (None, "", APP),
        ],
    );
    let (rows, notes) = rows_of(&root, &code, &map);
    assert_eq!(notes.unowned, 1, "the app entry's statement");
    let owners = Owners::of(&rows);
    for r in &rows {
        owners.check(&r.owner, &r.inside, &r.out_of, &format!("{r:?}"));
    }
    let main = whole(&rows, "alpha", "index.js");
    assert_eq!(
        main.inside,
        vec![whole(&rows, "alpha", "trim.js").hash.clone()]
    );
    assert_eq!(
        main.out_of
            .iter()
            .map(|c| (c.owner.as_str(), c.hash.as_str()))
            .collect::<Vec<_>>(),
        vec![("beta", whole(&rows, "beta", "index.js").hash.as_str())],
        "alpha calls beta: OUT-OF alpha, recorded on alpha's row"
    );
    // beta is called by alpha and by the app: neither caller is recorded
    // on beta (an INTO edge is the caller's, not the library's)
    for r in rows.iter().filter(|r| r.owner == "beta") {
        assert!(r.out_of.is_empty(), "{r:?}");
        assert!(r.inside.iter().all(|h| !owners.alpha.contains(h)), "{r:?}");
    }
    assert_eq!(
        whole(&rows, "beta", "index.js").inside,
        vec![whole(&rows, "beta", "esc.js").hash.clone()]
    );
    merged_edges_stay_with_their_owner(&root, &rows, &owners, &main.hash);
    std::fs::remove_dir_all(&root).ok();
}

/// The prints each package of the two-package build owns.
struct Owners {
    alpha: BTreeSet<String>,
    beta: BTreeSet<String>,
}

impl Owners {
    fn of(rows: &[ShardUnit]) -> Owners {
        let prints = |owner: &str| -> BTreeSet<String> {
            rows.iter()
                .filter(|r| r.owner == owner)
                .map(|r| r.hash.clone())
                .collect()
        };
        Owners {
            alpha: prints("alpha"),
            beta: prints("beta"),
        }
    }

    /// An INSIDE callee is the row's own package's; an OUT-OF one another
    /// package's.
    fn check(&self, owner: &str, inside: &[String], out_of: &[Callee], row: &str) {
        let own = if owner == "alpha" {
            &self.alpha
        } else {
            &self.beta
        };
        assert!(inside.iter().all(|h| own.contains(h)), "{row}");
        assert!(out_of.iter().all(|c| c.owner != owner), "{row}");
    }
}

/// Merged: every edge row stays with its owner, and the store reads
/// alpha's call into beta as OUT-OF alpha.
fn merged_edges_stay_with_their_owner(
    root: &Path,
    rows: &[ShardUnit],
    owners: &Owners,
    main: &str,
) {
    let (ix, files) = load_merged(&root.join("ix"), vec![shard_of(rows.to_vec())]);
    let edge_rows: Vec<EdgeRow> = std::str::from_utf8(&files["edges.bun-node-cjs-min.jsonl"])
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("an edge row"))
        .collect();
    assert!(!edge_rows.is_empty());
    for e in &edge_rows {
        owners.check(&e.owner, &e.inside, &e.out_of, &format!("{e:?}"));
    }
    let main_hit = ix
        .candidates(main)
        .iter()
        .copied()
        .find(|h| ix.package_name(h.owner) == "alpha")
        .expect("alpha's main row");
    let Ok(StoreEdges::Recorded(out)) = ix.edges(main_hit, EdgeRead::Scoped(Scope::OutOf)) else {
        panic!("recorded");
    };
    let names: Vec<&str> = out.iter().map(|c| ix.package_name(c.owner)).collect();
    assert_eq!(names, ["beta"]);
    assert!(ix.edges(main_hit, EdgeRead::Scoped(Scope::Into)).is_err());
}

// ---------------------------------------------------------------------------
// The call-context rule (exp153, library-names plan step 2)
// ---------------------------------------------------------------------------

const CC_SPLIT: &str = "function splitter(t){return t.split(\",\").map(function(e){return e.trim().toLowerCase().replace(/\\s+/g,\"-\")}).filter(Boolean).slice(0,t.length)}";
const CC_JOIN: &str = "function joiner(t){return t.join(\"|\").toUpperCase().trim().padStart(3,\"-\").slice(1).concat(\"!\").repeat(2).split(\"|\")}";
const CC_ESC: &str = "function escaper(t){return String(t).split(\"<\").join(\"&lt;\").split(\">\").join(\"&gt;\").trim().replace(/&/g,\"&amp;\").concat(\"\")}";
const CC_USE_A: &str = "function useA(x){var r=splitter(x),s=\"items\";return r.length+r.concat([x]).length+s.length+r.indexOf(\"first\")+r.lastIndexOf(\"last\")+1}";
const CC_USE_B: &str = "function useB(x){var r=joiner(x),s=\"items\";return r.length+r.concat([x]).length+s.length+r.indexOf(\"first\")+r.lastIndexOf(\"last\")+1}";
const CC_USE_ESC: &str = "function useEsc(x){var r=escaper(x),k=\"label\";return r.length+joiner(r).length+r.indexOf(x)+k.length+r.lastIndexOf(\"end\")+2}";

/// One package's fake build (`lines` all its own), as a shard of its rows.
fn package_shard(tag: &str, package: &str, lines: &[(&str, &str)]) -> (PathBuf, Vec<ShardUnit>) {
    let owned: Vec<(Option<&str>, &str, &str)> =
        lines.iter().map(|(f, t)| (Some(package), *f, *t)).collect();
    let (root, code, map) = bundle(tag, &owned);
    let (rows, _) = rows_of(&root, &code, &map);
    (root, rows)
}

/// Discovery on `code` (a Bun target-node bundle with no module helper)
/// against `ix` under the shipped rules with `call_context`; `census`
/// asks for the call-context rows (the exp153 instrument: decides
/// nothing).
fn discover_with(
    ix: &LibraryIndex,
    code: &str,
    call_context: humanify_core::libindex::lookup::CallContextRules,
    census: bool,
) -> (
    humanify_core::graph::references::ReferenceGraph,
    humanify_core::libdetect::discovery::Discovery,
) {
    use humanify_core::libdetect::discovery::{
        BundlePieces, DiscoveryOptions, IndexLookup, discover_in_bundle,
    };
    use humanify_core::libindex::lookup::LookupRules;
    use humanify_core::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "{:?}", ingest.errors);
    let rules = LookupRules {
        call_context,
        ..LookupRules::recommended()
    };
    let (graph, found) = discover_in_bundle(
        code,
        &ingest,
        BundlePieces {
            layout: BundleLayout::SingleWrapperFunction,
            grammar: ModuleWrapperGrammar::BunAndEsbuild,
            interop: InteropHelpers::Bun,
        },
        &[],
        &[],
        Some(IndexLookup {
            index: ix,
            rules,
            targeted: Some(LookupRules {
                min_nodes: 20,
                support: 0,
                ..rules
            }),
            census,
            restoration: false,
        }),
        DiscoveryOptions::recommended(),
    );
    (graph, found)
}

/// The claims discovery makes on `code` (a Bun target-node bundle with no
/// module helper) against `ix` under the shipped rules with `call_context`.
fn claims_with(
    ix: &LibraryIndex,
    code: &str,
    call_context: humanify_core::libindex::lookup::CallContextRules,
) -> Vec<(String, String)> {
    let (graph, found) = discover_with(ix, code, call_context, false);
    found
        .claims
        .iter()
        .map(|(&u, c)| {
            let span = graph.units[u as usize].span;
            let text = &code[span.start as usize..span.end as usize];
            (
                text.split('(').next().unwrap_or("").to_string(),
                c.package.clone(),
            )
        })
        .collect()
}

/// The call-context census rows of discovery on `code` (the exp153
/// instrument, plus exp155's scoped fits and `recorded_by`).
fn census_with(ix: &LibraryIndex, code: &str) -> Vec<CallContextRow> {
    let (_, found) = discover_with(
        ix,
        code,
        humanify_core::libindex::lookup::CallContextRules::default(),
        true,
    );
    found.call_context
}

#[test]
fn call_context_breaks_a_tie_between_twin_bodies_by_their_callees() {
    // alpha and beta ship the same body (`useA` / `useB` print the same:
    // the callee is a slot) calling different helpers
    let (ra, alpha) = package_shard(
        "cc-alpha",
        "alpha",
        &[("split.js", CC_SPLIT), ("use.js", CC_USE_A)],
    );
    let (rb, beta) = package_shard(
        "cc-beta",
        "beta",
        &[
            ("join.js", CC_JOIN),
            ("esc.js", CC_ESC),
            ("use.js", CC_USE_B),
            ("use-esc.js", CC_USE_ESC),
        ],
    );
    assert_eq!(
        whole(&alpha, "alpha", "use.js").hash,
        whole(&beta, "beta", "use.js").hash,
        "the fixture's premise: one print, two packages"
    );
    let (ix, _) = load_merged(&ra.join("ix"), vec![shard_of(alpha), shard_of(beta)]);
    // the app bundles beta: the twin first (no earlier pick to lean on)
    let code = format!(
        "{CC_USE_B}\n{CC_JOIN}\n{CC_ESC}\n{CC_USE_ESC}\nvar main=function(){{return useB(process.argv)+useEsc(process.argv[0])}};\n"
    );
    let off = humanify_core::libindex::lookup::CallContextRules::default();
    let today = claims_with(&ix, &code, off);
    let twin = |claims: &[(String, String)]| {
        claims
            .iter()
            .find(|(head, _)| head == "function useB")
            .map(|(_, p)| p.clone())
    };
    assert_ne!(
        twin(&today),
        Some("beta".to_string()),
        "today the tie goes alphabetically to alpha, which nothing supports: {today:?}"
    );
    let tie_break = humanify_core::libindex::lookup::CallContextRules {
        tie_break: true,
        ..off
    };
    let with = claims_with(&ix, &code, tie_break);
    assert_eq!(
        twin(&with),
        Some("beta".to_string()),
        "the twin calls joiner, which only beta's row records: {with:?}"
    );
    assert!(
        with.iter().all(|(_, p)| p == "beta"),
        "no alpha claim anywhere: {with:?}"
    );
    std::fs::remove_dir_all(&ra).ok();
    std::fs::remove_dir_all(&rb).ok();
}

// ---------------------------------------------------------------------------
// The DOWN tree's flat callee-print table (exp155, edges/2): a callee the
// index records without it owning a unit row — below the row floor, or in
// another package's OUT-OF list — is key-queryable, and the census's
// scoped fits can tell "the index knows this nowhere" (silence) apart
// from "another family records it" (the veto's evidence).
// ---------------------------------------------------------------------------

const DP_API: &str = "function depapi(t){return t.split(\",\").map(function(e){return e.trim().toUpperCase().padEnd(5,\"x\")}).filter(Boolean).join(\"|\").repeat(2)}";
const LB_TINY: &str = "function tinyBase(s){return s.trim()}";
const LB_MAIN: &str = "function mainlib(x){var r=tinyBase(x.split(\",\")),d=depapi(r),s=\"items\";return d.length+r.concat([x]).length+s.length+r.indexOf(\"first\").valueOf()+r.lastIndexOf(\"last\").trim()+1}";

/// A library calling an under-row-floor helper of its own and a function
/// in its dependency: the helper can own no unit row (the floor) and the
/// dependency's print sits in ANOTHER package's rows — the naive fit
/// always held both as contained keys, but before edges/2 nothing could
/// ask "who records this print" for either; the flat table answers, and
/// the DOWN containment stays confirming across the floor and the package
/// boundary.
#[test]
fn the_flat_table_names_a_rowless_below_floor_helper_and_a_cross_package_callee() {
    let (root, code, map) = bundle(
        "down-lib",
        &[
            (Some("dep"), "api.js", DP_API),
            (Some("lib"), "tiny.js", LB_TINY),
            (Some("lib"), "main.js", LB_MAIN),
            (
                None,
                "",
                "var main=function(){return mainlib(process.argv.slice(2))+depapi(\"x\").length}",
            ),
        ],
    );
    let (rows, _) = rows_of(&root, &code, &map);
    assert!(
        !rows.iter().any(|r| r.file == "tiny.js"),
        "the fixture's premise: the helper is below the row floor, so it owns no row"
    );
    let main = whole(&rows, "lib", "main.js");
    let helper = main.inside[0].clone();
    let api = whole(&rows, "dep", "api.js").hash.clone();
    assert_eq!(
        main.out_of
            .iter()
            .map(|c| (c.owner.clone(), c.hash.clone()))
            .collect::<Vec<_>>(),
        vec![("dep".to_string(), api.clone())]
    );
    let (ix, _) = load_merged(&root.join("ix"), vec![shard_of(rows)]);
    assert!(
        ix.has_callee_prints(),
        "a merged edges/2 index opens its flat callee-print table"
    );
    let id = |h: &str| u64::from_str_radix(h, 16).expect("a print key");
    assert!(
        ix.candidates(&helper).is_empty(),
        "the helper print owns no row anywhere"
    );
    assert_eq!(
        ix.callee_recorders(id(&helper))
            .iter()
            .map(|&o| ix.package_name(o).to_string())
            .collect::<Vec<_>>(),
        vec!["lib".to_string()],
        "the rowless helper print is key-queryable (INSIDE records the caller's package)"
    );
    assert_eq!(
        ix.callee_recorders(id(&api))
            .iter()
            .map(|&o| ix.package_name(o).to_string())
            .collect::<Vec<_>>(),
        vec!["dep".to_string()],
        "an OUT-OF callee is flat-recorded under its own package"
    );
    // lib's own code as the bundle: the pick's DOWN tree must hold BOTH
    // callees — the rowless helper and the one in another package's rows
    let down_code = format!(
        "{LB_TINY}\n{DP_API}\n{LB_MAIN}\nvar main=function(){{return mainlib(process.argv.slice(2))+depapi(\"x\").length}};\n"
    );
    let census = census_with(&ix, &down_code);
    let pick = census
        .iter()
        .find_map(|r| r.pick.as_ref().filter(|p| p.package == "lib"))
        .unwrap_or_else(|| panic!("a pick of lib: {census:?}"));
    assert!(
        pick.missing.is_empty(),
        "lib's row records both callees: {pick:?}"
    );
    let scoped = [
        pick.known_fit,
        pick.known_rowed_fit,
        pick.foreign_fit,
        pick.foreign_rowed_fit,
    ];
    assert!(
        scoped.iter().all(|f| f.is_some_and(|f| f.confirms())),
        "the DOWN containment stays confirming below the floor and across the package boundary: {pick:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

const UNKNOWN_HELPER: &str = "function quantum(x){return x.trim()}";
const CC_USE_Q: &str = "function useQ(x){var r=quantum(x),s=\"items\";return r.length+r.concat([x]).length+s.length+r.indexOf(\"first\")+r.lastIndexOf(\"last\")+1}";

/// A wrong-family claim whose callees genuinely diverge (the veto's case:
/// another family records them — `foreign_fit` refutes, `recorded_by`
/// names the right family), against an unknown callee (the census's old
/// refutation trap: the index knows it nowhere — the scoped fit goes
/// SILENT where the naive fit refutes).
#[test]
fn a_foreign_callee_refutes_the_wrong_family_and_an_unknown_callee_stays_silent() {
    let (ra, alpha) = package_shard(
        "c2-alpha",
        "alpha",
        &[("split.js", CC_SPLIT), ("use.js", CC_USE_A)],
    );
    let (rb, beta) = package_shard(
        "c2-beta",
        "beta",
        &[
            ("join.js", CC_JOIN),
            ("esc.js", CC_ESC),
            ("use.js", CC_USE_B),
            ("use-esc.js", CC_USE_ESC),
        ],
    );
    assert_eq!(
        whole(&alpha, "alpha", "use.js").hash,
        whole(&beta, "beta", "use.js").hash,
        "the fixture's premise: one twin print, two families"
    );
    let join = whole(&beta, "beta", "join.js").hash.clone();
    let (ix, _) = load_merged(&ra.join("ix"), vec![shard_of(alpha), shard_of(beta)]);
    // the app bundles alpha's splitter just before beta's useB (only beta
    // records joiner: foreign evidence) and a twin useQ whose helper only
    // the bundle knows. The structural tie follows the PREVIOUS pick, and
    // alpha's splitter right before the twin hands the tie to alpha: the
    // wrong family claims both twins
    let code = format!(
        "{CC_JOIN}\n{CC_ESC}\n{UNKNOWN_HELPER}\n{CC_SPLIT}\n{CC_USE_B}\n{CC_USE_Q}\nvar main=function(){{return useB(process.argv)+useQ(process.argv[0])+splitter(process.argv)}};\n"
    );
    let census = census_with(&ix, &code);
    let row_with =
        |wanted: &dyn Fn(&humanify_core::libdetect::discovery::MissingCallee) -> bool| {
            census
                .iter()
                .find(|r| {
                    r.pick
                        .as_ref()
                        .is_some_and(|p| p.missing.iter().any(wanted))
                })
                .unwrap_or_else(|| panic!("a pick with such a missing callee: {census:?}"))
        };
    // the WRONG family: alpha won the twin tie by adjacency, but only beta
    // records joiner
    let wrong_row = row_with(&|m| m.hash == join);
    let wrong = wrong_row.pick.as_ref().expect("checked");
    assert_eq!(
        wrong.package, "alpha",
        "the twin tie went to the family claimed just before it"
    );
    assert!(
        wrong.fit.is_some_and(|f| f.refutes()),
        "the naive fit refutes: {wrong:?}"
    );
    let foreign = wrong
        .foreign_fit
        .expect("the scoped fit reads the flat table");
    assert_eq!(
        (foreign.counted, foreign.held, foreign.skipped),
        (1, 0, 0),
        "joiner is recorded by another family: genuine refutation evidence, nothing skipped"
    );
    assert!(
        foreign.refutes(),
        "the veto's case: the wrong family's callees diverge"
    );
    assert!(wrong.known_fit.is_some_and(|f| f.refutes()));
    let missing = wrong.missing.iter().find(|m| m.hash == join).unwrap();
    assert!(missing.indexed && !missing.in_family);
    assert_eq!(
        missing.recorded_by,
        vec!["beta".to_string()],
        "the flat table names the right family for a row-holding callee too"
    );
    let family_fit = |pkg: &str| {
        wrong_row
            .families
            .iter()
            .find(|f| f.package == pkg)
            .unwrap_or_else(|| panic!("a {pkg} family: {wrong_row:?}"))
    };
    assert!(
        family_fit("beta").foreign_fit.is_some_and(|f| f.confirms()),
        "the right family's scoped fit confirms where the wrong one refutes"
    );
    assert!(
        family_fit("alpha").foreign_fit.is_some_and(|f| f.refutes()),
        "the wrong family's own scoped fit refutes: the veto's evidence"
    );
    // the UNKNOWN callee: the naive fit refutes; the scoped fit skips it
    let silent_row = row_with(&|m| !m.indexed);
    let silent = silent_row.pick.as_ref().expect("checked");
    assert!(
        silent.fit.is_some_and(|f| f.refutes()),
        "today's trap: unknown looks like refutation: {silent:?}"
    );
    let scoped = silent
        .known_fit
        .expect("the scoped fit reads the flat table");
    assert_eq!(
        (scoped.counted, scoped.held, scoped.skipped),
        (0, 0, 1),
        "the index knows the helper NOWHERE: skipped, not evidence against"
    );
    assert!(
        !scoped.confirms() && !scoped.refutes(),
        "silence, never a verdict"
    );
    assert!(
        silent.missing.iter().all(|m| m.recorded_by.is_empty()),
        "no flat record exists for it either: {silent:?}"
    );
    std::fs::remove_dir_all(&ra).ok();
    std::fs::remove_dir_all(&rb).ok();
}
