use super::*;

fn known<T>(v: T) -> F<T> {
    F::Known(v)
}

/// A names-mode eval card, every field known.
fn card(entry: &str) -> Identity {
    Identity {
        schema: SCHEMA,
        kind: CardKind::EvalPair,
        harness: Harness {
            commit: known("abc".into()),
            dirty: known(false),
        },
        binary: Binary {
            path: known(format!("/w/{entry}/target/release/humanify-ag")),
            sha256: known("binsha".into()),
            build_commit: known("abc".into()),
            dirty: known(false),
            name: known("humanify-ag".into()),
        },
        base_mode: known(Some("scratch".into())),
        naming_mode: known(Some("names".into())),
        library_index: known(Some(LibraryIndex {
            id: known("cb64".into()),
            dir: known("/work/library-index/cb64".into()),
            bytes: known(10),
        })),
        pipeline_settings: PipelineSettings {
            sets: known(vec![]),
            config_toml_sha256: known(Some(format!("toml-{entry}"))),
        },
        llm: known(Some(Llm {
            model: known("gpt-oss-20b".into()),
            reasoning_effort: known(Some("low".into())),
            concurrency: known(472),
            endpoint_id: known("e".into()),
            cache: known(None),
        })),
        corpus: Corpus {
            name: known("034-pairs".into()),
            manifest_sha256: known("pairs".into()),
            layers: known(None),
            entry: entry.into(),
            inputs: known(BTreeMap::from([(
                format!("/in/{entry}"),
                "insha".to_string(),
            )])),
        },
        scorer: Scorer {
            name: "034-kpi".into(),
            version: known("2".into()),
        },
        boot_gate_model: known(Some("claude-opus-4-1".into())),
        run_config_sha256: known(Some(format!("rc-{entry}"))),
    }
}

fn fields(r: Result<(), Vec<Mismatch>>) -> Vec<String> {
    r.err()
        .unwrap_or_default()
        .into_iter()
        .map(|m| m.field)
        .collect()
}

#[test]
fn cards_of_one_label_differ_only_in_per_card_fields() {
    let (a, b) = (card("2.1.85->2.1.86"), card("2.1.118->2.1.119"));
    assert_eq!(comparable(Purpose::AddCard, &a, &b), Ok(()));
    assert_eq!(label_cards(&[a, b]), Ok(()));
}

#[test]
fn adding_a_card_checks_the_naming_mode_and_the_index() {
    let a = card("p1");
    let mut b = card("p2");
    b.naming_mode = known(Some("off".into()));
    b.library_index = known(None);
    let got = fields(comparable(Purpose::AddCard, &a, &b));
    assert_eq!(
        got,
        ["libraryIndex", "namingMode"],
        "an ancestor null replaces the subtree"
    );
    let errs = label_cards(&[a, b]).unwrap_err();
    assert!(errs[0].contains("namingMode"), "{errs:?}");
}

#[test]
fn the_leaderboard_ignores_the_binary_but_not_the_index() {
    let a = card("p");
    let mut b = card("p");
    b.binary.sha256 = known("other".into());
    b.harness.commit = known("def".into());
    assert_eq!(
        comparable(Purpose::Leaderboard, &a, &b),
        Ok(()),
        "the binary is what changed"
    );
    b.library_index = known(Some(LibraryIndex {
        id: known("ix-v2".into()),
        dir: known("/x".into()),
        bytes: known(1),
    }));
    assert_eq!(
        fields(comparable(Purpose::Leaderboard, &a, &b)),
        ["libraryIndex.id"]
    );
    assert_eq!(
        fields(comparable(Purpose::Bands, &a, &b)),
        ["binary.sha256", "libraryIndex.id"]
    );
}

#[test]
fn unknown_never_equals_anything_not_even_unknown() {
    let mut a = card("p");
    let mut b = card("p");
    a.scorer.version = F::unknown("legacy");
    b.scorer.version = F::unknown("legacy");
    let m = comparable(Purpose::Leaderboard, &a, &b).unwrap_err();
    assert_eq!(m.len(), 1);
    assert_eq!(m[0].field, "scorer.version");
    assert_eq!(m[0].a, "UNKNOWN (legacy)");
}

#[test]
fn neutrality_legs_compare_settings_not_the_binary() {
    let a = card("p");
    let mut b = card("p");
    b.binary.sha256 = known("candidate".into());
    assert_eq!(comparable(Purpose::NeutralityLegs, &a, &b), Ok(()));
    b.pipeline_settings.sets = known(vec!["library.matching=off".into()]);
    assert_eq!(
        fields(comparable(Purpose::NeutralityLegs, &a, &b)),
        ["pipelineSettings.sets"]
    );
}

#[test]
fn force_mixed_turns_the_refusal_into_loud_lines_naming_each_field() {
    let a = card("p");
    let mut b = card("p");
    b.base_mode = known(Some("seeded".into()));
    let refusal = guard(Purpose::Leaderboard, ("A", &a), ("B", &b), false).unwrap_err();
    assert!(
        refusal.contains("baseMode: A = \"scratch\" vs B = \"seeded\""),
        "{refusal}"
    );
    assert!(refusal.contains("--force-mixed"), "{refusal}");
    let lines = guard(Purpose::Leaderboard, ("A", &a), ("B", &b), true).unwrap();
    assert!(lines[0].starts_with("!! MIXED"), "{lines:?}");
    assert_eq!(
        guard(Purpose::Leaderboard, ("A", &a), ("A", &a), false),
        Ok(vec![])
    );
}

#[test]
fn an_identity_round_trips_through_its_file() {
    let mut a = card("p");
    a.scorer.version = F::unknown("legacy");
    let text = a.to_text();
    assert!(text.starts_with("{\n  \"schema\": 1,"), "{text}");
    assert!(text.contains("\"unknown\": \"legacy\""), "{text}");
    assert_eq!(Identity::from_text(&text).unwrap(), a);
}

fn label_of(entries: &[&str]) -> Vec<Identity> {
    entries.iter().map(|e| card(e)).collect()
}

const FOUR: &[&str] = &[
    "2.1.85->2.1.86",
    "2.1.118->2.1.119",
    "2.1.197->2.1.198",
    "2.1.215->2.1.216",
];
const FIVE: &[&str] = &[
    "2.1.85->2.1.86",
    "2.1.118->2.1.119",
    "2.1.179->2.1.181",
    "2.1.197->2.1.198",
    "2.1.215->2.1.216",
];

#[test]
fn a_four_pair_and_a_five_pair_label_compare_over_their_common_pairs_out_loud() {
    let (four, five) = (label_of(FOUR), label_of(FIVE));
    let s = guard_labels(Purpose::Leaderboard, ("OLD", &four), ("NEW", &five), false).unwrap();
    assert_eq!(s.common, FOUR, "the common pairs, in the baseline's order");
    assert!(s.lines[0].starts_with("!! COMMON PAIRS"), "{:?}", s.lines);
    let all = s.lines.join("\n");
    assert!(all.contains("only NEW: 2.1.179->2.1.181"), "{all}");
    assert!(all.contains("over the 4 common pairs"), "{all}");
    // The same sets: nothing to say.
    let same = guard_labels(Purpose::Leaderboard, ("A", &five), ("B", &five), false).unwrap();
    assert_eq!(same.common, FIVE);
    assert!(same.lines.is_empty(), "{:?}", same.lines);
}

#[test]
fn the_common_pairs_still_go_through_the_field_guard() {
    let four = label_of(FOUR);
    let mut five = label_of(FIVE);
    five[0].naming_mode = known(Some("off".into()));
    let err =
        guard_labels(Purpose::Leaderboard, ("OLD", &four), ("NEW", &five), false).unwrap_err();
    assert!(err.contains("namingMode"), "{err}");
    // A field on the pair only one side scored is never compared.
    let mut five = label_of(FIVE);
    five[2].naming_mode = known(Some("off".into()));
    assert!(guard_labels(Purpose::Leaderboard, ("OLD", &four), ("NEW", &five), false).is_ok());
}

#[test]
fn disjoint_pair_sets_are_refused_even_when_forced() {
    let a = label_of(&["2.1.85->2.1.86"]);
    let b = label_of(&["2.1.179->2.1.181"]);
    let err = guard_labels(Purpose::Leaderboard, ("A", &a), ("B", &b), true).unwrap_err();
    assert!(err.contains("no pair in common"), "{err}");
}

#[test]
fn bands_refuse_repeats_over_different_pair_sets_even_when_forced() {
    let (four, five) = (label_of(FOUR), label_of(FIVE));
    let err = guard_labels(Purpose::Bands, ("r1", &four), ("r2", &five), true).unwrap_err();
    assert!(err.contains("different pairs"), "{err}");
    assert!(err.contains("only r2: 2.1.179->2.1.181"), "{err}");
}
