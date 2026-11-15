//! The webcrack adapter (`src/unpack/adapters/webcrack.ts`) as a
//! SUBPROCESS: webpack/browserify bundles go through `scripts/webcrack-
//! shim.ts`, which runs the pipeline's own `webcrack()` (plugins/
//! webcrack.ts — absorbed by the shim, never ported; 10-work-breakdown
//! WPB.2). The bundle goes in on stdin; one JSON line comes back:
//! `{"files":[{"path","metadata"?}],"bundleType"?}`.
//!
//! The shell runs the shim (cli `webcrack`, through the one subprocess
//! owner) and hands core what it printed: core spawns nothing, reads no
//! environment and touches no disk here. Core keeps the decisions: what
//! the contract line says, and that every file it names lies inside the
//! directory the shim was given.

use std::path::{Path, PathBuf};

use super::{ModuleMetadata, UnpackResult, UnpackedFile};

/// What one run of the external unpacker gave back: the ABSOLUTE,
/// canonical directory it wrote into (a relative `-o out` resolved
/// against the run's working directory, not the shim's) and its stdout.
#[derive(Clone, Debug)]
pub struct ShimRun {
    pub out_dir: PathBuf,
    pub stdout: String,
}

/// Why the external unpacker gave nothing: the shell's message, word for
/// word (a non-zero exit, a deadline, a directory it could not make).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnpackerError(pub String);

/// The webcrack shim as core sees it: given the bundle and the run's
/// output directory, run it and return what it printed. The shell
/// implements it (cli `webcrack::WebcrackShim`); a test may hand in its own.
pub trait ExternalUnpacker {
    fn run(&self, code: &str, out_dir: &Path) -> Result<ShimRun, UnpackerError>;
}

#[derive(serde::Deserialize)]
struct ShimFile {
    path: PathBuf,
    metadata: Option<ModuleMetadata>,
}

#[derive(serde::Deserialize)]
struct ShimOutput {
    files: Vec<ShimFile>,
}

/// `WebcrackAdapter.unpack`: run the shim through `unpacker` and return
/// its files in the order it listed them — every one inside the
/// directory the shim wrote into, or the unpack fails naming the one
/// outside. A failed run (non-zero exit, a shim past its deadline) or
/// unparseable output is an error — the TS adapter throws the same way
/// when webcrack does.
pub fn unpack_webcrack(
    code: &str,
    out_dir: &Path,
    unpacker: &dyn ExternalUnpacker,
) -> Result<UnpackResult, String> {
    let ShimRun { out_dir, stdout } = unpacker.run(code, out_dir).map_err(|e| e.0)?;
    let result = parse_shim_output(&stdout)?;
    if let Some(stray) = result.files.iter().find(|f| !f.path.starts_with(&out_dir)) {
        return Err(format!(
            "webcrack shim reported {}, outside its output directory {}",
            stray.path.display(),
            out_dir.display()
        ));
    }
    Ok(result)
}

/// The shim's stdout → the unpack result (its LAST non-empty line: a
/// dependency that logs to stdout cannot corrupt the contract line).
pub fn parse_shim_output(stdout: &str) -> Result<UnpackResult, String> {
    let line = stdout
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .ok_or("webcrack shim: no output")?;
    let parsed: ShimOutput =
        serde_json::from_str(line).map_err(|e| format!("webcrack shim output: {e}"))?;
    Ok(UnpackResult {
        files: parsed
            .files
            .into_iter()
            .map(|f| UnpackedFile {
                path: f.path,
                metadata: f.metadata,
            })
            .collect(),
    })
}

#[cfg(test)]
mod webcrack_test;
