//! Shards: keyed by exactly what went into them, so an update rebuilds
//! only what changed.

use crate::bun_node;
use humanify_core::libindex::recipe::Recipe;

use super::shard::{Shard, ShardInputs, ShardKind, ShardModule, read_current, write_shard};

fn inputs() -> ShardInputs {
    ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package: "ms".into(),
        version: "2.1.3".into(),
        bundler: Some("bun@1.4.0".into()),
        recipe: "recipe/1".into(),
        fingerprint: "vendor-content-key/4".into(),
    }
}

fn temp_store(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("humanify-index-store-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn the_key_changes_with_every_input_and_nothing_else() {
    let base = inputs().key();
    assert_eq!(base, inputs().key());
    assert_eq!(base.len(), 64);
    let changed = [
        ShardInputs {
            version: "2.1.2".into(),
            ..inputs()
        },
        ShardInputs {
            bundler: Some("bun@1.4.3".into()),
            ..inputs()
        },
        ShardInputs {
            recipe: "recipe/2".into(),
            ..inputs()
        },
        ShardInputs {
            fingerprint: "vendor-content-key/5".into(),
            ..inputs()
        },
        ShardInputs {
            kind: ShardKind::Modules(
                Recipe::parse("esbuild:target=node,format=cjs,minify").unwrap(),
            ),
            ..inputs()
        },
        ShardInputs {
            // the same bundler for another target is another slice
            kind: ShardKind::Modules(Recipe::parse("bun:target=bun,format=cjs,minify").unwrap()),
            ..inputs()
        },
        ShardInputs {
            kind: ShardKind::Literals,
            bundler: None,
            ..inputs()
        },
    ];
    for c in changed {
        assert_ne!(c.key(), base, "{c:?}");
    }
}

#[test]
fn a_stored_shard_is_reused_only_while_its_key_matches() {
    let store = temp_store("reuse");
    let mut shard = Shard::new(&inputs());
    shard.modules.push(ShardModule {
        owner: "ms".into(),
        version: "2.1.3".into(),
        file: "index.js".into(),
        key: "k1".into(),
    });
    write_shard(&store, &shard).expect("write");
    let back = read_current(&store, &inputs()).expect("reused");
    assert_eq!(back, shard);
    let newer = ShardInputs {
        bundler: Some("bun@1.4.3".into()),
        ..inputs()
    };
    assert!(read_current(&store, &newer).is_none());
    std::fs::remove_dir_all(&store).expect("cleanup");
}

#[test]
fn a_shard_writes_its_rows_sorted_and_deduplicated() {
    let store = temp_store("sorted");
    let mut shard = Shard::new(&inputs());
    for (file, key) in [("b.js", "k2"), ("a.js", "k1"), ("b.js", "k2")] {
        shard.modules.push(ShardModule {
            owner: "ms".into(),
            version: "2.1.3".into(),
            file: file.into(),
            key: key.into(),
        });
    }
    write_shard(&store, &shard).expect("write");
    let back = read_current(&store, &inputs()).expect("read");
    let files: Vec<&str> = back.modules.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(files, ["a.js", "b.js"]);
    std::fs::remove_dir_all(&store).expect("cleanup");
}
