//! The finish driver's SPAN coverage (docs/perf-inventory.md item 3):
//! `split:finish` was 40 s of serial work per with-prior hop with NO
//! internal spans — relink vs the reconcile's ledger read vs the
//! per-file reconcile vs the apply vs the carry was not measurable. The
//! reconcile+carry half is pinned here over the wp54 fixture's tree
//! materialized on disk, through the same disk-I/O driver
//! ([`super::reconcile_post_split`]) the pipeline calls; the relink half
//! needs a Bun manifest and is covered by the pipeline-level test
//! (crates/humanify-cli/tests/pipeline_stages.rs).

use std::path::PathBuf;

use humanify_model::js::{JsValue, stringify};

use super::{FinishReport, FinishSwitches, reconcile_post_split};

/// One scratch tree: (fresh output tree, prior tree) with the fixture's
/// files materialized where the drivers read them.
struct Scratch(PathBuf, PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let root = std::env::temp_dir().join(format!(
            "humanify-finish-spans-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let out = root.join("out");
        let prior = root.join("prior");
        std::fs::create_dir_all(out.join(".humanify")).unwrap();
        std::fs::create_dir_all(prior.join(".humanify")).unwrap();
        Scratch(out, prior)
    }

    fn write(&self, root: &std::path::Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}

#[test]
fn the_finish_reconcile_records_its_constituent_spans() {
    let _guard = crate::profiling::SPAN_TEST_LOCK.lock().unwrap();
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/wp54-postsplit.json");
    let raw = std::fs::read_to_string(&path).unwrap();
    let fx: serde_json::Value = serde_json::from_str(&raw).unwrap();
    // The ledger comes through the order-preserving JS value (serde_json
    // alphabetizes its maps — the ledger's key order is the output's).
    let ledger = match JsValue::parse(&raw).unwrap() {
        JsValue::Object(o) => o.get("ledgerIn").unwrap().clone(),
        _ => unreachable!("the fixture's top level is an object"),
    };
    let s = Scratch::new("wp54");
    s.write(&s.0, ".humanify/split-ledger.json", &stringify(&ledger));
    s.write(
        &s.0,
        ".humanify/humanified.js",
        fx["bundle"].as_str().unwrap(),
    );
    for (file, text) in fx["fresh"].as_object().unwrap() {
        s.write(&s.0, file, text.as_str().unwrap());
    }
    for (file, text) in fx["prior"].as_object().unwrap() {
        s.write(&s.1, file, text.as_str().unwrap());
    }
    let prior_bundle = s.1.join(".humanify/humanified.js");
    std::fs::write(&prior_bundle, fx["bundle"].as_str().unwrap()).unwrap();

    let profiler = crate::profiling::Profiler::new(true);
    profiler.install_global();
    let mut report = FinishReport::default();
    let result = reconcile_post_split(
        &s.0,
        Some(&prior_bundle),
        FinishSwitches::default(),
        &mut report,
    )
    .expect("the reconcile runs");
    let profile = profiler.finalize(None);
    crate::profiling::Profiler::uninstall_global();

    // The fixture's regime: one file considered, one changed, renames to
    // carry — so the apply and the carry really ran.
    let result = result.expect("a with-prior reconcile");
    assert_eq!(
        result.result.stats.changed, 1,
        "the fixture changes one file"
    );
    assert!(
        !result.result.renames.is_empty(),
        "the fixture restores names"
    );
    assert!(result.carry.is_some(), "the carry ran");

    let names: Vec<&str> = profile
        .spans
        .iter()
        .map(|s| s.name.as_str())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    for expected in [
        "split:finish:reconcile-ledger",
        "split:finish:reconcile",
        "split:finish:reconcile-apply",
        "split:finish:carry",
    ] {
        assert!(
            names.contains(&expected),
            "span {expected} missing: {names:?}"
        );
    }
    // The observation-only guard: the spans changed no outcome — the
    // report still carries the restore line (the pipeline prints these).
    assert!(
        report
            .messages
            .iter()
            .any(|m| m.contains("restored") && m.contains("prior name")),
        "the messages survive: {:?}",
        report.messages
    );
}
