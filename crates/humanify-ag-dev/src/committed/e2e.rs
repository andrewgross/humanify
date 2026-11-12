//! The e2e fixtures' `fixture.config.json` (read by `scripts/e2e.ts`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::{Loaded, load_json};
use crate::paths::E2E_FIXTURES;

/// One fixture's config.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FixtureConfig {
    pub package: String,
    pub repo: String,
    pub source_strategy: SourceStrategy,
    pub entry_points: Vec<String>,
    /// A bundled build (absent: the plain fixtures).
    pub bundle: Option<bool>,
    /// The input's behaviour is the compiled executable's, recorded beside
    /// each build (`build/v<ver>/compiled.stdout`): an unpacked Bun app that
    /// Node cannot run as-is (it reads embedded files through Bun's
    /// runtime, exp217).
    pub recorded_behavior: Option<bool>,
    pub expect: Option<Expect>,
    pub version_pairs: Vec<VersionPair>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceStrategy {
    #[serde(rename = "type")]
    pub kind: String,
}

/// What the run must detect and produce.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Expect {
    pub bundler: Option<String>,
    pub unpack_adapter: Option<String>,
    pub vendor: Option<bool>,
    pub split_method: Option<String>,
    /// Marker pairs the split must keep in different files.
    pub apart: Option<Vec<Vec<String>>>,
    /// Asset files the output must hold.
    pub assets: Option<Vec<String>>,
    /// Strings that must stay in the output.
    pub stays: Option<Vec<String>>,
    /// Markers by the top-level folder they must land in (`src`, `vendor`).
    pub placement: Option<BTreeMap<String, Vec<String>>>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VersionPair {
    pub v1: String,
    pub v2: String,
    pub description: String,
}

/// Every fixture's config, by fixture directory name (fixtures without a
/// config file are skipped, as e2e.ts skips them).
pub fn fixture_configs(repo: &Path) -> Result<BTreeMap<String, Loaded<FixtureConfig>>, String> {
    let dir = repo.join(E2E_FIXTURES);
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let file = path.join("fixture.config.json");
        if file.exists() {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            out.insert(name, load_json(&file)?);
        }
    }
    Ok(out)
}
