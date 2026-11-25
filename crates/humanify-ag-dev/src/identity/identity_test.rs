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
