//! Where the harness's committed files live (`dev/`, docs/dev-harness.md
//! §2.1; moved there byte for byte by lane L5). Only this module names one.

use std::path::{Path, PathBuf};

/// The repository root this binary was built from.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The eval's committed config (pairs.json, noise-bands.json,
/// self-hop-reference.json).
pub const EVAL_DIR: &str = "dev/eval";
/// The eval's labels (the committed references and local runs).
pub const EVAL_RESULTS: &str = "dev/eval/results";
/// Every launch golden (lane L0's recordings and the run-launch goldens).
pub const LAUNCH_GOLDENS: &str = "dev/eval/goldens";
/// The library scorer's committed config (bands.json, data-roots.json).
pub const LIBRARY_DIR: &str = "dev/library";
/// The library corpora: each corpus manifest and its truth-layer manifests.
pub const LIBRARY_CORPORA: &str = "dev/library/corpora";
/// The library scorer's labels.
pub const LIBRARY_RESULTS: &str = "dev/library/results";
/// The concurrency census's allowlist.
pub const CONCURRENCY_ALLOWLIST: &str = "dev/census/concurrency-allowlist.json";
/// The clone census's allowlist (reviewed twin groups, each justified).
pub const CLONE_ALLOWLIST: &str = "dev/census/clone-allowlist.json";
/// The e2e fixtures (one `fixture.config.json` each).
pub const E2E_FIXTURES: &str = "test/e2e/fixtures";
/// The git-ignored per-host config (the LLM endpoint).
pub const LOCAL_CONFIG: &str = ".humanify.local.json";
