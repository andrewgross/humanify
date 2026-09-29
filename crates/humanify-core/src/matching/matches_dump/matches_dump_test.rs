//! The match stage's private-renames rows (16-findings #27): the twins'
//! private-name rewrites were recorded nowhere — no trail row, no
//! names.json row — so the only per-set record lives here.

use humanify_model::dump::SpanKey;

use super::private_renames_file;

/// One row per set: the old/new private names and every rewritten node's
/// fresh-text span, in source order — the shape a transfer gate reads.
#[test]
fn private_rename_sets_become_rows_in_source_order() {
    let sets = vec![
        crate::twins::gates::PrivateRenameSet {
            old_name: "f".into(),
            new_name: "A".into(),
            node_spans: vec![oxc_span::Span::new(10, 12), oxc_span::Span::new(30, 32)],
        },
        crate::twins::gates::PrivateRenameSet {
            old_name: "g".into(),
            new_name: "f".into(),
            node_spans: vec![oxc_span::Span::new(50, 52)],
        },
    ];
    let file = private_renames_file(&sets);
    assert_eq!(
        file.schema_version,
        humanify_model::dump::DUMP_SCHEMA_VERSION
    );
    assert_eq!(file.sets.len(), 2);
    assert_eq!(
        file.sets[0],
        humanify_model::dump::PrivateRenameRow {
            old_name: "f".into(),
            new_name: "A".into(),
            spans: vec![
                SpanKey {
                    text: "fresh".into(),
                    start: 10,
                    end: 12
                },
                SpanKey {
                    text: "fresh".into(),
                    start: 30,
                    end: 32
                },
            ],
        }
    );
    // The second set renames g -> f INDEPENDENTLY (in-order application:
    // reconcile_test's `#f`→`#A`, `#g`→`#f` pin) — both rows carry the
    // OLD name, not a chained result.
    assert_eq!(file.sets[1].old_name, "g");
    assert_eq!(file.sets[1].new_name, "f");
}
