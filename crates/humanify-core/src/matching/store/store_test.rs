//! The two reference stores (docs/matching-architecture.md §3.4): the
//! index store never serves INTO edges, answers INSIDE / OUT-OF from a
//! `/4` index's edge table and `NotRecorded` from a `/3` one; the prior
//! store reads one program whole.

use crate::graph::edges::Scope;
use crate::libindex::format::{Callee, EdgeRow, UnitRow};
use crate::libindex::lookup::LibraryIndex;
use crate::matching::store::{EdgeRead, Label, ReferenceStore, StoreEdges, StoreId};

/// A test name's unit hash (16 hex digits, the only spelling an index
/// holds).
fn hx(name: &str) -> String {
    format!("{:016x}", crate::libindex::table::literal_key(name))
}

fn row(hash: &str, owner: &str) -> UnitRow {
    UnitRow {
        hash: hx(hash),
        owner: owner.into(),
        file: "index.js".into(),
        nodes: 40,
        content: 5,
        module_bytes: 900,
        versions: vec![("1.0.0".into(), "1.0.0".into())],
    }
}

#[test]
fn the_index_store_answers_by_print_and_never_serves_into_edges() {
    let mut ix = LibraryIndex::from_rows(vec![row("aa", "lodash"), row("aa", "lodash-es")], vec![]);
    ix.id = "id".into();
    ix.slice = "bun-bun-cjs-min".into();
    assert_eq!(
        ix.identity(),
        StoreId::Index {
            id: "id".into(),
            slice: "bun-bun-cjs-min".into()
        }
    );
    assert_eq!(ix.form().stamp(), crate::hash::forms::LIBRARY_UNITS.stamp());
    let hits = ix.candidates(&hx("aa"));
    assert_eq!(hits.len(), 2);
    assert_eq!(ix.label(hits[1]), Label::Package { name: "lodash-es" });
    assert!(ix.candidates(&hx("bb")).is_empty());
    assert_eq!(ix.literals(hits[0]), None, "literals are per file");
    let piece = hits[0];
    assert!(ix.edges(piece, EdgeRead::Scoped(Scope::Into)).is_err());
    assert!(ix.edges(piece, EdgeRead::All).is_err());
    for scope in [Scope::Inside, Scope::OutOf] {
        assert_eq!(
            ix.edges(piece, EdgeRead::Scoped(scope)),
            Ok(StoreEdges::NotRecorded),
            "this index format has no edge table: absence, never evidence"
        );
    }
}

#[test]
fn the_prior_store_is_the_fingerprint_index_read_whole() {
    use oxc_allocator::Allocator;
    let code = "function a() { return b() + 1; }\nfunction b() { return 2; }\nfunction c() { return 2; }\n";
    let allocator = Allocator::default();
    let ingest = crate::ingest::Ingest::parse(&allocator, code, "input.js");
    let tables = crate::hash::serialize::SymbolTables::build(ingest.semantic());
    let graph = crate::graph::build_unified_graph(
        ingest.semantic(),
        ingest.program,
        "input.js",
        crate::rename::eligibility::NeverRename::UNIVERSAL,
        crate::toolchain::BundleLayout::SingleWrapperFunction,
    );
    let index = crate::matching::build_fingerprint_index(&graph, ingest.semantic(), &tables);
    assert_eq!(index.identity(), StoreId::Prior { entries: 3 });
    // b and c print the same: one bucket of two, in entry order.
    let b = index.entry_of_session("input.js:2:0").expect("b");
    let hash = index.entries[b].fingerprint.structural_hash().to_string();
    assert_eq!(&*index.candidates(&hash), &[1, 2]);
    assert_eq!(
        index.label(b),
        Label::Prior {
            session_id: "input.js:2:0"
        }
    );
    let a = index.entry_of_session("input.js:1:0").expect("a");
    assert_eq!(
        index.edges(a, EdgeRead::All),
        Ok(StoreEdges::Recorded(vec![b]))
    );
    assert!(
        index.edges(a, EdgeRead::Scoped(Scope::Inside)).is_err(),
        "no regions"
    );
}

fn edge(hash: &str, owner: &str, inside: &[&str], out_of: &[(&str, &str)]) -> EdgeRow {
    EdgeRow {
        hash: hx(hash),
        owner: owner.into(),
        file: "index.js".into(),
        inside: inside.iter().map(|h| hx(h)).collect(),
        out_of: out_of
            .iter()
            .map(|(o, h)| Callee {
                owner: (*o).into(),
                hash: hx(h),
            })
            .collect(),
    }
}

/// The callees of `ix`'s first hit of `hash` owned by `owner`, in `scope`,
/// as (owner name, print).
fn callees(ix: &LibraryIndex, hash: &str, owner: &str, scope: Scope) -> Option<Vec<(String, u64)>> {
    let hit = ix
        .candidates(&hx(hash))
        .iter()
        .copied()
        .find(|h| ix.package_name(h.owner) == owner)
        .expect("a hit");
    match ix.edges(hit, EdgeRead::Scoped(scope)).expect("answered") {
        StoreEdges::Recorded(c) => Some(
            c.iter()
                .map(|c| (ix.package_name(c.owner).to_string(), c.key))
                .collect(),
        ),
        StoreEdges::NotRecorded => None,
    }
}

fn key(name: &str) -> u64 {
    crate::libindex::table::unit_key(&hx(name)).expect("a key")
}

#[test]
fn a_store_with_an_edge_table_answers_inside_and_out_of_apart() {
    let ix = LibraryIndex::from_rows_with_edges(
        vec![
            row("aa", "lodash"),
            row("aa", "lodash-es"),
            row("bb", "lodash"),
        ],
        vec![edge("aa", "lodash", &["cc", "dd"], &[("tslib-ish", "ee")])],
        vec![],
    );
    assert!(ix.has_edges());
    assert_eq!(
        callees(&ix, "aa", "lodash", Scope::Inside),
        Some(vec![
            ("lodash".to_string(), key("cc")),
            ("lodash".to_string(), key("dd"))
        ])
        .map(|mut v| {
            v.sort();
            v
        })
    );
    assert_eq!(
        callees(&ix, "aa", "lodash", Scope::OutOf),
        Some(vec![("tslib-ish".to_string(), key("ee"))])
    );
    // the same print in another package records no callee of its own:
    // recorded, and empty (the index records every row's edges)
    assert_eq!(callees(&ix, "aa", "lodash-es", Scope::Inside), Some(vec![]));
    assert_eq!(callees(&ix, "bb", "lodash", Scope::OutOf), Some(vec![]));
    let hit = ix.candidates(&hx("aa"))[0];
    assert!(ix.edges(hit, EdgeRead::Scoped(Scope::Into)).is_err());
    assert!(ix.edges(hit, EdgeRead::All).is_err());
}

/// An on-disk index of one Bun slice in `format` (with or without its
/// edge table), loaded for a Bun target-bun bundle.
fn on_disk(tag: &str, format: &str, with_edges: bool) -> Result<LibraryIndex, String> {
    use crate::libindex::format::{
        INDEX_FORMAT_NO_EDGES, Manifest, RECIPE_VERSION, SliceRecord, fingerprint_versions,
    };
    use crate::libindex::recipe::{TerserSlices, detect_recipe};
    use humanify_model::detection::{BundlerType, MinifierType};
    let dir = std::env::temp_dir().join(format!("humanify-store-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut fingerprints = fingerprint_versions();
    if format == INDEX_FORMAT_NO_EDGES {
        fingerprints.edges = None;
    }
    let m = Manifest {
        format: format.into(),
        as_of: "2026-10-12".into(),
        fingerprints,
        registry: Default::default(),
        slices: vec![SliceRecord {
            slice: "bun-bun-cjs-min".into(),
            recipe: "bun:target=bun,format=cjs,minify".into(),
            recipe_version: RECIPE_VERSION.into(),
            default_for_bundler: true,
            bundler: "bun".into(),
            version: "1.4.0".into(),
            published: None,
            newest_at_as_of: None,
            bundler_after_as_of: false,
        }],
        packages: vec![],
        refused: vec![],
        shards: Default::default(),
        tables: Default::default(),
        data: Default::default(),
    };
    std::fs::write(
        dir.join("manifest.json"),
        serde_json::to_string(&m).unwrap(),
    )
    .unwrap();
    let units = [(
        "bun-bun-cjs-min".to_string(),
        vec![row("aa", "zod"), row("cc", "zod")],
    )]
    .into_iter()
    .collect();
    let edges = [(
        "bun-bun-cjs-min".to_string(),
        vec![edge("aa", "zod", &["cc"], &[])],
    )]
    .into_iter()
    .collect();
    let files =
        crate::libindex::table::lookup_files(&units, with_edges.then_some(&edges), &[]).unwrap();
    for (name, bytes) in files {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    let recipe = detect_recipe(
        "// @bun @bun-cjs\nvar a;",
        BundlerType::Bun,
        MinifierType::Bun,
    );
    let ix = LibraryIndex::load(&dir, &recipe, TerserSlices::Off).map_err(|e| e.to_string());
    std::fs::remove_dir_all(&dir).ok();
    ix
}

#[test]
fn a_format_4_index_answers_its_edges_and_a_format_3_index_still_loads_without() {
    use crate::libindex::format::{INDEX_FORMAT, INDEX_FORMAT_NO_EDGES};
    let ix = on_disk("v4", INDEX_FORMAT, true).expect("a /4 index loads");
    assert!(ix.has_edges());
    assert_eq!(
        callees(&ix, "aa", "zod", Scope::Inside),
        Some(vec![("zod".to_string(), key("cc"))])
    );
    assert_eq!(callees(&ix, "cc", "zod", Scope::Inside), Some(vec![]));
    let old = on_disk("v3", INDEX_FORMAT_NO_EDGES, false).expect("a /3 index still loads");
    assert!(!old.has_edges());
    for scope in [Scope::Inside, Scope::OutOf] {
        assert_eq!(
            callees(&old, "aa", "zod", scope),
            None,
            "a /3 index records no edges: absence, never evidence"
        );
    }
    let err = on_disk("v4-no-table", INDEX_FORMAT, false)
        .err()
        .expect("a /4 index without its edge table is refused");
    assert!(err.contains("edges.bun-bun-cjs-min.bin"), "{err}");
}
