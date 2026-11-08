//! The clone census is a review ratchet: a cross-file twin among non-test
//! functions exits nonzero unless allowlisted. Both directions are proven
//! on planted trees (a detector whose red was never observed is a zero
//! nobody validated) — the port of test/clone-census.test.ts.

use std::path::Path;

use super::*;
use crate::census::rust_fns::rust_fns_test::plant;

const TWIN: &str = r#"
pub fn collect_retry_delays(attempts: &[u32], base: u64) -> Vec<u64> {
    let mut out = Vec::new();
    for (i, a) in attempts.iter().enumerate() {
        let factor = if *a > 3 { 4 } else { 2 };
        let delay = base.saturating_mul(factor).saturating_add(i as u64);
        if delay > 10_000 {
            out.push(10_000);
            continue;
        }
        out.push(delay);
    }
    out.sort_unstable();
    out.dedup();
    out.retain(|d| *d != 7);
    out
}
"#;

/// TWIN with every binding renamed, the function too.
const RENAMED: &str = r#"
pub fn retry_schedule(tries: &[u32], start: u64) -> Vec<u64> {
    let mut acc = Vec::new();
    for (n, t) in tries.iter().enumerate() {
        let mult = if *t > 3 { 4 } else { 2 };
        let wait = start.saturating_mul(mult).saturating_add(n as u64);
        if wait > 10_000 {
            acc.push(10_000);
            continue;
        }
        acc.push(wait);
    }
    acc.sort_unstable();
    acc.dedup();
    acc.retain(|w| *w != 7);
    acc
}
"#;

/// A second, unrelated function of TWIN's size.
const OTHER: &str = r#"
pub fn summarize_lines(text: &str, width: usize) -> String {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.lines() {
        let trimmed = raw.trim_end_matches(['\r', ' ']);
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let cut: String = trimmed.chars().take(width).collect();
        lines.push(format!("{cut:<width$}|"));
    }
    lines.sort();
    lines.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let total = lines.len();
    lines.push(format!("({total} lines)"));
    lines.join("\n")
}
"#;

const EMPTY_ALLOWLIST: &str = r#"{"$comment": "test", "entries": []}"#;
const ALLOW: &str = "dev/census/clone-allowlist.json";

fn run_on(root: &Path, loose: bool) -> (u8, String) {
    let (code, out, err) = census(root, loose);
    (code, format!("{out}\n{err}"))
}

#[test]
fn goes_red_on_a_planted_cross_file_twin_and_names_the_group() {
    let root = plant(
        "clones-red",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/k/src/beta.rs", TWIN),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 1, "expected REVIEW:\n{out}");
    assert!(
        out.contains("CENSUS: 1 unreviewed potential-duplication"),
        "{out}"
    );
    assert!(
        out.contains("crates/k/src/alpha.rs:2  collect_retry_delays"),
        "{out}"
    );
    assert!(
        out.contains("crates/k/src/beta.rs:2  collect_retry_delays"),
        "{out}"
    );
}

#[test]
fn finds_a_twin_whose_identifiers_were_renamed() {
    let root = plant(
        "clones-renamed",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/j/src/beta.rs", RENAMED),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("crates/j/src/beta.rs:2  retry_schedule"),
        "{out}"
    );
}

#[test]
fn an_allowlisted_group_is_accepted_and_a_stale_entry_is_informational() {
    let allow = r#"{"$comment": "test", "entries": [
      {"members": ["crates/k/src/alpha.rs:collect_retry_delays", "crates/k/src/beta.rs:collect_retry_delays"],
       "justification": "planted"},
      {"members": ["crates/k/src/gone.rs:f", "crates/k/src/gone2.rs:f"], "justification": "stale"}
    ]}"#;
    let root = plant(
        "clones-allowed",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/k/src/beta.rs", TWIN),
            (ALLOW, allow),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("CENSUS GREEN"), "{out}");
    assert!(out.contains("accepted: planted"), "{out}");
    assert!(out.contains("stale allowlist entries"), "{out}");
}

/// TWIN with one statement inserted and its last one changed: not an
/// identical stream, but it shares a run of ~100 masked tokens with TWIN.
fn near() -> String {
    TWIN.replace(
        "out.sort_unstable();",
        "out.truncate(64);\n    out.sort_unstable();",
    )
    .replace("*d != 7", "*d % 2 == 0")
}

#[test]
fn is_green_without_twins_and_keeps_test_only_twins_informational() {
    let small = "pub fn id(x: u32) -> u32 { x }\n";
    let test_twin = OTHER;
    let test_near = OTHER
        .replace("lines.sort();", "lines.reverse();\n    lines.sort();")
        .replace("({total} lines)", "[{total}]");
    let root = plant(
        "clones-clean",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/k/src/beta.rs", small),
            ("crates/k/src/gamma.rs", small),
            ("crates/k/src/a_test.rs", test_twin),
            ("crates/j/tests/b.rs", test_twin),
            ("crates/j/tests/c.rs", &test_near),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("CENSUS GREEN"), "{out}");
    let tests = out.split("test code only").nth(1).unwrap_or("");
    assert!(
        tests.contains("crates/j/tests/b.rs"),
        "test-only group listed:\n{out}"
    );
    assert!(
        tests.contains("crates/j/tests/c.rs"),
        "test-only shared run listed:\n{out}"
    );
    assert!(
        !out.contains("  id"),
        "below the cutoff, never grouped:\n{out}"
    );
}

#[test]
fn a_same_file_twin_wants_review_too() {
    let two_in_one = format!("{TWIN}\n{RENAMED}");
    let root = plant(
        "clones-same-file",
        &[
            ("crates/k/src/alpha.rs", &two_in_one),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("2 members in 1 file(s)"), "{out}");
}

#[test]
fn a_near_twin_sharing_a_long_run_wants_review() {
    let root = plant(
        "clones-near",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/k/src/beta.rs", &near()),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("shared run"), "{out}");
    assert!(
        out.contains("crates/k/src/beta.rs:2  collect_retry_delays"),
        "{out}"
    );
    let short = TWIN.replace("out.push(delay);", "out.push(delay + 1);");
    let root = plant(
        "clones-near-short",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            (
                "crates/k/src/beta.rs",
                &short.replace("let mut out", "let mut out: Vec<u64>"),
            ),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 0, "runs broken every < 80 tokens do not pair:\n{out}");
}

#[test]
fn loose_review_groups_literal_variants_and_always_exits_zero() {
    let variant = TWIN.replace("10_000", "20_000");
    let root = plant(
        "clones-loose",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/k/src/beta.rs", &variant),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    let (code, out) = run_on(&root, false);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("shared run") && !out.contains("identical,"),
        "literals differ: a shared run, not an identical group:\n{out}"
    );
    let (code, out) = run_on(&root, true);
    assert_eq!(code, 0, "{out}");
    let shape = out
        .split("same shape, different literals")
        .nth(1)
        .unwrap_or("");
    assert!(shape.contains("crates/k/src/beta.rs"), "{out}");
    let red = plant(
        "clones-loose-red",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            ("crates/k/src/beta.rs", TWIN),
            (ALLOW, EMPTY_ALLOWLIST),
        ],
    );
    assert_eq!(run_on(&red, true).0, 0, "loose never fails");
}

#[test]
fn a_missing_or_malformed_allowlist_is_an_error() {
    let missing = plant("clones-noallow", &[("crates/k/src/alpha.rs", TWIN)]);
    assert_eq!(run_on(&missing, false).0, 2);
    let typo = plant(
        "clones-typo",
        &[
            ("crates/k/src/alpha.rs", TWIN),
            (
                ALLOW,
                r#"{"$comment": "", "entries": [{"members": [], "justfication": "x"}]}"#,
            ),
        ],
    );
    let (code, out) = run_on(&typo, false);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("unknown field"), "{out}");
}

#[test]
fn the_committed_allowlist_loads() {
    let a = crate::committed::census::clone_allowlist(&crate::paths::repo_root()).unwrap();
    for e in &a.value.entries {
        let mut sorted = e.members.clone();
        sorted.sort();
        assert_eq!(sorted, e.members, "members are sorted");
        assert!(!e.justification.trim().is_empty());
    }
}
