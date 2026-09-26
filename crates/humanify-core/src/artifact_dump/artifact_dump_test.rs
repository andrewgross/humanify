//! The artifact dump writer's red tests.

use humanify_model::llm::CacheKeyParams;

use super::dispatch_rows;

/// `writePrompts` / `writeCacheKeys` join the rows with "\n" and add a
/// trailing "\n" — so a run that dispatched nothing writes ONE newline.
#[test]
fn no_dispatches_write_one_newline() {
    let params = CacheKeyParams {
        model: "m".into(),
        temperature: Some(0.0),
        max_tokens: None,
        reasoning_effort: None,
    };
    let (prompts, keys) = dispatch_rows(&[], &params);
    assert_eq!((prompts.as_str(), keys.as_str()), ("\n", "\n"));
}

/// A post-split reconcile row over `text` (the declaration of `a`).
pub(crate) fn post_split_row(text: &str) -> crate::trail::TrailEntry {
    use crate::trail::{Anchor, Attempt, Outcome, Tier, TrailEntry, TrailTarget};
    let start = text.find("a =").expect("a") as u32;
    TrailEntry {
        target: TrailTarget {
            anchor: Anchor::Generated,
            decl_span: oxc_span::Span::new(start, start + 1),
        },
        old_name: "a".into(),
        attempts: vec![Attempt::new(Tier::Reconcile, Outcome::Applied).proposed("count")],
        settled_by: Some(Tier::Reconcile),
        terminal_by: Some(Tier::Reconcile),
        final_name: Some("count".into()),
        post_settle_attempts: 0,
        post_settle_votes: 0,
    }
}

/// Finding #50: a post-split reconcile row indexes its SPLIT FILE, so its
/// key names that file (07 §1's tree-relative path key) and its span is
/// the row's own byte span in that file's text — never converted through
/// the generated text's table.
#[test]
fn post_split_rows_are_keyed_in_their_split_file() {
    use crate::naming::report::diagnostics::ExtraText;
    let text = "var é = 1;\nvar a = 2;\n";
    let start = text.find("a =").expect("a") as i64;
    let extra = vec![ExtraText {
        file: "src/tools/a.js".into(),
        text: text.into(),
        rows: vec![post_split_row(text)],
    }];
    let keys = super::extra_keys(&extra);
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].key.text, "src/tools/a.js");
    assert_eq!(
        (keys[0].key.start, keys[0].key.end),
        (start, start + 1),
        "the row's own UTF-8 byte span in its file"
    );
    assert_eq!(keys[0].loc, "2:4");
}

/// Finding #50, diag.json: the row's `declText` names its split file and
/// its `declSpan` is in that file's JS string units.
#[test]
fn post_split_trail_rows_name_their_split_file() {
    use crate::naming::report::diagnostics::{AnchorTexts, ExtraText, trail_report};
    use humanify_model::js::JsValue;
    let text = "var é = 1;\nvar a = 2;\n";
    let extra = vec![ExtraText {
        file: "src/tools/a.js".into(),
        text: text.into(),
        rows: vec![post_split_row(text)],
    }];
    let report = trail_report(
        &crate::trail::StrategyTrail::default(),
        &AnchorTexts {
            fresh: "",
            ..AnchorTexts::default()
        },
        &extra,
    );
    let JsValue::Object(report) = report else {
        panic!("an object")
    };
    let Some(JsValue::Array(trails)) = report.get("trails") else {
        panic!("trails")
    };
    let JsValue::Object(row) = &trails[0] else {
        panic!("a row")
    };
    assert_eq!(row.get("declText"), Some(&JsValue::str("src/tools/a.js")));
    let Some(JsValue::Object(span)) = row.get("declSpan") else {
        panic!("declSpan")
    };
    // `é` is 2 bytes / 1 unit: `a` sits at byte 16, JS index 15.
    assert_eq!(span.get("start"), Some(&JsValue::Number(15.0)));
    assert_eq!(row.get("loc"), Some(&JsValue::str("2:4")));
}
