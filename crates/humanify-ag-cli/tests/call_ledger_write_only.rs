//! The call ledger is RECORDING ONLY (exp206): timing, completion order
//! and the answer verdicts must never reach a decision or the emitted
//! tree. That holds by construction when nothing that decides can read
//! it, so this test pins the construction:
//!
//! - the pipeline crate (humanify-ag-core) never names the ledger — its
//!   namers see only `NameProvider`, which carries no ledger;
//! - the ledger's READ methods (`stats`, `coverage_llm`, `failed`) are
//!   called in exactly one place outside tests: the CLI's `--stats-json`
//!   writer, after the run has decided everything.
//!
//! The byte-level proof is `cargo dev eval neutrality` (warm) and the e2e
//! determinism double-run; this is the guard that keeps them meaningful.

use std::path::{Path, PathBuf};

fn crates_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn is_test_file(path: &Path) -> bool {
    let name = path.file_name().unwrap().to_string_lossy();
    name.ends_with("_test.rs") || path.components().any(|c| c.as_os_str() == "tests")
}

/// Every non-test source file under `crates/`, with its text.
fn sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    rust_files(&crates_dir(), &mut files);
    files
        .into_iter()
        .filter(|p| !is_test_file(p))
        .map(|p| {
            let text = std::fs::read_to_string(&p).unwrap();
            (p, text)
        })
        .collect()
}

#[test]
fn the_pipeline_crate_never_names_the_call_ledger() {
    let core = crates_dir().join("humanify-ag-core");
    let offenders: Vec<String> = sources()
        .into_iter()
        .filter(|(p, _)| p.starts_with(&core))
        .filter(|(_, t)| t.contains("call_ledger") || t.contains("CallLedger"))
        .map(|(p, _)| p.display().to_string())
        .collect();
    assert!(offenders.is_empty(), "core reads the ledger: {offenders:?}");
}

#[test]
fn the_ledger_is_read_only_by_the_stats_writer() {
    let reads = [".coverage_llm()", "ledger.stats()", "ledger.failed("];
    let readers: Vec<String> = sources()
        .into_iter()
        .filter(|(p, _)| !p.ends_with("humanify-ag-model/src/call_ledger.rs"))
        .filter(|(_, t)| reads.iter().any(|r| t.contains(r)))
        .map(|(p, _)| p.strip_prefix(crates_dir()).unwrap().display().to_string())
        .collect();
    assert_eq!(readers, vec!["humanify-ag-cli/src/unified.rs".to_string()]);
    let unified =
        std::fs::read_to_string(crates_dir().join("humanify-ag-cli/src/unified.rs")).unwrap();
    let writer = unified
        .find("fn write_stats_json(")
        .expect("the stats writer");
    let body_end = unified[writer..].find("\n}\n").map(|i| writer + i).unwrap();
    for read in reads {
        for (at, _) in unified.match_indices(read) {
            assert!(
                (writer..body_end).contains(&at),
                "`{read}` read outside write_stats_json at byte {at}"
            );
        }
    }
}
