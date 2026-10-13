//! The merge: shards of the selected versions → one index, each
//! fingerprint stored once with its version ranges; byte-deterministic.

use crate::bun_node;
use humanify_core::libindex::format::{LiteralRow, ModuleRow};

use super::merge::{MergeInput, merge};
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

fn input(shards: Vec<Shard>) -> MergeInput {
    MergeInput {
        as_of: "2026-10-11T23:59:59.999Z".into(),
        shards,
        ..MergeInput::default()
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
    let index = merge(&input(shards()));
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
    let literals: Vec<LiteralRow> = rows(&index.files["literals.jsonl"]);
    assert_eq!(literals.len(), 2);
    assert_eq!(
        literals[1].versions,
        vec![("2.0.0".to_string(), "2.1.3".to_string())]
    );
}

#[test]
fn a_key_held_by_two_owners_is_kept_as_two_rows() {
    let index = merge(&input(shards()));
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
    let index = merge(&input(shards()));
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
    let a = merge(&input(shards()));
    let mut reversed = shards();
    reversed.reverse();
    let b = merge(&input(reversed));
    assert_eq!(a.files, b.files);
    assert_eq!(a.id, b.id);
    assert_eq!(a.id.len(), 64);
}

#[test]
fn a_new_version_is_a_new_index_id() {
    let a = merge(&input(shards()));
    let mut more = shards();
    more.push(module_shard("2.1.4", &[("ms", "2.1.4", "index.js", "kA")]));
    let b = merge(&input(more));
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
    a.units = vec![unit("1.0.0", "h1", 900), unit("1.0.0", "h2", 900)];
    let mut b = module_shard("2.0.0", &[]);
    b.units = vec![unit("2.0.0", "h1", 700)];
    let mut t = module_shard("2.1.0", &[]);
    t.units = vec![ShardUnit {
        owner: "tslib".into(),
        ..unit("2.1.0", "hT", 10)
    }];
    let files = merge(&input(vec![a, b, t])).files;
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
            ("h1", "ms", 700, vec![("1.0.0".into(), "2.0.0".into())]),
            ("h2", "ms", 900, vec![("1.0.0".into(), "1.0.0".into())]),
        ],
        "tslib is a helper: dropped"
    );
    assert_eq!(
        (rows[0].nodes, rows[0].content, rows[0].file.as_str()),
        (40, 5, "index.js")
    );
}
