//! `.humanify.local.json`: the git-ignored, per-host file that holds the
//! LLM endpoint (the repo is public, so the address is never in a tracked
//! file). The one resolver of `llm.endpoint`'s default, replacing
//! `experiments/lib/llm-endpoint.sh`. Precedence:
//!
//! 1. the run's own `--endpoint`;
//! 2. `.humanify.local.json` at the checkout's root;
//! 3. the same file in the MAIN checkout (found through git's common dir),
//!    so a frozen or detached worktree uses the address its checkout has;
//! 4. none: an error naming both ways to set it — never an empty URL.

use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Loaded, load_json};
use crate::paths::LOCAL_CONFIG;

/// The file's contents.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalConfig {
    pub llm: LocalLlm,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalLlm {
    pub endpoint: String,
}

/// Load the file in `root`, if there is one.
pub fn local_config(root: &Path) -> Result<Option<Loaded<LocalConfig>>, String> {
    let path = root.join(LOCAL_CONFIG);
    if !path.exists() {
        return Ok(None);
    }
    let loaded: Loaded<LocalConfig> = load_json(&path)?;
    if loaded.value.llm.endpoint.trim().is_empty() {
        return Err(format!("{}: llm.endpoint is empty", path.display()));
    }
    Ok(Some(loaded))
}

/// The main checkout of the repo `root` belongs to (its git common dir's
/// parent), when that is not `root` itself.
fn main_checkout(root: &Path) -> Option<PathBuf> {
    let common = crate::proc::git(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )
    .ok()?;
    let main = Path::new(&common).parent()?.to_path_buf();
    let same = main.canonicalize().ok() == root.canonicalize().ok();
    (!same).then_some(main)
}

/// The endpoint a run uses, by the precedence above.
pub fn resolve_endpoint(flag: Option<&str>, root: &Path) -> Result<String, String> {
    if let Some(url) = flag {
        return Ok(url.to_string());
    }
    if let Some(cfg) = local_config(root)? {
        return Ok(cfg.value.llm.endpoint);
    }
    if let Some(main) = main_checkout(root)
        && let Some(cfg) = local_config(&main)?
    {
        return Ok(cfg.value.llm.endpoint);
    }
    Err(format!(
        "no LLM endpoint configured. Either pass --endpoint <url>, or create {}/{LOCAL_CONFIG} \
         (git-ignored; a worktree also reads the main checkout's copy) containing:\n  \
         {{\"llm\": {{\"endpoint\": \"http://<llm-host>:8000/v1\"}}}}",
        root.display()
    ))
}
