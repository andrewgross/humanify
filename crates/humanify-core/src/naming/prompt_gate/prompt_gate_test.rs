//! The gate on its committed fixture (test/parity/wp42-gate-fixture/capture/, cut
//! from the 2.1.85→86 oracle dump + capture by
//! test/parity/wp42-make-gate-fixture.mjs): IDENTICAL as cut, and every
//! section FAILS when one byte of its TS-side expectation is planted
//! (the instrument must be able to fail).

use std::path::{Path, PathBuf};

use super::*;

fn fixture() -> PathBuf {
    PathBuf::from(format!(
        "{}/../../test/parity/wp42-gate-fixture/capture",
        env!("CARGO_MANIFEST_DIR")
    ))
}

fn run_fixture(root: &Path) -> PromptGateReport {
    run(&root.join("dump"), Some(&root.join("rows"))).unwrap()
}

#[test]
fn the_fixture_is_identical_and_exercises_every_path() {
    let report = run_fixture(&fixture());
    assert!(report.identical(), "{}", report.summary());
    let c = report.capture.as_ref().unwrap();
    assert!(report.first_round > 0 && report.retries > 0 && report.verbatim > 0);
    assert!(report.retry_bodies.compared > 0);
    for path in [
        "module-retry",
        "module-retry-body",
        "code-window-windowed",
        "cap-context-capped",
        "context-with-callees",
        "context-with-context-vars",
    ] {
        assert!(
            c.exercised.iter().any(|(p, n)| *p == path && *n > 0),
            "{path} not exercised: {}",
            report.summary()
        );
    }
}

/// Copy the fixture to a scratch dir with `file`'s FIRST occurrence of
/// `needle` replaced (a one-token plant in the TS-side expectation).
fn planted(file: &str, needle: &str, replacement: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wp42-gate-plant-{}-{}",
        std::process::id(),
        file.replace('/', "_")
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["dump", "rows"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        for entry in std::fs::read_dir(fixture().join(sub)).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.join(sub).join(entry.file_name())).unwrap();
        }
    }
    let path = dir.join(file);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(needle), "{file} lacks {needle:?}");
    std::fs::write(&path, text.replacen(needle, replacement, 1)).unwrap();
    dir
}

#[test]
fn a_planted_prompt_byte_fails_the_prompts_section() {
    let dir = planted(
        "dump/prompts.jsonl",
        "Identifiers to rename",
        "Identifiers to renamE",
    );
    let report = run_fixture(&dir);
    assert!(!report.user_prompts.is_identical());
    assert!(!report.identical());
}

#[test]
fn a_planted_retry_body_fails_the_body_section() {
    let dir = planted(
        "dump/cache-keys.jsonl",
        "Please suggest DIFFERENT names for these",
        "Please suggest DIFFERENT names for THESE",
    );
    let report = run(&dir.join("dump"), None).unwrap();
    assert!(!report.retry_bodies.is_identical(), "{}", report.summary());
}

#[test]
fn planted_capture_outputs_fail_their_sections() {
    let cases = [
        (
            "rows/module-builders.jsonl",
            "Identifier: ",
            "Identifier:  ",
        ),
        ("rows/code-window.jsonl", "omitted] …", "omitted] …."),
        (
            "rows/context.jsonl",
            "\"contextVars\":[\"",
            "\"contextVars\":[\" ",
        ),
    ];
    for (file, needle, replacement) in cases {
        let report = run_fixture(&planted(file, needle, replacement));
        let c = report.capture.as_ref().unwrap();
        let failed = match file {
            "rows/module-builders.jsonl" => !c.module_builder_calls.is_identical(),
            "rows/code-window.jsonl" => !c.code_window.is_identical(),
            _ => !c.context.is_identical(),
        };
        assert!(failed, "{file}: {}", report.summary());
    }
}

#[test]
fn a_capture_from_another_run_fails_the_key_sequence() {
    let dir = planted("rows/requests.jsonl", "\"cacheKey\":\"", "\"cacheKey\":\"0");
    let report = run_fixture(&dir);
    assert!(!report.capture.unwrap().key_sequence_matches);
}
