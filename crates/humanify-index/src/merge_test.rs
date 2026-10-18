//! The merge: shards of the selected versions → one index, each
//! fingerprint stored once with its version ranges; byte-deterministic.

use crate::bun_node;
use humanify_core::libindex::format::{LiteralFileRow, ModuleRow};

use super::merge::{MergeMeta, Merger, merge};
use super::shard::{Shard, ShardInputs, ShardKind, ShardLiteral, ShardModule};

fn module_shard(version: &str, rows: &[(&str, &str, &str, &str)]) -> Shard {
    let mut s = Shard::new(&ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package: "ms".into(),
        version: version.into(),
        bundler: Some("bun@1.4.0".into()),
        recipe: "recipe/1".into(),
        fingerprint: "vendor-content-key/4".into(),
    });
    for (owner, v, file, key) in rows {
        s.modules.push(ShardModule {
            owner: (*owner).into(),
            version: (*v).into(),
            file: (*file).into(),
            key: (*key).into(),
        });
    }
    s
}

fn literal_shard(version: &str, lits: &[&str]) -> Shard {
    let mut s = Shard::new(&ShardInputs {
        kind: ShardKind::Literals,
        package: "ms".into(),
        version: version.into(),
        bundler: None,
        recipe: "recipe/1".into(),
        fingerprint: "literals/1".into(),
    });
    s.literals.push(ShardLiteral {
        owner: "ms".into(),
        version: version.into(),
        file: "index.js".into(),
        lits: lits.iter().map(|l| (*l).to_string()).collect(),
    });
    s
}

fn shards() -> Vec<Shard> {
    vec![
        module_shard("1.0.0", &[("ms", "1.0.0", "index.js", "kA")]),
        module_shard("2.0.0", &[("ms", "2.0.0", "index.js", "kB")]),
        module_shard(
            "2.1.0",
            &[
                ("ms", "2.1.0", "index.js", "kA"),
                ("tslib", "2.6.0", "tslib.js", "kT"),
            ],
        ),
        module_shard(
            "2.1.3",
            &[
                ("ms", "2.1.3", "index.js", "kA"),
                ("other", "1.0.0", "x.js", "kA"),
            ],
        ),
        literal_shard("1.0.0", &["S:a"]),
        literal_shard("2.0.0", &["S:b"]),
        literal_shard("2.1.0", &["S:b"]),
        literal_shard("2.1.3", &["S:b"]),
    ]
}

fn meta() -> MergeMeta {
    MergeMeta {
        as_of: "2026-10-11T23:59:59.999Z".into(),
        ..MergeMeta::default()
    }
}

fn rows<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Vec<T> {
    std::str::from_utf8(bytes)
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect()
}

#[test]
fn a_fingerprint_is_stored_once_with_its_version_ranges() {
    let index = merge(meta(), shards()).expect("merges");
    let modules: Vec<ModuleRow> = rows(&index.files["modules.bun-node-cjs-min.jsonl"]);
    let ka_ms = modules
        .iter()
        .find(|r| r.key == "kA" && r.owner == "ms")
        .expect("kA");
    // in 1.0.0, not 2.0.0, then 2.1.0..2.1.3
    assert_eq!(
        ka_ms.versions,
        vec![
            ("1.0.0".to_string(), "1.0.0".to_string()),
            ("2.1.0".to_string(), "2.1.3".to_string()),
        ]
    );
    let literals: Vec<LiteralFileRow> = rows(&index.files["literals.files.jsonl"]);
    assert_eq!(literals.len(), 2);
    assert_eq!(
        literals[1].versions,
        vec![("2.0.0".to_string(), "2.1.3".to_string())]
    );
}

#[test]
fn a_key_held_by_two_owners_is_kept_as_two_rows() {
    let index = merge(meta(), shards()).expect("merges");
    let modules: Vec<ModuleRow> = rows(&index.files["modules.bun-node-cjs-min.jsonl"]);
    let owners: Vec<&str> = modules
        .iter()
        .filter(|r| r.key == "kA")
        .map(|r| r.owner.as_str())
        .collect();
    assert_eq!(owners, ["ms", "other"]);
}

#[test]
fn stop_listed_helper_packages_leave_the_index() {
    let index = merge(meta(), shards()).expect("merges");
    let modules: Vec<ModuleRow> = rows(&index.files["modules.bun-node-cjs-min.jsonl"]);
    assert!(modules.iter().all(|r| r.owner != "tslib"));
    let versions: serde_json::Value =
        serde_json::from_slice(&index.files["versions.json"]).expect("json");
    assert!(versions.get("tslib").is_none());
    assert_eq!(
        versions["ms"],
        serde_json::json!(["1.0.0", "2.0.0", "2.1.0", "2.1.3"])
    );
}

#[test]
fn the_same_shards_in_any_order_give_the_same_bytes_and_id() {
    let a = merge(meta(), shards()).expect("merges");
    let mut reversed = shards();
    reversed.reverse();
    let b = merge(meta(), reversed).expect("merges");
    assert_eq!(a.files, b.files);
    assert_eq!(a.id, b.id);
    assert_eq!(a.id.len(), 64);
}

#[test]
fn a_new_version_is_a_new_index_id() {
    let a = merge(meta(), shards()).expect("merges");
    let mut more = shards();
    more.push(module_shard("2.1.4", &[("ms", "2.1.4", "index.js", "kA")]));
    let b = merge(meta(), more).expect("merges");
    assert_ne!(a.id, b.id);
    let manifest: serde_json::Value =
        serde_json::from_slice(&b.files["manifest.json"]).expect("json");
    assert_eq!(
        manifest["format"],
        humanify_core::libindex::format::INDEX_FORMAT
    );
    assert!(
        manifest["shards"]
            .get("bun-node-cjs-min/ms@2.1.4")
            .is_some()
    );
    assert!(manifest["data"].get("stoplists.json").is_some());
}

const H1: &str = "00000000000000a1";
const H2: &str = "00000000000000a2";
const HT: &str = "00000000000000af";

#[test]
fn unit_rows_merge_per_slice_with_the_smallest_module() {
    use super::shard::ShardUnit;
    let unit = |v: &str, hash: &str, module_bytes: u64| ShardUnit {
        owner: "ms".into(),
        version: v.into(),
        file: "index.js".into(),
        hash: hash.into(),
        nodes: 40,
        content: 5,
        module_bytes,
    };
    let mut a = module_shard("1.0.0", &[]);
    a.units = vec![unit("1.0.0", H1, 900), unit("1.0.0", H2, 900)];
    let mut b = module_shard("2.0.0", &[]);
    b.units = vec![unit("2.0.0", H1, 700)];
    let mut t = module_shard("2.1.0", &[]);
    t.units = vec![ShardUnit {
        owner: "tslib".into(),
        ..unit("2.1.0", HT, 10)
    }];
    let files = merge(meta(), vec![a, b, t]).expect("merges").files;
    let rows: Vec<humanify_core::libindex::format::UnitRow> =
        std::str::from_utf8(&files["units.bun-node-cjs-min.jsonl"])
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
    type View<'r> = (&'r str, &'r str, u64, Vec<(String, String)>);
    let view: Vec<View<'_>> = rows
        .iter()
        .map(|r| {
            (
                r.hash.as_str(),
                r.owner.as_str(),
                r.module_bytes,
                r.versions.clone(),
            )
        })
        .collect();
    assert_eq!(
        view,
        vec![
            (H1, "ms", 700, vec![("1.0.0".into(), "2.0.0".into())]),
            (H2, "ms", 900, vec![("1.0.0".into(), "1.0.0".into())]),
        ],
        "tslib is a helper: dropped"
    );
    assert_eq!(
        (rows[0].nodes, rows[0].content, rows[0].file.as_str()),
        (40, 5, "index.js")
    );
}

#[test]
fn the_lookup_tables_answer_what_the_row_tables_say() {
    use humanify_core::libindex::table::{LiteralTable, Source, UnitTable};
    let mut shards = shards();
    let mut u = module_shard("1.0.0", &[]);
    u.units = vec![super::shard::ShardUnit {
        owner: "ms".into(),
        version: "1.0.0".into(),
        file: "index.js".into(),
        hash: H1.into(),
        nodes: 40,
        content: 5,
        module_bytes: 900,
    }];
    shards.push(u);
    let files = merge(meta(), shards).expect("merges").files;
    assert!(
        !files.contains_key("literals.jsonl"),
        "the literal sets live in the lookup table only"
    );
    let packages: Vec<String> = serde_json::from_slice(&files["packages.json"]).unwrap();
    // the owners the lookup tables hold (`other` is in a module row only)
    assert_eq!(packages, ["ms"]);
    let units = UnitTable::open(Source::Memory(files["units.bun-node-cjs-min.bin"].clone()))
        .expect("opens");
    let hits = units.hits(H1);
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].owner, hits[0].module_bytes), (0, 900));
    let literals =
        LiteralTable::open(Source::Memory(files["literals.bin"].clone())).expect("opens");
    let provenance: Vec<LiteralFileRow> = rows(&files["literals.files.jsonl"]);
    for (f, row) in provenance.iter().enumerate() {
        let owner = literals.file_owner(f as u32) as usize;
        assert_eq!(packages[owner], row.owner, "file {f}");
    }
    let id = literals.resolve("S:b").expect("an indexed literal");
    assert_eq!(
        literals.postings(id),
        [1],
        "file 1: the S:b set, 2.0.0-2.1.3"
    );
    assert_eq!(literals.resolve("S:z"), None);
}

#[test]
fn the_streaming_merge_refuses_a_shard_out_of_label_order() {
    // a unit group keeps the FIRST shard's nodes and content: the order is
    // part of the bytes, so it is checked, never assumed
    let mut m = Merger::default();
    m.add(module_shard("2.0.0", &[])).expect("first");
    let err = m.add(module_shard("1.0.0", &[])).unwrap_err();
    assert!(err.contains("out of label order"), "{err}");
}

#[test]
fn a_unit_group_keeps_the_first_shards_nodes_whatever_order_the_list_had() {
    use super::shard::ShardUnit;
    let shard = |v: &str, nodes: u32| {
        let mut s = module_shard(v, &[]);
        s.units = vec![ShardUnit {
            owner: "ms".into(),
            version: v.into(),
            file: "index.js".into(),
            hash: H1.into(),
            nodes,
            content: 5,
            module_bytes: 100,
        }];
        s
    };
    let a = merge(meta(), vec![shard("2.0.0", 41), shard("1.0.0", 40)]).expect("merges");
    let rows: Vec<humanify_core::libindex::format::UnitRow> =
        rows(&a.files["units.bun-node-cjs-min.jsonl"]);
    assert_eq!(rows[0].nodes, 40, "label order: 1.0.0 first");
}

/// exp129: the stop-listed helper packages leave the unit tables, but the
/// HELPER SOURCES among them (tslib, @babel/runtime, @swc/helpers ... —
/// packages whose own code is the transpiler helpers every compiled
/// package inlines) leave their prints behind in `helpers.jsonl`: hash ->
/// the helper packages holding it, over every slice. @babel/helpers is
/// dropped but is no source (its dist carries @babel/types' code), nor
/// are the core-js runtimes (they re-export core-js-pure). An index with
/// no helper-source rows has no helper table.
#[test]
fn helper_sources_leave_their_prints_in_the_helper_table() {
    use super::shard::ShardUnit;
    use humanify_core::libindex::format::HelperRow;
    let unit = |owner: &str, hash: &str| ShardUnit {
        owner: owner.into(),
        version: "1.0.0".into(),
        file: "index.js".into(),
        hash: hash.into(),
        nodes: 40,
        content: 5,
        module_bytes: 100,
    };
    const HB: &str = "00000000000000b0";
    const HC: &str = "00000000000000c0";
    let mut a = module_shard("1.0.0", &[]);
    a.units = vec![
        unit("ms", H1),
        unit("tslib", HT),
        unit("@babel/runtime", HT),
        unit("@babel/runtime", H2),
        unit("@babel/helpers", HB),
        unit("@babel/runtime-corejs3", HC),
    ];
    let files = merge(meta(), vec![a]).expect("merges").files;
    let units = std::str::from_utf8(&files["units.bun-node-cjs-min.jsonl"]).unwrap();
    assert_eq!(units.lines().count(), 1, "only ms's row is a unit row");
    let helpers: Vec<HelperRow> = std::str::from_utf8(&files["helpers.jsonl"])
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(
        helpers,
        vec![
            HelperRow {
                hash: H2.into(),
                owners: vec!["@babel/runtime".into()]
            },
            HelperRow {
                hash: HT.into(),
                owners: vec!["@babel/runtime".into(), "tslib".into()]
            },
        ]
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&files["manifest.json"]).expect("json");
    assert!(manifest["tables"].get("helpers.jsonl").is_some());
    let plain = merge(meta(), shards()).expect("merges").files;
    assert!(!plain.contains_key("helpers.jsonl"));
}
