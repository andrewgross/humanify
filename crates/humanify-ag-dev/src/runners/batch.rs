//! The stub-run BATCHES over the bun-dev corpus (experiments/165's
//! `bun-dev-list.tsv`), on `run stub` and the library instruments:
//!
//! - `eval vendor-precision bun-dev <bin> <label>` — the port of
//!   experiments/166 `bundev-precision.sh`: per bundle a names-mode stub
//!   run, the vendor-placement precision check against its source-map
//!   truth (`<id>.precision.json`), the tree deleted; then the aggregate.
//! - `eval cjs-placement runs <bin> <label> <names|default>` — the port of
//!   experiments/172 `bundev-runs.sh`: per bundle a stub run keeping the
//!   diagnostics and a tree summary (`<id>/tree.tsv`: every output file's
//!   path and size, `.humanify/` excluded), never the tree.
//! - `eval cjs-placement trees <before> <after> <substring>...` — the port
//!   of experiments/172 `trees.py`: the two runs' tree summaries side by side.
//!
//! Locks: ONE owner. A batch takes `/work/heavy.lock` once (unless the
//! caller holds it: `HELD_LOCK`) and runs `jobs` bundles at a time inside.
//! A bundle whose output is already there is skipped (the scripts' resume).

use std::path::{Path, PathBuf};

use super::Ctx;

/// experiments/165's bun-dev list: (entry id, bundle path) per line.
pub fn bundev_list(repo: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let p = repo.join("experiments/165-claim-boundary/bun-dev-list.tsv");
    let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((f.next()?.to_string(), PathBuf::from(f.next()?)))
        })
        .collect())
}

fn nonempty(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.len() > 0)
}

/// One stub run, its one-line outcome written to `<out>.run.txt`; the
/// binary's exit code (or why it could not run).
fn stub_once(
    ctx: &Ctx,
    bin: &Path,
    bundle: &Path,
    out: &Path,
    index: Option<&str>,
) -> Result<i32, String> {
    let r = super::stub::run(ctx, bin, bundle, out, None, index);
    let line = match &r {
        Ok(code) => format!("{}: exit {code}\n", out.display()),
        Err(e) => format!("{e}\n"),
    };
    let run_txt = PathBuf::from(format!("{}.run.txt", out.display()));
    std::fs::write(&run_txt, line).map_err(|e| format!("{}: {e}", run_txt.display()))?;
    r
}

/// `bundev-precision.sh <bin> <label> [jobs]`; the batch's report lines.
pub fn vendor_precision_bundev(
    ctx: &Ctx,
    bin: &Path,
    out: &Path,
    jobs: usize,
    index: &str,
) -> Result<String, String> {
    use crate::library::{families::Families, vendor_precision as vp};
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let list = bundev_list(&ctx.repo)?;
    let fam = Families::shipped();
    let lines = {
        let _lock = super::heavy_lock(ctx, Path::new("/work/heavy.lock"))?;
        super::each(&list, jobs, |(id, bundle)| -> String {
            let card = out.join(format!("{id}.precision.json"));
            if nonempty(&card) {
                return String::new();
            }
            let dir = out.join(id);
            match stub_once(ctx, bin, bundle, &dir, Some(index)) {
                Ok(0) => {}
                _ => return format!("RUN FAIL {id}\n"),
            }
            let check = || -> Result<String, String> {
                let diag = dir.join("diag.json");
                let text = std::fs::read_to_string(&diag)
                    .map_err(|e| format!("{}: {e}", diag.display()))?;
                let d: serde_json::Value =
                    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", diag.display()))?;
                let truth = vp::truth_bundev(&ctx.repo, id)?;
                Ok(vp::card_text(&vp::check(&d, Some(&truth), Some(&fam))?))
            };
            let err = out.join(format!("{id}.precision.err"));
            let line = match check() {
                Ok(text) => {
                    let _ = std::fs::write(&card, text);
                    let _ = std::fs::write(&err, "");
                    String::new()
                }
                Err(e) => {
                    let _ = std::fs::write(&card, "");
                    let _ = std::fs::write(&err, format!("{e}\n"));
                    format!("CHECK FAIL {id}\n")
                }
            };
            super::remove(&dir.join("tree"));
            line
        })
    };
    let mut report: String = lines.concat();
    let agg = vp::aggregate(out)?;
    let p = out.join("aggregate.json");
    std::fs::write(&p, &agg).map_err(|e| format!("{}: {e}", p.display()))?;
    report.push_str(&format!("done {}\n", out.display()));
    Ok(report)
}

/// `<tree>`'s files as `find . -type f -not -path './.humanify/*' -printf
/// '%P\t%s\n' | sort`.
pub fn tree_tsv(tree: &Path) -> String {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for e in read.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            let rel = p
                .strip_prefix(base)
                .map(|r| r.to_string_lossy().to_string())
                .unwrap_or_default();
            if ft.is_dir() {
                walk(base, &p, out);
            } else if ft.is_file() {
                if rel.starts_with(".humanify/") {
                    continue;
                }
                let size = e.metadata().map_or(0, |m| m.len());
                out.push(format!("{rel}\t{size}"));
            }
        }
    }
    let mut lines = Vec::new();
    walk(tree, tree, &mut lines);
    lines.sort();
    lines.iter().map(|l| format!("{l}\n")).collect()
}

/// `bundev-runs.sh <bin> <label> <names|default> [jobs]`.
pub fn cjs_runs(
    ctx: &Ctx,
    bin: &Path,
    out: &Path,
    jobs: usize,
    index: Option<&str>,
) -> Result<String, String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let list = bundev_list(&ctx.repo)?;
    let lines = {
        let _lock = super::heavy_lock(ctx, Path::new("/work/heavy.lock"))?;
        super::each(&list, jobs, |(id, bundle)| -> String {
            let dir = out.join(id);
            if nonempty(&dir.join("tree.tsv")) {
                return String::new();
            }
            let mut line = String::new();
            if !matches!(stub_once(ctx, bin, bundle, &dir, index), Ok(0)) {
                line = format!("RUN FAIL {id}\n");
            }
            let tree = dir.join("tree");
            if tree.is_dir() {
                let _ = std::fs::write(dir.join("tree.tsv"), tree_tsv(&tree));
                super::remove(&tree);
            }
            line
        })
    };
    Ok(format!("{}done {}\n", lines.concat(), out.display()))
}

/// trees.py `summary`: files / bytes under src/, vendor/ (the manifest
/// excluded) and anything else.
fn summary(path: &Path) -> Option<[(u64, u64); 3]> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut out = [(0u64, 0u64); 3];
    for line in text.lines() {
        let Some((rel, size)) = line.rsplit_once('\t') else {
            continue;
        };
        if rel == "vendor/_bun-modules.json" {
            continue;
        }
        let side = match rel.split_once('/') {
            Some(("src", _)) => 0,
            Some(("vendor", _)) => 1,
            _ => 2,
        };
        out[side].0 += 1;
        out[side].1 += size.parse::<u64>().unwrap_or(0);
    }
    Some(out)
}

/// Python's `f"{n:,}"`.
fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// trees.py: the before / after table of the bundles whose id holds one
/// of `wanted`.
pub fn trees(before: &Path, after: &Path, wanted: &[String]) -> Result<String, String> {
    let mut ids: Vec<String> = std::fs::read_dir(after)
        .map_err(|e| format!("{}: {e}", after.display()))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|i| wanted.iter().any(|w| i.contains(w.as_str())))
        .collect();
    ids.sort();
    let cell = |s: &Option<[(u64, u64); 3]>, side: usize| match s {
        None => "—".to_string(),
        Some(x) => format!("{} / {}", x[side].0, thousands(x[side].1)),
    };
    let mut out = String::from(
        "| bundle | src/ before (files / bytes) | src/ after | vendor/ before | vendor/ after | other before | other after |\n| --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for i in ids {
        let b = summary(&before.join(&i).join("tree.tsv"));
        let a = summary(&after.join(&i).join("tree.tsv"));
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} |\n",
            i.replace("bun-dev-", ""),
            cell(&b, 0),
            cell(&a, 0),
            cell(&b, 1),
            cell(&a, 1),
            cell(&b, 2),
            cell(&a, 2)
        ));
    }
    Ok(out)
}

#[cfg(test)]
#[path = "batch_test.rs"]
mod batch_test;
