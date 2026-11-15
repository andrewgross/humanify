//! `index build --replay <index-dir>` (exp204): rebuild an EXISTING
//! index's plan — its exact package@versions, as-of, recipes and bundler
//! pins, read from its manifest — under this binary's fingerprint forms.
//!
//! What it is for: a fingerprint-form era (`units/N`, `vendor-content-key/N`)
//! re-keys every module shard, while the installs and bundles the shards
//! were fingerprinted from do not change. A replay over the same store and
//! work dir reuses every build whose stamp matches (`build::ensure_build`)
//! and re-runs only the fingerprinting and the merge, so the new index
//! differs from the old one by the forms alone — the comparison an
//! adoption decision needs. A spec cannot express that: an index built
//! from pins (cb64c1f5…, exp152) has no spec, and the spec's rules are
//! deliberately not a package list (exp184).
//!
//! What it is not: a way to make a NEW index. The package universe is the
//! source index's, chosen by whatever made it; the spec stays the only way
//! to choose one.

use humanify_ag_core::libindex::format::Manifest;
use humanify_ag_core::libindex::recipe::Recipe;

use crate::recipe::RECIPE_VERSION;

/// What a replay rebuilds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayPlan {
    /// The source index's as-of (already normalised: it is the manifest's).
    pub as_of: String,
    /// Every version the source index holds, `name@version`, in manifest
    /// order. Its refused versions are not pins: they are not in it.
    pub pinned: Vec<String>,
    /// Each slice's recipe and its bundler's version, in manifest order
    /// (the first of each bundler is its default slice, as it was).
    pub recipes: Vec<(Recipe, String)>,
    /// The source index carries the unit names table (`names/N` stamp).
    pub names: bool,
}

/// Why an index cannot be replayed.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("replay: manifest.json: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error(
        "replay: slice {slice} was built under {built}, this binary bundles under {RECIPE_VERSION} — \
         its stored builds are not what this binary would build; make the index from a spec"
    )]
    RecipeVersion { slice: String, built: String },
    #[error("replay: {0}")]
    Recipe(#[from] humanify_ag_core::libindex::recipe::RecipeError),
}

/// The replay plan of the index whose `manifest.json` text is given.
pub fn replay_plan(manifest_text: &str) -> Result<ReplayPlan, ReplayError> {
    let m: Manifest = serde_json::from_str(manifest_text)?;
    let mut recipes = Vec::new();
    for s in &m.slices {
        if s.recipe_version != RECIPE_VERSION {
            return Err(ReplayError::RecipeVersion {
                slice: s.slice.clone(),
                built: s.recipe_version.clone(),
            });
        }
        let recipe = Recipe::parse(&s.recipe)?;
        recipes.push((recipe, s.version.clone()));
    }
    Ok(ReplayPlan {
        as_of: m.as_of,
        pinned: m
            .packages
            .iter()
            .map(|p| format!("{}@{}", p.name, p.version))
            .collect(),
        recipes,
        names: m.fingerprints.names.is_some(),
    })
}
