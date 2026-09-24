//! The webcrack adapter (`src/unpack/adapters/webcrack.ts`) as a
//! SUBPROCESS: webpack/browserify bundles go through `scripts/webcrack-
//! shim.ts`, which runs the pipeline's own `webcrack()` (plugins/
//! webcrack.ts — absorbed by the shim, never ported; 10-work-breakdown
//! WPB.2). The bundle goes in on stdin; one JSON line comes back:
//! `{"files":[{"path","metadata"?}],"bundleType"?}`.
//!
//! The command is configuration (the caller resolves where node and the
//! shim live); core never reads the environment.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::{ModuleMetadata, UnpackResult, UnpackedFile};

/// How to launch the shim: the program and its leading arguments (e.g.
/// `npx tsx <repo>/scripts/webcrack-shim.ts`); the output directory is
/// appended as the last argument.
#[derive(Clone, Debug)]
pub struct WebcrackShim {
    pub program: String,
    pub args: Vec<String>,
    /// The working directory (the repo root, where `node_modules` resolves).
    pub cwd: Option<PathBuf>,
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

/// `WebcrackAdapter.unpack`: run the shim, return its files in the order it
/// listed them. A non-zero exit or unparseable output is an error — the
/// TS adapter throws the same way when webcrack does.
pub fn unpack_webcrack(
    code: &str,
    out_dir: &Path,
    shim: &WebcrackShim,
) -> Result<UnpackResult, String> {
    let mut command = Command::new(&shim.program);
    command
        .args(&shim.args)
        .arg(out_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = &shim.cwd {
        command.current_dir(cwd);
    }
    let mut child = command
        .spawn()
        .map_err(|e| format!("webcrack shim: cannot start {}: {e}", shim.program))?;
    child
        .stdin
        .take()
        .ok_or("webcrack shim: no stdin")?
        .write_all(code.as_bytes())
        .map_err(|e| format!("webcrack shim: write stdin: {e}"))?;
    let output = child
        .wait_with_output()
        .map_err(|e| format!("webcrack shim: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "webcrack shim exited {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_shim_output(&String::from_utf8_lossy(&output.stdout))
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
