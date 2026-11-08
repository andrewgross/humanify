//! The ONE owner of "build the pipeline binary for a commit and say what
//! was built" (docs/dev-harness.md §5, "build-bin across the rename"):
//! replaces `experiments/lib/build-bin.sh`, `pipeline-bin.ts`, run.py's
//! build and walk.sh's build.
//!
//! - The package and binary name come from the CHECKOUT being built, never
//!   from HEAD: a neutrality baseline or a walk hop builds an older commit.
//!   The pipeline crate is `crates/humanify-ag-cli` (since exp186) or
//!   `crates/humanify-cli` (before), and its binary is the `[[bin]]` whose
//!   path is `src/main.rs` in that crate's Cargo.toml (`humanify-ag` /
//!   `humanify`). A checkout with neither is from before the cutover: it
//!   has no pipeline binary, and it is refused.
//! - The record carries the binary's sha256, the commit it was built from
//!   and whether the tree was dirty — the binary block of the identity.
//! - A legacy leg (a baseline binary with no `--config` flag) is detected
//!   by probing its `--help`, as neutrality.sh's `leg_flags` does.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::proc::{Spawn, git, run, run_ok};

/// The pipeline crates, newest first: (crate directory, package).
const PIPELINE_CRATES: &[&str] = &["humanify-ag-cli", "humanify-cli"];

/// A release build of the whole pipeline (cold: ~2.5 min; the deadline
/// covers a loaded box).
pub const BUILD_DEADLINE: Duration = Duration::from_secs(60 * 60);

/// `--help` answers at once.
const HELP_DEADLINE: Duration = Duration::from_secs(60);

/// The pipeline crate of a checkout: its package and binary name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PipelineCrate {
    pub package: String,
    pub binary: String,
}

/// What was built.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BinRecord {
    pub path: PathBuf,
    pub sha256: String,
    /// The full sha of the checkout's HEAD.
    pub commit: String,
    /// The checkout had uncommitted changes to tracked files.
    pub dirty: bool,
    pub name: String,
}

/// Read a pipeline crate's Cargo.toml: `[package] name` and the `[[bin]]`
/// whose `path` is `src/main.rs`. Line-based (the harness does not parse
/// TOML; the one TOML reader is the pipeline's config owner).
pub fn pipeline_crate_from_manifest(cargo_toml: &str) -> Option<PipelineCrate> {
    let mut section = "";
    let mut package = None;
    let mut bin_name: Option<String> = None;
    let mut bins: Vec<(Option<String>, Option<String>)> = Vec::new();
    let value = |line: &str, key: &str| -> Option<String> {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
    };
    for raw in cargo_toml.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.starts_with('[') {
            section = if line == "[[bin]]" {
                bins.push((None, None));
                "bin"
            } else if line == "[package]" {
                "package"
            } else {
                "other"
            };
            continue;
        }
        match section {
            "package" => package = package.or_else(|| value(line, "name")),
            "bin" => {
                let last = bins.last_mut().expect("a [[bin]] header came first");
                last.0 = last.0.take().or_else(|| value(line, "name"));
                last.1 = last.1.take().or_else(|| value(line, "path"));
            }
            _ => {}
        }
    }
    for (name, path) in bins {
        if path.as_deref() == Some("src/main.rs") {
            bin_name = name;
        }
    }
    Some(PipelineCrate {
        package: package?,
        binary: bin_name?,
    })
}

/// The pipeline crate of the checkout at `root`.
pub fn pipeline_crate_at(root: &Path) -> Result<PipelineCrate, String> {
    for dir in PIPELINE_CRATES {
        let manifest = root.join("crates").join(dir).join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            return pipeline_crate_from_manifest(&text).ok_or_else(|| {
                format!(
                    "{}: no [package] name or no [[bin]] at src/main.rs",
                    manifest.display()
                )
            });
        }
    }
    Err(format!(
        "{} has no pipeline crate (crates/humanify-ag-cli or crates/humanify-cli): a checkout \
         from before the cutover has no pipeline binary",
        root.display()
    ))
}

/// The pipeline crate at `commit` in the repo at `repo`, without a
/// checkout (`git show <commit>:crates/<dir>/Cargo.toml`).
pub fn pipeline_crate_at_commit(repo: &Path, commit: &str) -> Result<PipelineCrate, String> {
    for dir in PIPELINE_CRATES {
        let spec = format!("{commit}:crates/{dir}/Cargo.toml");
        if let Ok(text) = git(repo, &["show", &spec]) {
            return pipeline_crate_from_manifest(&text)
                .ok_or_else(|| format!("{spec}: no [package] name or no [[bin]] at src/main.rs"));
        }
    }
    Err(format!(
        "{commit} has no pipeline crate (crates/humanify-ag-cli or crates/humanify-cli): a ref \
         from before the cutover has no pipeline binary"
    ))
}

/// Build the pipeline at the checkout `root` (`cargo build --release
/// --locked -p <its package>`) and describe the binary. Build output is
/// returned with a failure, never mixed into stdout.
/// `child_path` is the PATH cargo runs with (`env::HostEnv::child_path`:
/// the user-level cargo first).
pub fn build(root: &Path, child_path: &str) -> Result<BinRecord, String> {
    build_with_env(root, &[("PATH".to_string(), child_path.to_string())])
}

/// [`build`], cargo running with `env` set (PATH and anything a caller
/// plants for its children).
pub fn build_with_env(root: &Path, env: &[(String, String)]) -> Result<BinRecord, String> {
    let krate = pipeline_crate_at(root)?;
    let mut spawn = Spawn::new(
        "cargo",
        &["build", "--release", "--locked", "-p", &krate.package],
        BUILD_DEADLINE,
    )
    .cwd(root);
    spawn.env.extend(env.iter().cloned());
    run_ok(&spawn).map_err(|e| format!("cargo build failed in {}: {e}", root.display()))?;
    describe(root, &root.join("target/release").join(&krate.binary))
}

/// The record of an existing binary built from the checkout `root`.
pub fn describe(root: &Path, bin: &Path) -> Result<BinRecord, String> {
    let bytes = std::fs::read(bin).map_err(|e| format!("no binary at {}: {e}", bin.display()))?;
    let name = bin
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    Ok(BinRecord {
        path: bin.to_path_buf(),
        sha256: crate::committed::sha256_hex(&bytes),
        commit: git(root, &["rev-parse", "HEAD"])?,
        dirty: !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        name,
    })
}

/// Whether a binary's `--help` text is a LEGACY leg's: no `--config`
/// option (a baseline from before config sweep step 5 takes flags only).
pub fn is_legacy_help(help: &str) -> bool {
    !help.contains("--config")
}

/// Whether the binary is a LEGACY leg (probes its `--help`).
pub fn is_legacy_leg(bin: &Path) -> Result<bool, String> {
    let done = run(&Spawn::new(
        &bin.display().to_string(),
        &["--help"],
        HELP_DEADLINE,
    ))?;
    Ok(is_legacy_help(&String::from_utf8_lossy(&done.stdout)))
}

/// `--help` answers at once.
pub const HELP_PROBE_DEADLINE: Duration = HELP_DEADLINE;

/// Two commit spellings name the same commit (one is a prefix of the
/// other; an empty one is unknown and matches nothing).
pub fn same_commit(a: &str, b: &str) -> bool {
    !a.is_empty() && !b.is_empty() && (a.starts_with(b) || b.starts_with(a))
}

#[cfg(test)]
#[path = "bin_build_test.rs"]
mod bin_build_test;
