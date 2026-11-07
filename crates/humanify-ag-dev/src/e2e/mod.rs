//! The release binary against the one stub LLM: the e2e stage, the ask
//! trace and its differ, and the stub as a process.

pub mod ask_trace;
pub mod diff_asks;
pub mod expect;
pub mod launch_log;
pub mod run;
pub mod stub_llm;

use std::path::{Path, PathBuf};

/// The release binary of the checkout at `repo`, the one the gate's
/// rust:build stage builds; missing, the error names the build command.
pub fn release_binary(repo: &Path) -> Result<PathBuf, String> {
    let bin = repo.join("target/release/humanify-ag");
    if bin.is_file() {
        Ok(bin)
    } else {
        Err(format!(
            "no binary at {} — the rust:build stage builds it (cargo build --release --locked -p humanify-ag-cli)",
            bin.display()
        ))
    }
}

/// A fresh, empty directory `<tmp>/<prefix><unique>` (`mkdtemp`).
pub fn fresh_dir(prefix: &str) -> Result<PathBuf, String> {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.subsec_nanos());
    let dir = std::env::temp_dir().join(format!("{prefix}{}-{nanos:09}", std::process::id()));
    std::fs::create_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

/// The repository root this harness was built from, resolved (no `..`).
pub fn repo() -> PathBuf {
    let root = crate::paths::repo_root();
    root.canonicalize().unwrap_or(root)
}
