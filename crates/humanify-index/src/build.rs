//! The build: plan (as-of selection + which shards exist), then install,
//! bundle and fingerprint what is missing, then merge
//! (docs/design/library-index.md §2).
//!
//! Every fingerprint comes from humanify-core: the module key from
//! `modules::vendor_content::bundle_content_keys` with the pieces
//! `toolchain::resolve_toolchain` chooses for the slice's bundler, the
//! literal sets from `libindex::literals`. This file only moves files
//! around, runs npm and the bundlers, and reads sourcemaps.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use humanify_core::libindex::format::compare_versions;
use humanify_core::libindex::format::{
    PackageRecord, RefusedRecord, RegistrySnapshot, SliceRecord, fingerprint_versions,
};
use humanify_core::libindex::literals::{LiteralMode, code_literals, json_literals};
use humanify_core::libindex::recipe::Recipe;

use crate::asof::{VersionPolicy, normalize_time, select_versions};
use crate::merge::{MergeMeta, Merger};
use crate::recipe::{
    MAX_SOURCE_BYTES, RECIPE_VERSION, RecipeBuild, bundler_package, entry_specifiers,
    is_source_file, is_sponge_file, unresolved_specifiers,
};
use crate::registry;
use crate::shard::{
    Shard, ShardInputs, ShardKind, ShardLiteral, ShardModule, ShardNotes, ShardUnit, read_current,
    write_shard,
};
use crate::sourcemap::{SourceMap, lexical, owner_cached};

/// The two app entries every slice builds: (tag, entry file).
const ENTRIES: [(&str, &str); 2] = [("req", "entries/req.cjs"), ("dyn", "entries/dyn.mjs")];

/// A module span must take at least this share of its mapping segments
/// from one source file to be attributed (the lab's "pure" spans).
const PURITY: f64 = 0.9;

/// What a module shard's unit rows are, beyond the fingerprint: 2 = rows
/// of pre-bundled ("sponge") files are dropped ([`sponge_evidence`], exp097
/// step 3); 3 = the same rows, and the shard's notes now record the drops
/// (2's shards carried empty sponge notes); 4 = no file-name evidence, and
/// only a strict module-marker comment counts (2/3 dropped right rows:
/// esquery's own min build, glob comments that mention node_modules); 5 =
/// region-aware (exp124): a marker drops only the units inside the region
/// it opens, the file's own code outside every foreign region keeps its
/// rows ([`SpongeRegions`] says what the foreign regions become). In every
/// module shard's key.
pub const UNIT_ROWS_VERSION: &str = "unit-rows/5";

/// What the units inside a FOREIGN region of a pre-bundled file become (a
/// region whose bundler module marker names another package's
/// `node_modules` path; exp124).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SpongeRegions {
    /// No rows: the copy is nobody's here.
    #[default]
    Drop,
    /// Rows of the package the marker names, at the version its pnpm
    /// store path spells, as that package's file (no version: dropped).
    /// exp124 refuted it under scorer 9 on a truth gap (a nested chunk's
    /// publisher, exp125's scorer 10 `via`); exp130 re-judges it.
    Origin,
}

impl SpongeRegions {
    /// The unit-rows version this mode's module shards are keyed by.
    fn unit_rows_version(self) -> String {
        match self {
            SpongeRegions::Drop => UNIT_ROWS_VERSION.to_string(),
            SpongeRegions::Origin => format!("{UNIT_ROWS_VERSION}+origin"),
        }
    }
}

use humanify_core::libindex::units::UNIT_ROW_MIN_NODES;

pub struct BuildConfig {
    /// `name` or `name@version` (a pin), one per entry.
    pub packages: Vec<String>,
    /// Normalised (`asof::parse_as_of`).
    pub as_of: String,
    pub registry: PathBuf,
    pub store: PathBuf,
    pub work: PathBuf,
    pub out: Option<PathBuf>,
    /// The recipes to build (`--recipe`, in command-line order: the first
    /// of each bundler is its default slice).
    pub recipes: Vec<RecipeBuild>,
    pub jobs: usize,
    pub offline: bool,
    /// Which versions of each package to index.
    pub policy: VersionPolicy,
    /// `name@version` specs indexed at exactly these versions (no policy
    /// picks).
    pub pinned: Vec<String>,
    /// Build nothing: merge the shards the store already holds (a version
    /// with none is refused `no-shard`).
    pub merge_only: bool,
    /// What a pre-bundled file's foreign regions become.
    pub sponge_regions: SpongeRegions,
    /// Suppress the plan printout and the per-task progress lines: the
    /// in-process run-time build (exp154, `cache::build_runtime`) runs
    /// inside a pipeline, where stray stdout would corrupt the run's
    /// surface. The FAILED lines stay (stderr, errors only).
    pub quiet: bool,
    /// Restore the packages' ORIGINAL identifier names beside the unit
    /// rows (exp159): every unit print also carries the names its slots
    /// stand for, recovered from the build's own sourcemap and the
    /// package's installed sources. Default off — the shard rows are
    /// byte-identical without it (a `names/1` stamp marks an index built
    /// with it).
    pub names: bool,
    /// `name@version:reason` specs (exp161): the version leaves the plan
    /// and the refusal is recorded like the plan's own. A package whose
    /// build OOMs workers solo gets one refusal entry, never a
    /// special-case retry path.
    pub refuse: Vec<String>,
}

/// What the run-time build records into the run's profiler, and the two
/// noticeboards its caller reads (exp154, library-names-plan step 3):
/// the run passes the profiler it plans to write its own profile from,
/// a stop flag it sets when its wait deadline expires (checked between
/// tasks), the "current stage" the deadline error names, and the per-stage
/// wall times the build sidecar records. All None on the offline CLI verb.
#[derive(Clone, Copy, Default)]
pub struct Trace<'p> {
    /// The run's profiler; spans are `library.index.<stage>`.
    pub prof: Option<&'p humanify_core::profiling::Profiler>,
    /// The run set it: stop at the next task boundary.
    pub stop: Option<&'p AtomicBool>,
    /// The job's current stage ("plan", "install", ...).
    pub stage: Option<&'p Mutex<String>>,
    /// Stage name → wall ms, accumulated over every occurrence.
    pub stages: Option<&'p Mutex<BTreeMap<String, u64>>>,
}

impl Trace<'_> {
    /// The run asked the build to stop (its wait deadline expired).
    pub fn stopped(&self) -> bool {
        self.stop.is_some_and(|s| s.load(Ordering::SeqCst))
    }

    /// One stage: a `library.index.<stage>` span, the caller's noticeboards
    /// updated (the tracker at the START — it answers "where is it" — the
    /// wall time at the end). A guard: ended on every path, `?` errors
    /// included.
    fn stage(&self, name: &'static str) -> Option<Stage<'_>> {
        if let Some(t) = self.stage {
            *t.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = name.to_string();
        }
        let span = self.prof.map(|p| {
            p.start_span(
                name,
                "library-index",
                humanify_model::profiling::trace_tid::LIBRARY_INDEX,
                None,
            )
        });
        Some(Stage {
            span,
            name,
            started: Instant::now(),
            stages: self.stages,
        })
    }
}

/// A [`Trace::stage`] guard: recording happens in `Drop`, so an early `?`
/// still ends the span.
struct Stage<'p> {
    span: Option<humanify_core::profiling::SpanHandle<'p>>,
    name: &'static str,
    started: Instant,
    stages: Option<&'p Mutex<BTreeMap<String, u64>>>,
}

impl Drop for Stage<'_> {
    fn drop(&mut self) {
        if let Some(span) = self.span.take() {
            span.end(None);
        }
        if let Some(stages) = self.stages {
            let ms = self.started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
            stages
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(self.name.to_string())
                .and_modify(|t| *t += ms)
                .or_insert(ms);
        }
    }
}

/// One `package@version` to index.
#[derive(Clone, Debug)]
pub struct Task {
    pub name: String,
    pub version: String,
    pub role: String,
    /// Its publish time: npm resolves its dependencies as of THIS time.
    pub published: String,
    pub missing: Vec<ShardInputs>,
    pub reused: Vec<ShardInputs>,
}

#[derive(Debug)]
pub struct Plan {
    pub as_of: String,
    pub tasks: Vec<Task>,
    pub refused: Vec<RefusedRecord>,
    pub registry: RegistrySnapshot,
    pub slices: Vec<SliceRecord>,
}

/// `name` / `name@version` (a scope's leading `@` is not a separator).
pub fn parse_package_spec(spec: &str) -> (String, Option<String>) {
    match spec.rfind('@') {
        Some(at) if at > 0 => (spec[..at].to_string(), Some(spec[at + 1..].to_string())),
        _ => (spec.to_string(), None),
    }
}

/// Specs grouped by package, first-seen order, pins kept.
fn group_specs(specs: &[String]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for spec in specs.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let (name, pin) = parse_package_spec(spec);
        let slot = match out.iter().position(|(n, _)| *n == name) {
            Some(i) => i,
            None => {
                out.push((name, Vec::new()));
                out.len() - 1
            }
        };
        out[slot].1.extend(pin);
    }
    out
}

fn note_fetch(snapshot: &mut RegistrySnapshot, fetched: Option<&str>) {
    let Some(t) = fetched.and_then(normalize_time) else {
        return;
    };
    if snapshot.earliest_fetch.as_ref().is_none_or(|e| t < *e) {
        snapshot.earliest_fetch = Some(t.clone());
    }
    if snapshot.latest_fetch.as_ref().is_none_or(|l| t > *l) {
        snapshot.latest_fetch = Some(t);
    }
}

/// The slice records: each recipe with its bundler's version, its publish
/// time, the newest version at the as-of date, and whether the one used is
/// newer.
fn slice_records(
    cfg: &BuildConfig,
    snapshot: &mut RegistrySnapshot,
    trace: Trace<'_>,
) -> Result<Vec<SliceRecord>, String> {
    let mut out = Vec::new();
    let mut seen_bundlers = BTreeSet::new();
    for build in &cfg.recipes {
        let version = build.bundler_version()?;
        let package = bundler_package(build.recipe.bundler);
        let dated = {
            let _s = trace.stage("library.index.registry");
            registry::load(&cfg.registry, package, cfg.offline).ok()
        };
        let (published, newest) = match &dated {
            Some((p, sha)) => {
                snapshot.packuments.insert(package.to_string(), sha.clone());
                note_fetch(snapshot, p.fetched_at.as_deref());
                let newest = select_versions(p, &cfg.as_of, &[], VersionPolicy::AsOf)
                    .selected
                    .first()
                    .map(|s| s.version.clone());
                (p.time.get(&version).and_then(|t| normalize_time(t)), newest)
            }
            None => (None, None),
        };
        out.push(SliceRecord {
            slice: build.recipe.slice_name(),
            recipe: build.recipe.spelling(),
            recipe_version: RECIPE_VERSION.to_string(),
            default_for_bundler: seen_bundlers.insert(build.recipe.bundler),
            bundler: package.to_string(),
            bundler_after_as_of: published.as_ref().is_some_and(|t| *t > cfg.as_of),
            version,
            published,
            newest_at_as_of: newest,
        });
    }
    Ok(out)
}

/// The shard inputs of one `package@version`: literals, then each slice.
pub(crate) fn shard_inputs(
    name: &str,
    version: &str,
    slices: &[SliceRecord],
    regions: SpongeRegions,
    names: bool,
) -> Vec<ShardInputs> {
    let f = fingerprint_versions();
    // The literal rows are the files these builds contain, so the
    // recipes and bundlers that decide them are part of the shard's key.
    let builds: Vec<String> = slices
        .iter()
        .map(|s| format!("{}:{}@{}", s.recipe, s.bundler, s.version))
        .collect();
    let mut out = vec![ShardInputs {
        kind: ShardKind::Literals,
        package: name.to_string(),
        version: version.to_string(),
        bundler: (!builds.is_empty()).then(|| builds.join("+")),
        recipe: RECIPE_VERSION.to_string(),
        fingerprint: f.literals.clone(),
    }];
    for s in slices {
        let Ok(recipe) = Recipe::parse(&s.recipe) else {
            continue;
        };
        out.push(ShardInputs {
            kind: ShardKind::Modules(recipe),
            package: name.to_string(),
            version: version.to_string(),
            bundler: Some(format!("{}@{}", s.bundler, s.version)),
            recipe: format!("{RECIPE_VERSION} {}", s.recipe),
            // The shard holds ROWS: its key spells the rows' edge stamp
            // (LIBRARY_EDGE_ROWS_VERSION), never the readable contract's
            // (f.edges — "edges/2" since exp155's derived callee-print
            // table). The flat table is derived from these rows at merge
            // time, so a store keyed edges/1 — exp152's — is re-merged
            // by an edges/2 index without one re-fingerprint.
            fingerprint: format!(
                "{}+{}+{}+{}{}",
                f.module,
                f.units,
                humanify_core::libindex::format::LIBRARY_EDGE_ROWS_VERSION,
                regions.unit_rows_version(),
                // the names pass changes the ROWS (each unit row carries
                // the names the build's map recovers): its shard key spells
                // it, so a names build never reuses a plain shard
                if names {
                    format!("+{}", humanify_core::libindex::format::NAMES_SHARD_STAMP)
                } else {
                    String::new()
                }
            ),
        });
    }
    out
}

/// What an as-of build would index and which shards it would reuse.
///
/// Every registry read is a `library.index.registry` stage when the
/// run-time caller passed a [`Trace`]; the `library.index.plan` stage that
/// wraps the whole plan belongs to [`build`].
/// `--refuse` specs (`name@version:reason`): the version leaves the plan
/// and the refusal is recorded like the plan's own (exp161: a package whose
/// build OOMs workers solo gets one refusal entry, never a retry path).
fn refusal_specs(specs: &[String]) -> Result<Vec<(String, String, String)>, String> {
    specs
        .iter()
        .map(|s| {
            let (nv, reason) = s
                .split_once(':')
                .ok_or_else(|| format!("--refuse {s}: expected name@version:reason"))?;
            let (name, version) = nv
                .split_once('@')
                .ok_or_else(|| format!("--refuse {s}: expected name@version:reason"))?;
            if name.is_empty() || version.is_empty() || reason.is_empty() {
                return Err(format!("--refuse {s}: expected name@version:reason"));
            }
            Ok((name.to_string(), version.to_string(), reason.to_string()))
        })
        .collect()
}

pub fn plan(cfg: &BuildConfig, trace: Trace<'_>) -> Result<Plan, String> {
    let mut snapshot = RegistrySnapshot::default();
    let slices = slice_records(cfg, &mut snapshot, trace)?;
    let mut tasks = Vec::new();
    let mut refused = Vec::new();
    let refusals = refusal_specs(&cfg.refuse)?;
    // `--pinned` specs join a listed package's pins, or are indexed at
    // their pins alone
    let mut groups: Vec<(String, Vec<String>, VersionPolicy)> = group_specs(&cfg.packages)
        .into_iter()
        .map(|(name, pins)| (name, pins, cfg.policy))
        .collect();
    for (name, pins) in group_specs(&cfg.pinned) {
        match groups.iter_mut().find(|g| g.0 == name) {
            Some(g) => g.1.extend(pins),
            None => groups.push((name, pins, VersionPolicy::PinsOnly)),
        }
    }
    for (name, pins, policy) in groups {
        let (packument, sha) = {
            let _s = trace.stage("library.index.registry");
            registry::load(&cfg.registry, &name, cfg.offline)?
        };
        snapshot.packuments.insert(name.clone(), sha);
        note_fetch(&mut snapshot, packument.fetched_at.as_deref());
        let chosen = select_versions(&packument, &cfg.as_of, &pins, policy);
        refused.extend(chosen.refused.into_iter().map(|r| RefusedRecord {
            name: name.clone(),
            version: r.version,
            reason: r.reason,
        }));
        // an explicit --refuse removes the version from the plan; a version
        // the plan already refused keeps its first reason (no duplicate rows)
        let mut selected = chosen.selected;
        for (rn, rv, reason) in &refusals {
            if rn != &name {
                continue;
            }
            if refused.iter().any(|r| r.name == name && r.version == *rv) {
                continue;
            }
            if let Some(i) = selected.iter().position(|s| &s.version == rv) {
                selected.remove(i);
                refused.push(RefusedRecord {
                    name: name.clone(),
                    version: rv.clone(),
                    reason: reason.clone(),
                });
            }
        }
        for s in selected {
            let (mut missing, mut reused) = (Vec::new(), Vec::new());
            for inputs in shard_inputs(&name, &s.version, &slices, cfg.sponge_regions, cfg.names) {
                match read_current(&cfg.store, &inputs) {
                    Some(_) => reused.push(inputs),
                    None => missing.push(inputs),
                }
            }
            tasks.push(Task {
                name: name.clone(),
                version: s.version,
                role: s.role,
                published: s.published,
                missing,
                reused,
            });
        }
    }
    Ok(Plan {
        as_of: cfg.as_of.clone(),
        tasks,
        refused,
        registry: snapshot,
        slices,
    })
}

/// The plan, as `--dry-run` prints it.
pub fn describe(plan: &Plan) -> String {
    let mut out = format!("as-of {}\n", plan.as_of);
    for s in &plan.slices {
        out.push_str(&format!(
            "slice {} ({}): {}@{} published {} — newest at as-of: {}{}\n",
            s.slice,
            s.recipe,
            s.bundler,
            s.version,
            s.published.as_deref().unwrap_or("unknown"),
            s.newest_at_as_of.as_deref().unwrap_or("none"),
            if s.bundler_after_as_of {
                "  [bundlerAfterAsOf: the bundler is newer than the as-of date]"
            } else {
                ""
            },
        ));
    }
    for t in &plan.tasks {
        out.push_str(&format!(
            "index {}@{} ({}, published {})\n",
            t.name, t.version, t.role, t.published
        ));
        for i in &t.reused {
            out.push_str(&format!("  reuse shard {}\n", i.label()));
        }
        for i in &t.missing {
            out.push_str(&format!("  build shard {}\n", i.label()));
        }
        if !t.missing.is_empty() {
            out.push_str(&format!(
                "  would run: npm install {}@{} --ignore-scripts --before={}\n",
                t.name, t.version, t.published
            ));
        }
    }
    for r in &plan.refused {
        out.push_str(&format!("refused {}@{}: {}\n", r.name, r.version, r.reason));
    }
    out
}

/// `npm install` of one package (measured: tens of seconds; the deadline
/// is for a registry that never answers).
const INSTALL_DEADLINE: std::time::Duration = std::time::Duration::from_secs(30 * 60);

fn run_cmd(cmd: &mut Command) -> Result<(), String> {
    let out =
        humanify_core::subprocess::run(cmd, None, INSTALL_DEADLINE).map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Every file under `dir` (sorted, recursive), skipping `.bin`/`.cache`.
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if p.is_dir() {
            if name != ".bin" && name != ".cache" {
                walk_files(&p, out);
            }
        } else {
            out.push(p);
        }
    }
}

/// Input sourcemap comments are removed so a bundler does not chain them.
fn strip_source_map_comments(node_modules: &Path) {
    let mut files = Vec::new();
    walk_files(node_modules, &mut files);
    for f in files {
        let n = f.to_string_lossy();
        if !(n.ends_with(".js") || n.ends_with(".mjs") || n.ends_with(".cjs")) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else {
            continue;
        };
        if !text.contains("sourceMappingURL=") {
            continue;
        }
        let kept: Vec<&str> = text
            .split('\n')
            .filter(|l| {
                !(l.starts_with("//# sourceMappingURL=") || l.starts_with("//@ sourceMappingURL="))
            })
            .collect();
        let _ = std::fs::write(&f, kept.join("\n"));
    }
}

/// The package's files (relative to its root, sorted; its own nested
/// `node_modules` excluded) — what a wildcard export can name.
fn package_files(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk_files(root, &mut files);
    files
        .iter()
        .filter_map(|f| f.strip_prefix(root).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .filter(|r| !r.split('/').any(|c| c == "node_modules"))
        .collect()
}

/// Write `text` to `path` as a NEW file (unlinked first: a work directory
/// may be a hardlinked copy of another).
fn write_fresh(path: &Path, text: &str) -> Result<(), String> {
    let _ = std::fs::remove_file(path);
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// The two app entries: a `require()` and an `import()` of the package
/// and of every subpath its `exports` map gives a Node app
/// ([`entry_specifiers`]).
fn write_entries(dir: &Path, name: &str) -> Result<(), String> {
    let root = dir.join("node_modules").join(name);
    let manifest: serde_json::Value = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let specs = entry_specifiers(name, &manifest, &package_files(&root));
    let quoted: Vec<String> = specs
        .iter()
        .map(|s| serde_json::to_string(s).map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    let app = |tag: &str| {
        format!(
            "function labAppMain_{tag}(x){{ if (!x) throw new Error(\"lab-app {tag}: no input given\"); return Object.keys(x).length + \":{tag}\"; }}\n"
        )
    };
    let entries = dir.join("entries");
    std::fs::create_dir_all(&entries).map_err(|e| e.to_string())?;
    let requires: String = quoted
        .iter()
        .enumerate()
        .map(|(i, q)| format!("const m{i} = require({q});\n"))
        .collect();
    let names: Vec<String> = (0..quoted.len()).map(|i| format!("m{i}")).collect();
    write_fresh(
        &entries.join("req.cjs"),
        &format!(
            "{}{requires}module.exports = {{ run: labAppMain_req, {} }};\n",
            app("req"),
            names.join(", ")
        ),
    )?;
    let loaders: Vec<String> = quoted
        .iter()
        .map(|q| format!("() => import({q})"))
        .collect();
    write_fresh(
        &entries.join("dyn.mjs"),
        &format!(
            "{}export const loaders = [{}];\nexport {{ labAppMain_dyn }};\n",
            app("dyn"),
            loaders.join(", ")
        ),
    )
}

/// Install `task` (once) in its work directory: dependencies as of its
/// own publish time (`--before`), no install scripts.
fn install(work: &Path, task: &Task, trace: Trace<'_>) -> Result<PathBuf, String> {
    let dir = work.join(format!("{}@{}", task.name.replace('/', "__"), task.version));
    if dir.join(".installed").exists() {
        return Ok(dir);
    }
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let manifest = serde_json::json!({"private": true, "name": "ix-app", "dependencies": {&task.name: &task.version}});
    std::fs::write(dir.join("package.json"), format!("{manifest}\n")).map_err(|e| e.to_string())?;
    let before = format!("--before={}", task.published);
    let base = [
        "install",
        "--ignore-scripts",
        "--no-audit",
        "--no-fund",
        "--loglevel=error",
    ];
    let npm = |legacy: bool, trace: Trace<'_>| -> Result<(), String> {
        let mut cmd = Command::new("npm");
        cmd.current_dir(&dir).args(base).arg(&before);
        if legacy {
            cmd.arg("--legacy-peer-deps");
        }
        let _s = trace.stage("library.index.install");
        run_cmd(&mut cmd)
    };
    if let Err(e) = npm(false, trace) {
        npm(true, trace)
            .map_err(|e2| format!("npm install {}@{}: {e} / {e2}", task.name, task.version))?;
    }
    strip_source_map_comments(&dir.join("node_modules"));
    std::fs::write(dir.join(".installed"), "").map_err(|e| e.to_string())?;
    Ok(dir)
}

/// `path` with symlinks resolved (a bundler writes map sources relative to
/// the REAL directory it built in), lexically when it cannot be resolved.
fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| lexical(path))
}

/// The files `slices`' builds of this install contain: every source a
/// build's sourcemap maps code to (lexical paths under `dir`).
fn bundled_files(dir: &Path, recipes: &[Recipe]) -> BTreeSet<PathBuf> {
    let mut out = BTreeSet::new();
    for recipe in recipes {
        for (tag, _) in ENTRIES {
            let build = dir.join("build").join(recipe.slice_name()).join(tag);
            let Ok(text) = std::fs::read_to_string(build.join("out.js.map")) else {
                continue;
            };
            let Ok(mut map) = SourceMap::parse(&text) else {
                continue;
            };
            map.relocate(&build);
            for s in map.mapped_sources() {
                out.insert(canonical(&build.join(&map.sources[s as usize])));
            }
        }
    }
    out
}

/// The literal shard: every source file of the installed packages that a
/// build of `slices` contains (exp095 selection lab: ~70% of a whole-tree
/// literal table is files no bundle pulls in — other entry points, CLI
/// scripts, data, shipped tests), owned by its nearest package.json. With
/// no slices there is nothing to restrict by: every file.
pub fn literal_shard(dir: &Path, inputs: &ShardInputs, slices: &[Recipe]) -> Shard {
    let mut shard = Shard::new(inputs);
    let mut files = Vec::new();
    walk_files(&dir.join("node_modules"), &mut files);
    let bundled = bundled_files(dir, slices);
    let mut owners: HashMap<PathBuf, Option<(String, String)>> = HashMap::new();
    let out_dir = dir.join("out");
    for f in files {
        let Ok(rel) = f.strip_prefix(dir) else {
            continue;
        };
        if !slices.is_empty() && !bundled.contains(&canonical(&f)) {
            if is_source_file(&f.to_string_lossy()) {
                shard.notes.unbundled_files += 1;
            }
            continue;
        }
        let rel = format!("../{}", rel.to_string_lossy());
        let Some(owner) = owner_cached(&out_dir, &rel, &mut owners) else {
            continue;
        };
        if !is_source_file(&owner.file) || owner.file.contains("node_modules/") {
            continue;
        }
        if is_sponge_file(&owner.file) {
            shard.notes.sponge_files += 1;
            continue;
        }
        if std::fs::metadata(&f).map(|m| m.len()).unwrap_or(u64::MAX) > MAX_SOURCE_BYTES {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&f) else {
            shard.notes.unread_files += 1;
            continue;
        };
        let lits = if owner.file.ends_with(".json") {
            json_literals(&text)
        } else {
            code_literals(&text, LiteralMode::Source)
        };
        match lits {
            Some(l) if !l.is_empty() => shard.literals.push(ShardLiteral {
                owner: owner.package,
                version: owner.version,
                file: owner.file,
                lits: l.into_iter().collect(),
            }),
            Some(_) => {}
            None => shard.notes.unread_files += 1,
        }
    }
    shard
}

/// Build one entry, retrying unresolvable specifiers as externals (up to
/// 4 rounds) and top-level await as ESM.
fn build_entry(recipe: &RecipeBuild, dir: &Path, entry: &str, out: &Path) -> Result<(), String> {
    let (mut externals, mut esm) = (Vec::new(), false);
    let mut last = String::new();
    for _ in 0..6 {
        match recipe.build(dir, entry, out, &externals, esm) {
            Ok(()) => return Ok(()),
            Err(e) => last = e,
        }
        let new: Vec<String> = unresolved_specifiers(&last)
            .into_iter()
            .filter(|s| !externals.contains(s))
            .collect();
        if !new.is_empty() && externals.len() < 64 {
            externals.extend(new);
        } else if last.to_lowercase().contains("top-level await") && !esm {
            esm = true;
        } else {
            break;
        }
    }
    Err(last.lines().next().unwrap_or("build failed").to_string())
}

/// One bundle's module rows: the pipeline's module walk and content key,
/// attributed through the sourcemap.
pub(crate) fn fingerprint_bundle(
    recipe: Recipe,
    out: &Path,
    shard: &mut Shard,
    regions: SpongeRegions,
    names: bool,
) -> Result<(), String> {
    let code = std::fs::read_to_string(out.join("out.js")).map_err(|e| e.to_string())?;
    let map_text = std::fs::read_to_string(out.join("out.js.map")).map_err(|e| e.to_string())?;
    let mut map = SourceMap::parse(&map_text)?;
    map.relocate(out);
    let toolchain = humanify_core::toolchain::resolve_toolchain(
        &humanify_core::detect::detect_bundle(&code),
        Some(recipe.bundler.bundler_type()),
        None,
    );
    let modules = humanify_core::modules::vendor_content::bundle_content_keys(
        &code,
        toolchain.module_wrappers.piece,
        toolchain.layout.piece,
    );
    let spans: Vec<(u32, u32)> = modules
        .iter()
        .map(|m| (m.body_span.start, m.body_span.end))
        .collect();
    let sources = map.attribute(&code, &spans, PURITY);
    let mut credited: BTreeMap<u32, usize> = BTreeMap::new();
    for src in sources.iter().flatten() {
        *credited.entry(*src).or_default() += 1;
    }
    let mut owners = HashMap::new();
    // A source file maps to ONE module of a bundle: a file credited to two
    // or more is wrong for at least one of them, so none is labelled.
    let mut misattributed: BTreeSet<(String, String)> = BTreeSet::new();
    for (&src, &n) in &credited {
        if n < 2 {
            continue;
        }
        shard.notes.multi_module_files += 1;
        shard.notes.multi_module_dropped += n;
        if let Some(o) = owner_cached(out, &map.sources[src as usize], &mut owners) {
            misattributed.insert((o.package, o.file));
        }
    }
    for (m, src) in modules.iter().zip(sources) {
        let Some(src) = src else {
            shard.notes.mixed_spans += 1;
            continue;
        };
        if credited[&src] >= 2 {
            continue;
        }
        let Some(owner) = owner_cached(out, &map.sources[src as usize], &mut owners) else {
            shard.notes.unowned += 1;
            continue;
        };
        let Some(key) = &m.key else {
            shard.notes.unkeyed += 1;
            continue;
        };
        shard.modules.push(ShardModule {
            owner: owner.package,
            version: owner.version,
            file: owner.file,
            key: key.clone(),
        });
    }
    let (units, notes) = unit_rows(recipe, &code, &map, out, regions, names);
    let (made, kept_before) = (units.len(), shard.units.len());
    shard.units.extend(
        units
            .into_iter()
            .filter(|u| !misattributed.contains(&(u.owner.clone(), u.file.clone()))),
    );
    shard.notes.multi_module_units_dropped += made - (shard.units.len() - kept_before);
    shard.notes.unit_mixed_spans += notes.mixed_spans;
    for (kind, n) in notes.sponge_unit_files {
        *shard.notes.sponge_unit_files.entry(kind).or_default() += n;
    }
    shard.notes.sponge_unit_rows += notes.sponge_unit_rows;
    for (why, n) in notes.unrecorded_edges {
        *shard.notes.unrecorded_edges.entry(why).or_default() += n;
    }
    Ok(())
}

/// Unit rows of one bundle (statement-level library fingerprints): every
/// statement unit attributed through the sourcemap like a module, its
/// functions inheriting its owner; units under [`UNIT_ROW_MIN_NODES`]
/// nodes are not indexed. `module_bytes` is the bundled size of the owner
/// file in this build (the statement units it owns).
pub fn unit_rows(
    recipe: Recipe,
    code: &str,
    map: &SourceMap,
    out: &Path,
    regions: SpongeRegions,
    names: bool,
) -> (Vec<ShardUnit>, ShardNotes) {
    let mut notes = ShardNotes::default();
    let toolchain = humanify_core::toolchain::resolve_toolchain(
        &humanify_core::detect::detect_bundle(code),
        Some(recipe.bundler.bundler_type()),
        None,
    );
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        notes.unkeyed += 1;
        return (Vec::new(), notes);
    }
    let bundle = if names {
        humanify_core::libindex::units::bundle_unit_named(
            code,
            &ingest,
            toolchain.layout.piece,
            toolchain.module_wrappers.piece,
            toolchain.interop.piece,
        )
    } else {
        humanify_core::libindex::units::bundle_unit_graph(
            code,
            &ingest,
            toolchain.layout.piece,
            toolchain.module_wrappers.piece,
            toolchain.interop.piece,
        )
    };
    let printed: Vec<usize> = (0..bundle.units.len())
        .filter(|&i| bundle.prints[i].is_some())
        .collect();
    let spans: Vec<(u32, u32)> = printed
        .iter()
        .map(|&i| (bundle.units[i].span.start, bundle.units[i].span.end))
        .collect();
    let mut cache = HashMap::new();
    let mut sponges: HashMap<u32, Sponge> = HashMap::new();
    let mut sponge_files: BTreeSet<u32> = BTreeSet::new();
    // per unit (graph order), its owner when it has one — and the source
    // index it was credited to (the names pass reads that source's file)
    let mut owner_of: Vec<Option<crate::sourcemap::Owner>> = vec![None; bundle.units.len()];
    let mut src_of: Vec<Option<u32>> = vec![None; bundle.units.len()];
    for (&i, segs) in printed.iter().zip(map.span_segments(code, &spans)) {
        let tally = crate::sourcemap::tally_of(&segs);
        match unit_owner(map, out, &tally, &mut cache) {
            UnitOwner::Owned(owner, src) => {
                let evidence = sponges.entry(src).or_insert_with(|| {
                    sponge_evidence(&owner, &lexical(&out.join(&map.sources[src as usize])))
                });
                let Some(owner) = evidence.unit_owner(owner, src, &segs, regions) else {
                    if sponge_files.insert(src) {
                        let kind = evidence.kind().unwrap_or("bundler-marker");
                        *notes.sponge_unit_files.entry(kind.to_string()).or_default() += 1;
                    }
                    notes.sponge_unit_rows += 1;
                    continue;
                };
                owner_of[i] = Some(owner);
                src_of[i] = Some(src);
            }
            UnitOwner::Mixed => notes.mixed_spans += 1,
            UnitOwner::Unowned => notes.unowned += 1,
        }
    }
    let mut module_bytes: HashMap<(String, String), u64> = HashMap::new();
    for (i, o) in owner_of.iter().enumerate() {
        if let Some(o) = o {
            *module_bytes
                .entry((o.package.clone(), o.file.clone()))
                .or_default() += u64::from(bundle.units[i].bytes);
        }
    }
    // the names pass's inputs, once per bundle: the map as a byte-offset
    // anchor table and the ORIGINAL sources' declared identifiers (the
    // attributed sources only)
    let (anchors, orig) = if names {
        let anchors = crate::names::Anchors::new(code, map);
        let mut wanted: Vec<u32> = src_of.iter().flatten().copied().collect();
        wanted.sort_unstable();
        wanted.dedup();
        (
            Some(anchors),
            crate::names::original_decls(out, map, &wanted),
        )
    } else {
        (None, HashMap::new())
    };
    let mut rows = Vec::new();
    for (i, o) in owner_of.iter().enumerate() {
        let (Some(o), Some(prints)) = (o, &bundle.prints[i]) else {
            continue;
        };
        let (inside, out_of) = unit_edges(&bundle, &owner_of, i, &o.package, &mut notes);
        let size = module_bytes[&(o.package.clone(), o.file.clone())];
        let with_mappings: Vec<(
            &humanify_core::libindex::units::UnitPrint,
            Option<&humanify_core::libindex::units::Mapping>,
        )> = match &bundle.named[i] {
            Some(named) => {
                let whole = &named.whole;
                std::iter::once((&whole.print, Some(&whole.mapping)))
                    .chain(named.functions.iter().map(|f| (&f.print, Some(&f.mapping))))
                    .collect()
            }
            None => std::iter::once(&prints.whole)
                .chain(&prints.functions)
                .map(|p| (p, None))
                .collect(),
        };
        for (p, mapping) in with_mappings {
            if p.nodes >= UNIT_ROW_MIN_NODES {
                let restored = match (anchors.as_ref(), mapping) {
                    (Some(anchors), Some(mapping)) => crate::names::print_names(
                        mapping,
                        p.span,
                        src_of[i].expect("owned unit has its source"),
                        ingest.semantic(),
                        anchors,
                        &orig,
                    ),
                    _ => Vec::new(),
                };
                rows.push(ShardUnit {
                    owner: o.package.clone(),
                    version: o.version.clone(),
                    file: o.file.clone(),
                    hash: p.hash.clone(),
                    nodes: p.nodes,
                    content: p.content,
                    module_bytes: size,
                    inside: inside.clone(),
                    out_of: out_of.clone(),
                    names: restored,
                });
            }
        }
    }
    (rows, notes)
}

/// Unit `i`'s recorded callees (`edges/1`): the WHOLE print of every unit
/// its statement references in the reference graph, split by the callee's
/// owner into INSIDE (`package`'s own) and OUT-OF (another package's).
/// Only the unit's OWN references are read — never who references it (an
/// INTO edge describes the caller, docs/matching-architecture.md §4). A
/// callee with no print (a container) or no owner is counted in the notes,
/// not recorded.
fn unit_edges(
    bundle: &humanify_core::libindex::units::BundleUnits,
    owner_of: &[Option<crate::sourcemap::Owner>],
    i: usize,
    package: &str,
    notes: &mut ShardNotes,
) -> (Vec<String>, Vec<humanify_core::libindex::format::Callee>) {
    let Some(graph) = bundle
        .graph
        .as_ref()
        .filter(|g| g.units.len() == bundle.units.len())
    else {
        return (Vec::new(), Vec::new());
    };
    let mut inside: BTreeSet<String> = BTreeSet::new();
    let mut out_of: BTreeSet<humanify_core::libindex::format::Callee> = BTreeSet::new();
    for &v in &graph.out[i] {
        let v = v as usize;
        let why = match (&bundle.prints[v], &owner_of[v]) {
            (None, _) => "container",
            (Some(_), None) => "unowned",
            (Some(p), Some(o)) => {
                if o.package == package {
                    inside.insert(p.whole.hash.clone());
                } else {
                    out_of.insert(humanify_core::libindex::format::Callee {
                        owner: o.package.clone(),
                        hash: p.whole.hash.clone(),
                    });
                }
                continue;
            }
        };
        *notes.unrecorded_edges.entry(why.to_string()).or_default() += 1;
    }
    (inside.into_iter().collect(), out_of.into_iter().collect())
}

/// Whether a package's source file holds pre-bundled copies of OTHER
/// packages ("sponge", exp095 SYNTHESIS §8, exp097 step 3), read off the
/// file itself. A file NAME is not evidence here (unlike the literal
/// table's [`is_sponge_file`]): a package's own minified build is its own
/// code — esquery's main IS `dist/esquery.min.js` (measured: the name rule
/// cost 74 KB of right esquery claims on the held-out apps).
#[derive(Debug)]
pub enum Sponge {
    /// The package's own code.
    Own,
    /// A vendored directory: the whole file is a copy.
    VendoredPath,
    /// A bundle that kept its bundler's MODULE MARKERS, at least one naming
    /// another package's `node_modules` path ([`module_marker`]): the
    /// file's regions in line order (exp124: only the foreign ones are
    /// copies).
    Regions(Vec<Region>),
}

/// One module region of a pre-bundled file: from `start` (a 0-based line)
/// to the next region's start. `origin` is the OTHER package its marker
/// names, None for the file's own code (a marker naming the package
/// itself or a path outside `node_modules`, an `//#endregion`).
#[derive(Debug, PartialEq, Eq)]
pub struct Region {
    pub start: u32,
    pub origin: Option<Origin>,
}

/// The package a foreign region's marker names, the version its pnpm store
/// path spells (`.pnpm/<name>@<version>/`), and the file inside it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin {
    pub package: String,
    pub version: Option<String>,
    pub file: String,
}

impl Sponge {
    /// The sponge note kind, None for the package's own code.
    pub fn kind(&self) -> Option<&'static str> {
        match self {
            Sponge::Own => None,
            Sponge::VendoredPath => Some("vendored-path"),
            Sponge::Regions(_) => Some("bundler-marker"),
        }
    }

    /// Who a unit of this file belongs to, from its segments' (source,
    /// original line): the file's owner when at least [`PURITY`] of the
    /// segments in this file lie outside every foreign region; with
    /// [`SpongeRegions::Origin`], the origin holding [`PURITY`] of them
    /// (when its version is known). None: the unit has no row.
    fn unit_owner(
        &self,
        owner: crate::sourcemap::Owner,
        src: u32,
        segs: &[(u32, u32)],
        mode: SpongeRegions,
    ) -> Option<crate::sourcemap::Owner> {
        let regions = match self {
            Sponge::Own => return Some(owner),
            Sponge::VendoredPath => return None,
            Sponge::Regions(r) => r,
        };
        let mut by: BTreeMap<Option<&Origin>, usize> = BTreeMap::new();
        let mut total = 0usize;
        for &(s, line) in segs {
            if s != src {
                continue;
            }
            total += 1;
            let at = regions.partition_point(|r| r.start <= line);
            let origin = at.checked_sub(1).and_then(|i| regions[i].origin.as_ref());
            *by.entry(origin).or_default() += 1;
        }
        let (origin, n) = by.into_iter().max_by_key(|&(_, n)| n)?;
        if (n as f64) < PURITY * total as f64 {
            return None;
        }
        match (origin, mode) {
            (None, _) => Some(owner),
            (Some(_), SpongeRegions::Drop) => None,
            (Some(o), SpongeRegions::Origin) => Some(crate::sourcemap::Owner {
                package: o.package.clone(),
                version: o.version.clone()?,
                file: o.file.clone(),
            }),
        }
    }
}

/// Read a package's source file for [`Sponge`] evidence: a vendored
/// directory, else its module regions when a marker names another
/// package.
pub fn sponge_evidence(owner: &crate::sourcemap::Owner, path: &Path) -> Sponge {
    let vendored = ["vendor", "vendored", "third_party", "third-party"];
    let mut parts = owner.file.split('/').collect::<Vec<_>>();
    let name = parts.pop().unwrap_or_default();
    if parts.iter().any(|p| vendored.contains(p)) || name.starts_with("third-party.") {
        return Sponge::VendoredPath;
    }
    if std::fs::metadata(path).map_or(true, |m| m.len() > MAX_SOURCE_BYTES) {
        return Sponge::Own;
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return Sponge::Own;
    };
    let regions = module_regions(&owner.package, &text);
    if regions.iter().any(|r| r.origin.is_some()) {
        Sponge::Regions(regions)
    } else {
        Sponge::Own
    }
}

/// A bundled file's module regions, in line order (the truth's marker rule,
/// experiments/096-library-matching-score/origins.py `marker_regions7`):
/// every module marker opens a region ([`module_marker`]); rolldown's
/// `//#endregion` returns to the file's own code, except that a lazily
/// initialised module's body right after it (`__esm(` / `__esmMin(`) stays
/// the module's until the next marker or a top-level `export`.
pub fn module_regions(owner: &str, text: &str) -> Vec<Region> {
    let mut out: Vec<Region> = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut last: Option<Origin> = None;
    let mut lazy = false;
    for (i, line) in lines.iter().enumerate() {
        let at = i as u32;
        if line.starts_with("//#endregion") {
            let body = last
                .take()
                .filter(|_| lines.get(i + 1).is_some_and(|next| is_lazy_body(next)));
            out.push(Region {
                start: at,
                origin: None,
            });
            if let Some(o) = body {
                out.push(Region {
                    start: at + 1,
                    origin: Some(o),
                });
                lazy = true;
            }
            continue;
        }
        if lazy && is_export(line) {
            out.push(Region {
                start: at,
                origin: None,
            });
            lazy = false;
            continue;
        }
        if let Some(path) = module_marker(line) {
            let origin = marker_origin(path).filter(|o| o.package != owner);
            last = origin.clone();
            out.push(Region { start: at, origin });
            lazy = false;
        }
    }
    out
}

/// A top-level `export` statement (`^export\b`).
fn is_export(line: &str) -> bool {
    line.strip_prefix("export").is_some_and(|rest| {
        !rest.starts_with(|c: char| c.is_alphanumeric() || c == '_' || c == '$')
    })
}

/// A lazily initialised module's body (rolldown/tsdown): `__esm(` or
/// `__esmMin(`, optionally assigned (`var init_x = __esmMin(`).
fn is_lazy_body(line: &str) -> bool {
    let l = line.trim_start();
    let l = ["var ", "let ", "const "]
        .iter()
        .find_map(|k| l.strip_prefix(k))
        .and_then(|rest| {
            let name_end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$'))?;
            (name_end > 0).then(|| rest[name_end..].strip_prefix(" = "))?
        })
        .unwrap_or(l);
    l.starts_with("__esm(") || l.starts_with("__esmMin(")
}

/// The package a marker path names (its LAST `node_modules/` segment), the
/// version a pnpm store path spells for it, and the file inside it.
fn marker_origin(path: &str) -> Option<Origin> {
    let package = marker_package(path)?.to_string();
    let at = path.rfind("node_modules/")? + "node_modules/".len() + package.len();
    let file = path[at..].trim_start_matches('/').to_string();
    let store = format!("{}@", package.replace('/', "+"));
    let version = path.split('/').find_map(|seg| {
        let v = seg.strip_prefix(&store)?;
        let v = v.split(['_', '(']).next()?;
        (!v.is_empty()).then(|| v.to_string())
    });
    Some(Origin {
        package,
        version,
        file,
    })
}

/// The module path a bundler's module-boundary comment names, when the
/// line IS one (not prose that mentions a path — glob's comments on
/// ignoring `node_modules` are not markers): esbuild / Bun `// <path>` (the
/// whole comment one source path), rolldown `//#region <path>`, webpack
/// `/***/ "<path>":` and `/*!*** <path> ***!*/`.
fn module_marker(line: &str) -> Option<&str> {
    let l = line.trim();
    let path = if let Some(rest) = l.strip_prefix("//#region ") {
        rest.trim()
    } else if let Some(rest) = l.strip_prefix("/*!*** ") {
        rest.strip_suffix("***!*/")?.trim()
    } else if let Some(rest) = l.strip_prefix("/***/ ") {
        let rest = rest.trim().strip_suffix(':')?.trim();
        rest.strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .or_else(|| rest.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')))?
    } else {
        let rest = l.strip_prefix("// ")?.trim();
        let source = [
            ".js", ".mjs", ".cjs", ".ts", ".mts", ".cts", ".jsx", ".tsx", ".json",
        ];
        (!rest.contains(char::is_whitespace) && source.iter().any(|e| rest.ends_with(e)))
            .then_some(rest)?
    };
    (!path.is_empty()).then_some(path)
}

/// The package a marker path's LAST `node_modules/` segment names.
fn marker_package(path: &str) -> Option<&str> {
    let at = path.rfind("node_modules/")? + "node_modules/".len();
    let rest = &path[at..];
    let mut segs = rest.split('/');
    let first = segs.next()?;
    if first.starts_with('@') {
        let second = segs.next()?;
        Some(&rest[..first.len() + 1 + second.len()])
    } else {
        (!first.is_empty()).then_some(first)
    }
}

enum UnitOwner {
    Owned(crate::sourcemap::Owner, u32),
    Mixed,
    Unowned,
}

/// A statement unit's owner: the PACKAGE holding at least [`PURITY`] of
/// its segments (a unit may span several files of one package — a
/// package's own pre-bundled file mapped back to its sources), named by
/// that package's majority file. Segments outside `node_modules` count
/// against it.
fn unit_owner(
    map: &SourceMap,
    out: &Path,
    tally: &[(u32, usize)],
    cache: &mut HashMap<PathBuf, Option<(String, String)>>,
) -> UnitOwner {
    let total: usize = tally.iter().map(|t| t.1).sum();
    type Seen = (usize, usize, Option<(crate::sourcemap::Owner, u32)>);
    let mut by_package: BTreeMap<String, Seen> = BTreeMap::new();
    for &(src, n) in tally {
        let Some(owner) = owner_cached(out, &map.sources[src as usize], cache) else {
            continue;
        };
        let entry = by_package
            .entry(owner.package.clone())
            .or_insert((0, 0, None));
        entry.0 += n;
        if n > entry.1 {
            entry.1 = n;
            entry.2 = Some((owner, src));
        }
    }
    let Some((_, (n, _, owner))) = by_package.into_iter().max_by_key(|(_, (n, _, _))| *n) else {
        return UnitOwner::Unowned;
    };
    match owner {
        Some((owner, src)) if n as f64 >= PURITY * total as f64 => UnitOwner::Owned(owner, src),
        _ => UnitOwner::Mixed,
    }
}

/// The stamp a finished build leaves: the recipe version, the recipe and
/// the entry it was built from.
const BUILD_STAMP: &str = ".recipe";

/// Build `entry` into `out` unless a build of the same entry under the
/// current recipe is already there (the literal shard reads the same
/// builds).
fn ensure_build(
    recipe: &RecipeBuild,
    dir: &Path,
    entry: &str,
    out: &Path,
    trace: Trace<'_>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(dir.join(entry)).map_err(|e| format!("{entry}: {e}"))?;
    let want = format!("{RECIPE_VERSION}\n{}\n{text}", recipe.recipe.spelling());
    let stamp = out.join(BUILD_STAMP);
    if std::fs::read_to_string(&stamp).is_ok_and(|s| s == want) {
        return Ok(());
    }
    let _s = trace.stage("library.index.bundle");
    build_entry(recipe, dir, entry, out)?;
    std::fs::write(&stamp, want).map_err(|e| e.to_string())
}

/// The module shard of one slice: both entries built and fingerprinted.
fn module_shard(
    dir: &Path,
    recipe: &RecipeBuild,
    inputs: &ShardInputs,
    regions: SpongeRegions,
    names: bool,
    trace: Trace<'_>,
) -> Shard {
    let mut shard = Shard::new(inputs);
    for (tag, entry) in ENTRIES {
        let out = dir.join("build").join(recipe.recipe.slice_name()).join(tag);
        let result = ensure_build(recipe, dir, entry, &out, trace).and_then(|()| {
            let _s = trace.stage("library.index.fingerprint");
            fingerprint_bundle(recipe.recipe, &out, &mut shard, regions, names)
        });
        let status = match result {
            Ok(()) => "ok".to_string(),
            Err(e) => e,
        };
        shard.notes.builds.insert(tag.to_string(), status);
    }
    shard
}

/// Build every missing shard of `task`: the module shards first, since the
/// literal shard reads their builds (any build not yet made is made).
fn execute(cfg: &BuildConfig, task: &Task, trace: Trace<'_>) -> Result<(), String> {
    let dir = install(&cfg.work, task, trace)?;
    // the entries are the recipe's: rewritten on every build
    write_entries(&dir, &task.name)?;
    let mut missing: Vec<&ShardInputs> = task.missing.iter().collect();
    missing.sort_by_key(|i| i.kind == ShardKind::Literals);
    for inputs in missing {
        if inputs.kind == ShardKind::Literals {
            for recipe in &cfg.recipes {
                for (tag, entry) in ENTRIES {
                    let out = dir.join("build").join(recipe.recipe.slice_name()).join(tag);
                    // a failed build contributes no files; the module
                    // shard records the error
                    let _ = ensure_build(recipe, &dir, entry, &out, trace);
                }
            }
        }
        let shard = match inputs.kind {
            ShardKind::Literals => {
                let slices: Vec<Recipe> = cfg.recipes.iter().map(|r| r.recipe).collect();
                let _s = trace.stage("library.index.fingerprint");
                literal_shard(&dir, inputs, &slices)
            }
            ShardKind::Modules(want) => {
                let recipe = cfg
                    .recipes
                    .iter()
                    .find(|r| r.recipe == want)
                    .ok_or("no recipe for slice")?;
                module_shard(&dir, recipe, inputs, cfg.sponge_regions, cfg.names, trace)
            }
        };
        write_shard(&cfg.store, &shard)?;
    }
    Ok(())
}

/// Run the missing tasks on `cfg.jobs` threads; per-task errors are
/// returned (a failed task writes no shard and is not indexed). A stop in
/// `trace` (the run's wait deadline expired) ends the sweep at the next
/// task boundary — every task under way ends at its OWN deadline.
fn execute_all(cfg: &BuildConfig, tasks: &[Task], trace: Trace<'_>) -> Vec<(String, String)> {
    let todo: Vec<&Task> = tasks.iter().filter(|t| !t.missing.is_empty()).collect();
    let next = Mutex::new(0usize);
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..cfg.jobs.clamp(1, 4) {
            scope.spawn(|| {
                loop {
                    if trace.stopped() {
                        break;
                    }
                    let i = {
                        let mut n = next.lock().expect("lock");
                        *n += 1;
                        *n - 1
                    };
                    let Some(task) = todo.get(i) else { break };
                    if !cfg.quiet {
                        eprintln!("building {}@{}", task.name, task.version);
                    }
                    if let Err(e) = execute(cfg, task, trace) {
                        let label = format!("{}@{}", task.name, task.version);
                        errors.lock().expect("lock").push((label, e));
                    }
                }
            });
        }
    });
    let mut errors = errors.into_inner().unwrap_or_default();
    errors.sort();
    errors
}

/// What a full build ended with (the run-time caller records the counts
/// in its `build.json` sidecar; the CLI verb wants only the id).
#[derive(Default)]
pub struct BuildOutcome {
    pub index_id: String,
    /// The package versions the plan indexed.
    pub tasks: usize,
    /// Versions every shard of which the store already held.
    pub reused: usize,
    /// Versions with shards built this run.
    pub built: usize,
    /// The slice records, as the manifest carries them.
    pub slices: Vec<SliceRecord>,
    /// Task errors: `pkg@version` → why.
    pub errors: Vec<(String, String)>,
    /// Versions the plan refused (a run-time build fails on any), with
    /// their reasons.
    pub refused: Vec<RefusedRecord>,
}

/// Plan, build what is missing (none with `cfg.merge_only`), merge into
/// `cfg.out`. Returns the index id. `dry_run`: print the plan and stop
/// (returns an empty id).
///
/// The merge streams: every task's shards are read from the store one at
/// a time, in label order, into the [`Merger`] — never all at once.
pub fn run(cfg: &BuildConfig, dry_run: bool) -> Result<String, String> {
    build(cfg, dry_run, Trace::default()).map(|o| o.index_id)
}

/// [`run`] with the run-time caller's [`Trace`] (exp154,
/// library-names-plan step 3): every stage is a `library.index.<stage>`
/// span when a profiler is given, a stop flag ends the package sweep at
/// the next task boundary, and the counts come back for the caller's
/// build sidecar.
pub fn build(cfg: &BuildConfig, dry_run: bool, trace: Trace<'_>) -> Result<BuildOutcome, String> {
    let mut plan = {
        let _s = trace.stage("library.index.plan");
        plan(cfg, trace)?
    };
    if !cfg.quiet {
        print!("{}", describe(&plan));
    }
    if dry_run {
        return Ok(BuildOutcome::default());
    }
    let mut errors = Vec::new();
    if !cfg.merge_only {
        errors = execute_all(cfg, &plan.tasks, trace);
        for (label, e) in &errors {
            eprintln!("FAILED {label}: {e}");
        }
        if trace.stopped() {
            return Err("the run stopped the build: its wait deadline \
                 (library.index.wait) expired"
                .to_string());
        }
    }
    let mut inputs: Vec<(String, usize, ShardInputs)> = plan
        .tasks
        .iter()
        .enumerate()
        .flat_map(|(i, t)| {
            shard_inputs(
                &t.name,
                &t.version,
                &plan.slices,
                cfg.sponge_regions,
                cfg.names,
            )
            .into_iter()
            .map(move |s| (s.label(), i, s))
        })
        .collect();
    inputs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut found = vec![false; plan.tasks.len()];
    let mut merger = Merger::default();
    {
        let _s = trace.stage("library.index.merge");
        for (_, i, s) in &inputs {
            if let Some(shard) = read_current(&cfg.store, s) {
                found[*i] = true;
                merger.add(shard)?;
            }
        }
    }
    let mut packages = Vec::new();
    let (mut built, mut reused) = (0usize, 0usize);
    for (t, found) in plan.tasks.iter().zip(found) {
        if !found {
            plan.refused.push(RefusedRecord {
                name: t.name.clone(),
                version: t.version.clone(),
                reason: if cfg.merge_only {
                    "no-shard"
                } else {
                    "build-failed"
                }
                .into(),
            });
            continue;
        }
        if t.missing.is_empty() {
            reused += 1;
        } else {
            built += 1;
        }
        packages.push(PackageRecord {
            name: t.name.clone(),
            version: t.version.clone(),
            role: t.role.clone(),
            published: t.published.clone(),
        });
    }
    packages.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(compare_versions(&a.version, &b.version))
    });
    let index = merger.finish(MergeMeta {
        as_of: plan.as_of.clone(),
        registry: plan.registry.clone(),
        slices: plan.slices.clone(),
        packages,
        refused: plan.refused.clone(),
        staging: cfg.out.clone(),
    })?;
    if let Some(out) = &cfg.out {
        write_index(out, &index.files)?;
    }
    Ok(BuildOutcome {
        index_id: index.id,
        tasks: plan.tasks.len(),
        reused,
        built,
        slices: plan.slices,
        errors,
        refused: plan.refused,
    })
}

fn write_index(out: &Path, files: &BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("mkdir {}: {e}", out.display()))?;
    for (name, bytes) in files {
        std::fs::write(out.join(name), bytes).map_err(|e| format!("write {name}: {e}"))?;
    }
    Ok(())
}
