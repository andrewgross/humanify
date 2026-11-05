//! The barrel rule on real barrels of the run-2 eval trees (finding #94).

use super::{evidence_words, rule_candidates, shared_phrase};

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn a_barrel_takes_the_words_its_loaded_modules_share() {
    for (loaded, want) in [
        (&["theme-modal", "theme-picker", "theme-store"][..], "theme"),
        (
            &[
                "event-stream-middleware",
                "event-stream-header-middleware",
                "event-stream-plugin",
            ][..],
            "event-stream",
        ),
        (
            &["wizard-context", "wizard-renderer", "wizard-hook"][..],
            "wizard",
        ),
        (
            &[
                "diff-tokenizer-base",
                "line-diff-tokenizer",
                "no-op-tokenizer-module",
                "diff-patch-generator",
            ][..],
            "diff",
        ),
        (
            &["confirm-prompt", "input-prompt", "select-prompt"][..],
            "prompts",
        ),
        (&["base64-utils", "base64-encoder"][..], "base64"),
        (&["error-classes"][..], "error-classes"),
    ] {
        assert_eq!(
            shared_phrase(&names(loaded)).as_deref(),
            Some(want),
            "{loaded:?}"
        );
    }
}

#[test]
fn a_barrel_whose_modules_share_nothing_has_no_rule_name() {
    for loaded in [
        &[
            "session-utils",
            "attribution-state",
            "tool-permission-context",
        ][..],
        &["msal-constants", "performance-measurement"][..],
        // Half is not a majority.
        &[
            "ansi-code-utils",
            "token-merger",
            "token-sequence-processor",
            "ansi-parser",
        ][..],
        // Nothing but filler and counters.
        &["do-nothing13", "initialize-module", "setup-modules5"][..],
    ] {
        assert_eq!(shared_phrase(&names(loaded)), None, "{loaded:?}");
    }
}

#[test]
fn filler_and_a_loaded_barrels_index_are_not_evidence() {
    assert_eq!(evidence_words("setup-modules-once-var"), None);
    assert_eq!(evidence_words("module-gamma"), None);
    assert_eq!(
        evidence_words("otlp-log-exporter-index"),
        Some(names(&["otlp", "log", "exporter"]))
    );
    assert_eq!(
        shared_phrase(&names(&["otlp-log-exporter-index", "initialize-module"])).as_deref(),
        Some("otlp-log-exporter")
    );
    assert_eq!(
        rule_candidates("error-classes"),
        names(&[
            "error-classes",
            "error-classes-index",
            "error-classes-exports"
        ])
    );
    assert_eq!(super::base_name("otlp-exporter-exports"), "otlp-exporter");
    assert_eq!(super::base_name("index"), "index");
}
