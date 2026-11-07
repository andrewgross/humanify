//! The ONE changed-line counter (TS: `experiments/lib/diff.ts` over
//! `line-diff.ts` `computeNormalDiff`): the system `diff`, normal format,
//! counting `<` and `>` lines — a modified line counts TWICE, which is what
//! every published "git lines" figure means. CRLF is normalised first (a
//! CRLF copy would otherwise differ on every line). A `diff` that cannot
//! run is an error, never "no change".

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::proc::{Spawn, run};

/// One `diff` may take this long (a vendored library is one line of
/// ~100KB; a whole bundle ~30MB).
const DIFF_DEADLINE: Duration = Duration::from_secs(600);

/// The counter. CRLF-normalised copies (only when an input has CRLF
/// line ends) go to a private scratch dir (`humanify_ag_core::scratch`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Differ;

fn is_marker(line: &str) -> bool {
    line.starts_with('<') || line.starts_with('>')
}

impl Differ {
    /// The normal-format `diff` of two files' texts.
    pub fn normal_diff(&self, a: &Path, b: &Path) -> Result<String, String> {
        let scratch = std::cell::OnceCell::new();
        let (a, a_copy) = normalised(a, &scratch)?;
        let (b, b_copy) = normalised(b, &scratch)?;
        let spawn = Spawn::new(
            "diff",
            &[&a.display().to_string(), &b.display().to_string()],
            DIFF_DEADLINE,
        );
        let done = run(&spawn);
        for copy in [a_copy, b_copy].into_iter().flatten() {
            let _ = std::fs::remove_file(copy);
        }
        if let Some(dir) = scratch.get() {
            let _ = std::fs::remove_dir_all(dir);
        }
        let done = done?;
        match done.status.code() {
            Some(0 | 1) => String::from_utf8(done.stdout)
                .or_else(|e| Ok(String::from_utf8_lossy(e.as_bytes()).into_owned())),
            _ => Err(format!(
                "{} failed ({}): {}",
                spawn.describe(),
                done.status,
                String::from_utf8_lossy(&done.stderr).trim()
            )),
        }
    }

    /// Changed lines between two files: `<` and `>` lines of their diff.
    pub fn changed_lines(&self, a: &Path, b: &Path) -> Result<usize, String> {
        Ok(self
            .normal_diff(a, b)?
            .split('\n')
            .filter(|l| is_marker(l))
            .count())
    }

    /// The changed lines themselves, marker stripped (`< x` -> `x`).
    pub fn changed_line_texts(&self, a: &Path, b: &Path) -> Result<Vec<String>, String> {
        Ok(self
            .normal_diff(a, b)?
            .split('\n')
            .filter(|l| is_marker(l))
            .map(|l| l.get(2..).unwrap_or("").to_string())
            .collect())
    }
}

/// `path` itself, or a CRLF-normalised copy of it in a private scratch dir
/// (made on first need; the copy is returned second, for removal).
fn normalised(
    path: &Path,
    scratch: &std::cell::OnceCell<PathBuf>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !bytes.windows(2).any(|w| w == b"\r\n") {
        return Ok((path.to_path_buf(), None));
    }
    let text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    let dir = match scratch.get() {
        Some(d) => d.clone(),
        None => {
            let d = humanify_ag_core::scratch::temp_dir("humanify-eval-diff")?;
            let _ = scratch.set(d.clone());
            d
        }
    };
    let copy = dir.join(format!("lf-{}", scratch_name(path)));
    std::fs::write(&copy, text).map_err(|e| format!("{}: {e}", copy.display()))?;
    Ok((copy.clone(), Some(copy)))
}

/// A distinct file name per input path.
fn scratch_name(path: &Path) -> String {
    crate::committed::sha256_hex(path.display().to_string().as_bytes())
}

#[cfg(test)]
#[path = "difftool_test.rs"]
mod difftool_test;
