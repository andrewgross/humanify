//! Keep the evidence when the rename invariant rejects a file (TS:
//! `src/failed-output.ts`).
//!
//! The split consumes and deletes the processed source, so the run copies
//! each rejected file and BOTH sides of the check — the pre-rename source
//! (`.original`) and the exact code the invariant examined (`.validated`,
//! not the file on disk, which later passes rewrote) — under
//! `<outputDir>/.humanify/failed/`. A capture is only visible as a pair
//! (`b !== b` alone reads like a NaN check).
//!
//! Best-effort by construction: every operation is independent and a
//! failure to preserve never turns a reportable failure into a crash.

use std::path::Path;

use crate::report::FAILED_OUTPUT_DIR;

/// One rejected file.
#[derive(Clone, Debug)]
pub struct FailedOutputFile {
    /// Path of the emitted file that violated the invariant.
    pub file_path: String,
    /// The file's contents BEFORE the rename pass ran.
    pub original_code: String,
    /// The code the invariant check actually examined.
    pub validated_code: Option<String>,
}

/// `preserveFailedOutput(outputDir, failures)`.
pub fn preserve_failed_output(output_dir: &Path, failures: &[FailedOutputFile]) {
    if failures.is_empty() {
        return;
    }
    let dest = output_dir.join(FAILED_OUTPUT_DIR);
    if std::fs::create_dir_all(&dest).is_err() {
        return;
    }
    for f in failures {
        let path = Path::new(&f.file_path);
        let base = path
            .file_name()
            .map(|b| b.to_string_lossy().into_owned())
            .unwrap_or_default();
        if path.exists() {
            let _ = std::fs::copy(path, dest.join(&base));
        }
        let _ = std::fs::write(dest.join(format!("{base}.original")), &f.original_code);
        if let Some(v) = &f.validated_code {
            let _ = std::fs::write(dest.join(format!("{base}.validated")), v);
        }
    }
}
