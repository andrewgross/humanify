//! Every committed config file loads through its typed loader, and a
//! field the loader does not declare fails the load.

use super::*;
use crate::paths::repo_root;

#[test]
fn pairs_json_loads_typed() {
    let p = eval::pairs(&repo_root()).expect("pairs.json");
    let names: Vec<String> = p.value.pairs.iter().map(eval::Pair::name).collect();
    assert_eq!(
        names,
        [
            "2.1.85->2.1.86",
            "2.1.118->2.1.119",
            "2.1.179->2.1.181",
            "2.1.197->2.1.198",
            "2.1.215->2.1.216"
        ],
        "version order: the self-hop runs on the LAST pair's `to`, and the \
         self-hop reference is 2.1.216's (exp201 added 179->181 mid-list)"
    );
    assert_eq!(
        p.value.llm.concurrency, 472,
        "the string count is typed once"
    );
    assert_eq!(p.sha256.len(), 64);
}

#[test]
fn bands_and_self_hop_reference_load_per_mode() {
    let b = eval::noise_bands(&repo_root()).expect("noise-bands.json");
    assert_eq!(
        b.value.for_mode("off").and_then(|m| m.get("novel")),
        Some(&0.0)
    );
    assert!(b.value.for_mode("names").is_some());
    let s = eval::self_hop_reference(&repo_root()).expect("self-hop-reference.json");
    assert_eq!(s.value.range("off"), Some((2, 184)));
    assert_eq!(s.value.range("names"), Some((8, 8)));
}

#[test]
fn every_launch_golden_loads() {
    let g = eval::launch_goldens(&repo_root()).expect("goldens");
    assert!(g.contains_key("run-launch.golden.txt"));
    assert!(g.contains_key("neutrality-names.golden.txt"));
    assert!(g.len() >= 13, "{:?}", g.keys().collect::<Vec<_>>());
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
            assert_eq!(loaded.manifest.value.corpus.as_deref(), Some(c.name));
        }
    }
    // 16 corpora x 10 layers + 7 corpora with the scorer-4 `copies` layer.
    assert_eq!(layers, 167);
    library::bands(&repo_root()).expect("bands.json");
    assert!(library::corpus_named("nope").is_err());
}

#[test]
fn the_concurrency_allowlist_and_fixture_configs_load() {
    let a = census::concurrency_allowlist(&repo_root()).expect("allowlist");
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
/// committed scorer-14 summary's corpus and layer shas.
#[test]
fn corpus_and_layer_ids_equal_the_recorded_summary_shas() {
    let c = library::load_corpus(&repo_root(), "apps").unwrap();
    let summary = std::fs::read_to_string(
        repo_root()
            .join(crate::paths::LIBRARY_RESULTS)
            .join("apps-exp127-off/summary.json"),
    )
    .unwrap();
    let v = crate::json::py::PyValue::parse(&summary).unwrap();
    let config = v.get("config").unwrap();
    let recorded = |k: &str| match config.get(k) {
        Some(crate::json::py::PyValue::Str(s)) => s.clone(),
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

#[test]
fn bands_are_keyed_by_scorer_version() {
    // Scorer 1 (the TS scorer) is the top level and its `byMode`; a newer
    // scorer's bands live under `byScorer.<version>`, never mixed in.
    let text = r#"{"provenance":{"provisional":false,"sources":[],"commit":"c","labels":[]},
      "bands":{"novel":0,"noise":17},"byMode":{"names":{"provenance":{"provisional":false,"sources":[],"commit":"c","labels":[]},"bands":{"noise":397}}},
      "byScorer":{"2":{"provenance":{"provisional":false,"sources":[],"commit":"c","labels":[]},"bands":{"noise":20},
        "byMode":{"names":{"provenance":{"provisional":false,"sources":[],"commit":"c","labels":[]},"bands":{"noise":300}}}}}}"#;
    let b: eval::NoiseBands = serde_json::from_str(text).unwrap();
    let get = |scorer: u64, mode: &str| {
        b.for_scorer_mode(scorer, mode)
            .and_then(|m| m.get("noise"))
            .copied()
    };
    assert_eq!(get(1, "off"), Some(17.0));
    assert_eq!(get(1, "names"), Some(397.0));
    assert_eq!(get(2, "off"), Some(20.0));
    assert_eq!(get(2, "names"), Some(300.0));
    assert_eq!(get(3, "off"), None);
}

#[test]
fn the_committed_bands_carry_scorer_2_for_both_modes() {
    let b = eval::noise_bands(&repo_root()).expect("noise-bands.json");
    let get = |mode: &str, k: &str| {
        b.value
            .for_scorer_mode(2, mode)
            .and_then(|m| m.get(k))
            .copied()
    };
    assert_eq!(get("off", "novel"), Some(0.0));
    assert_eq!(get("names", "realLn"), Some(0.0));
    assert!(get("names", "noise").is_some());
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
    note(&eval::noise_bands(&repo).unwrap().path);
    note(&eval::self_hop_reference(&repo).unwrap().path);
    for g in eval::launch_goldens(&repo).unwrap().values() {
        note(&g.path);
    }
    note(&library::data_roots(&repo).unwrap().path);
    note(&library::bands(&repo).unwrap().path);
    for c in library::CORPORA {
        let loaded = library::load_corpus(&repo, c.name).unwrap();
        note(&loaded.manifest.path);
        for l in &loaded.layers {
            note(&l.path);
        }
    }
    note(&census::concurrency_allowlist(&repo).unwrap().path);
    note(&census::clone_allowlist(&repo).unwrap().path);

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
