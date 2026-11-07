//! Where the harness's committed files are TODAY. The `dev/` move is lane
//! L5 (docs/dev-harness.md §2.1); until then every path is read where it
//! lives now, and only this module names one.

use std::path::{Path, PathBuf};

/// The repository root this binary was built from.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The 034 eval harness directory (pairs.json, bands, goldens, results).
pub const EVAL_DIR: &str = "experiments/034-eval-harness";
/// The eval's committed labels.
pub const EVAL_RESULTS: &str = "experiments/034-eval-harness/results";
/// The launch goldens recorded by lane L0.
pub const LAUNCH_GOLDENS: &str = "experiments/034-eval-harness/launch-goldens";
/// The exp096 library scorer's directory (corpora, layers, bands).
pub const LIBRARY_DIR: &str = "experiments/096-library-matching-score";
/// The library scorer's committed labels.
pub const LIBRARY_RESULTS: &str = "experiments/096-library-matching-score/results";
/// The concurrency census's allowlist.
pub const CONCURRENCY_ALLOWLIST: &str = "scripts/concurrency-allowlist.json";
/// The clone census's allowlist (reviewed twin groups, each justified).
pub const CLONE_ALLOWLIST: &str = "scripts/clone-allowlist.json";
/// The e2e fixtures (one `fixture.config.json` each).
pub const E2E_FIXTURES: &str = "test/e2e/fixtures";
/// The git-ignored per-host config (the LLM endpoint).
pub const LOCAL_CONFIG: &str = ".humanify.local.json";
