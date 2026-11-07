//! The slop census: mass, erosion and clone share per crate, and their
//! delta against a git base read from blobs.

use super::*;
use crate::census::rust_fns::Source;
use crate::census::rust_fns::rust_fns_test::plant;

fn src(rel: &str, text: &str) -> Source {
    Source {
        rel: rel.to_string(),
        text: text.to_string(),
    }
}

/// cc 1, sloc 1: mass 1.
const SMALL: &str = "fn a() -> u32 { 1 }\n";

/// cc 11 (ten ifs), sloc 4: mass 11 * 2 = 22 — eroded (CC > 10).
const ERODED: &str = "fn b(x: u32) -> u32 {
    if x == 1 { return 1; } if x == 2 { return 2; } if x == 3 { return 3; } if x == 4 { return 4; } if x == 5 { return 5; }
    if x == 6 { return 6; } if x == 7 { return 7; } if x == 8 { return 8; } if x == 9 { return 9; } if x == 10 { return 10; }
    0 }
";

/// cc 10 (ten arms), sloc 3: mass 10 * sqrt(3) — NOT eroded (CC = 10).
const AT_TEN: &str = "fn c(x: u32) -> u32 {
    match x { 1 => 1, 2 => 4, 3 => 9, 4 => 16, 5 => 25, 6 => 36, 7 => 49, 8 => 64, 9 => 81, _ => 0 }
}
";

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn erosion_is_eroded_mass_over_all_mass() {
    let text = format!("{SMALL}{ERODED}{AT_TEN}");
    let m = measure(&[src("crates/k/src/lib.rs", &text)]).unwrap();
    let k = &m.stats[&("k".to_string(), Kind::NonTest)];
    assert_eq!(k.fns, 3);
    let all = 1.0 + 22.0 + 10.0 * 3f64.sqrt();
    assert!(approx(k.mass, all), "{}", k.mass);
    assert!(approx(k.erosion(), 22.0 / all), "{}", k.erosion());
    assert_eq!(k.clone_share(), 0.0);
}

#[test]
fn test_code_is_measured_apart() {
    let text = format!("{SMALL}#[cfg(test)]\nmod tests {{\n{ERODED}}}\n");
    let m = measure(&[src("crates/k/src/lib.rs", &text)]).unwrap();
    let head = &m.stats[&("k".to_string(), Kind::NonTest)];
    assert_eq!(
        (head.fns, head.erosion()),
        (1, 0.0),
        "the headline has no test code"
    );
    let test = &m.stats[&("k".to_string(), Kind::Test)];
    assert_eq!(test.fns, 1);
    assert!(approx(test.erosion(), 1.0));
}

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
    out
}
"#;

#[test]
fn clone_share_is_the_sloc_in_a_finding_of_the_one_detector() {
    let m = measure(&[
        src("crates/k/src/a.rs", TWIN),
        src("crates/k/src/b.rs", TWIN),
        src("crates/k/src/c.rs", SMALL),
    ])
    .unwrap();
    let k = &m.stats[&("k".to_string(), Kind::NonTest)];
    // TWIN is 15 SLOC; two copies of it plus SMALL's one line.
    assert_eq!(k.sloc, 31);
    assert!(approx(k.clone_share(), 30.0 / 31.0), "{}", k.clone_share());
}

fn git(root: &std::path::Path, args: &[&str]) {
    let mut all = vec!["-c", "user.name=t", "-c", "user.email=t@example.invalid"];
    all.extend(args);
    crate::proc::git(root, &all).unwrap();
}

/// A repo whose `main` has `base_files`; HEAD is a branch on top that
/// commits `head_files`.
fn history(
    tag: &str,
    base_files: &[(&str, &str)],
    head_files: &[(&str, &str)],
) -> std::path::PathBuf {
    let root = plant(tag, base_files);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "base"]);
    git(&root, &["checkout", "-qb", "feature"]);
    for (rel, text) in head_files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "head", "--allow-empty"]);
    root
}

#[test]
fn an_unchanged_tree_is_green_with_zero_deltas() {
    let root = history("slop-same", &[("crates/k/src/lib.rs", SMALL)], &[]);
    let (code, out, err) = census(&root, "main");
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("slop/1"), "{out}");
    assert!(out.contains("SLOP CENSUS GREEN"), "{out}");
}

#[test]
fn a_rise_in_erosion_against_the_merge_base_wants_review() {
    let eroded = format!("{SMALL}{ERODED}");
    let root = history(
        "slop-rise",
        &[
            ("crates/k/src/lib.rs", SMALL),
            ("crates/j/src/lib.rs", SMALL),
        ],
        &[("crates/k/src/lib.rs", &eroded)],
    );
    // main moves on after the branch: the base is the MERGE-BASE, not main.
    git(&root, &["checkout", "-q", "main"]);
    std::fs::write(root.join("crates/j/src/lib.rs"), format!("{SMALL}{ERODED}")).unwrap();
    git(&root, &["commit", "-qam", "main moves"]);
    git(&root, &["checkout", "-q", "feature"]);
    let (code, out, err) = census(&root, "main");
    assert_eq!(code, 1, "{out}{err}");
    assert!(out.contains("REVIEW"), "{out}");
    let k_line = out
        .lines()
        .find(|l| l.trim_start().starts_with("k "))
        .unwrap_or("");
    assert!(k_line.contains("+95.7pp"), "erosion 0 -> 22/23:\n{out}");
    let j_line = out
        .lines()
        .find(|l| l.trim_start().starts_with("j "))
        .unwrap_or("");
    assert!(
        j_line.contains("+0.0pp"),
        "main's later change is not this branch's:\n{out}"
    );
}

#[test]
fn the_working_tree_is_now_and_the_base_comes_from_blobs() {
    let root = history("slop-worktree", &[("crates/k/src/lib.rs", SMALL)], &[]);
    // Uncommitted: the census measures it as NOW.
    std::fs::write(root.join("crates/k/src/lib.rs"), format!("{SMALL}{ERODED}")).unwrap();
    let (code, out, _) = census(&root, "main");
    assert_eq!(code, 1, "{out}");
    // Deleting the file from the working tree leaves the base intact.
    std::fs::remove_file(root.join("crates/k/src/lib.rs")).unwrap();
    let (_, out, err) = census(&root, "main");
    assert!(
        out.contains("k "),
        "the base still has crate k:\n{out}{err}"
    );
}

#[test]
fn cognitive_complexity_allows_are_listed_and_a_new_one_wants_review() {
    let allowed = "#[allow(clippy::cognitive_complexity)]\nfn big() {}\n";
    let root = history(
        "slop-allow",
        &[("crates/k/src/lib.rs", SMALL)],
        &[("crates/k/src/lib.rs", allowed)],
    );
    let (code, out, _) = census(&root, "main");
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("crates/k/src/lib.rs:1  big"), "{out}");
}

#[test]
fn an_unknown_base_is_an_error() {
    let root = history("slop-nobase", &[("crates/k/src/lib.rs", SMALL)], &[]);
    assert_eq!(census(&root, "no-such-ref").0, 2);
}
