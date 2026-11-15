//! The system `diff` the reconcile aligns with (`computeNormalDiff`):
//! normal format, over CRLF-normalized temp files. The alignment IS GNU diff's: re-implementing its LCS heuristics would
//! be a second diff with its own hunk boundaries, so the binary is a
//! pipeline dependency. The core declares the `Differ` trait it needs and
//! the shell hands it [`compute_normal_diff`].

/// The `diff` deadline: 10 minutes. Measured: the largest diff
/// the pipeline runs — the pre-split reconcile over the whole Claude Code
/// bundle, 793k vs 805k lines (2.1.215 -> 2.1.216) — takes 4.5 s; two
/// unrelated 800k-line files 0.85 s; the post-split per-file diffs are a
/// fraction of that. 10 minutes is >100x the worst measurement, room for
/// up to one `diff` per core on a loaded shared box (audit R3). Reaching it
/// is a failure the caller reports, not a silent discard.
pub const DIFF_DEADLINE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Why `diff` gave no answer, in the words the run prints (a WARNING
/// line, or the post-split pass's `diff_failed` count and warning).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiffError(pub String);

impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// `computeNormalDiff(prior, new)`: `diff prior new` (normal format) over
/// CRLF-normalized copies, under [`DIFF_DEADLINE`]. Err on a real `diff`
/// failure (exit >= 2, spawn failure, the deadline) — each caller reports
/// it: the pre-split step as a WARNING, the post-split pass as a counted,
/// warned `diff_failed` (audit R3).
pub fn compute_normal_diff(prior_text: &str, new_text: &str) -> Result<String, DiffError> {
    let dir = tempdir().map_err(DiffError)?;
    let prior_path = dir.join("prior.js");
    let new_path = dir.join("new.js");
    let result = (|| {
        std::fs::write(&prior_path, prior_text.replace("\r\n", "\n"))
            .map_err(|e| format!("write prior: {e}"))?;
        std::fs::write(&new_path, new_text.replace("\r\n", "\n"))
            .map_err(|e| format!("write new: {e}"))?;
        let mut command = std::process::Command::new("diff");
        command.arg(&prior_path).arg(&new_path);
        let out = crate::subprocess::run(&mut command, None, DIFF_DEADLINE)
            .map_err(|e| format!("diff failed (status null): {e}"))?;
        match out.status.code() {
            Some(0 | 1) => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
            code => Err(format!(
                "diff failed (status {code:?}): {}",
                String::from_utf8_lossy(&out.stderr)
            )),
        }
    })()
    .map_err(DiffError);
    // A scratch dir left behind is litter in the temp dir, not a wrong
    // answer: the diff's own result is what the caller judges.
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// A private temp directory (`fs.mkdtempSync(os.tmpdir()/humanify-reconcile-)`).
fn tempdir() -> Result<std::path::PathBuf, String> {
    crate::scratch::temp_dir("humanify-reconcile")
        .map_err(|_| "cannot create a reconcile temp dir".to_string())
}

#[cfg(test)]
mod diff_test;
