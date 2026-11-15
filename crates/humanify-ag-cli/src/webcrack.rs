//! The webcrack shim as the shell runs it: webpack/browserify bundles go
//! through `tools/webcrack-shim.ts` (`npx tsx`), which runs the pipeline's
//! own `webcrack()`. The bundle goes in on stdin; one JSON line comes back
//! on stdout, which core reads (`humanify_ag_core::unpack::webcrack`,
//! the contract line and the files-inside-the-out-dir check). This module
//! makes the output directory, runs the child through the one subprocess
//! owner (`humanify_ag_io::subprocess`) and judges its exit.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use humanify_ag_core::unpack::webcrack::{ExternalUnpacker, ShimRun, UnpackerError};

/// How to launch the shim: the program and its leading arguments (e.g.
/// `npx tsx <repo>/tools/webcrack-shim.ts`); the output directory is
/// appended as the last argument.
#[derive(Clone, Debug)]
pub struct WebcrackShim {
    pub program: String,
    pub args: Vec<String>,
    /// The working directory (the repo root, where `node_modules` resolves).
    pub cwd: Option<PathBuf>,
    /// The deadline over the shim's whole run (start, stdin, output,
    /// exit); the production value is [`WEBCRACK_DEADLINE`].
    pub deadline: Duration,
}

/// The shim's deadline: 30 minutes. Measured: a 2.9 MB,
/// 3,000-module webpack 4 bundle takes 12 s end to end (`npx` start
/// included), a 10 KB one 0.85 s; time grows about linearly with size, so
/// a 20 MB bundle should take under 2 minutes. 30 minutes is 150x the
/// largest measurement and >10x the extrapolation: only a webcrack that
/// does not finish reaches it (docs/concurrency.md, "Subprocesses").
pub const WEBCRACK_DEADLINE: Duration = Duration::from_secs(30 * 60);

impl ExternalUnpacker for WebcrackShim {
    /// Run the shim on the ABSOLUTE output directory ([`shim_out_dir`]),
    /// the bundle on stdin. A non-zero exit or a shim past its deadline
    /// (killed) is an error; what it printed is core's to read.
    fn run(&self, code: &str, out_dir: &Path) -> Result<ShimRun, UnpackerError> {
        let out_dir = shim_out_dir(out_dir).map_err(UnpackerError)?;
        let mut command = Command::new(&self.program);
        command.args(&self.args).arg(&out_dir);
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        let output = humanify_ag_io::subprocess::run(
            &mut command,
            Some(code.as_bytes().to_vec()),
            self.deadline,
        )
        .map_err(|e| UnpackerError(format!("webcrack shim ({}): {e}", self.program)))?;
        if !output.status.success() {
            return Err(UnpackerError(format!(
                "webcrack shim exited {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(ShimRun {
            out_dir,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        })
    }
}

/// The output directory as the shim receives it: created, then
/// ABSOLUTE and canonical. The shim runs in its own working directory (the
/// repo root, [`WebcrackShim::cwd`]), so a relative `out_dir` — `-o out` —
/// resolved THERE: webcrack wrote into `<repo>/out/` and reported
/// `out/deobfuscated.js`, which the run then read against its own working
/// directory and did not find. Canonical, so the paths the shim reports
/// (`path.join(outputDir, file)`, which normalizes) start with it.
fn shim_out_dir(out_dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(out_dir).map_err(|e| format!("mkdir {}: {e}", out_dir.display()))?;
    std::fs::canonicalize(out_dir).map_err(|e| format!("resolve {}: {e}", out_dir.display()))
}

#[cfg(test)]
mod webcrack_test;
