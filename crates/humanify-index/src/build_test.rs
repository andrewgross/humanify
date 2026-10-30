//! The plan: what an as-of build would index, and which shards it would
//! reuse — no network (a recorded registry), no subprocess (no slices).

use super::build::{BuildConfig, SpongeRegions, parse_package_spec, plan};
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
        names: false,
        packages: vec!["ms".into()],
        as_of: as_of.into(),
        registry,
        store: root.join("store"),
        work: root.join("work"),
        out: None,
        recipes: Vec::new(),
        jobs: 1,
        merge_jobs: 1,
        offline: true,
        policy: Default::default(),
        pinned: Vec::new(),
        merge_only: false,
        sponge_regions: SpongeRegions::Drop,
        quiet: true,
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
    let p = plan(&cfg, Default::default()).expect("plan");
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
    let err = plan(&cfg, Default::default()).expect_err("no snapshot");
    assert!(err.contains("left-pad"), "{err}");
}

#[test]
fn a_shard_already_in_the_store_is_reused() {
    let cfg = config("reuse", "2020-12-01T23:59:59.999Z");
    let first = plan(&cfg, Default::default()).expect("plan");
    let inputs = first.tasks[0].missing[0].clone();
    write_shard(&cfg.store, &Shard::new(&inputs)).expect("write");
    let second = plan(&cfg, Default::default()).expect("plan");
    assert_eq!(second.tasks[0].reused, vec![inputs]);
    assert!(second.tasks[0].missing.is_empty());
    assert_eq!(second.tasks[1].missing.len(), 1);
}

#[test]
fn merge_only_builds_nothing_and_merges_what_the_store_holds() {
    use super::shard::ShardLiteral;
    let mut cfg = config("merge-only", "2020-12-01T23:59:59.999Z");
    cfg.merge_only = true;
    let out = cfg.store.parent().unwrap().join("ix");
    cfg.out = Some(out.clone());
    let first = plan(&cfg, Default::default()).expect("plan");
    // the literal shard of ms@2.1.2 only; the other three versions have none
    let mut shard = Shard::new(&first.tasks[0].missing[0]);
    shard.literals.push(ShardLiteral {
        owner: "ms".into(),
        version: "2.1.2".into(),
        file: "index.js".into(),
        lits: vec!["S:ms".into()],
    });
    write_shard(&cfg.store, &shard).expect("write");
    let id = super::build::run(&cfg, false).expect("merges");
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(id.len(), 64);
    assert_eq!(manifest["packages"].as_array().unwrap().len(), 1);
    let no_shard: Vec<&str> = manifest["refused"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["reason"] == "no-shard")
        .map(|r| r["version"].as_str().unwrap())
        .collect();
    assert_eq!(no_shard, ["2.1.1", "1.0.0", "0.7.3"]);
    assert!(out.join("literals.bin").exists());
    assert!(
        !cfg.work.exists(),
        "merge-only never installs or bundles anything"
    );
}

#[test]
fn a_module_shard_key_spells_the_rows_edge_stamp_never_the_readable_one() {
    // exp155: the readable edge data's stamp is edges/2 (the rows plus the
    // derived callee-print table), but a module shard HOLDS only rows, so
    // its key keeps edges/1 — an edges/2 index re-merges an edges/1 store
    // (exp152's) without one re-fingerprint. If these stamps ever collapse
    // or swap, this is the line that says which one a shard is keyed by.
    use crate::build::shard_inputs;
    use crate::shard::ShardKind;
    use humanify_core::libindex::format::{
        LIBRARY_EDGE_ROWS_VERSION, LIBRARY_EDGES_VERSION, RECIPE_VERSION, SliceRecord,
    };
    assert_eq!(
        (LIBRARY_EDGE_ROWS_VERSION, LIBRARY_EDGES_VERSION),
        ("edges/1", "edges/2")
    );
    let slices = vec![SliceRecord {
        slice: "bun-node-cjs-min".into(),
        recipe: "bun:target=node,format=cjs,minify".into(),
        recipe_version: RECIPE_VERSION.into(),
        default_for_bundler: true,
        bundler: "bun".into(),
        version: "1.0.0".into(),
        published: None,
        newest_at_as_of: None,
        bundler_after_as_of: false,
    }];
    let inputs = shard_inputs("ms", "2.1.2", &slices, SpongeRegions::Drop, false);
    let module = inputs
        .iter()
        .find(|i| matches!(i.kind, ShardKind::Modules(_)))
        .expect("a module shard");
    assert!(
        module.fingerprint.contains("+edges/1+"),
        "the shard key carries the ROWS stamp: {}",
        module.fingerprint
    );
    assert!(
        !module.fingerprint.contains("edges/2"),
        "the readable stamp never keys a shard: {}",
        module.fingerprint
    );
    // exp159: a names build's shard key spells it — the rows differ (each
    // unit row carries its names) — and a plain build's does not
    let plain = shard_inputs("ms", "2.1.2", &slices, SpongeRegions::Drop, false);
    assert_eq!(plain, inputs, "names off is the old key");
    let named = shard_inputs("ms", "2.1.2", &slices, SpongeRegions::Drop, true);
    let module = named
        .iter()
        .find(|i| matches!(i.kind, ShardKind::Modules(_)))
        .expect("a module shard");
    assert!(
        module.fingerprint.ends_with("+unit-names/1"),
        "the names shard key: {}",
        module.fingerprint
    );
    let literals = named
        .iter()
        .find(|i| i.kind == ShardKind::Literals)
        .expect("the literals shard");
    assert_eq!(
        literals.fingerprint, inputs[0].fingerprint,
        "the literal rows are untouched by the names pass"
    );
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
            .map(|c| (0, c, 0, 0))
            .chain(col(app.len()).map(|c| (1, c, 1, 0)))
            .collect(),
    };
    let (rows, notes) = unit_rows(
        bun_node(),
        &code,
        &map,
        &root.join("out"),
        SpongeRegions::Drop,
        false,
    );
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
            .map(|c| (0, c, u32::from(c >= half), 0))
            .collect(),
    };
    let (rows, notes) = unit_rows(
        bun_node(),
        &format!("{lib}\n"),
        &map,
        &root.join("out"),
        SpongeRegions::Drop,
        false,
    );
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
                .map(|c| (0, c, src, 0)),
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
    fingerprint_bundle(
        bun_node(),
        &root.join("out"),
        &mut shard,
        SpongeRegions::Drop,
        false,
    )
    .expect("fingerprint");
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
    let p = plan(&cfg, Default::default()).expect("plan");
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

#[test]
fn unit_rows_of_a_pre_bundled_file_are_dropped_by_evidence() {
    // exp097 step 3: a published file that pre-bundles OTHER packages
    // ("sponge": prettier's index.mjs) would name their code as its own.
    // Evidence, each read off the file: a bundler module marker naming
    // another package's node_modules path, or a vendored directory. The
    // package's own files keep their rows — its own minified build too
    // (esquery's main IS dist/esquery.min.js: a file NAME is not evidence).
    use crate::build::unit_rows;
    use crate::sourcemap::SourceMap;
    let root = std::env::temp_dir().join(format!("humanify-index-sponge-{}", std::process::id()));
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/sp/package.json",
        r#"{"name":"sp","version":"3.0.0"}"#,
    );
    write(
        "node_modules/sp/index.mjs",
        "// node_modules/other-pkg/lib/parse.js\nvar parse = 1;\n",
    );
    write(
        "node_modules/sp/own.js",
        "// src/own.ts\nexport const own = 1;\n",
    );
    write("node_modules/sp/vendor/copy.js", "var copy = 1;\n");
    write("node_modules/sp/dist/sp.min.js", "var m = 1;\n");
    let stmt = |n: &str| {
        format!(
            "var {n}=function(t){{return t.split(\",\").map(function(e){{return e.trim().toLowerCase()}})}};"
        )
    };
    let lines = [stmt("a"), stmt("b"), stmt("c"), stmt("d")];
    let code = lines.join("\n") + "\n";
    let col = |n: usize| (0..n as u32).step_by(4);
    let map = SourceMap {
        sources: vec![
            "../node_modules/sp/index.mjs".into(),
            "../node_modules/sp/own.js".into(),
            "../node_modules/sp/vendor/copy.js".into(),
            "../node_modules/sp/dist/sp.min.js".into(),
        ],
        segments: lines
            .iter()
            .enumerate()
            .flat_map(|(i, l)| col(l.len()).map(move |c| (i as u32, c, i as u32, 1)))
            .collect(),
    };
    let (rows, notes) = unit_rows(
        bun_node(),
        &code,
        &map,
        &root.join("out"),
        SpongeRegions::Drop,
        false,
    );
    let files: std::collections::BTreeSet<&str> = rows.iter().map(|r| r.file.as_str()).collect();
    assert_eq!(
        files,
        ["dist/sp.min.js", "own.js"].into_iter().collect(),
        "{rows:?}"
    );
    assert_eq!(
        notes.sponge_unit_files,
        [
            ("bundler-marker".to_string(), 1),
            ("vendored-path".to_string(), 1)
        ]
        .into_iter()
        .collect(),
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_module_marker_names_its_package_in_every_bundlers_spelling() {
    use crate::build::sponge_evidence;
    use crate::sourcemap::Owner;
    let dir = std::env::temp_dir().join(format!("humanify-index-markers-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let owner = Owner {
        package: "@scope/own".into(),
        version: "1.0.0".into(),
        file: "dist/index.mjs".into(),
    };
    let check = |text: &str| {
        let p = dir.join("f.js");
        std::fs::write(&p, text).unwrap();
        sponge_evidence(&owner, &p).kind()
    };
    assert_eq!(
        check("//#region ../../node_modules/.pnpm/a@1/node_modules/@x/y/dist/i.js\n"),
        Some("bundler-marker")
    );
    assert_eq!(
        check("/***/ \"./node_modules/lodash/lodash.js\":\n"),
        Some("bundler-marker")
    );
    assert_eq!(
        check("// node_modules/@scope/own/src/a.js\n// src/b.ts\n"),
        None,
        "its own path"
    );
    assert_eq!(
        check("var s = \"node_modules/x/y\";\n"),
        None,
        "not a comment"
    );
    assert_eq!(
        check("/*!*** ./node_modules/x/lib/a.js ***!*/\n"),
        Some("bundler-marker")
    );
    // prose that mentions a node_modules path is not a module marker
    // (glob's common.js comments on ignoring node_modules)
    assert_eq!(check("// ignore **/node_modules/** unless asked\n"), None);
    assert_eq!(check("  // see node_modules/foo/README.md for why\n"), None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_shard_records_the_sponge_files_its_units_were_dropped_for() {
    // The shard's notes say what the filter dropped (fingerprint_bundle
    // carries unit_rows' sponge notes, not only its mixed spans).
    use crate::build::fingerprint_bundle;
    use crate::shard::{Shard, ShardInputs, ShardKind};
    let root = std::env::temp_dir().join(format!(
        "humanify-index-sponge-notes-{}",
        std::process::id()
    ));
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/dep/package.json",
        r#"{"name":"dep","version":"1.2.3"}"#,
    );
    write("node_modules/dep/vendor/copy.js", "x");
    let stmt = "var parse=function(t){return t.split(\",\").map(function(e){return e.trim().toLowerCase()})};";
    let code = format!("{stmt}\n");
    let segments: Vec<(u32, u32, u32, u32)> = (0..stmt.len() as u32)
        .step_by(4)
        .map(|c| (0, c, 0, 0))
        .collect();
    let map = serde_json::json!({
        "version": 3,
        "sources": ["../node_modules/dep/vendor/copy.js"],
        "mappings": crate::sourcemap::encode_mappings(&segments),
    });
    write("out/out.js", &code);
    write("out/out.js.map", &map.to_string());
    let mut shard = Shard::new(&ShardInputs {
        kind: ShardKind::Modules(bun_node()),
        package: "dep".into(),
        version: "1.2.3".into(),
        bundler: Some("bun@test".into()),
        recipe: "test".into(),
        fingerprint: "test".into(),
    });
    fingerprint_bundle(
        bun_node(),
        &root.join("out"),
        &mut shard,
        SpongeRegions::Drop,
        false,
    )
    .expect("fingerprint");
    assert!(shard.units.is_empty(), "{:?}", shard.units);
    assert_eq!(
        shard.notes.sponge_unit_files.get("vendored-path"),
        Some(&1),
        "{:?}",
        shard.notes
    );
    assert!(shard.notes.sponge_unit_rows > 0, "{:?}", shard.notes);
    std::fs::remove_dir_all(&root).ok();
}

/// A package's own pre-bundled chunk (rolldown/esbuild `dist/chunk.mjs`)
/// that inlines a few dependencies, each between its bundler's module
/// markers; one unit per line of interest, mapped to that original line.
fn region_chunk_rows(
    mode: SpongeRegions,
) -> (Vec<crate::shard::ShardUnit>, crate::shard::ShardNotes) {
    use crate::build::unit_rows;
    use crate::sourcemap::SourceMap;
    let root = std::env::temp_dir().join(format!(
        "humanify-index-regions-{mode:?}-{}",
        std::process::id()
    ));
    let write = |rel: &str, text: &str| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().expect("dir")).expect("mkdir");
        std::fs::write(p, text).expect("write");
    };
    write(
        "node_modules/sp/package.json",
        r#"{"name":"sp","version":"3.0.0"}"#,
    );
    let chunk = [
        "//#region ../../node_modules/.pnpm/other-pkg@2.1.0/node_modules/other-pkg/lib/parse.js", // 0
        "var parse = 1;",       // 1 other-pkg
        "//#endregion",         // 2
        "//#region src/own.ts", // 3
        "var own = 1;",         // 4 own
        "//#endregion",         // 5
        "var tail = 1;",        // 6 own (between regions)
        "// ../node_modules/.pnpm/third@1.0.0/node_modules/third/x.js", // 7 esbuild marker
        "var x = 1;",           // 8 third
        "// src/back.ts",       // 9 own marker ends it
        "var back = 1;",        // 10 own
        "//#region node_modules/lazy-pkg/index.js", // 11
        "var lz;",              // 12 lazy-pkg
        "//#endregion",         // 13
        "var init_lz = __esmMin(() => {", // 14 lazy body
        "  lz = 1;",            // 15 lazy-pkg
        "});",                  // 16
        "export { own };",      // 17 own again
    ];
    write("node_modules/sp/dist/chunk.mjs", &(chunk.join("\n") + "\n"));
    let stmt = |n: &str| {
        format!(
            "var {n}=function(t){{return t.split(\",\").map(function(e){{return e.trim().toLowerCase()}})}};"
        )
    };
    let at = [1u32, 4, 6, 8, 10, 12, 15, 17];
    let lines: Vec<String> = ["a", "b", "c", "d", "e", "f", "g", "h"]
        .into_iter()
        .map(stmt)
        .collect();
    let code = lines.join("\n") + "\n";
    let map = SourceMap {
        sources: vec!["../node_modules/sp/dist/chunk.mjs".into()],
        segments: lines
            .iter()
            .zip(at)
            .enumerate()
            .flat_map(|(i, (l, orig))| {
                (0..l.len() as u32)
                    .step_by(4)
                    .map(move |c| (i as u32, c, 0, orig))
            })
            .collect(),
    };
    let out = unit_rows(bun_node(), &code, &map, &root.join("out"), mode, false);
    std::fs::remove_dir_all(&root).ok();
    out
}

/// Rows per (owner, version, file), in that order.
fn row_groups(rows: &[crate::shard::ShardUnit]) -> Vec<(String, String, String, usize)> {
    let mut g: std::collections::BTreeMap<(String, String, String), usize> = Default::default();
    for r in rows {
        *g.entry((r.owner.clone(), r.version.clone(), r.file.clone()))
            .or_default() += 1;
    }
    g.into_iter().map(|((o, v, f), n)| (o, v, f, n)).collect()
}

#[test]
fn a_pre_bundled_file_keeps_its_own_code_outside_the_foreign_regions() {
    // exp124: one `//#region node_modules/<other>/...` marker used to drop
    // EVERY unit of the file (exp097's whole-file sponge), so a package's
    // own rolldown/esbuild chunk was never indexed (exp123: apps-esbuild
    // 1.72 points of library bytes). The markers delimit the inlined
    // copies exactly: rolldown `//#region` .. `//#endregion` (plus a lazily
    // initialised module's body after its `//#endregion`, up to the next
    // marker or the top-level `export`), esbuild `// <path>` up to the next
    // marker. Units outside them are the shipper's own code.
    let (rows, notes) = region_chunk_rows(SpongeRegions::Drop);
    // 4 own units (lines 4, 6, 10, 17), 2 rows each (statement + function)
    assert_eq!(
        row_groups(&rows),
        [("sp".into(), "3.0.0".into(), "dist/chunk.mjs".into(), 8)],
        "{rows:?}"
    );
    assert_eq!(notes.sponge_unit_rows, 4, "parse, x, lz and its lazy body");
    assert_eq!(
        notes.sponge_unit_files,
        [("bundler-marker".to_string(), 1)].into_iter().collect()
    );
}

#[test]
fn a_foreign_region_can_be_credited_to_the_package_its_marker_names() {
    // The alternative to dropping: the copy is the ORIGIN's code (the
    // truth's marker rule says so too), named by the marker's last
    // node_modules package at the version its pnpm path spells, as that
    // package's file. A marker without a version is still dropped.
    let (rows, notes) = region_chunk_rows(SpongeRegions::Origin);
    assert_eq!(
        row_groups(&rows),
        [
            ("other-pkg".into(), "2.1.0".into(), "lib/parse.js".into(), 2),
            ("sp".into(), "3.0.0".into(), "dist/chunk.mjs".into(), 8),
            ("third".into(), "1.0.0".into(), "x.js".into(), 2),
        ],
        "{rows:?}"
    );
    assert_eq!(
        notes.sponge_unit_rows, 2,
        "lazy-pkg's two units: no version"
    );
}

/// exp162's fixture: two packages (ms's committed packument plus an
/// inline second one), and every planned shard filled with rows whose
/// unit prints straddle the four-way range split (2^62 multiples) and
/// share one callee list across ranges.
fn exp162_fixture(root: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    use super::shard::{ShardLiteral, ShardModule, ShardUnit, write_shard};
    use humanify_core::libindex::format::Callee;

    let registry = root.join("registry");
    std::fs::create_dir_all(&registry).expect("mkdir");
    std::fs::write(
        registry.join("ms.json"),
        include_str!("../testdata/ms.packument.json"),
    )
    .expect("fixture");
    // a second package, two versions (pins, the readers' own list)
    std::fs::write(
        registry.join("aa-exp162.json"),
        r#"{"name":"aa-exp162","fetchedAt":"2026-10-11T00:00:00.000Z",
            "time":{"1.0.0":"2018-01-01T00:00:00.000Z","1.1.0":"2019-01-01T00:00:00.000Z"},
            "versions":["1.0.0","1.1.0"]}"#,
    )
    .expect("fixture");

    let cfg = exp162_config(root, 1, root.join("unused"));
    let p = plan(&cfg, Default::default()).expect("plan");
    assert!(p.tasks.len() >= 6, "{} tasks", p.tasks.len());
    let store = root.join("store");
    let splits = [
        "0000000000000001",
        "4000000000000001",
        "8000000000000001",
        "c000000000000001",
    ];
    for (i, task) in p.tasks.iter().enumerate() {
        for inputs in &task.missing {
            let mut shard = Shard::new(inputs);
            if shard.kind != "literals" {
                for (j, hash) in splits.iter().enumerate() {
                    shard.units.push(ShardUnit {
                        owner: format!("{}-{}", task.name, task.version),
                        version: task.version.clone(),
                        file: format!("file{j}.js"),
                        hash: (*hash).into(),
                        nodes: 40,
                        content: 5,
                        module_bytes: 900,
                        // the SAME callee list everywhere: stored once
                        inside: vec!["00000000000000cc".into()],
                        out_of: vec![Callee {
                            owner: "aa-exp162@1.0.0".into(),
                            hash: "00000000000000dd".into(),
                        }],
                        names: vec![],
                    });
                    shard.modules.push(ShardModule {
                        owner: task.name.clone(),
                        version: task.version.clone(),
                        file: format!("file{j}.js"),
                        key: format!("{:016x}", i * 7 + j),
                    });
                }
            } else {
                shard.literals.push(ShardLiteral {
                    owner: task.name.clone(),
                    version: task.version.clone(),
                    file: "index.js".into(),
                    lits: vec!["S:a".into(), "S:b".into()],
                });
            }
            write_shard(&store, &shard).expect("write");
        }
    }
    // the CLI spelling of the same plan (the anchor's other binary reads it)
    std::fs::write(root.join("packages.txt"), "ms\n").expect("packages");
    std::fs::write(root.join("pins.txt"), "aa-exp162@1.0.0\naa-exp162@1.1.0\n").expect("pins");
    (registry, store)
}

/// exp162's merge-only config over the fixture.
fn exp162_config(
    root: &std::path::Path,
    merge_jobs: usize,
    out: std::path::PathBuf,
) -> BuildConfig {
    BuildConfig {
        names: false,
        packages: vec!["ms".into()],
        as_of: "2020-12-01T23:59:59.999Z".into(),
        registry: root.join("registry"),
        store: root.join("store"),
        work: root.join("work"),
        out: Some(out),
        recipes: vec![super::recipe::RecipeBuild {
            recipe: bun_node(),
            bin: "bun".into(),
        }],
        jobs: 1,
        merge_jobs,
        offline: true,
        policy: Default::default(),
        pinned: vec!["aa-exp162@1.0.0".into(), "aa-exp162@1.1.0".into()],
        merge_only: true,
        sponge_regions: SpongeRegions::Drop,
        quiet: true,
    }
}

/// Every file of a merged index directory, by name.
fn index_bytes(out: &std::path::Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(out)
        .expect("dir")
        .map(|e| e.expect("entry").path())
        .map(|p| {
            (
                p.file_name().expect("name").to_string_lossy().into_owned(),
                std::fs::read(&p).expect("read"),
            )
        })
        .collect()
}

/// exp162: `--merge-jobs N` over the fixture store — merges into
/// BYTE-IDENTICAL index directories at 1, 4 and 8 workers.
#[test]
fn merge_jobs_partitions_merge_into_the_same_index_dir() {
    let root = temp("merge-jobs");
    exp162_fixture(&root);
    let dirs: Vec<_> = [1usize, 4, 8]
        .iter()
        .map(|&jobs| {
            let out = root.join(format!("ix-{jobs}"));
            super::build::run(&exp162_config(&root, jobs, out.clone()), false).expect("merges");
            index_bytes(&out)
        })
        .collect();
    assert_eq!(dirs[0], dirs[1], "1 vs 4 workers");
    assert_eq!(dirs[0], dirs[2], "1 vs 8 workers");
    // and rows of one owner share their callee list even across ranges: an
    // inside callee inherits its row's owner, so one list per owner, not
    // one per row (24 rows would otherwise hold 24 lists)
    let edges = &dirs[0]["edges.bun-node-cjs-min.bin"];
    let h: [u64; 3] = std::array::from_fn(|i| {
        u64::from_le_bytes(edges[8 + i * 8..16 + i * 8].try_into().expect("header"))
    });
    assert_eq!(h[0], 24, "one edge row per unit print: 6 tasks x 4 prints");
    assert_eq!(h[1], 6, "one callee list per owner, shared across ranges");
}

/// exp162's byte-identity anchor, run by hand (`cargo test -p
/// humanify-index -- --ignored exp162_anchor`): writes the fixture store
/// under `EXP162_ANCHOR_DIR` and merges it with THIS build at
/// `--merge-jobs` 1/4/8 — and, when `EXP162_ANCHOR_OTHER_BIN` names
/// another `humanify-index` binary (main's, today's serial merge), with
/// that at its default — every pair of index dirs byte-identical.
#[test]
#[ignore = "fixture anchor: EXP162_ANCHOR_DIR=<dir> cargo test -p humanify-index -- --ignored exp162_anchor"]
// env: a hand-run anchor takes its paths from the environment (a
// fixture generator, never a pipeline switch; WP1.1 reads env once)
#[allow(clippy::disallowed_methods)]
fn exp162_anchor() {
    let dir = match std::env::var("EXP162_ANCHOR_DIR") {
        Ok(d) if !d.is_empty() => std::path::PathBuf::from(d),
        _ => return,
    };
    std::fs::create_dir_all(&dir).expect("mkdir");
    exp162_fixture(&dir);
    let mut dirs = vec![index_bytes(&{
        let out = dir.join("ix-lib-1");
        super::build::run(&exp162_config(&dir, 1, out.clone()), false).expect("merges");
        out
    })];
    for jobs in [4usize, 8] {
        let out = dir.join(format!("ix-lib-{jobs}"));
        super::build::run(&exp162_config(&dir, jobs, out.clone()), false).expect("merges");
        dirs.push(index_bytes(&out));
    }
    if let Ok(other) = std::env::var("EXP162_ANCHOR_OTHER_BIN") {
        if other.is_empty() {
            return;
        }
        let out = dir.join("ix-other");
        let status = std::process::Command::new(&other)
            .args(["build"])
            .arg("--packages")
            .arg(dir.join("packages.txt"))
            .arg("--pinned")
            .arg(dir.join("pins.txt"))
            .args(["--as-of", "2020-12-01T23:59:59.999Z"])
            .args(["--registry"])
            .arg(dir.join("registry"))
            .args(["--store"])
            .arg(dir.join("store"))
            .args(["--work"])
            .arg(dir.join("work"))
            .args(["--out"])
            .arg(&out)
            .args(["--recipe", "bun:target=node,format=cjs,minify"])
            .args(["--bun", "bun"])
            .args(["--merge-only", "--offline"])
            .status()
            .expect("runs the other binary");
        assert!(status.success(), "the other binary failed");
        dirs.push(index_bytes(&out));
    }
    for (i, d) in dirs.iter().enumerate().skip(1) {
        assert_eq!(&dirs[0], d, "dir {i} differs from the serial merge");
    }
}
