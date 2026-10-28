//! The run's library-index cache (docs/design/library-names-plan.md
//! step 3, exp154): ONE root directory a run finds by itself —
//! `library.index_store_dir`, default `.humanify/library-index` — holding
//! everything a run-time build reads or writes, so the second run reuses
//! the first one's work:
//!
//! - `store/` — the shard store ([`build::BuildConfig::store`]): one
//!   shard per `pkg@version` per recipe and bundler version. This is what
//!   makes reuse across runs and app versions automatic (shard keys).
//! - `<index id>/` — a merged index: a COMPLETE directory (its
//!   `manifest.json`'s sha256 IS the id), plus its `build.json` sidecar.
//!   Times and counts live in the sidecar, never the manifest — the same
//!   shards must give the same id whatever the clock said.
//! - `registry/` — the packument snapshot the build reads (and fills, by
//!   fetching, when a seed's package is not recorded yet).
//! - `work/` — the build's install directories.
//! - `tools/bun-<version>/` — the pinned Bun installs the build uses (a
//!   `bun` npm package installed under the directory; [`bun_binary`] says
//!   the exact command). The run NEVER downloads one.
//!
//! Nothing outside this root is ever autoloaded.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use humanify_core::libindex::format::{
    INDEX_FORMAT, Manifest, check_manifest_stamps, compare_versions,
};
use humanify_core::libindex::recipe::{DetectedRecipe, Recipe, choose_slice};
use serde::{Deserialize, Serialize};

use crate::asof::{VersionPolicy, parse_as_of, select_versions};
use crate::build::{self, BuildConfig, Trace};
use crate::recipe::RecipeBuild;
use crate::registry;

/// The shard store's directory inside the cache root.
pub const STORE_DIR: &str = "store";
/// The packument snapshot's directory inside the cache root.
pub const REGISTRY_DIR: &str = "registry";
/// The install work directories' root inside the cache root.
pub const WORK_DIR: &str = "work";
/// The pinned bundler tools' directory inside the cache root.
pub const TOOLS_DIR: &str = "tools";
/// The sidecar's name, inside each merged index's directory.
pub const BUILD_SIDECAR: &str = "build.json";
/// The sidecar's schema (the same family exp152's full indexes write).
pub const BUILD_SIDECAR_SCHEMA: &str = "library-index-build/1";

/// One package the bundle itself reveals. The run-time build's package
/// list is ONLY these (plus the dependency closure installing them pulls
/// in — npm resolves it at the seed's publish time); there is NO
/// base-list bootstrap in this lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Seed {
    pub name: String,
    /// The version the bundle spelled, when it did.
    pub version: Option<String>,
    /// Where the evidence came from, for `build.json` and the report.
    pub evidence: String,
}

/// What `plan_runtime` decided per seed (the sidecar's seed rows).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PlannedSeed {
    pub name: String,
    pub version: String,
    /// `declared` (the bundle spelled the version) or `newest-at-as-of`.
    pub how: String,
    pub evidence: String,
}

/// One slice a build sidecar records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SidecarSlice {
    pub slice: String,
    pub recipe: String,
    pub bundler: String,
    pub bundler_version: String,
}

/// `build.json` (schema `library-index-build/1`): how an index was built
/// — tool versions, as-of, the seed list, counts and per-stage wall
/// times. A SIDECAR because the index id is the manifest's hash: times in
/// the manifest would give identical indexes different ids.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BuildSidecar {
    pub schema: String,
    pub index_id: String,
    /// What built it ("run-time build (exp154)" or the offline lane's
    /// name).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub built_from: String,
    pub as_of: String,
    pub format: String,
    pub slices: Vec<SidecarSlice>,
    pub seeds: Vec<PlannedSeed>,
    pub versions_in_index: usize,
    pub packages: usize,
    pub refused: usize,
    /// Versions whose shards this build made, and versions the store
    /// already held complete.
    #[serde(default)]
    pub built: usize,
    #[serde(default)]
    pub reused: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub stage_ms: BTreeMap<String, u64>,
    /// UTC ISO time when the build finished.
    pub completed: String,
}

/// Write an index's sidecar (`<dir>/build.json`).
pub fn write_build_sidecar(dir: &Path, side: &BuildSidecar) -> Result<(), String> {
    let text = serde_json::to_string_pretty(side).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(BUILD_SIDECAR), format!("{text}\n"))
        .map_err(|e| format!("{}: {e}", dir.join(BUILD_SIDECAR).display()))
}

/// Read an index's sidecar, if it has one.
pub fn read_build_sidecar(dir: &Path) -> Result<Option<BuildSidecar>, String> {
    let path = dir.join(BUILD_SIDECAR);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let side: BuildSidecar =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    if side.schema != BUILD_SIDECAR_SCHEMA {
        return Err(format!(
            "{}: schema {} (this build reads {})",
            path.display(),
            side.schema,
            BUILD_SIDECAR_SCHEMA
        ));
    }
    Ok(Some(side))
}

/// Whether `name` is an index id (64 hex digits — a manifest's sha256):
/// the store's merged indexes are `<root>/<id>/`, every other directory
/// belongs to the layout ([`STORE_DIR`], [`TOOLS_DIR`], ...).
pub fn looks_like_id(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// The index the search settled on, with the properties it matched on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoundIndex {
    pub id: String,
    pub dir: PathBuf,
    pub slice: String,
    pub bundler: String,
    pub bundler_version: String,
    /// The index's own as-of date, recorded in the trail; whether it
    /// COVERS the bundle is deliberately not judged in this lane.
    pub as_of: String,
    /// The sidecar's `completed` time, when it has one.
    pub completed: Option<String>,
    /// What matched, for the report trail.
    pub verdict: String,
}

/// One candidate the search looked at, and what it said about it.
pub type Candidate = (String, String);

/// What searching the store found: every candidate's verdict (even the
/// refused ones — the report trail says why nothing was reused), and the
/// newest usable index there is.
#[derive(Clone, Debug, Default)]
pub struct StoreSearch {
    pub root: PathBuf,
    /// Why the root itself had nothing to say (it may not exist yet).
    pub note: Option<String>,
    pub candidates: Vec<Candidate>,
    pub found: Option<FoundIndex>,
}

/// Find the newest index under `root` with the slice the bundle's
/// DETECTED recipe needs ([`choose_slice`] — the same question the
/// lookup asks, so a found index is one the run can actually load).
/// As-of FRESHNESS and the bundle's own bundler version are not judged
/// here (a JS bundle's Bun version is not detected in this lane); the
/// chosen slice's versions are recorded in the trail instead. Nothing
/// outside `root` is read.
pub fn search_store(root: &Path, detected: &DetectedRecipe) -> StoreSearch {
    let mut out = StoreSearch {
        root: root.to_path_buf(),
        ..Default::default()
    };
    let entries = match std::fs::read_dir(root) {
        Ok(rd) => {
            let mut names: Vec<String> = rd
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            names.sort();
            names
        }
        Err(_) => {
            out.note = Some(format!(
                "the store directory {} does not exist yet (the first build \
                 creates it)",
                root.display()
            ));
            return out;
        }
    };
    let mut usable: Vec<FoundIndex> = Vec::new();
    for name in entries {
        if name.starts_with('.') {
            continue; // the in-progress `.building-*` directory
        }
        let dir = root.join(&name);
        if !looks_like_id(&name) {
            out.candidates.push((
                name,
                "not an index id (a store layout directory)".to_string(),
            ));
            continue;
        }
        let text = match std::fs::read_to_string(dir.join("manifest.json")) {
            Ok(t) => t,
            Err(e) => {
                out.candidates
                    .push((name, format!("manifest.json unreadable: {e}")));
                continue;
            }
        };
        if let Err(e) = check_manifest_stamps(&text) {
            out.candidates.push((name, e));
            continue;
        }
        let Ok(m) = serde_json::from_str::<Manifest>(&text) else {
            out.candidates
                .push((name, "manifest.json is not an index manifest".to_string()));
            continue;
        };
        if let Err(e) = m.check_compatible() {
            out.candidates.push((name, e));
            continue;
        }
        // The same question the lookup asks later, asked through the same
        // function: a FOUND index is one the run can load (choose_slice's
        // recipe/target semantics, never a looser local twin).
        let rows: Vec<(Recipe, bool, &humanify_core::libindex::format::SliceRecord)> = m
            .slices
            .iter()
            .filter_map(|s| {
                Recipe::parse(&s.recipe)
                    .ok()
                    .map(|r| (r, s.default_for_bundler, s))
            })
            .collect();
        let parsed: Vec<(Recipe, bool)> = rows.iter().map(|(r, d, _)| (*r, *d)).collect();
        let slice = match choose_slice(&parsed, detected) {
            Ok((recipe, _why)) => match rows.iter().find(|(r, _, _)| *r == *recipe) {
                Some((_, _, rec)) => *rec,
                // unreachable: every parsed row came from a chosen recipe
                None => continue,
            },
            Err(why) => {
                out.candidates.push((name, why));
                continue;
            }
        };
        let completed = read_build_sidecar(&dir)
            .ok()
            .flatten()
            .map(|s| s.completed)
            .filter(|c| !c.is_empty());
        let found = FoundIndex {
            id: name.clone(),
            dir,
            slice: slice.slice.clone(),
            bundler: slice.bundler.clone(),
            bundler_version: slice.version.clone(),
            as_of: m.as_of.clone(),
            completed,
            verdict: format!(
                "usable: {}@{} ({}), as-of {} (freshness not checked)",
                slice.bundler, slice.version, slice.slice, m.as_of
            ),
        };
        out.candidates.push((name, found.verdict.clone()));
        usable.push(found);
    }
    // the newest by the sidecar's completed time (ISO: text order is time
    // order), else by as-of; the first in directory order breaks ties
    usable.sort_by(|a, b| {
        let ka = (a.completed.as_deref().unwrap_or(""), a.as_of.as_str());
        let kb = (b.completed.as_deref().unwrap_or(""), b.as_of.as_str());
        kb.cmp(&ka)
    });
    out.found = usable.into_iter().next();
    out
}

/// The Bun binary the run-time build uses: `<root>/tools/bun-<V>/`, an
/// npm `bun` package install whose binary is at
/// `node_modules/.bin/bun` (the layout exp152 pinned). `version` None —
/// the run does not know the bundle's own Bun version in this lane —
/// takes the NEWEST installed, which the run records in its diagnostics.
/// Nothing is ever downloaded: a cache without a Bun is a loud failure
/// naming the exact fetch command.
pub fn bun_binary(root: &Path, version: Option<&str>) -> Result<(PathBuf, String), String> {
    let tools = root.join(TOOLS_DIR);
    let bin_of = |v: &str| tools.join(format!("bun-{v}")).join("node_modules/.bin/bun");
    if let Some(v) = version {
        if bin_of(v).is_file() {
            return Ok((bin_of(v), v.to_string()));
        }
    } else {
        let mut found: Vec<String> = std::fs::read_dir(&tools)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| {
                n.starts_with("bun-") && tools.join(n).join("node_modules/.bin/bun").is_file()
            })
            .filter_map(|n| n.strip_prefix("bun-").map(str::to_string))
            .collect();
        // newest first (semver, not text: 1.10.0 > 1.4.0)
        found.sort_by(|a, b| compare_versions(b, a));
        if let Some(v) = found.into_iter().next() {
            return Ok((bin_of(&v), v));
        }
    }
    Err(format!(
        "no Bun under {} — the run-time index build needs one and never \
         downloads it; fetch it once (the build looks for \
         {0}/bun-<version>/node_modules/.bin/bun): \
         npm install bun@1.4.0 --prefix {0}/bun-1.4.0",
        tools.display()
    ))
}

/// The run-time build's package list resolved into a [`BuildConfig`]:
/// every seed becomes a PIN — the version the bundle declared, else the
/// newest version at `as_of` (the registry snapshot answers; the plan
/// refuses anything published after). `offline` keeps the whole thing on
/// the recorded snapshot (the test path). Over `max_packages` seeds is a
/// loud refusal BEFORE anything is fetched.
pub fn plan_runtime(
    root: &Path,
    seeds: Vec<Seed>,
    recipes: Vec<RecipeBuild>,
    as_of: &str,
    max_packages: usize,
    offline: bool,
) -> Result<(BuildConfig, Vec<PlannedSeed>), String> {
    if seeds.len() > max_packages {
        return Err(format!(
            "the bundle reveals {} package(s), over \
             library.index_build_max_packages = {max_packages}: the run-time \
             build refuses to index that much — curate the bundle's evidence \
             or raise the cap",
            seeds.len()
        ));
    }
    let as_of = parse_as_of(as_of)?;
    let registry_dir = root.join(REGISTRY_DIR);
    for dir in [&registry_dir, &root.join(STORE_DIR), &root.join(WORK_DIR)] {
        std::fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    let mut pinned: Vec<String> = Vec::new();
    let mut planned: Vec<PlannedSeed> = Vec::new();
    for seed in seeds {
        let version = match &seed.version {
            Some(v) => v.clone(),
            None => {
                let (packument, _) =
                    registry::load(&registry_dir, &seed.name, offline).map_err(|e| {
                        format!(
                            "the bundle reveals package {}, but the registry answers nothing: {e}",
                            seed.name
                        )
                    })?;

                select_versions(&packument, &as_of, &[], VersionPolicy::AsOf)
                    .selected
                    .first()
                    .map(|s| s.version.clone())
                    .ok_or_else(|| {
                        format!(
                            "the bundle reveals package {}, which has no \
                             version published by {as_of}",
                            seed.name
                        )
                    })?
            }
        };
        planned.push(PlannedSeed {
            name: seed.name.clone(),
            version: version.clone(),
            how: if seed.version.is_some() {
                "declared".to_string()
            } else {
                "newest-at-as-of".to_string()
            },
            evidence: seed.evidence.clone(),
        });
        pinned.push(format!("{}@{}", seed.name, version));
    }
    let cfg = BuildConfig {
        packages: Vec::new(),
        as_of,
        registry: registry_dir,
        store: root.join(STORE_DIR),
        work: root.join(WORK_DIR),
        out: None,
        recipes,
        jobs: 4,
        offline,
        policy: VersionPolicy::PinsOnly,
        pinned,
        merge_only: false,
        sponge_regions: Default::default(),
        names: false,
        quiet: true,
    };
    Ok((cfg, planned))
}

/// What a run-time build produced: the index's directory (a complete
/// merged index, moved to its id's name) and its sidecar.
#[derive(Clone, Debug)]
pub struct RuntimeBuild {
    pub index_id: String,
    pub dir: PathBuf,
    pub sidecar: BuildSidecar,
}

/// Build the index a run needs, into its cache: plan the seeds
/// ([`plan_runtime`]), build what the store does not hold, merge into
/// `<root>/<index id>/` and write the `build.json` sidecar. EVERY failure
/// is loud (a task error, a refused pin, an unknown package) — the run
/// exits 1, never silently does without the index.
pub fn build_runtime(
    root: &Path,
    seeds: Vec<Seed>,
    recipes: Vec<RecipeBuild>,
    as_of: &str,
    max_packages: usize,
    trace: Trace<'_>,
) -> Result<RuntimeBuild, String> {
    let (cfg, planned) = plan_runtime(root, seeds, recipes, as_of, max_packages, false)?;
    let temp = root.join(format!(
        ".building-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    ));
    let cfg = BuildConfig {
        out: Some(temp.clone()),
        ..cfg
    };
    let outcome = build::build(&cfg, false, trace)
        .map_err(|e| format!("the run-time index build failed: {e}"))?;
    let fail = |stage: &str, what: String| -> String {
        let _ = std::fs::remove_dir_all(&temp);
        format!("the run-time index build failed ({stage}): {what}")
    };
    if !outcome.errors.is_empty() {
        let what = outcome
            .errors
            .iter()
            .map(|(label, e)| format!("{label}: {e}"))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(fail("install/bundle", what));
    }
    if !outcome.refused.is_empty() {
        let what = outcome
            .refused
            .iter()
            .map(|r| format!("{}@{}: {}", r.name, r.version, r.reason))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(fail("plan", what));
    }
    let dir = root.join(&outcome.index_id);
    if dir.is_dir() {
        // someone built these exact shards into this exact index already
        let _ = std::fs::remove_dir_all(&temp);
    } else {
        std::fs::rename(&temp, &dir)
            .map_err(|e| format!("moving the fresh index into place ({}): {e}", dir.display()))?;
    }
    // `versions_in_index` counts the version units (the "versions" of the
    // report's outcome line); `packages` the DISTINCT pinned packages.
    let packages = planned
        .iter()
        .map(|s| s.name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let stage_ms = match trace.stages {
        Some(map) => map.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        None => BTreeMap::new(),
    };
    let sidecar = BuildSidecar {
        schema: BUILD_SIDECAR_SCHEMA.to_string(),
        index_id: outcome.index_id.clone(),
        built_from: "run-time build (exp154): the bundle's own revealed packages".to_string(),
        as_of: cfg.as_of.clone(),
        format: INDEX_FORMAT.to_string(),
        slices: outcome
            .slices
            .iter()
            .map(|s| SidecarSlice {
                slice: s.slice.clone(),
                recipe: s.recipe.clone(),
                bundler: s.bundler.clone(),
                bundler_version: s.version.clone(),
            })
            .collect(),
        seeds: planned,
        versions_in_index: outcome.tasks,
        packages,
        refused: outcome.refused.len(),
        built: outcome.built,
        reused: outcome.reused,
        stage_ms,
        completed: now_iso(),
    };
    write_build_sidecar(&dir, &sidecar)?;
    Ok(RuntimeBuild {
        index_id: outcome.index_id,
        dir,
        sidecar,
    })
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` now (the sidecar's `completed`).
fn now_iso() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    humanify_core::profiling::iso_from_unix_ms(ms)
}
