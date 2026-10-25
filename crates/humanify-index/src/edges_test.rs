//! INSIDE call edges in the index (exp151, format `/4`): the builder
//! records, per unit row, the prints of the units its statement references
//! inside its own package (and, apart, in other packages), from the same
//! reference graph the pipeline builds — never who references it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use humanify_core::graph::edges::Scope;
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

/// A shard of `rows` (the bun-node slice of `dep@1.0.0`).
fn shard_of(rows: Vec<ShardUnit>) -> Shard {
    let mut s = Shard::new(&ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package: "dep".into(),
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
