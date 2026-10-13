//! The plan: what an as-of build would index, and which shards it would
//! reuse — no network (a recorded registry), no subprocess (no slices).

use super::build::{BuildConfig, parse_package_spec, plan};
use super::shard::{Shard, write_shard};
use crate::bun_node;

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
        recipes: Vec::new(),
        jobs: 1,
        offline: true,
        policy: Default::default(),
        pinned: Vec::new(),
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
    let (rows, notes) = unit_rows(bun_node(), &code, &map, &root.join("out"));
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
    let (rows, notes) = unit_rows(bun_node(), &format!("{lib}\n"), &map, &root.join("out"));
    assert_eq!(notes.mixed_spans, 0);
    assert!(
        !rows.is_empty() && rows.iter().all(|r| r.owner == "dep"),
        "{rows:?}"
    );
    std::fs::remove_dir_all(&root).ok();
}

/// A fake install: `node_modules/dep` with three files, and one Bun build
/// whose map names two of them — `used.js` with mapping segments,
/// `listed.js` listed in `sources` with none.
fn fake_install(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("humanify-index-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.2.3"}"#,
    );
    write(
        "node_modules/dep/used.js",
        "module.exports = function () { return \"used-literal-one\" + \"used-literal-two\"; };\n",
    );
    write(
        "node_modules/dep/listed.js",
        "module.exports = \"listed-but-unmapped-literal\";\n",
    );
    write(
        "node_modules/dep/cli.js",
        "console.log(\"a cli script no bundle contains\");\n",
    );
    // "AAAA": line 0 col 0 → source 0 (used.js); listed.js has no segment.
    write(
        "build/bun-node-cjs-min/req/out.js.map",
        r#"{"version":3,"sources":["../../../node_modules/dep/used.js","../../../node_modules/dep/listed.js"],"mappings":"AAAA"}"#,
    );
    write("build/bun-node-cjs-min/req/out.js", "x\n");
    root
}

#[test]
fn the_literal_shard_holds_only_files_a_build_contains() {
    use crate::build::literal_shard;
    use crate::shard::{ShardInputs, ShardKind};
    let root = fake_install("lits-bundled");
    let inputs = ShardInputs {
        kind: ShardKind::Literals,
        package: "dep".into(),
        version: "1.2.3".into(),
        bundler: None,
        recipe: "test".into(),
        fingerprint: "test".into(),
    };
    let shard = literal_shard(&root, &inputs, &[bun_node()]);
    let files: Vec<&str> = shard.literals.iter().map(|l| l.file.as_str()).collect();
    assert_eq!(
        files,
        ["used.js"],
        "only the file the build maps code to (package.json, the unmapped source and the CLI script are not bundled)"
    );
    assert_eq!(shard.notes.unbundled_files, 3);
    // No slices, no builds to restrict by: every file (the lab's whole-tree
    // table).
    let all = literal_shard(&root, &inputs, &[]);
    assert_eq!(all.literals.len(), 4, "{:?}", all.literals);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_source_file_credited_to_two_modules_labels_neither() {
    // Bun 1.4.0's maps can credit a whole module to the wrong file in a
    // large bundle (exp095 selection lab: 5.5% of held-out keyed bytes).
    // A source file maps to ONE module per bundle, so a file the map
    // credits to two modules is wrong for at least one; neither is
    // labelled.
    use crate::build::fingerprint_bundle;
    use crate::shard::{Shard, ShardInputs, ShardKind};
    let root = std::env::temp_dir().join(format!("humanify-index-multi-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.2.3"}"#,
    );
    let bodies = [
        r#"(ge,q)=>{q.exports=function(e,t){var n=e.split(",");for(var r=0;r<n.length;r++){t.push(n[r].trim()+"abc")}return t}}"#,
        r#"(x,y)=>{y.exports=function(e,t){var n=e.split("|");for(var r=0;r<n.length;r++){t.push(n[r].toLowerCase()*1024)}return t}}"#,
        r#"(u,w)=>{w.exports=function(e,t){var n=e.split(";");for(var r=0;r<n.length;r++){t.push(n[r].toUpperCase()+"parts")}return t}}"#,
    ];
    let code = format!(
        "var m=(e,r)=>()=>(r||e((r={{exports:{{}}}}).exports,r),r.exports);var A=m({});var B=m({});var C=m({});module.exports=[A(),B(),C()];\n",
        bodies[0], bodies[1], bodies[2]
    );
    // modules A and B both credited to a.js (one of them wrongly); C to b.js
    let mut segments = Vec::new();
    for (body, src) in bodies.iter().zip([0u32, 0, 1]) {
        let start = code.find(body).expect("body") as u32;
        segments.extend(
            (start..start + body.len() as u32)
                .step_by(2)
                .map(|c| (0, c, src)),
        );
    }
    let map = serde_json::json!({
        "version": 3,
        "sources": ["../node_modules/dep/a.js", "../node_modules/dep/b.js"],
        "mappings": crate::sourcemap::encode_mappings(&segments),
    });
    write("out/out.js", &code);
    write("out/out.js.map", &map.to_string());
    let inputs = ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package: "dep".into(),
        version: "1.2.3".into(),
        bundler: Some("bun@test".into()),
        recipe: "test".into(),
        fingerprint: "test".into(),
    };
    let mut shard = Shard::new(&inputs);
    fingerprint_bundle(bun_node(), &root.join("out"), &mut shard).expect("fingerprint");
    let files: Vec<&str> = shard.modules.iter().map(|m| m.file.as_str()).collect();
    assert_eq!(files, ["b.js"], "{:?}", shard.notes);
    assert_eq!(shard.notes.multi_module_files, 1);
    assert_eq!(shard.notes.multi_module_dropped, 2);
    assert!(
        shard.notes.multi_module_units_dropped > 0 && shard.units.iter().all(|u| u.file == "b.js"),
        "a.js's units are not labelled either: {:?} {:?}",
        shard.notes,
        shard.units
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_pinned_only_package_is_indexed_at_its_pins_alone() {
    // `--pinned <file>`: the bundle's own readers name exact versions; the
    // version policy adds nothing for them.
    let mut cfg = config("pinned-only", "2020-12-01T23:59:59.999Z");
    cfg.packages = Vec::new();
    cfg.pinned = vec!["ms@1.0.0".into(), "ms@0.7.1".into()];
    let p = plan(&cfg).expect("plan");
    let tasks: Vec<(String, String)> = p
        .tasks
        .iter()
        .map(|t| (t.version.clone(), t.role.clone()))
        .collect();
    assert_eq!(
        tasks,
        [
            ("1.0.0".to_string(), "pinned".to_string()),
            ("0.7.1".to_string(), "pinned".to_string())
        ]
    );
}

#[test]
fn a_work_directory_reached_through_a_symlink_keeps_its_bundled_literals() {
    // Bun writes map sources relative to the REAL directory it built in;
    // the walk sees the path it was given. Both are canonicalised.
    use crate::build::literal_shard;
    use crate::shard::{ShardInputs, ShardKind};
    let real = fake_install("lits-real");
    let link = real.with_file_name(format!(
        "{}-link",
        real.file_name().expect("name").to_string_lossy()
    ));
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    // the map now names the source through the real directory's name
    let name = real
        .file_name()
        .expect("name")
        .to_string_lossy()
        .into_owned();
    std::fs::write(
        real.join("build/bun-node-cjs-min/req/out.js.map"),
        format!(
            r#"{{"version":3,"sources":["../../../../{name}/node_modules/dep/used.js"],"mappings":"AAAA"}}"#
        ),
    )
    .expect("map");
    let inputs = ShardInputs {
        kind: ShardKind::Literals,
        package: "dep".into(),
        version: "1.2.3".into(),
        bundler: None,
        recipe: "test".into(),
        fingerprint: "test".into(),
    };
    let shard = literal_shard(&link, &inputs, &[bun_node()]);
    let files: Vec<&str> = shard.literals.iter().map(|l| l.file.as_str()).collect();
    assert_eq!(files, ["used.js"]);
    std::fs::remove_file(&link).ok();
    std::fs::remove_dir_all(&real).ok();
}
