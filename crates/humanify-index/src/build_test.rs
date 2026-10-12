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

#[test]
fn unit_rows_are_attributed_through_the_sourcemap() {
    use crate::build::unit_rows;
    use crate::sourcemap::SourceMap;
    use humanify_core::libindex::IndexSlice;
    let root = std::env::temp_dir().join(format!("humanify-index-units-{}", std::process::id()));
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.2.3"}"#,
    );
    write("node_modules/dep/lib.js", "");
    // line 0: the library's statement; line 1: the app entry's.
    let lib = "var parse=function(t){return t.split(\",\").map(function(e){return e.trim().toLowerCase()})};";
    let app = "var main=function(){return parse(process.argv[2]).join(\"|\").length+1};";
    let code = format!("{lib}\n{app}\n");
    let col = |n: usize| (0..n as u32).step_by(4);
    let map = SourceMap {
        sources: vec![
            "../node_modules/dep/lib.js".into(),
            "../entries/req.cjs".into(),
        ],
        segments: col(lib.len())
            .map(|c| (0, c, 0))
            .chain(col(app.len()).map(|c| (1, c, 1)))
            .collect(),
    };
    let (rows, notes) = unit_rows(IndexSlice::Bun, &code, &map, &root.join("out"));
    assert_eq!(notes.unowned, 1, "the app entry's statement");
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .all(|r| r.owner == "dep" && r.version == "1.2.3" && r.file == "lib.js")
    );
    assert!(rows.iter().all(|r| r.module_bytes == lib.len() as u64));
    assert_eq!(
        rows.len(),
        2,
        "the statement and its outer function; the inner one is under the floor: {rows:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_unit_spanning_several_files_of_one_package_is_that_packages() {
    use crate::build::unit_rows;
    use crate::sourcemap::SourceMap;
    use humanify_core::libindex::IndexSlice;
    let root = std::env::temp_dir().join(format!("humanify-index-units2-{}", std::process::id()));
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.2.3"}"#,
    );
    // A class whose methods came from two of the package's files (its own
    // pre-bundled lib, mapped back to its sources).
    let lib = "var T=class{list(t){return t.split(\",\").map(function(e){return e.trim().toLowerCase()})}table(t){return t.split(\"|\").map(function(e){return e.trim().toUpperCase()})}};";
    let half = lib.len() as u32 / 2;
    let map = SourceMap {
        sources: vec![
            "../node_modules/dep/src/Tokenizer.ts".into(),
            "../node_modules/dep/src/rules.ts".into(),
        ],
        segments: (0..lib.len() as u32)
            .step_by(4)
            .map(|c| (0, c, u32::from(c >= half)))
            .collect(),
    };
    let (rows, notes) = unit_rows(
        IndexSlice::Bun,
        &format!("{lib}\n"),
        &map,
        &root.join("out"),
    );
    assert_eq!(notes.mixed_spans, 0);
    assert!(
        !rows.is_empty() && rows.iter().all(|r| r.owner == "dep"),
        "{rows:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}
