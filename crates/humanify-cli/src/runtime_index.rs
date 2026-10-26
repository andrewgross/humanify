//! The run-time index machinery (exp154, docs/design/library-names-plan.md
//! step 3): when `library.index_build = "auto"` and matching is `report`
//! with NO index passed, the run finds or builds the index it needs in its
//! cache (`library.index_store_dir`), CONCURRENT with its own ingest —
//! started as soon as detection resolves the bundler, joined by the library
//! load with a deadline (`library.index.wait`), never a sleep-poll.
//!
//! Default OFF: `unified::pipeline_body` only reaches this module through
//! `Config::library::runs_index_machinery`, which is false unless the three
//! keys say otherwise — a default run never reads the store directory and
//! never spawns a build process.

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
use humanify_index::cache::{Seed, StoreSearch, build_runtime, bun_binary, search_store};
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

/// The job thread's whole body: every failure is a loud `Failed` naming its
/// stage.
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
        return JobAnswer::Failed(format!(
            "run-time index build (exp154): bun-bundled apps only in this \
             lane — this bundle was built with {has}. Pass --library-index \
             <dir|id>, or set library.index_build = \"off\""
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
    let candidates = search
        .candidates
        .iter()
        .map(|(id, verdict)| StoreCandidate {
            id: id.clone(),
            verdict: verdict.clone(),
        })
        .collect::<Vec<_>>();
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
                candidates,
                note: search.note.clone(),
                ..Default::default()
            }
        };
    if let Some(found) = &search.found {
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
    // Nothing usable: build. The seeds come from the bundle itself — no
    // LLM, no base-list bootstrap.
    let seeds = bundle_revealed_seeds(code);
    if seeds.is_empty() {
        return JobAnswer::Failed(
            "run-time index build (plan): the bundle reveals no package to \
             index — no library banner, no inlined package.json. Pass \
             --library-index <dir|id>, or set library.index_build = \"off\""
                .to_string(),
        );
    }
    // The pinned Bun: nothing is ever downloaded; a cache without one is a
    // loud failure naming the exact fetch command.
    let (bin, picked_bun) = match bun_binary(root, None) {
        Ok((bin, version)) => (bin, version),
        Err(e) => return JobAnswer::Failed(format!("run-time index build (bun): {e}")),
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
    let built = match build_runtime(
        root,
        seeds,
        recipes,
        &today(),
        library.index_build_max_packages,
        trace,
    ) {
        Ok(built) => built,
        Err(e) => return JobAnswer::Failed(e),
    };
    let s = &built.sidecar;
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
