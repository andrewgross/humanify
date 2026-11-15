//! The stub-run BATCHES over the bun-dev corpus (`dev/library/bun-dev-list.tsv`,
//! loaded by `committed::library::bundev_list`), on `run stub` and the library instruments:
//!
//! - `eval vendor-precision bun-dev <bin> <label>` — the port of
//!   experiments/166 `bundev-precision.sh`: per bundle a names-mode stub
//!   run, the vendor-placement precision check against its source-map
//!   truth (`<id>.precision.json`), the tree deleted; then the aggregate.
//!
//! No host-wide lock (exp205): a batch runs `jobs` bundles at a time and
//! CLAIMS its out dir (`super::claim`), so a second batch into the same dir
//! is refused at once. A bundle whose output is already there is skipped
//! (the scripts' resume).

use std::path::{Path, PathBuf};

use super::Ctx;

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
    let list = crate::committed::library::bundev_list(&ctx.repo)?.value;
    let fam = Families::shipped();
    let lines = {
        let _claim = super::claim::claim_folder(out, "eval vendor-precision bun-dev")?;
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
