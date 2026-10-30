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
        inside: vec![],
        out_of: vec![],
        names: vec![],
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
fn unit_names_merge_per_slice_and_their_versions() {
    use super::shard::ShardUnit;
    let unit = |v: &str, hash: &str, names: Vec<humanify_core::libindex::names::RestoredName>| {
        let mut u = ShardUnit {
            owner: "ms".into(),
            version: v.into(),
            file: "index.js".into(),
            hash: hash.into(),
            nodes: 40,
            content: 5,
            module_bytes: 900,
            inside: vec![],
            out_of: vec![],
            names: vec![],
        };
        u.names = names;
        u
    };
    let n = |slot: u32, name: &str| humanify_core::libindex::names::RestoredName {
        slot,
        name: name.into(),
        kind: humanify_core::libindex::names::NameKind::Local,
    };
    // H1's sources agree across versions: one names row, both versions
    let mut a = module_shard("1.0.0", &[]);
    a.units = vec![unit("1.0.0", H1, vec![n(0, "alpha")])];
    let mut b = module_shard("2.0.0", &[]);
    b.units = vec![unit("2.0.0", H1, vec![n(0, "alpha")])];
    // H2's disagree: two rows, each its versions
    let mut c = module_shard("2.1.0", &[]);
    c.units = vec![
        unit("2.1.0", H2, vec![n(0, "beta")]),
        unit("2.1.0", H1, vec![]),
    ];
    let files = merge(meta(), vec![a, b, c]).expect("merges").files;
    let name_rows: Vec<humanify_core::libindex::format::NameRow> =
        rows(&files["names.bun-node-cjs-min.jsonl"]);
    type View<'r> = (&'r str, Vec<&'r str>, Vec<(String, String)>);
    let view: Vec<View<'_>> = name_rows
        .iter()
        .map(|r| {
            (
                r.hash.as_str(),
                r.names.iter().map(|n| n.name.as_str()).collect(),
                r.versions.clone(),
            )
        })
        .collect();
    assert_eq!(
        view,
        vec![
            (H1, vec!["alpha"], vec![("1.0.0".into(), "2.0.0".into())]),
            (H2, vec!["beta"], vec![("2.1.0".into(), "2.1.0".into())]),
        ],
        "{view:?}"
    );
    // the lookup tables beside them, and the strings file
    assert!(files.contains_key("names.bun-node-cjs-min.bin"));
    assert!(files.contains_key("names.json"));
    // H1's 2.1.0 row carried NO names: it joins nothing (no empty row)
}

#[test]
fn a_merge_without_names_writes_no_names_files() {
    let files = merge(meta(), shards()).expect("merges").files;
    assert!(
        !files.keys().any(|k| k.starts_with("names.")),
        "{:?}",
        files.keys()
    );
    assert!(!files.contains_key("names.json"));
    // and the manifest still reads (its names stamp is optional)
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
        inside: vec![],
        out_of: vec![],
        names: vec![],
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
            inside: vec![],
            out_of: vec![],
            names: vec![],
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
        inside: vec![],
        out_of: vec![],
        names: vec![],
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

// ------------------------------------------------------------------ exp162

use super::merge::{key_ranges, merge_parallel};
use humanify_core::libindex::table::{EdgeTable, Source};

/// `--merge-jobs N`: the parallel merge over the SAME shards is
/// byte-identical to the streaming merge, at every worker count.
#[test]
fn the_parallel_merge_gives_the_streaming_merges_bytes_at_every_job_count() {
    let serial = merge(meta(), shards()).expect("merges");
    for jobs in [1usize, 4, 8, 30] {
        let parallel = merge_parallel(meta(), shards(), jobs).expect("merges");
        assert_eq!(parallel.files, serial.files, "jobs {jobs}");
        assert_eq!(parallel.id, serial.id, "jobs {jobs}");
    }
}

/// A package whose unit prints straddle a range split: both prints' rows
/// keep their version ranges (each worker recomputes the owner's indexed
/// versions from the WHOLE stream, so no row's ranges depend on which
/// worker held it).
#[test]
fn a_print_straddling_a_range_split_keeps_its_version_ranges() {
    use super::shard::ShardUnit;
    // four ranges split the key space at 2^62 multiples; one print in the
    // first range, one just past the second split
    const LOW: &str = "0000000000000001";
    const HIGH: &str = "5000000000000000";
    let unit = |v: &str, hash: &str| ShardUnit {
        owner: "ms".into(),
        version: v.into(),
        file: "index.js".into(),
        hash: hash.into(),
        nodes: 40,
        content: 5,
        module_bytes: 900,
        inside: vec![],
        out_of: vec![],
        names: vec![],
    };
    let shard = |v: &str| {
        let mut s = module_shard(v, &[]);
        s.units = vec![unit(v, LOW), unit(v, HIGH)];
        s
    };
    let shards = vec![shard("1.0.0"), shard("2.0.0"), shard("2.1.0")];
    let serial = merge(meta(), shards.clone()).expect("merges");
    let parallel = merge_parallel(meta(), shards, 4).expect("merges");
    assert_eq!(parallel.files, serial.files);
    for slice in ["units.bun-node-cjs-min.jsonl", "units.bun-node-cjs-min.bin"] {
        assert_eq!(
            parallel.files.get(slice).map(Vec::len),
            serial.files.get(slice).map(Vec::len),
            "{slice}"
        );
    }
}

/// The interning contract across ranges: two unit rows in DIFFERENT key
/// ranges whose callees agree store that callee list ONCE in the final
/// edge table (the table's lists are interned over the whole slice, not
/// per segment).
#[test]
fn a_callee_list_shared_by_rows_in_different_ranges_is_stored_once() {
    use super::shard::ShardUnit;
    const A: &str = "0000000000000001";
    const B: &str = "9000000000000000";
    const CALLEE: &str = "00000000000000cc";
    let unit = |hash: &str| ShardUnit {
        owner: "ms".into(),
        version: "1.0.0".into(),
        file: "index.js".into(),
        hash: hash.into(),
        nodes: 40,
        content: 5,
        module_bytes: 900,
        inside: vec![CALLEE.into()],
        out_of: vec![],
        names: vec![],
    };
    let mut shards = vec![module_shard("1.0.0", &[]); 1];
    shards[0].units = vec![unit(A), unit(B)];
    let files = merge_parallel(meta(), shards, 4).expect("merges").files;
    let src = Source::Memory(files["edges.bun-node-cjs-min.bin"].clone());
    let table = EdgeTable::open(src).expect("opens");
    // one keyed row per unit row, ONE list
    assert_eq!(table.callees(0).len(), 1);
    assert_eq!(table.callees(1).len(), 1);
    // both rows answer the same callees, and the table stored one list
    // (its callees column is one list of one: read the header)
    assert_eq!(table.callees(0), table.callees(1));
    assert_eq!(table.callees(0)[0].key, 0xcc);
}

/// The streamed finish (exp162): with `staging` set, EVERY table is
/// written into the staging dir as the finish produces it and recorded by
/// sha — the buffered finish's `files` map (the tests' path) is what a
/// caller would have held. The contract: the same bytes either way, the
/// same id, and nothing on disk the buffered path would not have held.
#[test]
fn the_streamed_finish_writes_the_same_bytes_the_buffered_finish_holds() {
    let buffered = merge(meta(), shards()).expect("merges");
    let dir = std::env::temp_dir().join(format!("humanify-index-stage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    let mut staged_meta = meta();
    staged_meta.staging = Some(dir.clone());
    let streamed = merge(staged_meta, shards()).expect("merges");
    assert_eq!(streamed.id, buffered.id, "same manifest, same id");
    // the manifest and the static data files stay buffered (the caller's
    // write); every PRODUCED table streamed
    assert_eq!(
        streamed.files.keys().collect::<Vec<_>>(),
        ["families.json", "manifest.json", "stoplists.json"],
        "the manifest + data stay buffered; every produced table streamed"
    );
    let names: Vec<String> = std::fs::read_dir(&dir)
        .expect("dir")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names.len(),
        buffered.files.len() - streamed.files.len(),
        "every produced table on disk: {names:?}"
    );
    for (name, bytes) in &buffered.files {
        if streamed.files.contains_key(name) {
            continue;
        }
        let on_disk = std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("read {name}: {e}"));
        assert_eq!(&on_disk, bytes, "{name} differs on disk");
    }
    // the manifest's tables name the streamed files with their shas — the
    // buffered manifest's bytes
    let manifest: serde_json::Value =
        serde_json::from_slice(&streamed.files["manifest.json"]).expect("json");
    for name in buffered.files.keys() {
        if streamed.files.contains_key(name) {
            continue;
        }
        assert!(
            manifest["tables"].get(name.as_str()).is_some(),
            "{name} sha'd in the streamed manifest"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The prefix a non-hash table partitions on is order-preserving: strings
/// in different ranges compare as their strings do, so segments
/// concatenate in sorted order (a fuzz: two strings out of order that
/// land in reversed ranges would fail).
#[test]
fn the_partition_prefix_is_monotone_in_the_string_order() {
    let mut all: Vec<String> = vec![
        "".into(),
        "a".into(),
        "ab".into(),
        "abc".into(),
        "abcdefgh".into(),
        "abcdefghi".into(),
        "abcdefgh0".into(),
        "z".into(),
        "zzzzzzzz".into(),
        "\u{0}a".into(),
        "abc\u{0}def".into(),
        "\u{fffd}".into(),
        "\u{10ffff}x".into(),
        "00000000000000a1".into(),
        "ffffffffffffffff".into(),
        "ABCDEFGH".into(),
    ];
    // a fuzz of short random byte strings (NUL and multi-byte starts included)
    let mut pseudo = 0x2463_4c9d_u64;
    for _ in 0..2000 {
        pseudo = pseudo
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let len = (pseudo >> 33) as usize % 12;
        all.push(
            (0..len)
                .map(|i| {
                    let byte = (pseudo.wrapping_mul(1 + i as u64) >> 24) as u8;
                    char::from_u32(u32::from(byte % 0xff)).unwrap_or('x')
                })
                .collect(),
        );
    }
    all.sort();
    all.dedup();
    for jobs in [2usize, 4, 30] {
        let ranges = key_ranges(jobs);
        let bucket = |s: &str| {
            ranges
                .iter()
                .position(|r| r.holds_str(s))
                .unwrap_or_else(|| panic!("{s:?} is in no range of {jobs}"))
        };
        for w in all.windows(2) {
            let (a, b) = (w[0].as_str(), w[1].as_str());
            assert!(
                bucket(a) <= bucket(b),
                "{a:?} < {b:?} but buckets reversed at jobs {jobs}"
            );
        }
        // and every string is in exactly one range
        for s in &all {
            let n = ranges.iter().filter(|r| r.holds_str(s)).count();
            assert_eq!(n, 1, "{s:?} in {n} ranges at jobs {jobs}");
        }
    }
}

/// A hash is in exactly one range, and the ranges cover the whole key
/// space (a split boundary itself is in the range above it).
#[test]
fn the_key_ranges_partition_the_whole_key_space() {
    for jobs in [1usize, 2, 4, 30] {
        let ranges = key_ranges(jobs);
        for h in [0u64, 1, u64::MAX, u64::MAX - 1, 0x4000_0000_0000_0000] {
            assert_eq!(
                ranges.iter().filter(|r| r.holds(h)).count(),
                1,
                "{h:016x} at jobs {jobs}"
            );
        }
    }
}

/// The giant-shard gate (exp162): every worker streams every shard, so on
/// a store with a giant shard all N workers would parse the same giant
/// together — N × ~25GB is an OOM at any real job count. The gate holds
/// ONE worker's read+parse+add of a giant at a time. The contract: the
/// gate is a scheduling detail only — forcing it on for EVERY shard
/// leaves the bytes alone.
#[test]
fn the_giant_shard_gate_serialises_a_store_merge_without_changing_a_byte() {
    fn store_shards(store: &std::path::Path) -> (Vec<Shard>, Vec<(String, usize, ShardInputs)>) {
        let module = |version: &str, rows: &[(&str, &str, &str, &str)]| {
            let inputs = ShardInputs {
                kind: ShardKind::Modules(bun_node()),
                package: "ms".into(),
                version: version.into(),
                bundler: Some("bun@1.4.0".into()),
                recipe: "recipe/1".into(),
                fingerprint: "vendor-content-key/4".into(),
            };
            let mut s = Shard::new(&inputs);
            for (owner, v, file, key) in rows {
                s.modules.push(ShardModule {
                    owner: (*owner).into(),
                    version: (*v).into(),
                    file: (*file).into(),
                    key: (*key).into(),
                });
            }
            (s, inputs)
        };
        let literal = |version: &str, lits: &[&str]| {
            let inputs = ShardInputs {
                kind: ShardKind::Literals,
                package: "ms".into(),
                version: version.into(),
                bundler: None,
                recipe: "recipe/1".into(),
                fingerprint: "literals/1".into(),
            };
            let mut s = Shard::new(&inputs);
            s.literals.push(ShardLiteral {
                owner: "ms".into(),
                version: version.into(),
                file: "index.js".into(),
                lits: lits.iter().map(|l| (*l).to_string()).collect(),
            });
            (s, inputs)
        };
        let pairs = vec![
            module("1.0.0", &[("ms", "1.0.0", "index.js", "0000000000000001")]),
            module(
                "2.0.0",
                &[
                    ("ms", "2.0.0", "index.js", "8000000000000001"),
                    ("other", "1.0.0", "x.js", "0000000000000002"),
                ],
            ),
            literal("1.0.0", &["S:a"]),
            literal("2.0.0", &["S:b"]),
        ];
        let mut refs = Vec::new();
        for (shard, inputs) in &pairs {
            super::shard::write_shard(store, shard).expect("writes the shard");
            refs.push((shard.label(), 0usize, inputs.clone()));
        }
        refs.sort_by(|a, b| a.0.cmp(&b.0));
        (pairs.into_iter().map(|(s, _)| s).collect(), refs)
    }

    let root = std::env::temp_dir().join(format!("humanify-index-giant-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let store = root.join("store");
    let (shards_written, refs) = store_shards(&store);

    let gated: super::merge::IndexFiles = {
        let flow = super::merge::ShardFlow::Store {
            store: &store,
            refs: &refs,
            n_tasks: 1,
            giant: 1, // the gate ON for EVERY shard — fully serialized
        };
        let workers = super::merge::stream_parallel(&flow, 4).expect("workers");
        super::merge::finish_workers(meta(), workers).expect("gated finish")
    };
    let ungated: super::merge::IndexFiles = {
        let flow = super::merge::ShardFlow::Store {
            store: &store,
            refs: &refs,
            n_tasks: 1,
            giant: u64::MAX, // the gate off
        };
        let workers = super::merge::stream_parallel(&flow, 4).expect("workers");
        super::merge::finish_workers(meta(), workers).expect("ungated finish")
    };
    let serial = merge(meta(), shards_written).expect("serial");
    assert_eq!(gated.files, serial.files, "gated == serial");
    assert_eq!(ungated.files, serial.files, "ungated == serial");
    let _ = std::fs::remove_dir_all(&root);
}
