//! The artifact dump writer's red tests.

use humanify_ag_model::llm::CacheKeyParams;
use serde_json::{Value, json};

use super::{Dispatch, DumpSink, dispatch_rows};

/// A section is its records exactly as serde writes them, in `<name>.json`.
#[test]
fn a_section_is_written_as_its_records() {
    #[derive(serde::Serialize)]
    struct Row {
        tier: &'static str,
        count: u32,
    }
    let dir = humanify_ag_testkit::ScratchDir::new("dump-sink");
    let sink = DumpSink::create(&dir.path().join("dump")).expect("the sink");
    sink.section(
        "rows",
        &[Row {
            tier: "hash",
            count: 3,
        }],
    )
    .expect("written");
    let text = std::fs::read_to_string(dir.path().join("dump/rows.json")).expect("rows.json");
    let value: Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(value, json!([{"tier": "hash", "count": 3}]));
}

/// A run that dispatched nothing writes ONE newline.
#[test]
fn no_dispatches_write_one_newline() {
    let params = CacheKeyParams {
        model: "m".into(),
        temperature: Some(0.0),
        max_tokens: None,
        reasoning_effort: None,
    };
    assert_eq!(dispatch_rows(&[], &params), "\n");
}

/// `--dump-asks` fixtures: the shared record builders and row accessors
/// (one row per dispatch — the ask-trace taxonomy of `naming::ask_trace`).
use crate::naming::ask_trace::AskSite;
use crate::naming::waves::processor::DispatchRecord;

/// The bulk ask rows, as JSON values.
fn ask_rows(dispatches: &[Dispatch<'_>]) -> Vec<Value> {
    super::ask_rows(dispatches)
        .iter()
        .map(|r| serde_json::to_value(r).expect("a row"))
        .collect()
}

fn ask_record(id: &str, ids: &[&str], retry: bool, ask: AskSite) -> DispatchRecord {
    let mut request = humanify_ag_model::llm::BatchRenameRequest {
        code: "function f(a, b){}".into(),
        identifiers: ids.iter().map(|s| s.to_string()).collect(),
        used_names: vec!["one".into(), "two".into()],
        system_prompt: None,
        user_prompt: None,
        ..humanify_ag_model::llm::BatchRenameRequest::default()
    };
    if retry {
        request.is_retry = Some(true);
        request.failures = Some(humanify_ag_model::llm::RenameFailures {
            duplicates: vec!["a".into()],
            ..Default::default()
        });
        request.prior_version_code = Some("function f(a){}".into());
    }
    DispatchRecord {
        seq: 0,
        function_id: id.to_string(),
        round: 2,
        wave: 3,
        request,
        cache_key: String::new(),
        system_prompt: String::new(),
        user_prompt: String::new(),
        targets: Vec::new(),
        ask,
    }
}

fn ask_sweep() -> crate::naming::passes::sweep::SweepDispatch {
    crate::naming::passes::sweep::SweepDispatch {
        request: humanify_ag_model::llm::BatchRenameRequest::default(),
        system_prompt: String::new(),
        user_prompt: String::new(),
        cache_key: String::new(),
        targets: Vec::new(),
        ask: AskSite::fresh(0),
    }
}

fn ask_vendor_call() -> humanify_ag_model::llm::LlmCall {
    humanify_ag_model::llm::LlmCall {
        request: humanify_ag_model::llm::BatchRenameRequest {
            identifiers: vec!["lib_ab12".into()],
            user_prompt: Some("name these".into()),
            ..humanify_ag_model::llm::BatchRenameRequest::default()
        },
        system_prompt: String::new(),
        user_prompt: String::new(),
        site: humanify_ag_model::llm::CallSite::Naming,
    }
}

fn str_of<'a>(row: &'a Value, k: &str) -> &'a str {
    row[k]
        .as_str()
        .unwrap_or_else(|| panic!("no string field {k}: {row:?}"))
}

fn num_of(row: &Value, k: &str) -> f64 {
    row[k]
        .as_f64()
        .unwrap_or_else(|| panic!("no number field {k}: {row:?}"))
}

fn bool_of(row: &Value, k: &str) -> bool {
    row[k]
        .as_bool()
        .unwrap_or_else(|| panic!("no bool field {k}: {row:?}"))
}

/// A first-round ask (fn or module lane): reason and variant by site,
/// identifiers, the avoid-list size — and no cause.
#[test]
fn fresh_and_module_ask_rows_read_by_site() {
    let fresh = ask_record("fn-1:1", &["a", "b"], false, AskSite::fresh(0));
    let module = ask_record(
        "module-binding-batch:e0,q",
        &["e0", "q"],
        false,
        AskSite {
            prior: true,
            ..AskSite::fresh(0)
        },
    );
    let rows = ask_rows(&[Dispatch::Naming(&fresh), Dispatch::Naming(&module)]);
    assert_eq!(str_of(&rows[0], "reason"), "fresh");
    assert_eq!(str_of(&rows[0], "site"), "naming");
    assert_eq!(str_of(&rows[0], "scope"), "fn-1:1");
    assert_eq!(str_of(&rows[0], "scopeKind"), "fn");
    assert_eq!(str_of(&rows[0], "promptVariant"), "batch");
    assert!(!bool_of(&rows[0], "isRetry"));
    assert!(rows[0].as_object().unwrap().get("retryCause").is_none());
    assert_eq!(num_of(&rows[0], "usedNamesCount"), 2.0);
    assert_eq!(rows[0]["identifiers"], json!(["a", "b"]));
    // A module lane: its prior suggestions live in the PROMPT text, so the
    // site's flag is what the row reads.
    assert_eq!(str_of(&rows[1], "reason"), "module-lane");
    assert_eq!(str_of(&rows[1], "scopeKind"), "module");
    assert!(bool_of(&rows[1], "priorContext"), "the site's flag");
    assert_eq!(str_of(&rows[1], "promptVariant"), "module");
}

/// A re-ask row: the barrier seed's RECORDED cause wins (its request's
/// failures were fabricated `duplicates`; the detail names the applier's
/// code) — a lane's round-2 cause is DERIVED from `failures`.
#[test]
fn reask_rows_carry_the_recorded_cause_or_derive_it() {
    let barrier_reask = ask_record(
        "fn-1:3",
        &["a"],
        true,
        AskSite::reask(
            crate::naming::reask::ReaskClass::NameTaken,
            0,
            Some("target-free-name"),
        ),
    );
    let lane_reask = ask_record("fn-2", &["c"], true, AskSite::fresh(1));
    let rows = ask_rows(&[
        Dispatch::Naming(&barrier_reask),
        Dispatch::Naming(&lane_reask),
    ]);
    assert_eq!(str_of(&rows[0], "reason"), "retry");
    assert_eq!(str_of(&rows[0], "retryCause"), "NameTaken");
    assert_eq!(str_of(&rows[0], "retryCauseDetail"), "target-free-name");
    assert!(bool_of(&rows[0], "priorContext"));
    assert_eq!(str_of(&rows[0], "promptVariant"), "batch-retry");
    assert_eq!(num_of(&rows[0], "wave"), 3.0);
    assert_eq!(num_of(&rows[0], "round"), 2.0);
    assert_eq!(str_of(&rows[1], "reason"), "retry");
    assert_eq!(str_of(&rows[1], "retryCause"), "NameTaken");
    assert_eq!(num_of(&rows[1], "phase"), 1.0);
}

/// The single-call sites (the coverage sweep, the vendor namer) read as
/// their own reasons, with their own scope kinds — no wave, no phase.
#[test]
fn sweep_and_vendor_rows_read_by_site() {
    let sweep = ask_sweep();
    let vendor = ask_vendor_call();
    let rows = ask_rows(&[
        Dispatch::Sweep(crate::trail::Anchor::Generated, &sweep),
        Dispatch::Plain {
            function_id: "vendor-namer",
            site: "vendor",
            call: &vendor,
        },
    ]);
    assert_eq!(str_of(&rows[0], "reason"), "sweep");
    assert_eq!(str_of(&rows[0], "site"), "sweep");
    assert_eq!(str_of(&rows[0], "scopeKind"), "sweep");
    assert!(rows[0].as_object().unwrap().get("wave").is_none());
    assert_eq!(str_of(&rows[1], "reason"), "vendor");
    assert_eq!(str_of(&rows[1], "scopeKind"), "vendor");
    assert_eq!(str_of(&rows[1], "promptVariant"), "vendor");
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

/// A post-split reconcile row indexes its SPLIT FILE, so its key names
/// that file and its span is the row's own byte span in that file's text
/// — not converted through the generated text's table.
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
    let rows: Vec<_> = super::extra_rows(&extra).collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target.text, "src/tools/a.js");
    assert_eq!(
        (rows[0].target.start, rows[0].target.end),
        (start, start + 1),
        "the row's own UTF-8 byte span in its file"
    );
}

/// Finding #50, diag.json: the row's `declText` names its split file and
/// its `declSpan` is in that file's JS string units.
#[test]
fn post_split_trail_rows_name_their_split_file() {
    use crate::naming::report::diagnostics::{AnchorTexts, ExtraText, trail_report};
    use humanify_ag_model::js::JsValue;
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
