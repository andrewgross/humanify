//! Index-time origin de-duplication (exp110): a unit print held both by a
//! package and by a package that INLINES it into its own dist keeps only
//! the origin's row.

use std::collections::BTreeMap;

use crate::bun_node;
use humanify_core::libindex::format::UnitRow;

use super::merge::{MergeMeta, Merger};
use super::origin::{DEDUP_FILE, declared_names, read_or_record_declared};
use super::shard::{Shard, ShardInputs, ShardKind, ShardLiteral, ShardUnit};

const H1: &str = "00000000000000a1";
const H2: &str = "00000000000000b2";

fn unit(owner: &str, file: &str, hash: &str) -> ShardUnit {
    sized(owner, file, hash, 500)
}

fn sized(owner: &str, file: &str, hash: &str, module_bytes: u64) -> ShardUnit {
    ShardUnit {
        owner: owner.into(),
        version: "1.0.0".into(),
        file: file.into(),
        hash: hash.into(),
        nodes: 40,
        content: 6,
        module_bytes,
    }
}

/// A Bun-slice shard of `package@1.0.0` holding `units`.
fn unit_shard(package: &str, units: Vec<ShardUnit>) -> Shard {
    let mut s = Shard::new(&ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package: package.into(),
        version: "1.0.0".into(),
        bundler: Some("bun@1.4.0".into()),
        recipe: "recipe/1".into(),
        fingerprint: "units/3".into(),
    });
    s.units = units;
    s
}

/// A literal shard of `package@1.0.0` whose rows are owned by `owners`
/// (its installed, bundled closure).
fn literal_shard(package: &str, owners: &[&str]) -> Shard {
    let mut s = Shard::new(&ShardInputs {
        kind: ShardKind::Literals,
        package: package.into(),
        version: "1.0.0".into(),
        bundler: None,
        recipe: "recipe/1".into(),
        fingerprint: "literals/1".into(),
    });
    for o in owners {
        s.literals.push(ShardLiteral {
            owner: (*o).into(),
            version: "1.0.0".into(),
            file: "index.js".into(),
            lits: vec![format!("S:{o}")],
        });
    }
    s
}

/// Merge `shards` (sorted into label order) with `declared` (package →
/// names its package.json declares).
fn merged(mut shards: Vec<Shard>, declared: &[(&str, &[&str])]) -> BTreeMap<String, Vec<u8>> {
    shards.sort_by(|a, b| a.label().cmp(&b.label()).then(a.key.cmp(&b.key)));
    let mut m = Merger::default();
    for (p, names) in declared {
        m.declare(p, names.iter().map(|n| (*n).to_string()));
    }
    for s in shards {
        m.add(s).expect("adds");
    }
    m.finish(MergeMeta::default()).expect("merges").files
}

/// (hash, owner) of every unit row.
fn unit_owners(files: &BTreeMap<String, Vec<u8>>) -> Vec<(String, String)> {
    std::str::from_utf8(&files["units.bun-node-cjs-min.jsonl"])
        .expect("utf8")
        .lines()
        .map(|l| serde_json::from_str::<UnitRow>(l).expect("row"))
        .map(|r| (r.hash, r.owner))
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| ((*a).to_string(), (*b).to_string()))
        .collect()
}

#[test]
fn an_inlined_copy_of_a_dependency_leaves_the_inliners_rows() {
    // `ora`'s build bundles `yoctocolors` from node_modules (rows owned by
    // yoctocolors) AND an inlined copy in its own dist (rows owned by ora):
    // ora depends on yoctocolors, so yoctocolors is the origin.
    let files = merged(
        vec![
            unit_shard(
                "ora",
                vec![
                    unit("ora", "dist/index.js", H1),
                    unit("ora", "dist/index.js", H2),
                    unit("yoctocolors", "index.js", H1),
                ],
            ),
            unit_shard("yoctocolors", vec![unit("yoctocolors", "index.js", H1)]),
        ],
        &[],
    );
    assert_eq!(
        unit_owners(&files),
        pairs(&[(H1, "yoctocolors"), (H2, "ora")])
    );
    let dedup: serde_json::Value = serde_json::from_slice(&files[DEDUP_FILE]).expect("json");
    assert_eq!(dedup["slices"]["bun-node-cjs-min"]["rows"], 1);
    assert_eq!(dedup["pairs"][0]["inliner"], "ora");
    assert_eq!(dedup["pairs"][0]["origin"], "yoctocolors");
}

#[test]
fn the_installed_closure_of_the_literal_shards_is_dependency_evidence() {
    let files = merged(
        vec![
            unit_shard("mermaid", vec![unit("mermaid", "dist/chunk.mjs", H1)]),
            literal_shard("mermaid", &["mermaid", "cytoscape"]),
            unit_shard("cytoscape", vec![unit("cytoscape", "dist/cy.js", H1)]),
        ],
        &[],
    );
    assert_eq!(unit_owners(&files), pairs(&[(H1, "cytoscape")]));
}

#[test]
fn a_declared_dev_dependency_is_dependency_evidence() {
    // @rspack/core pre-bundles tinypool, a devDependency it never installs
    let shards = || {
        vec![
            unit_shard(
                "@rspack/core",
                vec![unit("@rspack/core", "compiled/x.js", H1)],
            ),
            unit_shard("tinypool", vec![unit("tinypool", "dist/index.js", H1)]),
        ]
    };
    assert_eq!(
        unit_owners(&merged(shards(), &[])),
        pairs(&[(H1, "@rspack/core"), (H1, "tinypool")])
    );
    assert_eq!(
        unit_owners(&merged(
            shards(),
            &[("@rspack/core", &["tinypool", "typescript"])]
        )),
        pairs(&[(H1, "tinypool")])
    );
}

#[test]
fn mutual_dependents_unrelated_owners_and_one_family_keep_every_row() {
    let both = || {
        vec![
            unit_shard("a", vec![unit("a", "a.js", H1)]),
            unit_shard("b", vec![unit("b", "b.js", H1)]),
        ]
    };
    let kept = pairs(&[(H1, "a"), (H1, "b")]);
    assert_eq!(unit_owners(&merged(both(), &[])), kept);
    assert_eq!(
        unit_owners(&merged(both(), &[("a", &["b"]), ("b", &["a"])])),
        kept
    );
    // one npm scope is one family: siblings both own their shared source
    let files = merged(
        vec![
            unit_shard(
                "@vue/runtime-dom",
                vec![unit("@vue/runtime-dom", "x.js", H1)],
            ),
            unit_shard("@vue/shared", vec![unit("@vue/shared", "y.js", H1)]),
        ],
        &[("@vue/runtime-dom", &["@vue/shared"])],
    );
    assert_eq!(
        unit_owners(&files),
        pairs(&[(H1, "@vue/runtime-dom"), (H1, "@vue/shared")])
    );
}

#[test]
fn a_print_held_by_more_owners_than_the_lookup_reads_is_left_alone() {
    // generic code (four owners): the lookup refuses it as ambiguous, and
    // dropping one owner must not make it claimable
    let owners = ["w", "x", "y", "z"];
    let shards = owners
        .iter()
        .map(|o| unit_shard(o, vec![unit(o, "i.js", H1)]))
        .collect();
    let files = merged(shards, &[("w", &["x"])]);
    assert_eq!(unit_owners(&files).len(), 4);
}

#[test]
fn every_owner_an_inliner_keeps_every_row() {
    // a -> b -> c -> a (no owner is free of the relation): no origin
    let files = merged(
        vec![
            unit_shard("a", vec![unit("a", "a.js", H1)]),
            unit_shard("b", vec![unit("b", "b.js", H1)]),
            unit_shard("c", vec![unit("c", "c.js", H1)]),
        ],
        &[("a", &["b"]), ("b", &["c"]), ("c", &["a"])],
    );
    assert_eq!(unit_owners(&files).len(), 3);
}

#[test]
fn declared_names_read_every_dependency_field() {
    let pkg = serde_json::json!({
        "name": "x",
        "dependencies": {"a": "1"},
        "devDependencies": {"b": "1"},
        "peerDependencies": {"c": "1"},
        "optionalDependencies": {"d": "1"},
        "bundleDependencies": ["e"],
        "bundledDependencies": ["f"]
    });
    assert_eq!(
        declared_names(&pkg),
        vec!["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_declared_record_is_read_from_the_install_once_then_from_the_store() {
    let tmp = std::env::temp_dir().join(format!("exp110-declared-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (store, work) = (tmp.join("store"), tmp.join("work"));
    let pkg_dir = work.join("@s__p@1.0.0/node_modules/@s/p");
    std::fs::create_dir_all(&pkg_dir).expect("mkdir");
    std::fs::write(
        pkg_dir.join("package.json"),
        r#"{"name":"@s/p","devDependencies":{"tinypool":"1"}}"#,
    )
    .expect("write");
    assert_eq!(
        read_or_record_declared(&store, &work, "@s/p", "1.0.0"),
        Some(vec!["tinypool".to_string()])
    );
    std::fs::remove_dir_all(&work).expect("rm work");
    assert_eq!(
        read_or_record_declared(&store, &work, "@s/p", "1.0.0"),
        Some(vec!["tinypool".to_string()])
    );
    assert_eq!(read_or_record_declared(&store, &work, "q", "1.0.0"), None);
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn a_dependency_whose_copy_sits_in_a_far_bigger_module_is_not_the_origin() {
    // micromark devDepends on prettier, whose markdown plugin INLINES an
    // old micromark: the print sits in micromark's 3 KB source file and in
    // prettier's 300 KB bundle. The origin is the smaller context.
    let shards = || {
        vec![
            unit_shard(
                "micromark",
                vec![sized("micromark", "lib/create-tokenizer.js", H1, 3_000)],
            ),
            unit_shard(
                "prettier",
                vec![sized("prettier", "plugins/markdown.js", H1, 300_000)],
            ),
        ]
    };
    let declared: &[(&str, &[&str])] = &[("micromark", &["prettier"])];
    assert_eq!(
        unit_owners(&merged(shards(), declared)),
        pairs(&[(H1, "micromark"), (H1, "prettier")])
    );
    // within the lookup's specificity bound (4x + 4 KB) it still is
    let near = vec![
        unit_shard("ora", vec![sized("ora", "index.js", H1, 9_000)]),
        unit_shard(
            "yoctocolors",
            vec![sized("yoctocolors", "index.js", H1, 30_000)],
        ),
    ];
    assert_eq!(
        unit_owners(&merged(near, &[("ora", &["yoctocolors"])])),
        pairs(&[(H1, "yoctocolors")])
    );
}
