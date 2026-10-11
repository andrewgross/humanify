//! The plan: what an as-of build would index, and which shards it would
//! reuse — no network (a recorded registry), no subprocess (no slices).

use super::build::{BuildConfig, parse_package_spec, plan};
use super::shard::{Shard, write_shard};

fn temp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("humanify-index-plan-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("mkdir");
    d
}

fn config(tag: &str, as_of: &str) -> BuildConfig {
    let root = temp(tag);
    let registry = root.join("registry");
    std::fs::create_dir_all(&registry).expect("mkdir");
    std::fs::write(
        registry.join("ms.json"),
        include_str!("../testdata/ms.packument.json"),
    )
    .expect("fixture");
    BuildConfig {
        packages: vec!["ms".into()],
        as_of: as_of.into(),
        registry,
        store: root.join("store"),
        work: root.join("work"),
        out: None,
        slices: Vec::new(),
        jobs: 1,
        offline: true,
    }
}

#[test]
fn a_package_spec_is_a_name_or_a_pinned_version() {
    assert_eq!(parse_package_spec("ms"), ("ms".to_string(), None));
    assert_eq!(
        parse_package_spec("ms@2.1.3"),
        ("ms".to_string(), Some("2.1.3".to_string()))
    );
    assert_eq!(
        parse_package_spec("@scope/pkg"),
        ("@scope/pkg".to_string(), None)
    );
    assert_eq!(
        parse_package_spec("@scope/pkg@1.0.0"),
        ("@scope/pkg".to_string(), Some("1.0.0".to_string()))
    );
}

#[test]
fn the_plan_lists_the_as_of_versions_and_the_shards_to_build() {
    let cfg = config("list", "2020-12-01T23:59:59.999Z");
    let p = plan(&cfg).expect("plan");
    let tasks: Vec<String> = p
        .tasks
        .iter()
        .map(|t| format!("{}@{}", t.name, t.version))
        .collect();
    assert_eq!(tasks, ["ms@2.1.2", "ms@2.1.1", "ms@1.0.0", "ms@0.7.3"]);
    assert!(
        p.tasks
            .iter()
            .all(|t| t.missing.len() == 1 && t.reused.is_empty())
    );
    assert!(p.refused.iter().any(|r| r.version == "2.1.3"));
    // npm resolves each version's dependencies as of ITS publish day
    assert_eq!(p.tasks[0].published, "2019-06-06T17:31:55.859Z");
}

#[test]
fn an_offline_plan_refuses_a_package_it_has_no_snapshot_of() {
    let mut cfg = config("offline", "2020-12-01T23:59:59.999Z");
    cfg.packages.push("left-pad".into());
    let err = plan(&cfg).expect_err("no snapshot");
    assert!(err.contains("left-pad"), "{err}");
}

#[test]
fn a_shard_already_in_the_store_is_reused() {
    let cfg = config("reuse", "2020-12-01T23:59:59.999Z");
    let first = plan(&cfg).expect("plan");
    let inputs = first.tasks[0].missing[0].clone();
    write_shard(&cfg.store, &Shard::new(&inputs)).expect("write");
    let second = plan(&cfg).expect("plan");
    assert_eq!(second.tasks[0].reused, vec![inputs]);
    assert!(second.tasks[0].missing.is_empty());
    assert_eq!(second.tasks[1].missing.len(), 1);
}
