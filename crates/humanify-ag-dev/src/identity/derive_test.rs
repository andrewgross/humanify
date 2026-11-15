use std::path::PathBuf;

use super::*;
use crate::identity::{Purpose, comparable, label_cards};
use crate::paths::{LIBRARY_LABEL_FIXTURES, repo_root};

fn labels(tree: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(repo_root().join(tree))
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

fn scratch_label(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("derive-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A label as `eval score` writes it: every identity field it records is
/// known, and its cards agree.
#[test]
fn a_label_as_eval_score_writes_it_derives_known_identities() {
    let dir = scratch_label("score");
    std::fs::write(dir.join("commit.txt"), "abc1234\n").unwrap();
    std::fs::write(
        dir.join("pipeline.json"),
        r#"{"pipeline":{"kind":"rust-bin","bin":{"path":"/w/target/release/humanify-ag","sha256":"binsha","commit":"abc1234","dirty":false,"built":true}},"namingMode":"names","libraryIndex":"/work/library-index/4736dcfadff79ee2dd921d1d778fd7d92f604a708f3ad5b652c146ed89d67c84","runConfig":{"path":"/w/x.run-config.json","sha256":"cfgsha"},"bootGateModel":"claude-opus-4-1"}"#,
    )
    .unwrap();
    for (v, pair) in [
        ("2.1.86", "2.1.85->2.1.86"),
        ("2.1.119", "2.1.118->2.1.119"),
    ] {
        let input = dir.join(format!("{v}.input.js"));
        std::fs::write(&input, "input\n").unwrap();
        std::fs::write(
            dir.join(format!("{v}.json")),
            format!(r#"{{"scorerVersion":3,"pair":"{pair}"}}"#),
        )
        .unwrap();
        std::fs::write(
            dir.join(format!("{v}-run.json")),
            format!(
                r#"{{"pair":"{pair}","provenance":{{"dirty":false}},"inputs":{{"input":"{}","prior":"/w/x-rebased/.humanify/humanified.js","priorKind":"rebased","baseMode":"scratch"}},"config":{{"endpoint":"http://e/v1","model":"m","reasoningEffort":"low","concurrency":472,"cache":{{"enabled":false}},"resolved":{{"configFile":{{"sha256":"toml-{v}"}},"library":{{"matching":"names"}}}}}},"library":{{"indexId":"4736dcfadff79ee2dd921d1d778fd7d92f604a708f3ad5b652c146ed89d67c84"}}}}"#,
                input.display()
            ),
        )
        .unwrap();
    }
    let ids = eval_label(&dir).unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(ids.len(), 2);
    let first = &ids[0];
    assert_eq!(first.corpus.entry, "2.1.118->2.1.119");
    assert_eq!(first.naming_mode, F::Known(Some("names".into())));
    assert_eq!(first.scorer.version, F::Known("3".into()));
    assert_eq!(first.harness.commit, F::Known("abc1234".into()));
    assert_eq!(first.binary.sha256, F::Known("binsha".into()));
    let ix = first.library_index.known().unwrap().as_ref().unwrap();
    assert_eq!(
        ix.id.known().map(String::as_str),
        Some("4736dcfadff79ee2dd921d1d778fd7d92f604a708f3ad5b652c146ed89d67c84")
    );
    assert_eq!(label_cards(&ids), Ok(()), "a label's cards agree");
    assert_eq!(comparable(Purpose::Leaderboard, &ids[0], &ids[0]), Ok(()));
}

/// A card that records no scorer version is not this scorer's: refused,
/// never read as some default version.
#[test]
fn a_card_without_a_scorer_version_is_refused() {
    let dir = scratch_label("no-scorer");
    std::fs::write(dir.join("2.1.86.json"), r#"{"pair":"2.1.85->2.1.86"}"#).unwrap();
    let e = eval_label(&dir).unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(e.contains("scorerVersion"), "{e}");
}

/// Every fixture library label derives from its summary: scorer
/// version, corpus identity, layer identities, index, binary.
#[test]
fn every_fixture_library_label_derives() {
    let mut cards = 0;
    for dir in labels(LIBRARY_LABEL_FIXTURES) {
        let ids = library_label(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        if let Some(id) = ids.first() {
            assert_eq!(id.scorer.name, "exp096-score");
            assert!(matches!(
                id.scorer.version.known().map(String::as_str),
                Some("13" | "14")
            ));
            assert!(
                id.corpus.manifest_sha256.known().is_some(),
                "{}",
                dir.display()
            );
            assert_eq!(label_cards(&ids), Ok(()), "{}", dir.display());
        }
        cards += ids.len();
    }
    // No `cards/` in a fixture: its one identity stands for the corpus.
    assert_eq!(cards, 3, "{cards} cards");
}

/// A library label's identity uses the SAME corpus identity the loader
/// computes, so a new run and a legacy label compare on the corpus.
#[test]
fn a_library_label_matches_its_corpus_as_loaded_today() {
    let ids = library_label(
        &repo_root()
            .join(LIBRARY_LABEL_FIXTURES)
            .join("apps-exp127-off"),
    )
    .unwrap();
    let corpus = crate::committed::library::load_corpus(&repo_root(), "apps").unwrap();
    assert_eq!(
        ids[0].corpus.manifest_sha256.known(),
        Some(&corpus.manifest_id)
    );
    let layers = ids[0].corpus.layers.known().unwrap().as_ref().unwrap();
    for (k, v) in &corpus.layer_ids {
        if let Some(recorded) = layers.get(k) {
            assert_eq!(recorded, v, "{k}");
        }
    }
}
