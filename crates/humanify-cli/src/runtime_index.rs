//! The run-time index machinery (exp154, docs/design/library-names-plan.md
//! step 3): when matching is `names` (the default) or `report` with NO
//! index passed, the run looks for the index it needs in its store
//! (`library.index_store_dir`, under the WORKING directory since exp174),
//! CONCURRENT with its own ingest — started as soon as detection resolves
//! the bundler, joined by the library load with a deadline
//! (`library.index.wait`), never a sleep-poll.
//!
//! `unified::pipeline_body` reaches this module through
//! `Config::library::runs_index_machinery`; `--library-matching off` (the
//! opt-out) and an explicit `--library-index` never do. WHICH index the run
//! needs is the bundle's KIND (exp175, Andrew 2026-11-01: "use our discovery
//! of the bundle type to inform how we build our index"): its packing tool,
//! read once off detection (`DetectedRecipe::kind`, `recipe::index_kind`),
//! the key index slices are already recorded under. What the user sees:
//!
//! - an index FOUND in the store for the kind: one line naming it — nothing
//!   is built;
//! - by DEFAULT, no stored index for the kind: ONE line ([`no_index_line`]:
//!   what is missing, where it looked, how to get one) and the run goes on
//!   with library name matching SKIPPED — vendor code still placed and
//!   named like app code, off's bytes. Not a failure (exp174's refusal is
//!   gone), and recorded as `skipped` so it cannot pass for a names run.
//! - a kind no index recipe exists for yet (browserify, parcel, a bundle
//!   whose bundler detection does not recognise) is skipped with a line
//!   saying so, with or without the run-time build;
//! - with `--library-index-from-bundle` (exp171's path, opt-in; every
//!   recipe kind since exp181 — Bun, esbuild, webpack, rollup): a bundle
//!   that reveals no npm package is skipped the same way, with its own
//!   line; otherwise [`building_line`], then the store's tool for the
//!   recipe the bundle needs (`DetectedRecipe::needed_recipe`), fetched into
//!   `<store>/tools/` when absent (`humanify_index::tools`), then the notice
//!   ([`build_notice`]: what is built, the estimated disk and time, where it
//!   is stored, how to opt out) — then the build. The run does not wait for
//!   a confirmation: a prompt would hang every batch run.
//!
//! An EXPLICIT index that does not fit the kind is not this module's: the
//! library load refuses it loudly (the user named it).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use humanify_core::libindex::recipe::DetectedRecipe;
use humanify_core::profiling::Profiler;
use humanify_model::profiling::trace_tid::LIBRARY_INDEX;
use serde::Serialize;

use crate::config::Config;
use humanify_index::build::Trace;
use humanify_index::cache::{
    Seed, StoreSearch, build_planned, estimate_build, plan_build, read_build_sidecar, search_store,
    uncovered, valid_package_name,
};
use humanify_index::recipe::RecipeBuild;
use humanify_index::tools::{Tool, fetch_tool, find_tool, pin};

/// How long the run waits for the machinery before it stops the build and
/// fails (one deadline over the whole wait, docs/concurrency.md).
pub const WAIT_DEADLINE: Duration = Duration::from_secs(90 * 60);

/// After the wait deadline stops the build: the grace for its last word.
/// Every subprocess the build runs is itself deadline'd and the stop flag
/// is checked between tasks, so the job thread is guaranteed to answer
/// inside this window; a miss is a bug, and the test panics rather than
/// hang the scope's join.
pub const STOP_GRACE: Duration = Duration::from_secs(15 * 60);

/// What the machinery answered the run's question with — delivered to the
/// join (`IndexJob::join`), never polled for.
pub enum JobAnswer {
    /// The store already held a usable index (its directory; the report
    /// trail is fully populated).
    Found(PathBuf, IndexSource),
    /// The run built one at run time (its fresh directory + trail).
    Built(PathBuf, IndexSource),
    /// No index for the bundle's kind, and none to build (exp175): the run
    /// goes on with library name matching skipped — as `off` — having
    /// printed the line (the text), and records the skip (the trail).
    Skipped(String, IndexSource),
    /// Why the machinery failed — the run fails loudly with it.
    Failed(String),
}

/// The usage trail the report's `libraryMatching` block gains when the
/// machinery ran (`indexSource`): what was found or built, from what, and
/// with which properties — the trail the brief asks the report to carry.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSource {
    /// "found" / "built" / "skipped".
    pub outcome: String,
    pub index_id: String,
    /// The store's root, as the run resolved it.
    pub store: String,
    /// The slice the bundle's recipe picked.
    pub slice: String,
    /// The bundle's index kind (`DetectedRecipe::kind`; for a found or
    /// built index, the kind its slice was recorded under — the same name).
    pub bundler: String,
    /// The bundler version the chosen slice was built with (the slice
    /// record's — NOT the bundle's own, which is not detected).
    pub bundler_version: String,
    /// Always "unknown": reading a bundle's own bundler version out of
    /// the bundle is not in scope.
    pub detected_bundler_version: String,
    /// The store's tool the run-time build ran (`esbuild 0.28.2`; built
    /// only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// The index's as-of, recorded (freshness deliberately not judged).
    pub as_of: String,
    /// The seed rows the bundle's own evidence gave the build (built only).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seeds: Vec<humanify_index::cache::PlannedSeed>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub versions_in_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub packages: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reused: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused: Option<usize>,
    /// The store search's per-candidate verdicts (what was considered and
    /// why it was or was not used).
    pub candidates: Vec<StoreCandidate>,
    /// Why the store said nothing (it may not exist before the first
    /// build).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub stage_ms: BTreeMap<String, u64>,
}

/// One candidate the store search looked at.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StoreCandidate {
    pub id: String,
    pub verdict: String,
}

/// The machinery's handle: a job thread running inside the caller's
/// `thread::scope`, and the channel its one answer arrives on. (The channel,
/// not a poll, is the wait.) Take it to [`IndexJob::join`].
impl IndexJob {
    /// Wait for the answer, one deadline over the whole wait. On expiry
    /// the stop flag is set and the build's final word (a failure naming
    /// `(library.index.wait)`) is awaited inside the grace. The wait's
    /// span — `library.index.wait` — carries the outcome metadata the
    /// profile summary's "Library index" block reads.
    pub fn join(self, profiler: &Profiler, stop: &AtomicBool) -> JobAnswer {
        let span = profiler.start_span("library.index.wait", "library-index", LIBRARY_INDEX, None);
        let answer = match self.rx.recv_timeout(WAIT_DEADLINE) {
            Ok(a) => a,
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                match self.rx.recv_timeout(STOP_GRACE) {
                    Ok(a) => a,
                    Err(_) => panic!(
                        "the run-time index build did not stop within \
                         {}s of the run's deadline — a subprocess ignored \
                         its own deadline, which is a bug",
                        STOP_GRACE.as_secs()
                    ),
                }
            }
        };
        span.end(Some(outcome_metadata(&answer)));
        answer
    }
}

/// The wait span's metadata: the outcome line the profile summary prints.
fn outcome_metadata(answer: &JobAnswer) -> humanify_model::profiling::JsObject {
    use humanify_model::profiling::JsObject;
    use serde_json::Value;
    let mut m = JsObject::new();
    let mut set = |k: &str, v: Value| m.set(k, v);
    match answer {
        JobAnswer::Found(_, trail) => {
            set("outcome", Value::String("found".into()));
            set("indexId", Value::String(trail.index_id.clone()));
        }
        JobAnswer::Built(_, trail) => {
            set("outcome", Value::String("built".into()));
            set("indexId", Value::String(trail.index_id.clone()));
            let n = |v: &Option<usize>| Value::from(v.unwrap_or(0));
            set("versions", n(&trail.versions_in_index));
            set("built", n(&trail.built));
            set("reused", n(&trail.reused));
            set("refused", n(&trail.refused));
        }
        JobAnswer::Skipped(why, _) => {
            set("outcome", Value::String("skipped".into()));
            set("why", Value::String(why.clone()));
        }
        JobAnswer::Failed(error) => {
            set("outcome", Value::String("failed".into()));
            set("error", Value::String(error.clone()));
        }
    }
    m
}

/// A started job: its thread runs inside the caller's scope; `stop` is the
/// flag the join sets when the wait deadline expires (checked by the
/// build between tasks).
pub struct IndexJob {
    rx: mpsc::Receiver<JobAnswer>,
}

/// Start the machinery on the caller's `thread::scope`: search the store
/// for an index of the bundle's kind (`detected`: the recipe detection read
/// off the bundle, once, in `unified::pipeline_body`), and when there is
/// none, skip — or, with `--library-index-from-bundle`, build one from the
/// packages the bundle itself reveals. Called right after detection
/// resolves the bundler, so the search/build overlaps the run's own
/// ingest; the answer is taken at `IndexJob::join`.
pub fn start<'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    profiler: &'scope Profiler,
    code: &'scope str,
    config: &Config,
    detected: &DetectedRecipe,
    stop: &'scope AtomicBool,
) -> IndexJob {
    let (tx, rx) = mpsc::sync_channel(1);
    let library = config.library.clone();
    let root = library.index_store_root();
    let detected = detected.clone();
    scope.spawn(move || {
        let answer = job(profiler, code, &library, &root, &detected, stop);
        let _ = tx.send(answer);
    });
    IndexJob { rx }
}

/// The opt-out every notice names.
const OPT_OUT: &str = "--library-matching off";

/// The flag that opts into the run-time build (exp174).
const FROM_BUNDLE: &str = "--library-index-from-bundle";

/// What the skip lines end with: where an index for this bundle's kind
/// comes from — the offline builder with the recipe this bundle needs
/// (`DetectedRecipe::needed_recipe`), pointing to one, and the run-time
/// build. A kind no recipe exists for has none of these: it says so.
fn ways_to_an_index(detected: &DetectedRecipe, offer_from_bundle: bool) -> String {
    let Some(recipe) = detected.needed_recipe() else {
        return format!("no index recipe exists for {} bundles yet", detected.kind);
    };
    let mut ways = vec![
        format!(
            "build one with `humanify-index build --names --recipe {} …`",
            recipe.spelling()
        ),
        "point to one with --library-index <dir>".to_string(),
    ];
    if offer_from_bundle {
        ways.push(format!("pass {FROM_BUNDLE}"));
    }
    let n = ways.len();
    format!("{}, or {}", ways[..n - 1].join(", "), ways[n - 1])
}

/// The ONE line a run prints when its store holds no index for the
/// bundle's kind and nothing will be built (exp175): the run goes on with
/// library name matching skipped. `from_bundle`: the run-time build is
/// already on (then it is not offered again).
pub fn no_index_line(
    detected: &DetectedRecipe,
    root: &std::path::Path,
    from_bundle: bool,
) -> String {
    format!(
        "no library index for {} bundles in {} — skipping library name matching ({})",
        detected.kind,
        root.display(),
        ways_to_an_index(detected, !from_bundle)
    )
}

/// `--library-index-from-bundle`, a bundle that reveals no npm package to
/// build from. `dropped`: names it offered that the registry does not
/// know (0: it offered none that can be a package).
fn no_package_line(detected: &DetectedRecipe, root: &std::path::Path, dropped: usize) -> String {
    format!(
        "no library index for {} bundles in {}, and this bundle reveals no npm package name \
         to build one from (its library banners and inlined package.json files name none{}) — \
         skipping library name matching ({})",
        detected.kind,
        root.display(),
        if dropped == 0 {
            String::new()
        } else {
            format!("; {dropped} name(s) it offered are not packages the npm registry knows")
        },
        ways_to_an_index(detected, false)
    )
}

/// The line a `--library-index-from-bundle` run prints just before it
/// builds (exp175), then the notice. `stored`: the store holds indexes of
/// this kind, none built from this bundle's packages.
pub fn building_line(kind: &str, root: &std::path::Path, stored: bool) -> String {
    if stored {
        format!(
            "no library index for {kind} bundles in {} covers this bundle's packages — building \
             one from this bundle into it",
            root.display()
        )
    } else {
        format!(
            "no library index for {kind} bundles — building one from this bundle into {}",
            root.display()
        )
    }
}

/// The trail of a skipped run (the report's `indexSource`): the kind, the
/// store, and what the search looked at and why none served.
fn skipped_trail(
    root: &std::path::Path,
    detected: &DetectedRecipe,
    candidates: Vec<StoreCandidate>,
    note: Option<String>,
) -> IndexSource {
    IndexSource {
        outcome: "skipped".to_string(),
        store: root.display().to_string(),
        bundler: detected.kind.to_string(),
        detected_bundler_version: "unknown".to_string(),
        candidates,
        note,
        ..Default::default()
    }
}

/// A stored index the run uses: the one quiet line, and the trail.
fn found_answer(
    root: &std::path::Path,
    found: &humanify_index::cache::FoundIndex,
    candidates: Vec<StoreCandidate>,
    note: Option<String>,
) -> JobAnswer {
    notice(&format!(
        "Library index: using {} for {} bundles from {} (found; nothing to build)",
        short_id(&found.id),
        found.bundler,
        root.display()
    ));
    JobAnswer::Found(
        found.dir.clone(),
        IndexSource {
            outcome: "found".to_string(),
            index_id: found.id.clone(),
            store: root.display().to_string(),
            slice: found.slice.clone(),
            bundler: found.bundler.clone(),
            bundler_version: found.bundler_version.clone(),
            detected_bundler_version: "unknown".to_string(),
            as_of: found.as_of.clone(),
            candidates,
            note,
            ..Default::default()
        },
    )
}

/// A notice line from the job thread, straight to stderr: the machinery
/// runs beside the run's setup, before any progress display draws.
fn notice(text: &str) {
    eprintln!("{text}");
}

/// The job thread's whole body: every failure is a loud `Failed` naming its
/// stage; no index for the bundle's kind, and none to build, is `Skipped`.
fn job(
    profiler: &Profiler,
    code: &str,
    library: &crate::config::Library,
    root: &std::path::Path,
    detected: &DetectedRecipe,
    stop: &AtomicBool,
) -> JobAnswer {
    if stop.load(Ordering::SeqCst) {
        return JobAnswer::Failed("the run stopped before the index machinery started".into());
    }
    // Search the store for the kind: one span over the whole look (the
    // verdicts it collected go to the trail even when it finds nothing).
    let span = profiler.start_span(
        "library.index.search-store",
        "library-index",
        LIBRARY_INDEX,
        None,
    );
    let search: StoreSearch = search_store(root, detected);
    span.end(None);
    let mut candidates = search
        .candidates
        .iter()
        .map(|(id, verdict)| StoreCandidate {
            id: id.clone(),
            verdict: verdict.clone(),
        })
        .collect::<Vec<_>>();
    let recipe = match detected.needed_recipe() {
        Some(recipe) if library.index_from_bundle => recipe,
        _ => {
            return match search.usable.first() {
                Some(found) => found_answer(root, found, candidates, search.note.clone()),
                None => JobAnswer::Skipped(
                    no_index_line(detected, root, library.index_from_bundle),
                    skipped_trail(root, detected, candidates, search.note.clone()),
                ),
            };
        }
    };
    // The bundle's own packages: what a build would index, and what a
    // found RUN-TIME index must cover to serve it (exp171 — the search
    // matches on the slice alone, so app A's index would otherwise serve
    // app B forever and B's packages would never be indexed).
    let seeds = bundle_revealed_seeds(code);
    let covering = search.usable.iter().find(|found| {
        let side = read_build_sidecar(&found.dir).ok().flatten();
        let missing = uncovered(side.as_ref(), &seeds);
        if !missing.is_empty() {
            candidates.push(StoreCandidate {
                id: found.id.clone(),
                verdict: format!(
                    "skipped: built from other packages — misses {}",
                    missing.join(", ")
                ),
            });
        }
        missing.is_empty()
    });
    let base =
        |outcome: &str, index_id: String, slice: String, bundler_version: String, as_of: String| {
            IndexSource {
                outcome: outcome.to_string(),
                index_id,
                store: root.display().to_string(),
                slice,
                bundler: detected.kind.to_string(),
                bundler_version,
                detected_bundler_version: "unknown".to_string(),
                as_of,
                candidates: candidates.clone(),
                note: search.note.clone(),
                ..Default::default()
            }
        };
    if let Some(found) = covering {
        return found_answer(root, found, candidates, search.note.clone());
    }
    // Nothing usable: build, from the bundle's own packages — no LLM, no
    // base-list bootstrap.
    // Only a name that can be an npm package is anything to build from
    // (Claude Code 2.1.216 offers two, both `azure/msal-…` without the `@`:
    // nothing to build, so nothing is created, fetched or asked).
    if !seeds.iter().any(|s| valid_package_name(&s.name)) {
        return JobAnswer::Skipped(
            no_package_line(detected, root, 0),
            base(
                "skipped",
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        );
    }
    notice(&building_line(
        detected.kind,
        root,
        !search.usable.is_empty(),
    ));
    // The recipe the bundle needs, run by the store's tool for its
    // bundler: a store without one gets the pinned tool first, announced
    // before the download.
    let (tool, fetched) = match store_tool(root, recipe.bundler) {
        Ok(found) => found,
        Err(e) => return JobAnswer::Failed(format!("run-time index build (tool): {e}")),
    };
    let recipes = vec![RecipeBuild {
        recipe,
        bin: tool.bin.clone(),
    }];
    let stage = std::sync::Mutex::new(String::new());
    let stages = std::sync::Mutex::new(BTreeMap::new());
    let trace = Trace {
        prof: Some(profiler),
        stop: Some(stop),
        stage: Some(&stage),
        stages: Some(&stages),
    };
    let plan = match plan_build(
        root,
        seeds,
        recipes,
        &today(),
        library.index_build_max_packages,
        trace,
    ) {
        Ok(plan) => plan,
        Err(e) => return JobAnswer::Failed(format!("run-time index build (plan): {e}")),
    };
    if plan.planned.is_empty() {
        return JobAnswer::Skipped(
            no_package_line(detected, root, plan.dropped.len()),
            base(
                "skipped",
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ),
        );
    }
    if plan.to_build == 0 {
        notice(&format!(
            "Library index: assembling one from {} version(s) already in {} (nothing to \
             download or build)",
            plan.reused,
            root.display()
        ));
    } else {
        notice(&build_notice(root, &plan, &tool, fetched));
    }
    let started = std::time::Instant::now();
    let built = match build_planned(root, plan, trace) {
        Ok(built) => built,
        Err(e) => return JobAnswer::Failed(e),
    };
    let s = &built.sidecar;
    notice(&format!(
        "Library index: built {} in {} — {} version(s) indexed, {} of them reused from the store{}; the \
         store now takes {}",
        short_id(&s.index_id),
        humanize_seconds(started.elapsed().as_secs()),
        s.built + s.reused,
        s.reused,
        if s.failed.is_empty() {
            String::new()
        } else {
            format!(
                ", {} could not be indexed ({})",
                s.failed.len(),
                s.failed
                    .iter()
                    .map(|f| f.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        },
        humanize_bytes(dir_bytes(root))
    ));
    let mut trail = base(
        "built",
        s.index_id.clone(),
        s.slices
            .first()
            .map(|x| x.slice.clone())
            .unwrap_or_default(),
        s.slices
            .first()
            .map(|x| x.bundler_version.clone())
            .unwrap_or_default(),
        s.as_of.clone(),
    );
    trail.tool = Some(tool.describe());
    trail.seeds = s.seeds.clone();
    trail.versions_in_index = Some(s.versions_in_index);
    trail.packages = Some(s.packages);
    trail.built = Some(s.built);
    trail.reused = Some(s.reused);
    trail.refused = Some(s.refused);
    trail.stage_ms = s.stage_ms.clone();
    JobAnswer::Built(built.dir.clone(), trail)
}

/// The notice a run prints BEFORE it builds an index (exp171): what is
/// built, the estimated disk and time, where it is stored, how to opt out.
pub(crate) fn build_notice(
    root: &std::path::Path,
    plan: &humanify_index::cache::RuntimePlan,
    tool: &Tool,
    fetched: bool,
) -> String {
    let est = estimate_build(tool.bundler, plan.to_build, 0);
    let mut lines = vec![
        "NOTICE: library names (the default) needs a library index for this bundle, and \
         none was found."
            .to_string(),
        format!(
            "  Building one now from the {} npm package(s) this bundle reveals ({} to build, \
             {} already in the store): each is installed from the npm registry (install \
             scripts skipped) and bundled with {}{}.",
            plan.planned.len(),
            plan.to_build,
            plan.reused,
            tool.describe(),
            if fetched {
                " (just fetched into the store)"
            } else {
                ""
            }
        ),
        format!(
            "  Estimated: {} to {} of disk, {} to {} (from measured {} builds; the run waits \
             for it, then goes on).",
            humanize_bytes(est.bytes_low),
            humanize_bytes(est.bytes_high),
            humanize_seconds(est.seconds_low),
            humanize_seconds(est.seconds_high),
            pin(tool.bundler).label
        ),
        format!(
            "  Stored in: {} (later runs with this store reuse it).",
            root.display()
        ),
        format!(
            "  To skip it: {OPT_OUT} (no library names), or --library-index <dir|id> to use \
             a prebuilt index."
        ),
    ];
    if !plan.dropped.is_empty() {
        let names: Vec<&str> = plan.dropped.iter().map(|d| d.name.as_str()).collect();
        lines.push(format!(
            "  Skipped {} name(s) the bundle offered that are not npm packages: {}",
            names.len(),
            names.join(", ")
        ));
    }
    lines.join("\n")
}

/// The store's tool for `bundler` (the newest it holds), else the pinned
/// one fetched into `<root>/tools/` — announced first, with its size.
/// The bool: it was fetched by this run.
fn store_tool(
    root: &std::path::Path,
    bundler: humanify_core::libindex::recipe::RecipeBundler,
) -> Result<(Tool, bool), String> {
    if let Ok(tool) = find_tool(root, bundler) {
        return Ok((tool, false));
    }
    let p = pin(bundler);
    notice(&format!(
        "Library index: fetching {} {} (~{}) into {} to build one ({OPT_OUT} skips library names)",
        p.label,
        p.version,
        humanize_bytes(p.fetch_bytes),
        root.join(humanify_index::cache::TOOLS_DIR).display()
    ));
    fetch_tool(root, bundler).map(|tool| (tool, true))
}

/// The first 12 hex digits of an index id (enough to find it on disk).
fn short_id(id: &str) -> &str {
    &id[..id.len().min(12)]
}

/// Bytes as the notice prints them (`512 MB`, `4.2 GB`).
pub(crate) fn humanize_bytes(bytes: u64) -> String {
    const GB: f64 = 1e9;
    const MB: f64 = 1e6;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else {
        format!("{:.0} MB", (b / MB).max(1.0))
    }
}

/// Seconds as the notice prints them (`40 s`, `12 min`, `2.5 h`).
pub(crate) fn humanize_seconds(s: u64) -> String {
    if s < 90 {
        format!("{s} s")
    } else if s < 90 * 60 {
        format!("{} min", (s + 30) / 60)
    } else {
        format!("{:.1} h", s as f64 / 3600.0)
    }
}

/// The bytes under `dir` (every regular file, recursively; a link is not
/// followed). The store's size after a build, for the closing line.
fn dir_bytes(dir: &std::path::Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.filter_map(|e| e.ok())
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_bytes(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// The packages the bundle ITSELF reveals (the run-time build's seed list:
/// no base-list bootstrap, no LLM): library banner comments, plus the
/// declarations inlined app package.json-style readers report. Dedup by
/// name; a declaration that spells the version beats a bare banner.
///
/// NOT rerouted through the discovery evidence exp153 owns
/// (crates/humanify-core/src/libdetect) — reroute the seed list onto its
/// captured evidence once that lane lands, and keep this scan answerable
/// in one place.
pub(crate) fn bundle_revealed_seeds(code: &str) -> Vec<Seed> {
    let mut by_name: BTreeMap<String, Seed> = BTreeMap::new();
    let mut admit = |seed: Seed| match by_name.get(&seed.name) {
        Some(held) if held.version.is_some() || seed.version.is_none() => {}
        _ => {
            by_name.insert(seed.name.clone(), seed);
        }
    };
    for region in humanify_core::libdetect::find_comment_regions(code) {
        if region.library_name.is_empty() {
            continue;
        }
        admit(Seed {
            name: region.library_name,
            version: None,
            evidence: format!("banner comment at byte {}", region.start),
        });
    }
    let allocator = oxc_allocator::Allocator::default();
    let ingest = humanify_core::ingest::Ingest::parse_unambiguous(&allocator, code);
    if ingest.errors.is_empty() {
        use oxc_span::GetSpan;
        let spans: Vec<oxc_span::Span> = ingest.program.body.iter().map(|s| s.span()).collect();
        for row in humanify_core::modules::identity::scan_app_declarations(code, &spans) {
            let Some(name) = row.package else { continue };
            admit(Seed {
                name,
                version: row.version,
                evidence: format!(
                    "{}{}",
                    row.reader,
                    if row.evidence.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", row.evidence)
                    }
                ),
            });
        }
    }
    by_name.into_values().collect()
}

/// Today UTC as `YYYY-MM-DD`: the run-time build's as-of.
fn today() -> String {
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    humanify_core::profiling::iso_from_unix_ms(ms)
        .chars()
        .take(10)
        .collect()
}
