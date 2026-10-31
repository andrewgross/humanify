//! The run-time index machinery (exp154, docs/design/library-names-plan.md
//! step 3): when `library.index_build = "auto"` and matching is `names` or
//! `report` with NO index passed, the run finds or builds the index it
//! needs in its cache (`library.index_store_dir`), CONCURRENT with its own
//! ingest — started as soon as detection resolves the bundler, joined by
//! the library load with a deadline (`library.index.wait`), never a
//! sleep-poll.
//!
//! DEFAULT ON since exp171 (names is the default): `unified::pipeline_body`
//! reaches this module through `Config::library::runs_index_machinery`;
//! `--library-matching off` (the opt-out) and an explicit `--library-index`
//! never do. What the user sees:
//!
//! - an index FOUND in the store: one line naming it — nothing is built;
//! - a bundle no index can be built for (not a Bun bundle, or one that
//!   reveals no npm package): one notice, and the run goes on as `off`
//!   (`JobAnswer::Inert`) — a default run must not fail on its input's
//!   bundler;
//! - a BUILD: the notice ([`build_notice`]) before anything is installed —
//!   what is built, the estimated disk and time, where it is stored, how
//!   to opt out — then the build. The run does not wait for a confirmation:
//!   a prompt would hang every batch run (exp171's decision; Andrew's rule
//!   "if the index is not ready, wait" means build it).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use humanify_core::libindex::recipe::{Recipe, RecipeBundler, detect_recipe};
use humanify_core::profiling::Profiler;
use humanify_model::detection::{BundlerType, MinifierType};
use humanify_model::profiling::trace_tid::LIBRARY_INDEX;
use serde::Serialize;

use crate::config::Config;
use humanify_index::build::Trace;
use humanify_index::cache::{
    PINNED_BUN, Seed, StoreSearch, build_planned, bun_binary, estimate_build, fetch_bun,
    plan_build, read_build_sidecar, search_store, uncovered, valid_package_name,
};
use humanify_index::recipe::RecipeBuild;

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
    /// No index can serve this bundle (not Bun-built, or it reveals no
    /// package): the run goes on as `--library-matching off`, and says so
    /// (the notice is the text).
    Inert(String),
    /// Why the machinery failed — the run fails loudly with it.
    Failed(String),
}

/// The usage trail the report's `libraryMatching` block gains when the
/// machinery ran (`indexSource`): what was found or built, from what, and
/// with which properties — the trail the brief asks the report to carry.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexSource {
    /// "found" / "built".
    pub outcome: String,
    pub index_id: String,
    /// The store's root, as the run resolved it.
    pub store: String,
    /// The slice the bundle's recipe picked.
    pub slice: String,
    pub bundler: String,
    /// The bundler version the chosen slice was built with (the slice
    /// record's npm `bun` version — NOT the bundle's own, which this lane
    /// does not detect).
    pub bundler_version: String,
    /// Always "unknown" in this lane: reading a JS bundle's own Bun
    /// version out of the bundle is not in scope.
    pub detected_bundler_version: String,
    /// The pinned Bun the run-time build used (built only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picked_bun: Option<String>,
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
        JobAnswer::Inert(why) => {
            set("outcome", Value::String("inert".into()));
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
/// for an index with the bundle's slice, and when there is none, build one
/// from the packages the bundle itself reveals. Called by
/// `unified::pipeline_body` right after detection resolves the bundler, so
/// the search/build overlaps the run's own ingest; the answer is taken at
/// `IndexJob::join`.
pub fn start<'scope>(
    scope: &'scope std::thread::Scope<'scope, '_>,
    profiler: &'scope Profiler,
    code: &'scope str,
    config: &Config,
    bundler: BundlerType,
    minifier: MinifierType,
    stop: &'scope AtomicBool,
) -> IndexJob {
    let (tx, rx) = mpsc::sync_channel(1);
    let library = config.library.clone();
    let output_dir = config.io.output_dir.clone();
    let root = library.index_store_root(&output_dir);
    scope.spawn(move || {
        let answer = job(profiler, code, &library, &root, bundler, minifier, stop);
        let _ = tx.send(answer);
    });
    IndexJob { rx }
}

/// The opt-out every notice names.
const OPT_OUT: &str = "--library-matching off";

/// A notice line from the job thread, straight to stderr: the machinery
/// runs beside the run's setup, before any progress display draws.
fn notice(text: &str) {
    eprintln!("{text}");
}

/// The job thread's whole body: every failure is a loud `Failed` naming its
/// stage; a bundle no index can serve is `Inert`.
fn job(
    profiler: &Profiler,
    code: &str,
    library: &crate::config::Library,
    root: &std::path::Path,
    bundler: BundlerType,
    minifier: MinifierType,
    stop: &AtomicBool,
) -> JobAnswer {
    if stop.load(Ordering::SeqCst) {
        return JobAnswer::Failed("the run stopped before the index machinery started".into());
    }
    let detected = detect_recipe(code, bundler, minifier);
    if detected.bundler != Some(RecipeBundler::Bun) {
        let has = detected
            .bundler
            .map(|b| b.name())
            .unwrap_or("an unknown bundler");
        return JobAnswer::Inert(format!(
            "Library names: off for this run — this bundle was built with {has}, and a \
             library index can only be built for Bun bundles so far (pass --library-index \
             <dir|id> with an index that has its slice, or {OPT_OUT} to silence this)"
        ));
    }
    // Search the store: one span over the whole look (the verdicts it
    // collected go to the trail even when it finds nothing).
    let span = profiler.start_span(
        "library.index.search-store",
        "library-index",
        LIBRARY_INDEX,
        None,
    );
    let search: StoreSearch = search_store(root, &detected);
    span.end(None);
    // The bundle's own packages: what a build would index, and what a
    // found RUN-TIME index must cover to serve it (exp171 — the search
    // matches on the slice alone, so app A's index would otherwise serve
    // app B forever and B's packages would never be indexed).
    let seeds = bundle_revealed_seeds(code);
    let mut candidates = search
        .candidates
        .iter()
        .map(|(id, verdict)| StoreCandidate {
            id: id.clone(),
            verdict: verdict.clone(),
        })
        .collect::<Vec<_>>();
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
                bundler: "bun".to_string(),
                bundler_version,
                detected_bundler_version: "unknown".to_string(),
                as_of,
                candidates: candidates.clone(),
                note: search.note.clone(),
                ..Default::default()
            }
        };
    if let Some(found) = covering {
        notice(&format!(
            "Library index: using {} from {} (found; nothing to build)",
            short_id(&found.id),
            root.display()
        ));
        return JobAnswer::Found(
            found.dir.clone(),
            base(
                "found",
                found.id.clone(),
                found.slice.clone(),
                found.bundler_version.clone(),
                found.as_of.clone(),
            ),
        );
    }
    // Nothing usable: build, from the bundle's own packages — no LLM, no
    // base-list bootstrap.
    // Only a name that can be an npm package is anything to build from
    // (Claude Code 2.1.216 offers two, both `azure/msal-…` without the `@`:
    // nothing to build, so nothing is created, fetched or asked).
    if !seeds.iter().any(|s| valid_package_name(&s.name)) {
        return JobAnswer::Inert(no_package_notice(root));
    }
    // The plan asks the bundler its version: a store with no Bun gets the
    // pinned one first, announced before the download.
    let fetched_bun = bun_binary(root, None).is_err();
    let (bin, picked_bun) = match bun_binary(root, None) {
        Ok(found) => found,
        Err(_) => {
            notice(&format!(
                "Library index: none in {} serves this bundle — fetching Bun {PINNED_BUN} \
                 (~{}) into it to build one ({OPT_OUT} skips library names)",
                root.display(),
                humanize_bytes(humanify_index::cache::BUN_FETCH_BYTES)
            ));
            match fetch_bun(root, PINNED_BUN) {
                Ok(fetched) => fetched,
                Err(e) => return JobAnswer::Failed(format!("run-time index build (bun): {e}")),
            }
        }
    };
    let mut recipes = Vec::new();
    for target in ["bun", "node"] {
        let recipe = Recipe::parse(&format!("bun:target={target},format=cjs,minify,production"))
            .expect("a literal recipe of this build");
        recipes.push(RecipeBuild {
            recipe,
            bin: bin.clone(),
        });
    }
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
        return JobAnswer::Inert(format!(
            "{} ({} name(s) it offered are not packages the npm registry knows)",
            no_package_notice(root),
            plan.dropped.len()
        ));
    }
    if plan.to_build == 0 {
        notice(&format!(
            "Library index: assembling one from {} version(s) already in {} (nothing to \
             download or build)",
            plan.reused,
            root.display()
        ));
    } else {
        notice(&build_notice(root, &plan, fetched_bun));
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
    trail.picked_bun = Some(picked_bun);
    trail.seeds = s.seeds.clone();
    trail.versions_in_index = Some(s.versions_in_index);
    trail.packages = Some(s.packages);
    trail.built = Some(s.built);
    trail.reused = Some(s.reused);
    trail.refused = Some(s.refused);
    trail.stage_ms = s.stage_ms.clone();
    JobAnswer::Built(built.dir.clone(), trail)
}

/// The notice for a Bun bundle that reveals no npm package to index.
fn no_package_notice(root: &std::path::Path) -> String {
    format!(
        "Library names: off for this run — no library index in {} serves this bundle, \
         and it reveals no npm package name to build one from (its library banners and \
         inlined package.json files name none). Pass --library-index <dir|id> to use a prebuilt index, or \
         {OPT_OUT} to silence this",
        root.display()
    )
}

/// The notice a run prints BEFORE it builds an index (exp171): what is
/// built, the estimated disk and time, where it is stored, how to opt out.
pub(crate) fn build_notice(
    root: &std::path::Path,
    plan: &humanify_index::cache::RuntimePlan,
    fetched_bun: bool,
) -> String {
    let est = estimate_build(plan.to_build, false);
    let mut lines = vec![
        "NOTICE: library names (the default) needs a library index for this bundle, and \
         none was found."
            .to_string(),
        format!(
            "  Building one now from the {} npm package(s) this bundle reveals ({} to build, \
             {} already in the store): each is installed from the npm registry (install \
             scripts skipped) and bundled with Bun{}.",
            plan.planned.len(),
            plan.to_build,
            plan.reused,
            if fetched_bun {
                format!(" {PINNED_BUN} (just fetched into the store)")
            } else {
                String::new()
            }
        ),
        format!(
            "  Estimated: {} to {} of disk, {} to {} (measured on Bun-built apps; the run \
             waits for it, then goes on).",
            humanize_bytes(est.bytes_low),
            humanize_bytes(est.bytes_high),
            humanize_seconds(est.seconds_low),
            humanize_seconds(est.seconds_high)
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
