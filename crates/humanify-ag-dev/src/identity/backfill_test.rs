use std::path::PathBuf;

use super::*;
use crate::identity::{Purpose, comparable, label_cards};
use crate::paths::{EVAL_RESULTS, LIBRARY_RESULTS, repo_root};

fn labels(tree: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(repo_root().join(tree))
        .unwrap()
        .filter_map(|e| Some(e.ok()?.path()))
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// Every committed eval label backfills: one identity per pair card, all
/// of its fields known or UNKNOWN with a reason.
#[test]
fn every_committed_eval_label_backfills() {
    let mut cards = 0;
    for dir in labels(EVAL_RESULTS) {
        let ids = eval_label(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
        for id in &ids {
            assert!(
                id.corpus.entry.contains("->"),
                "{}: {}",
                dir.display(),
                id.corpus.entry
            );
            assert!(
                id.scorer.version.known().is_none(),
                "legacy: no scorer version"
            );
        }
        cards += ids.len();
    }
    assert!(cards >= 40, "{cards} cards");
}

#[test]
fn a_pre_names_label_reads_as_off_mode_seeded_with_its_commit() {
    let ids = eval_label(&repo_root().join(EVAL_RESULTS).join("session-2026-08-05")).unwrap();
    assert!(!ids.is_empty());
    for id in &ids {
        assert_eq!(
            id.naming_mode,
            F::Known(Some("off".into())),
            "{}",
            id.corpus.entry
        );
        assert_eq!(id.library_index, F::Known(None));
        assert_eq!(
            id.base_mode,
            F::Known(Some("seeded".into())),
            "{}",
            id.corpus.entry
        );
        assert!(id.harness.commit.known().is_some());
    }
}

/// The frozen reference cards (lane L0's copy): names mode, scratch bases,
/// the index id, the binary — and the three repeats are leaderboard- and
/// band-comparable except for what was never recorded.
#[test]
fn the_frozen_names_references_backfill_comparably() {
    let root = PathBuf::from("/work/dev-harness-proof/cards");
    assert!(
        root.exists(),
        "the frozen proof cards are missing at {} (lane L0 copies them; docs/dev-harness.md §4)",
        root.display()
    );
    let r1 = eval_label(&root.join("ref-names-0357696e-r1")).unwrap();
    let r2 = eval_label(&root.join("ref-names-0357696e-r2")).unwrap();
    assert_eq!(r1.len(), 4);
    let first = &r1[0];
    assert_eq!(first.naming_mode, F::Known(Some("names".into())));
    assert_eq!(first.base_mode, F::Known(Some("scratch".into())));
    let ix = first.library_index.known().unwrap().as_ref().unwrap();
    assert_eq!(
        ix.id.known().map(String::as_str),
        Some("cb64c1f5fd507bee7f80efd46e0d01cec405a8b3028a9c05d56e94d7c071ec28")
    );
    assert_eq!(
        first.binary.name.known().map(String::as_str),
        Some("humanify")
    );
    assert!(
        first
            .llm
            .known()
            .unwrap()
            .as_ref()
            .unwrap()
            .endpoint_id
            .known()
            .is_some()
    );
    assert_eq!(label_cards(&r1), Ok(()), "a label's cards agree");
    // Legacy cards record no scorer version, so even same-commit repeats
    // are refused for the leaderboard until re-scored (or forced).
    let m = comparable(Purpose::Leaderboard, &r1[0], &r2[0]).unwrap_err();
    let fields: Vec<&str> = m.iter().map(|x| x.field.as_str()).collect();
    assert_eq!(fields, ["scorer.version"]);
    // Off-mode references against names-mode ones: the naming mode and
    // the index are named too.
    let off = eval_label(&root.join("ref-scratch-0f338ffa-r1")).unwrap();
    let m = comparable(Purpose::Leaderboard, &off[0], &r1[0]).unwrap_err();
    let fields: Vec<&str> = m.iter().map(|x| x.field.as_str()).collect();
    assert!(
        fields.contains(&"namingMode") && fields.contains(&"libraryIndex"),
        "{fields:?}"
    );
}

/// Every committed library label backfills from its summary: scorer
/// version, corpus identity, layer identities, index, binary.
#[test]
fn every_committed_library_label_backfills() {
    let mut cards = 0;
    for dir in labels(LIBRARY_RESULTS) {
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
    assert!(cards > 26_000, "{cards} cards");
}

/// A library label's identity uses the SAME corpus identity the loader
/// computes, so a new run and a legacy label compare on the corpus.
#[test]
fn a_library_label_matches_its_corpus_as_loaded_today() {
    let ids = library_label(&repo_root().join(LIBRARY_RESULTS).join("apps-exp127-off")).unwrap();
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
