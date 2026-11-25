//! Every committed config file loads through its typed loader, and a
//! field the loader does not declare fails the load.

use std::collections::BTreeMap;

use super::*;
use crate::paths::repo_root;

#[test]
fn pairs_json_loads_typed() {
    let p = eval::pairs(&repo_root()).expect("pairs.json");
    let names: Vec<String> = p
        .value
        .pairs
        .iter()
        .map(|p| format!("{}->{}", p.from, p.to))
        .collect();
    assert_eq!(
        names,
        [
            "2.1.85->2.1.86",
            "2.1.118->2.1.119",
            "2.1.179->2.1.181",
            "2.1.197->2.1.198",
            "2.1.215->2.1.216",
            "2.1.260->2.1.261"
        ],
        "version order; the five single-file pairs come first, unchanged, \
         and the code-split pair after them"
    );
    assert_eq!(
        p.value.llm.concurrency, 472,
        "the string count is typed once"
    );
}

#[test]
fn the_self_hop_pair_is_the_one_the_self_hop_reference_was_measured_on() {
    let p = eval::pairs(&repo_root()).expect("pairs.json").value;
    let marked: Vec<&eval::Pair> = p.pairs.iter().filter(|p| p.self_hop).collect();
    let [pair] = marked.as_slice() else {
        panic!("exactly one pair carries selfHop: {marked:?}")
    };
    let reference = eval::self_hop_reference(&repo_root()).unwrap().value;
    assert_eq!(pair.to, reference.version);
    for mode in reference.by_mode.values() {
        assert_eq!(pair.to, mode.version);
    }
}

#[test]
fn a_pairs_file_with_no_self_hop_pair_or_two_is_refused() {
    let dir = std::env::temp_dir().join(format!("dev-pairs-hop-{}", std::process::id()));
    let eval_dir = dir.join(crate::paths::EVAL_DIR);
    std::fs::create_dir_all(&eval_dir).unwrap();
    let committed = std::fs::read_to_string(repo_root().join("dev/eval/pairs.json")).unwrap();
    let none = committed.replace(",\n      \"selfHop\": true", "");
    assert_ne!(
        none, committed,
        "the committed file marks its self-hop pair"
    );
    std::fs::write(eval_dir.join("pairs.json"), &none).unwrap();
    let err = eval::pairs(&dir).unwrap_err();
    assert!(err.contains("selfHop"), "{err}");
    let two = committed.replace(
        "\"kind\": \"quiet-80s (small base, control)\"",
        "\"kind\": \"quiet-80s (small base, control)\",\n      \"selfHop\": true",
    );
    std::fs::write(eval_dir.join("pairs.json"), &two).unwrap();
    let err = eval::pairs(&dir).unwrap_err();
    assert!(err.contains("selfHop"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_self_hop_reference_loads_per_mode() {
    let s = eval::self_hop_reference(&repo_root()).expect("self-hop-reference.json");
    assert_eq!(s.value.range("off"), Some((2, 182)));
    assert_eq!(
        s.value.range("names"),
        Some((2, 8)),
        "exp207: ref-names-40921b9f r1/r2/r3 + serial-40921b9f-s1"
    );
}

#[test]
fn every_launch_golden_loads() {
    let g = launch_goldens(&repo_root()).expect("goldens");
    assert!(g.contains_key("run-launch.golden.txt"));
    assert!(g.contains_key("neutrality-names.golden.txt"));
    assert!(g.len() >= 10, "{:?}", g.keys().collect::<Vec<_>>());
}

#[test]
fn every_library_corpus_and_layer_loads() {
    let mut layers = 0;
    for c in library::CORPORA {
        let loaded = library::load_corpus(&repo_root(), c.name)
            .unwrap_or_else(|e| panic!("corpus {}: {e}", c.name));
        layers += loaded.layers.len();
        assert!(
            loaded.layers.len() >= 10,
            "{}: {} layers",
            c.name,
            loaded.layers.len()
        );
        if c.name != "dev" {
            assert_eq!(loaded.manifest.value._corpus.as_deref(), Some(c.name));
        }
    }
    // 16 corpora x 10 layers + 7 corpora with the scorer-4 `copies` layer.
    assert_eq!(layers, 167);
    assert!(library::corpus_named("nope").is_err());
}

#[test]
fn the_concurrency_allowlist_and_fixture_configs_load() {
    let a =
        census::concurrency_allowlist_at(&repo_root().join(crate::paths::CONCURRENCY_ALLOWLIST))
            .expect("allowlist");
    assert!(a.value.entries.len() > 80);
    let f = e2e::fixture_configs(&repo_root()).expect("fixtures");
    assert!(f.len() >= 19, "{}", f.len());
    assert!(f.values().all(|c| !c.value.version_pairs.is_empty()));
}

#[test]
fn an_undeclared_field_fails_the_load() {
    let dir = std::env::temp_dir().join(format!("dev-committed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("p.json");
    std::fs::write(&file, r#"{"from":"a","to":"b","kind":"k","note":"x"}"#).unwrap();
    let err = load_json::<eval::Pair>(&file).unwrap_err();
    assert!(err.contains("unknown field `note`"), "{err}");
    std::fs::write(&file, r#"{"from":"a","to":"b","kind":"k"}"#).unwrap();
    assert!(load_json::<eval::Pair>(&file).is_ok());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn the_endpoint_resolves_from_the_flag_then_the_local_files() {
    let dir = std::env::temp_dir().join(format!("dev-local-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(
        local::resolve_endpoint(Some("http://flag/v1"), &dir).unwrap(),
        "http://flag/v1"
    );
    let err = local::resolve_endpoint(None, &dir).unwrap_err();
    assert!(
        err.contains("--endpoint") && err.contains(".humanify.local.json"),
        "{err}"
    );
    std::fs::write(
        dir.join(".humanify.local.json"),
        r#"{"llm":{"endpoint":"http://file/v1"}}"#,
    )
    .unwrap();
    assert_eq!(
        local::resolve_endpoint(None, &dir).unwrap(),
        "http://file/v1"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_layer_with_a_block_its_kind_does_not_have_fails_the_load() {
    let repo = std::env::temp_dir().join(format!("dev-layer-{}", std::process::id()));
    let dir = repo.join(crate::paths::LIBRARY_CORPORA);
    std::fs::create_dir_all(&dir).unwrap();
    let dev = library::corpus_named("dev").unwrap();
    let body = r#"{"schema":"s","corpusSha256":"c","entries":{"a":"b"}"#;
    std::fs::write(dir.join("origins.json"), format!("{body}}}")).unwrap();
    assert!(
        library::layer(&repo, library::LayerKind::Origins, dev)
            .unwrap()
            .is_some()
    );
    std::fs::write(dir.join("origins.json"), format!("{body},\"rules\":{{}}}}")).unwrap();
    let err = library::layer(&repo, library::LayerKind::Origins, dev).unwrap_err();
    assert!(err.contains("rules"), "{err}");
    std::fs::write(dir.join("chained.json"), format!("{body}}}")).unwrap();
    let err = library::layer(&repo, library::LayerKind::Chained, dev).unwrap_err();
    assert!(err.contains("about"), "a missing block fails too: {err}");
    std::fs::remove_dir_all(&repo).unwrap();
}

/// The loader's identities are the ones the library scorer recorded: a
/// scorer-14 summary's corpus and layer shas (a test fixture).
#[test]
fn corpus_and_layer_ids_equal_the_recorded_summary_shas() {
    let c = library::load_corpus(&repo_root(), "apps").unwrap();
    let summary = std::fs::read_to_string(
        repo_root()
            .join(crate::paths::LIBRARY_LABEL_FIXTURES)
            .join("apps-exp127-off/summary.json"),
    )
    .unwrap();
    let v = serde_json::from_str::<serde_json::Value>(&summary).unwrap();
    let config = v.get("config").unwrap();
    let recorded = |k: &str| match config.get(k) {
        Some(serde_json::Value::String(s)) => s.clone(),
        other => panic!("{k}: {other:?}"),
    };
    assert_eq!(c.manifest_id, recorded("corpusSha256"));
    for key in [
        "originsSha256",
        "origins7Sha256",
        "chainedSha256",
        "mapdefect14Sha256",
        "misattr13Sha256",
    ] {
        assert_eq!(c.layer_ids[key], recorded(key), "{key}");
    }
}

/// The bun-dev list parses to (entry id, bundle path) rows, blank lines
/// skipped; a row without a path fails the load, naming the line.
#[test]
fn the_bundev_list_is_entry_id_and_bundle_path_per_line() {
    let repo = std::env::temp_dir().join(format!("dev-bundev-{}", std::process::id()));
    let dir = repo.join(crate::paths::LIBRARY_DIR);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("bun-dev-list.tsv");
    std::fs::write(&file, "a\t/x/a.js\tbun\n\nb\t/x/b.js\tbun\n").unwrap();
    let rows = library::bundev_list(&repo).unwrap().value;
    assert_eq!(
        rows,
        [
            ("a".to_string(), std::path::PathBuf::from("/x/a.js")),
            ("b".to_string(), std::path::PathBuf::from("/x/b.js")),
        ]
    );
    std::fs::write(&file, "a\t/x/a.js\tbun\nb\n").unwrap();
    let err = library::bundev_list(&repo).unwrap_err();
    assert!(err.contains("line 2"), "{err}");
    std::fs::remove_dir_all(&repo).unwrap();
}

/// Every committed file under `dev/` (the labels aside, which have their
/// own readers) is read by its typed loader: a file moved there that no
/// loader reads — or a loader still pointing at the old home — fails.
#[test]
fn every_committed_dev_file_is_read_by_its_loader() {
    use std::collections::BTreeSet;
    let repo = repo_root();
    let mut read: BTreeSet<std::path::PathBuf> = BTreeSet::new();
    let mut note = |p: &std::path::Path| {
        read.insert(p.canonicalize().expect("a loaded file exists"));
    };
    note(&eval::pairs(&repo).unwrap().path);
    note(&eval::self_hop_reference(&repo).unwrap().path);
    for g in launch_goldens(&repo).unwrap().values() {
        note(&g.path);
    }
    note(&library::data_roots(&repo).unwrap().path);
    // read by path (`library leaderboard` / `determinism` check it is there)
    note(
        &load_json::<serde_json::Value>(&repo.join("dev/library/bands.json"))
            .unwrap()
            .path,
    );
    note(&library::bundev_list(&repo).unwrap().path);
    for c in library::CORPORA {
        let loaded = library::load_corpus(&repo, c.name).unwrap();
        note(&loaded.manifest.path);
        for l in &loaded.layers {
            note(&l.path);
        }
    }
    note(
        &census::concurrency_allowlist_at(&repo.join(crate::paths::CONCURRENCY_ALLOWLIST))
            .unwrap()
            .path,
    );
    note(
        &census::effects_allowlist_at(&repo.join(crate::paths::EFFECTS_ALLOWLIST))
            .unwrap()
            .path,
    );
    note(&census::clone_allowlist(&repo).unwrap().path);
    note(&census::docs_allowlist(&repo).unwrap().path);

    let mut on_disk = BTreeSet::new();
    let mut stack = vec![repo.join("dev")];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "results" {
                    stack.push(path);
                }
            } else if name != ".gitignore" {
                on_disk.insert(path.canonicalize().unwrap());
            }
        }
    }
    let unread: Vec<_> = on_disk.difference(&read).collect();
    assert!(
        unread.is_empty(),
        "files under dev/ no loader reads: {unread:?}"
    );
    assert!(read.len() > 150, "only {} files read", read.len());
}

/// Every launch golden (`dev/eval/goldens/`), by file name.
fn launch_goldens(repo: &Path) -> Result<BTreeMap<String, Loaded<String>>, String> {
    let mut out = BTreeMap::new();
    for dir in [repo.join(crate::paths::LAUNCH_GOLDENS)] {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.ends_with(".golden.txt") {
                out.insert(name, load_text(&path)?);
            }
        }
    }
    Ok(out)
}
