//! The two reference stores (docs/matching-architecture.md §3.4): the
//! index store never serves INTO edges; the prior store reads one program
//! whole.

use crate::graph::edges::Scope;
use crate::libindex::format::UnitRow;
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
