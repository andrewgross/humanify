//! The run's library-index cache (docs/design/library-names-plan.md
//! step 3, exp154): ONE root directory a run finds by itself —
//! `library.index_store_dir`, default `.humanify/library-index` under the
//! WORKING directory (exp174) — holding the merged indexes a run with no
//! `--library-index` searches, and everything a run-time build
//! (`--library-index-from-bundle`) reads or writes, so the second run
//! reuses the first one's work:
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
//! - `tools/<bundler>-<version>/` — the pinned bundler tools the build
//!   runs, one per recipe bundler (Bun, esbuild, webpack, rollup; owned by
//!   [`crate::tools`], exp181). A build that finds none for its recipe
//!   FETCHES the pinned one — announced in the run's notice, once per
//!   store.
//!
//! Nothing outside this root is ever autoloaded.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use humanify_ag_core::libindex::format::{INDEX_FORMAT, Manifest, check_manifest_stamps};
use humanify_ag_core::libindex::recipe::{DetectedRecipe, Recipe, RecipeBundler, choose_slice};
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

/// A name the bundle's evidence offered that the plan could not index:
/// not an npm package name at all (a banner's URL or prose — the seed
/// scan is a heuristic), or a package the registry does not know. Recorded
/// in `build.json` and printed in the run's notice — never silent, never
/// fatal (exp171: one wrong guess of the scanner must not fail a default
/// run).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DroppedSeed {
    pub name: String,
    pub reason: String,
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

/// One version an index spec's resolved plan indexes, with the rule that
/// chose it (exp184).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpecPlannedVersion {
    pub version: String,
    pub role: String,
    pub published: String,
    /// `sel=<id> … rule=<policy>` / `override=<id> …` / `dependents=<n> first=…`.
    pub attribution: String,
}

/// One package of an index spec's resolved plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpecSelection {
    pub name: String,
    /// Why the package is in the plan (`sel=<id> …`).
    pub attribution: String,
    pub versions: Vec<SpecPlannedVersion>,
    /// (version, reason), rule order.
    pub refused: Vec<(String, String)>,
}

/// The spec block of a spec-built index's `build.json` (exp184): the spec
/// file's sha256 and schema, and the RESOLVED PLAN — every package@version
/// and the rule that chose it, the versions refused with their reasons,
/// the packages a names-refusal skipped, and the tool pins. Same spec +
/// same registry snapshot ⇒ same plan.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SpecSidecar {
    pub schema: String,
    pub spec_sha256: String,
    pub as_of: String,
    pub default_policy: String,
    pub dependents: usize,
    /// bundler → the version the spec pins.
    pub tools: BTreeMap<String, String>,
    pub selections: Vec<SpecSelection>,
    /// refusal rule id → the package names it skipped.
    pub skipped: BTreeMap<String, Vec<String>>,
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
    /// The spec the build came from, and its resolved plan (exp184; a
    /// `--spec` build only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<SpecSidecar>,
    pub seeds: Vec<PlannedSeed>,
    /// The seeds the plan dropped, and why (exp171).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dropped: Vec<DroppedSeed>,
    /// The versions the build could not index (an install or bundle that
    /// failed, a pin the plan refused), and why (exp171) — the index holds
    /// the rest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed: Vec<DroppedSeed>,
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

/// The merged indexes `root` holds: every id-named directory with a
/// `manifest.json`, sorted (stale or foreign ones included — the store
/// search judges each). Empty when `root` does not exist. The question a
/// run with no `--library-index` asks before any work (exp174): an empty
/// store is a loud refusal, never a silent fallback.
pub fn stored_index_ids(root: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|name| looks_like_id(name) && root.join(name).join("manifest.json").is_file())
        .collect();
    ids.sort();
    ids
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
    /// Every usable index, newest first (`found` is the first): the run
    /// takes the newest that COVERS its bundle ([`uncovered`], exp171).
    pub usable: Vec<FoundIndex>,
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
        let rows: Vec<(
            Recipe,
            bool,
            &humanify_ag_core::libindex::format::SliceRecord,
        )> = m
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
    out.found = usable.first().cloned();
    out.usable = usable;
    out
}

/// Whether `name` can be an npm package name (npm's own rules: at most
/// 214 characters, lowercase URL-safe characters, an optional `@scope/`,
/// no leading `.` or `_`). The seed scan reads banners, so it offers URLs
/// and prose too (`http://mths.be/fromcodepoint` on a bun-dev bundle):
/// those are dropped before anything asks the registry.
pub fn valid_package_name(name: &str) -> bool {
    let part_ok = |p: &str| {
        !p.is_empty()
            && !p.starts_with('.')
            && !p.starts_with('_')
            && p.bytes().all(|b| {
                b.is_ascii_lowercase()
                    || b.is_ascii_digit()
                    || matches!(b, b'-' | b'.' | b'_' | b'~')
            })
    };
    if name.is_empty() || name.len() > 214 {
        return false;
    }
    match name.strip_prefix('@') {
        Some(scoped) => match scoped.split_once('/') {
            Some((scope, pkg)) => part_ok(scope) && part_ok(pkg),
            None => false,
        },
        None => part_ok(name),
    }
}

/// The package names `seeds` reveal that the index built with `sidecar`
/// does not cover (exp171): a run-time index covers what its build planned
/// or dropped; an index with no sidecar, or no seed record (an offline full
/// build), covers everything. Names that cannot be packages never count.
pub fn uncovered(sidecar: Option<&BuildSidecar>, seeds: &[Seed]) -> Vec<String> {
    let Some(side) = sidecar.filter(|s| !s.seeds.is_empty() || !s.dropped.is_empty()) else {
        return Vec::new();
    };
    let known: std::collections::BTreeSet<&str> = side
        .seeds
        .iter()
        .map(|s| s.name.as_str())
        .chain(side.dropped.iter().map(|d| d.name.as_str()))
        .collect();
    seeds
        .iter()
        .filter(|s| valid_package_name(&s.name) && !known.contains(s.name.as_str()))
        .map(|s| s.name.clone())
        .collect()
}

/// Whether a registry error says the package does not exist (npm's E404),
/// as opposed to the registry or npm itself failing.
fn is_not_found(error: &str) -> bool {
    error.contains("E404") || error.contains("404 Not Found")
}

/// The run-time build's package list resolved into a [`BuildConfig`]:
/// every seed becomes a PIN — the version the bundle declared, else the
/// newest version at `as_of` (the registry snapshot answers; the plan
/// refuses anything published after). A seed that is not an npm package
/// name, or that the registry does not know (npm's E404), is DROPPED with
/// its reason (exp171); any other registry failure — npm missing, the
/// registry unreachable, a missing snapshot `offline` — is loud. `offline`
/// keeps the whole thing on the recorded snapshot (the test path). Over
/// `max_packages` seeds is a loud refusal BEFORE anything is fetched. The
/// build records each unit's original names (`names`, exp159): names mode
/// restores them.
pub fn plan_runtime(
    root: &Path,
    seeds: Vec<Seed>,
    recipes: Vec<RecipeBuild>,
    as_of: &str,
    max_packages: usize,
    offline: bool,
) -> Result<(BuildConfig, Vec<PlannedSeed>, Vec<DroppedSeed>), String> {
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
    let mut dropped: Vec<DroppedSeed> = Vec::new();
    let drop = |seed: &Seed, reason: String| DroppedSeed {
        name: seed.name.clone(),
        reason,
        evidence: seed.evidence.clone(),
    };
    for seed in seeds {
        if !valid_package_name(&seed.name) {
            dropped.push(drop(&seed, "not an npm package name".to_string()));
            continue;
        }
        let version = match &seed.version {
            Some(v) => v.clone(),
            None => {
                let packument = match registry::load(&registry_dir, &seed.name, offline) {
                    Ok((packument, _)) => packument,
                    Err(e) if !offline && is_not_found(&e) => {
                        dropped.push(drop(&seed, "the npm registry does not know it".to_string()));
                        continue;
                    }
                    Err(e) => {
                        return Err(format!(
                            "the bundle reveals package {}, but the registry answers nothing: {e}",
                            seed.name
                        ));
                    }
                };
                match select_versions(&packument, &as_of, &[], VersionPolicy::AsOf)
                    .selected
                    .first()
                {
                    Some(s) => s.version.clone(),
                    None => {
                        dropped.push(drop(&seed, format!("no version published by {as_of}")));
                        continue;
                    }
                }
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
        spec: None,
        packages: Vec::new(),
        as_of,
        registry: registry_dir,
        store: root.join(STORE_DIR),
        work: root.join(WORK_DIR),
        out: None,
        recipes,
        jobs: 4,
        merge_jobs: 1,
        offline,
        policy: VersionPolicy::PinsOnly,
        pinned,
        merge_only: false,
        sponge_regions: Default::default(),
        names: true,
        refuse: Vec::new(),
        chunk: None,
        quiet: true,
    };
    Ok((cfg, planned, dropped))
}

/// A run-time build, planned and not yet run: what the run's notice
/// prints BEFORE anything is installed (exp171) — how many versions the
/// store already holds, how many must be built, and what was dropped.
pub struct RuntimePlan {
    pub cfg: BuildConfig,
    pub planned: Vec<PlannedSeed>,
    pub dropped: Vec<DroppedSeed>,
    /// Versions with at least one shard missing from the store (built).
    pub to_build: usize,
    /// Versions whose every shard the store already holds (reused).
    pub reused: usize,
}

/// Plan the build a run needs ([`plan_runtime`] then the builder's own
/// [`build::plan`], which reads the store for each version's shards), and
/// stop: the caller prints its notice, then [`build_planned`] runs it.
pub fn plan_build(
    root: &Path,
    seeds: Vec<Seed>,
    recipes: Vec<RecipeBuild>,
    as_of: &str,
    max_packages: usize,
    trace: Trace<'_>,
) -> Result<RuntimePlan, String> {
    let (cfg, planned, dropped) = plan_runtime(root, seeds, recipes, as_of, max_packages, false)?;
    let plan = build::plan(&cfg, trace)?;
    let to_build = plan.tasks.iter().filter(|t| !t.missing.is_empty()).count();
    Ok(RuntimePlan {
        reused: plan.tasks.len() - to_build,
        to_build,
        cfg,
        planned,
        dropped,
    })
}

/// The disk and time a run-time build of `to_build` versions is expected
/// to cost — what the run's notice prints before it starts (exp171).
/// Per-version figures CALIBRATED on measured builds (exp171's README,
/// "What the index costs"), at the builder's 4 jobs. A fetch of the
/// recipe's tool adds its [`crate::tools::ToolPin::fetch_bytes`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildEstimate {
    pub bytes_low: u64,
    pub bytes_high: u64,
    pub seconds_low: u64,
    pub seconds_high: u64,
}

/// What one built version costs for a recipe bundler: disk (store shards,
/// merged index, install work directory) and wall time at the
/// builder's 4 jobs, each (low, high) — the per-app means of measured
/// run-time builds. Bun: three Bun-built apps (exp171: 1.5, 12.5 and 39 MB
/// and 0.5, 2 and 14 s per built version; 17 versions built). esbuild,
/// webpack, rollup: Bun's range scaled by each kind's measured ratio to
/// Bun on the same 30 versions built once per kind (exp181 calib.sh: disk
/// 0.62 / 0.95 / 0.58 of Bun's, wall time 0.56 / 2.3 / 2.1 of Bun's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VersionCost {
    pub bytes: (u64, u64),
    pub seconds: (u64, u64),
}

/// [`VersionCost`] of `bundler`.
pub fn version_cost(bundler: RecipeBundler) -> VersionCost {
    match bundler {
        RecipeBundler::Bun => VersionCost {
            bytes: (12_000_000, 40_000_000),
            seconds: (2, 15),
        },
        RecipeBundler::Esbuild => VersionCost {
            bytes: (8_000_000, 25_000_000),
            seconds: (1, 9),
        },
        RecipeBundler::Webpack => VersionCost {
            bytes: (11_000_000, 38_000_000),
            seconds: (5, 35),
        },
        RecipeBundler::Rollup => VersionCost {
            bytes: (7_000_000, 23_000_000),
            seconds: (4, 32),
        },
    }
}

/// The fixed part of every build (registry, plan, merge), seconds.
pub const BUILD_FIXED_SECONDS: u64 = 10;

/// [`BuildEstimate`] for `to_build` versions built by `bundler`'s recipe,
/// plus `fetch_bytes` for the tool the build fetches first (0: the store
/// holds it).
pub fn estimate_build(bundler: RecipeBundler, to_build: usize, fetch_bytes: u64) -> BuildEstimate {
    let n = to_build as u64;
    let c = version_cost(bundler);
    BuildEstimate {
        bytes_low: n * c.bytes.0 + fetch_bytes,
        bytes_high: n * c.bytes.1 + fetch_bytes,
        seconds_low: BUILD_FIXED_SECONDS + n * c.seconds.0,
        seconds_high: BUILD_FIXED_SECONDS + n * c.seconds.1,
    }
}

/// What a run-time build produced: the index's directory (a complete
/// merged index, moved to its id's name) and its sidecar.
#[derive(Clone, Debug)]
pub struct RuntimeBuild {
    pub index_id: String,
    pub dir: PathBuf,
    pub sidecar: BuildSidecar,
}

/// Run a planned build into the run's cache: build what the store does not
/// hold, merge into `<root>/<index id>/` and write the `build.json`
/// sidecar. A version that cannot be indexed is recorded (`failed`) and the
/// index holds the rest (exp171); an index with NOTHING in it fails loudly.
pub fn build_planned(
    root: &Path,
    runtime: RuntimePlan,
    trace: Trace<'_>,
) -> Result<RuntimeBuild, String> {
    let RuntimePlan {
        cfg,
        planned,
        dropped,
        ..
    } = runtime;
    let temp = root.join(format!(
        ".building-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default()
    ));
    let cfg = BuildConfig {
        spec: None,
        out: Some(temp.clone()),
        ..cfg
    };
    let outcome = build::build(&cfg, false, trace)
        .map_err(|e| format!("the run-time index build failed: {e}"))?;
    let fail = |stage: &str, what: String| -> String {
        let _ = std::fs::remove_dir_all(&temp);
        format!("the run-time index build failed ({stage}): {what}")
    };
    // exp171: a version that fails to install or bundle, or that the plan
    // refuses (unpublished, a pin past as-of), leaves the index without its
    // rows — recorded in the sidecar and the run's closing line — never a
    // failed DEFAULT run: one odd package (a peer-dependency conflict) must
    // not take every run of an app down. Only an index with nothing in it
    // fails.
    let mut failed: Vec<DroppedSeed> = outcome
        .errors
        .iter()
        .map(|(label, e)| DroppedSeed {
            name: label.clone(),
            reason: format!("install/bundle failed: {}", first_line(e)),
            evidence: String::new(),
        })
        .collect();
    // Only the versions this build PINNED: the plan's refusal list also
    // records every other version of a pinned package it passed over.
    let pinned: std::collections::BTreeSet<String> = planned
        .iter()
        .map(|p| format!("{}@{}", p.name, p.version))
        .collect();
    failed.extend(
        outcome
            .refused
            .iter()
            .filter(|r| pinned.contains(&format!("{}@{}", r.name, r.version)))
            .map(|r| DroppedSeed {
                name: format!("{}@{}", r.name, r.version),
                reason: format!("refused: {}", r.reason),
                evidence: String::new(),
            }),
    );
    failed.sort_by(|a, b| a.name.cmp(&b.name));
    failed.dedup_by(|a, b| a.name == b.name);
    if outcome.built + outcome.reused == 0 {
        let what = failed
            .iter()
            .map(|f| format!("{}: {}", f.name, f.reason))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(fail(
            "install/bundle",
            format!("no version could be indexed — {what}"),
        ));
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
        dropped,
        failed,
        versions_in_index: outcome.tasks,
        packages,
        refused: outcome.refused.len(),
        built: outcome.built,
        reused: outcome.reused,
        stage_ms,
        completed: now_iso(),
        spec: None,
    };
    write_build_sidecar(&dir, &sidecar)?;
    Ok(RuntimeBuild {
        index_id: outcome.index_id,
        dir,
        sidecar,
    })
}

/// The first line of a (possibly long, multi-line) npm error.
fn first_line(e: &str) -> String {
    e.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .chars()
        .take(300)
        .collect()
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` now (the sidecar's `completed`).
pub fn now_iso() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    humanify_ag_core::profiling::iso_from_unix_ms(ms)
}
