//! The finish driver's tests: the SPAN coverage (perf-instrumentation)
//! and the manifest gate (feat/esbuild-unpack).

//! The finish driver's SPAN coverage (docs/perf-inventory.md item 3):
//! `split:finish` was 40 s of serial work per with-prior hop with NO
//! internal spans — relink vs the reconcile's ledger read vs the
//! per-file reconcile vs the apply vs the carry was not measurable. The
//! reconcile+carry half is pinned here over the wp54 fixture's tree
//! materialized on disk, through the same disk-I/O driver
//! ([`super::reconcile_post_split`]) the pipeline calls; the relink half
//! needs a Bun manifest and is covered by the pipeline-level test
//! (crates/humanify-cli/tests/pipeline_stages.rs).

//! The manifest gate's doc:
//! The finish driver's manifest gate: `loadBunManifest` reads the vendor
//! manifest BOTH bundler adapters write (the format is shared; the esbuild
//! adapter advertises the runnable split the same way bun's does).

use std::fs;
use std::path::{Path, PathBuf};

use humanify_model::js::{JsValue, stringify};

use super::{FinishReport, FinishSwitches, load_bun_manifest, reconcile_post_split};
use crate::unpack::bun::bun_manifest_path;

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

// ---- the manifest gate's tests (feat/esbuild-unpack) ----

struct TempDir(PathBuf);
impl TempDir {
    fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "humanify-finish-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(dir.join("vendor")).unwrap();
        TempDir(dir)
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}
/// Write a manifest with the given adapter stamp and one factory.
fn manifest(dir: &Path, adapter: &str) {
    fs::write(
        bun_manifest_path(dir),
        format!(
            "{{\"adapter\":\"{adapter}\",\"hashVersion\":3,\"runtimeFile\":\"runtime.js\",\
             \"factories\":[{{\"fileName\":\"vendor/lib_aaaaaaaa.js\",\"name\":\"lib_aaaaaaaa\",\
             \"nameSource\":\"fallback\",\"structuralHash\":\"aaaaaaaaaaaaaaaa\",\
             \"runtimeIdentifier\":\"lib_aaaaaaaa\"}}]}}\n"
        ),
    )
    .unwrap();
}
#[test]
fn the_manifest_gate_accepts_both_bundler_adapters() {
    let bun_dir = TempDir::new("bun");
    manifest(&bun_dir.0, "bun");
    let bun = load_bun_manifest(&bun_dir.0)
        .expect("reads")
        .expect("bun manifest loads");
    assert_eq!(bun.factories.len(), 1);
    // The esbuild adapter writes the same format under its own stamp —
    // the relink machinery is shared (exp075's port).
    let esbuild_dir = TempDir::new("esbuild");
    manifest(&esbuild_dir.0, "esbuild");
    let esbuild = load_bun_manifest(&esbuild_dir.0)
        .expect("reads")
        .expect("the esbuild manifest loads the same way");
    assert_eq!(esbuild.factories.len(), 1);
    assert_eq!(esbuild.runtime_file.as_deref(), Some("runtime.js"));
}
#[test]
fn unknown_adapter_stamps_still_decline() {
    let dir = TempDir::new("other");
    manifest(&dir.0, "webcrack");
    assert!(load_bun_manifest(&dir.0).expect("reads").is_none());
}
