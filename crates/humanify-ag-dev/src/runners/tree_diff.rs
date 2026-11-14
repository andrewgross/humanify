//! The runners' byte-identity counts, through the SYSTEM `diff` (the
//! convention of `experiments/lib/diff.ts`, the harness's changed-line
//! owner: normal diff, a changed line is a `<` or `>` line, so a modified
//! line counts twice; `-rN` for trees), with the scorer's algorithm flags
//! (`eval::difftool::LINE_DIFF_FLAGS`, the minimal edit). Used for verdicts that must read
//! zero — neutrality's tree diff, the self-hop's cold count and warm
//! identity — and for the walk report's per-directory columns; never for
//! a scorecard KPI (the scorer counts its own lines).

use std::path::Path;
use std::time::Duration;

use crate::eval::difftool::LINE_DIFF_FLAGS;
use crate::proc::{Spawn, run};

/// A whole-tree `diff` of two Claude Code trees (seconds; minutes on a
/// loaded box).
const DIFF_DEADLINE: Duration = Duration::from_secs(60 * 60);

/// The diagnostics-only sidecars excluded from every byte-identity
/// verdict (neutrality.sh `DIAGNOSTIC_ONLY`, run.sh's warm self-hop):
/// written for a reader, consumed by nothing. `humanified.js` and
/// `split-ledger.json` are NOT here: they are the next release's prior.
pub const DIAGNOSTIC_ONLY: &[&str] = &[
    ".humanify/placement-stats.json",
    ".humanify/stage-hashes.json",
    ".humanify/library-report.json",
];

fn excludes() -> Vec<String> {
    DIAGNOSTIC_ONLY
        .iter()
        .map(|f| format!("--exclude={}", f.rsplit('/').next().unwrap_or(f)))
        .collect()
}

fn diff_output(args: Vec<String>) -> Result<String, String> {
    let spawn = Spawn {
        program: "diff".into(),
        args,
        cwd: None,
        env: Vec::new(),
        stdin: None,
        deadline: DIFF_DEADLINE,
    };
    let done = run(&spawn)?;
    // diff exits 0 (same), 1 (differs), 2 (trouble — a missing side is
    // counted by its lines like the shell pipeline did).
    Ok(String::from_utf8_lossy(&done.stdout).into_owned())
}

/// The counting arguments: the algorithm flags, then `rest`.
fn counting(rest: &[String]) -> Vec<String> {
    LINE_DIFF_FLAGS
        .iter()
        .map(|f| f.to_string())
        .chain(rest.iter().cloned())
        .collect()
}

fn changed(out: &str) -> u64 {
    out.lines()
        .filter(|l| l.starts_with('<') || l.starts_with('>'))
        .count() as u64
}

/// Changed lines between two files (`diff a b | grep -cE '^[<>]'`).
pub fn changed_lines(a: &Path, b: &Path) -> Result<u64, String> {
    diff_output(counting(&[
        a.display().to_string(),
        b.display().to_string(),
    ]))
    .map(|o| changed(&o))
}

/// Changed lines between two trees, the diagnostics-only sidecars
/// excluded (`diff -rN --exclude=...`).
pub fn changed_lines_in_trees(a: &Path, b: &Path) -> Result<u64, String> {
    let mut args = vec!["-rN".to_string()];
    args.extend(excludes());
    args.push(a.display().to_string());
    args.push(b.display().to_string());
    diff_output(counting(&args)).map(|o| changed(&o))
}

/// The differing files between two trees, one `diff -rq` line each, the
/// diagnostics-only sidecars excluded.
pub fn differing_files(a: &Path, b: &Path) -> Result<Vec<String>, String> {
    let mut args = vec!["-rq".to_string()];
    args.extend(excludes());
    args.push(a.display().to_string());
    args.push(b.display().to_string());
    diff_output(args).map(|o| o.lines().map(str::to_string).collect())
}

/// The diagnostics-only sidecars that differ (reported, never fatal).
pub fn differing_sidecars(a: &Path, b: &Path) -> Result<Vec<String>, String> {
    let all = diff_output(vec![
        "-rq".to_string(),
        a.display().to_string(),
        b.display().to_string(),
    ])?;
    let names: Vec<&str> = DIAGNOSTIC_ONLY
        .iter()
        .map(|f| f.rsplit('/').next().unwrap_or(f))
        .collect();
    Ok(all
        .lines()
        .filter(|l| names.iter().any(|n| l.contains(n)))
        .map(str::to_string)
        .collect())
}

/// Two files are byte-identical (`cmp -s`); a missing file is not.
pub fn same_bytes(a: &Path, b: &Path) -> bool {
    match (std::fs::read(a), std::fs::read(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}
