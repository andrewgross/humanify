//! The e2e stage's pure judgements (ported from scripts/e2e.ts): the
//! KNOWN-GAP contract and each fixture expectation of `fixture.config.json`
//! (`expect`), checked after every run step passed — so a fixture with a
//! known detection gap is still run, split and booted end to end first.
//!
//! - `bundler` — `humanify-ag detect`'s bundler verdict on the v1 input;
//! - `unpackAdapter` — the toolchain's unpack adapter for it;
//! - `vendor` — the fresh split tree's `vendor/` holds an extracted file;
//! - `splitMethod` — the fresh run's stats file `splitMethod.method`;
//! - `apart` — marker pairs that must sit in DIFFERENT files of `src/`;
//! - `assets` — app text asset files the fresh tree must hold (#90);
//! - `stays` — markers whose vendor/ file keeps its path across the
//!   release (#90);
//! - `placement` — markers that must sit in `src/` or `vendor/` and NOT on
//!   the other side (exp172).

use std::collections::BTreeMap;

use crate::committed::e2e::Expect;

/// A tree's files, `(relative path, text)`, in tree order.
pub type Files = Vec<(String, String)>;

/// A gap the pipeline HAS today, recorded by a fixture that exposes it.
/// The fixture runs like every other; it must then fail with a message
/// containing `error`. The entry is the contract, both ways
/// ([`judge_known_gap`]): a different failure fails the stage (the gap
/// moved), and a PASS fails it too (the gap is fixed: delete the entry so
/// the fixture gates as a passing case). Keyed by fixture.
#[derive(Clone, Copy, Debug)]
pub struct KnownGap {
    pub fixture: &'static str,
    /// The spec / review / finding item the gap is filed under.
    pub spec: &'static str,
    pub reason: &'static str,
    /// A substring of the failure, exact as the pipeline or harness says it.
    pub error: &'static str,
}

/// Today's known gaps: none (the last, spec I2, closed 2026-10-12).
pub const KNOWN_GAPS: &[KnownGap] = &[];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    KnownGap,
    Fail,
}

#[derive(Debug)]
pub struct Judged {
    pub verdict: Verdict,
    pub message: String,
}

/// `humanify-ag detect --toolchain`'s verdict on one input.
#[derive(Clone, Debug, Default)]
pub struct Detection {
    pub bundler: String,
    pub bundler_tier: Option<String>,
    pub minifier: Option<String>,
    pub unpack_adapter: String,
    pub bundle_layout: Option<String>,
}

/// What the fresh run did, as the expectations read it.
#[derive(Clone, Debug)]
pub struct ObservedRun {
    pub vendor_files: usize,
    pub split_method: Option<String>,
}

/// A value as a JS template literal prints it (`undefined` when absent).
pub fn or_undefined(v: &Option<String>) -> &str {
    v.as_deref().unwrap_or("undefined")
}

/// How a fixture's outcome (`failure`: None when it passed) reads against
/// its known-gap entry, if any.
pub fn judge_known_gap(gap: Option<&KnownGap>, failure: Option<&str>) -> Judged {
    let judged = |verdict, message: String| Judged { verdict, message };
    let Some(gap) = gap else {
        return match failure {
            None => judged(Verdict::Pass, String::new()),
            Some(f) => judged(Verdict::Fail, f.to_string()),
        };
    };
    let Some(failure) = failure else {
        return judged(
            Verdict::Fail,
            format!(
                "known gap \"{}\" ({}) now PASSES — the gap is fixed: delete its KNOWN_GAPS entry in \
                 crates/humanify-ag-dev/src/e2e/expect.rs so the fixture gates as a passing case",
                gap.fixture, gap.spec
            ),
        );
    };
    if !failure.contains(gap.error) {
        return judged(
            Verdict::Fail,
            format!(
                "known gap \"{}\" ({}) changed: expected a failure containing {:?}, got:\n{failure}",
                gap.fixture, gap.spec, gap.error
            ),
        );
    }
    let quoted = failure
        .split('\n')
        .find(|l| l.contains(gap.error))
        .unwrap_or(gap.error);
    judged(
        Verdict::KnownGap,
        format!(
            "KNOWN GAP {} ({}): {}\n    fails as declared: {}",
            gap.fixture,
            gap.spec,
            gap.reason,
            quoted.trim()
        ),
    )
}

/// KNOWN_GAPS entries that name no fixture, or a fixture twice.
pub fn stale_known_gaps(gaps: &[KnownGap], fixtures: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for g in gaps {
        if seen.contains(&g.fixture) {
            out.push(format!("KNOWN_GAPS declares \"{}\" twice", g.fixture));
        } else if !fixtures.iter().any(|f| f == g.fixture) {
            out.push(format!(
                "KNOWN_GAPS entry \"{}\" names no fixture",
                g.fixture
            ));
        }
        seen.push(g.fixture);
    }
    out
}

/// Each detection / toolchain / split expectation the run missed.
pub fn expectation_failures(
    expect: Option<&Expect>,
    observed: &Detection,
    run: &ObservedRun,
) -> Vec<String> {
    let mut out = Vec::new();
    let Some(e) = expect else {
        return out;
    };
    if let Some(b) = &e.bundler
        && observed.bundler != *b
    {
        out.push(format!(
            "detection: bundler is {}, expected {b}",
            observed.bundler
        ));
    }
    if let Some(a) = &e.unpack_adapter
        && observed.unpack_adapter != *a
    {
        out.push(format!(
            "toolchain: unpack adapter is {}, expected {a}",
            observed.unpack_adapter
        ));
    }
    if e.vendor == Some(true) && run.vendor_files == 0 {
        out.push(
            "unpack: the split tree's vendor/ is empty, expected the extracted dependencies".into(),
        );
    }
    if let Some(m) = &e.split_method
        && run.split_method.as_ref() != Some(m)
    {
        out.push(format!(
            "split: the fresh run's split method is {}, expected {m}",
            or_undefined(&run.split_method)
        ));
    }
    out
}

/// The files holding `marker`, in tree order.
fn holders<'a>(files: &'a Files, marker: &str) -> Vec<&'a str> {
    files
        .iter()
        .filter(|(_, text)| text.contains(marker))
        .map(|(f, _)| f.as_str())
        .collect()
}

/// The first file holding `marker`.
fn holder<'a>(files: &'a Files, marker: &str) -> Option<&'a str> {
    holders(files, marker).first().copied()
}

/// Each `apart` pair that is NOT in separate files.
pub fn apart_failures(pairs: &[Vec<String>], files: &Files) -> Vec<String> {
    let mut out = Vec::new();
    for pair in pairs {
        let (a, b) = (
            pair.first().map_or("", String::as_str),
            pair.get(1).map_or("", String::as_str),
        );
        let (in_a, in_b) = (holders(files, a), holders(files, b));
        let mut missing = false;
        for (m, hits) in [(a, &in_a), (b, &in_b)] {
            if hits.is_empty() {
                out.push(format!("split: no file holds \"{m}\""));
                missing = true;
            }
        }
        if missing {
            continue;
        }
        if let Some(shared) = in_a.iter().find(|f| in_b.contains(f)) {
            out.push(format!(
                "split: \"{a}\" and \"{b}\" share {shared}, expected separate files"
            ));
        }
    }
    out
}

/// Each expected app text asset the tree's file list lacks.
pub fn asset_failures(assets: &[String], files: &[String]) -> Vec<String> {
    assets
        .iter()
        .filter(|a| !files.contains(a))
        .map(|a| format!("unpack: no app text asset {a}"))
        .collect()
}

/// Each `placement` marker not found on its side (`src`, `vendor`).
pub fn placement_failures(
    placement: &BTreeMap<String, Vec<String>>,
    src: &Files,
    vendor: &Files,
) -> Vec<String> {
    let mut out = Vec::new();
    for (side, want, home, away) in [
        ("src", "src/", src, vendor),
        ("vendor", "vendor/", vendor, src),
    ] {
        for m in placement.get(side).into_iter().flatten() {
            if let Some(wrong) = holder(away, m) {
                out.push(format!("placement: \"{m}\" is in {wrong}, expected {want}"));
            } else if holder(home, m).is_none() {
                out.push(format!("placement: no file holds \"{m}\""));
            }
        }
    }
    out
}

/// Each `stays` marker whose vendor file moved between the two releases.
pub fn stays_failures(markers: &[String], fresh: &Files, prior: &Files) -> Vec<String> {
    let mut out = Vec::new();
    for m in markers {
        match (holder(fresh, m), holder(prior, m)) {
            (Some(a), Some(b)) if a != b => out.push(format!(
                "unpack: \"{m}\" moved from {a} to {b} across the release"
            )),
            (Some(_), Some(_)) => {}
            _ => out.push(format!(
                "unpack: no vendor file holds \"{m}\" in both releases"
            )),
        }
    }
    out
}

#[cfg(test)]
#[path = "expect_test.rs"]
mod expect_test;
