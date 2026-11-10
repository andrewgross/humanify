//! The library score's RUNNER half (the launch part of
//! `experiments/096-library-matching-score/run.py`): build the binary at a
//! commit, write the discover verb's config (the claim rule as
//! `[library.lookup] agreement`, report mode unless the label sets
//! matching itself, then the label's `--pipeline-set` entries), and run
//! `humanify-ag discover --unit-prints` once per corpus bundle under
//! `taskset`, biggest first, `jobs` at a time, each with its wall time and
//! peak RSS recorded beside its report (`<id>.run.json`).
//!
//! The corpus (which bundles, verified frozen) and the scoring of the
//! reports are the library scorer's (lane L2b, `library score`); the
//! library launch goldens hold the build, every discover launch and its
//! config file byte for byte.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{Ctx, text, write};
use crate::bin_build;
use crate::json::py::PyValue;

/// One discover run on the biggest corpus bundle (seconds; the deadline
/// covers a loaded box).
const DISCOVER_DEADLINE: Duration = Duration::from_secs(2 * 3600);

/// One corpus bundle to run.
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: String,
    pub bundle: String,
    /// The bundler that BUILT it (`builtWith`, else the corpus bundler).
    pub bundler: String,
    pub bytes: u64,
}

/// One label's discover runs.
pub struct DiscoverRun<'a> {
    pub bin: &'a Path,
    pub index: &'a Path,
    pub config: &'a Path,
    pub reports: &'a Path,
    pub cpus: &'a str,
    pub jobs: usize,
    /// The runs' working directory (None: inherited).
    pub cwd: Option<PathBuf>,
}

/// The discover verb's config file for a claim rule (`agreement` or
/// `structural`) and the label's settings. Every other library tunable
/// stays at its compiled-in default. The score measures CLAIMS, which
/// every mode makes alike, so it pins `report` — the mode every card
/// before exp171 was scored in — unless the label sets matching itself.
pub fn write_rule_config(
    run_dir: &Path,
    rule: &str,
    settings: &[String],
) -> Result<PathBuf, String> {
    let path = run_dir.join("discover.toml");
    let mut entries = vec![format!(
        "library.lookup.agreement={}",
        if rule == "agreement" { "true" } else { "false" }
    )];
    if !settings.iter().any(|s| s.starts_with("library.matching=")) {
        entries.push("library.matching=report".into());
    }
    entries.extend(settings.iter().cloned());
    crate::launch::write_config(&path, &entries)
        .map_err(|e| format!("REFUSED: the --pipeline-set settings {settings:?} ({e})"))?;
    Ok(path)
}

/// The binary a label runs: this checkout's HEAD (clean under crates/ and
/// Cargo.*: a dirty tree would make the label describe code that did not
/// run), or `commit` built in a detached worktree under `builds`.
pub fn build(
    ctx: &Ctx,
    commit: Option<&str>,
    builds: &Path,
) -> Result<bin_build::BinRecord, String> {
    match commit {
        Some(c) => super::bin::build_ref(ctx, &ctx.repo, c, builds),
        None => {
            let dirty = crate::proc::git(
                &ctx.repo,
                &[
                    "status",
                    "--porcelain",
                    "--",
                    "crates",
                    "Cargo.toml",
                    "Cargo.lock",
                ],
            )?;
            if !dirty.is_empty() {
                return Err(format!(
                    "REFUSED: crates/ is dirty — commit first, or pass --commit:\n{dirty}"
                ));
            }
            ctx.build(&ctx.repo)
        }
    }
}

fn run_one(ctx: &Ctx, run: &DiscoverRun, e: &Entry) -> Result<f64, String> {
    let report = run.reports.join(format!("{}.json", e.id));
    let args: Vec<String> = [
        "-c",
        run.cpus,
        &text(run.bin),
        "discover",
        &e.bundle,
        "--library-index",
        &text(run.index),
        "--config",
        &text(run.config),
        "--bundler",
        &e.bundler,
        "--unit-prints",
        "--out",
        &text(&report),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut s = ctx.spawn("taskset", &args, DISCOVER_DEADLINE);
    s.cwd.clone_from(&run.cwd);
    let log = run.reports.join(format!("{}.discover.log", e.id));
    let t = Instant::now();
    let mut peak: Option<u64> = None;
    let exit = crate::proc::run_to_file(&s, &log, Duration::from_millis(250), &mut |pid| {
        let tree = super::leg::proc_tree();
        if let Some(mb) = super::leg::peak_rss_mb_of_tree(&tree, pid) {
            peak = Some(peak.map_or(mb, |p| p.max(mb)));
        }
    })?;
    let wall = t.elapsed().as_secs_f64();
    if exit.code != 0 {
        let tail = std::fs::read_to_string(&log).unwrap_or_default();
        let tail: String = tail
            .chars()
            .rev()
            .take(2000)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        return Err(format!(
            "FATAL: {}: taskset {}\n{tail}",
            e.id,
            args.join(" ")
        ));
    }
    let _ = std::fs::remove_file(&log);
    // Python's round(x, 2) and round(kb / 1024) (banker's rounding on an
    // exact half, which a wall time in seconds never is).
    let record = PyValue::Dict(vec![
        (
            "wallSeconds".into(),
            PyValue::Float((wall * 100.0).round() / 100.0),
        ),
        (
            "peakRssMb".into(),
            PyValue::Int(peak.unwrap_or(0).to_string()),
        ),
    ]);
    write(
        &run.reports.join(format!("{}.run.json", e.id)),
        &crate::json::py_text(&record, true),
    )?;
    Ok(wall)
}

/// Run every entry (biggest first, so the long ones do not trail), `jobs`
/// at a time; the first failure stops the label. Completion order reaches
/// nothing but the progress lines.
pub fn run_discover(ctx: &Ctx, run: &DiscoverRun, entries: &[Entry]) -> Result<(), String> {
    let mut order: Vec<&Entry> = entries.iter().collect();
    order.sort_by_key(|e| std::cmp::Reverse(e.bytes));
    std::fs::create_dir_all(run.reports).map_err(|e| format!("{}: {e}", run.reports.display()))?;
    let t0 = Instant::now();
    let results = super::each(&order, run.jobs, |e| run_one(ctx, run, e));
    for (e, r) in order.iter().zip(&results) {
        match r {
            Ok(wall) => ctx.say(format!("  ran {} in {wall:.1}s", e.id)),
            Err(err) => return Err(err.clone()),
        }
    }
    ctx.say(format!(
        "ran {} bundles in {:.0}s",
        order.len(),
        t0.elapsed().as_secs_f64()
    ));
    Ok(())
}

/// One `library score` run's settings (run.py `parse`).
pub struct LibraryScore {
    pub label: String,
    pub index: PathBuf,
    pub rule: String,
    pub commit: Option<String>,
    pub jobs: usize,
    pub cpus: String,
    pub corpus: String,
    pub sets: Option<Vec<String>>,
    pub force_mixed: bool,
    pub settings: Vec<String>,
    pub data_root: Option<String>,
    /// The npm registry-manifest cache scorer 15's copy credit reads.
    pub registry: PathBuf,
}

fn py_str(s: &str) -> PyValue {
    PyValue::Str(s.to_string())
}

/// The label's run record (run.py `config`), Python spelling.
fn label_config(
    c: &LibraryScore,
    bin: &bin_build::BinRecord,
    index: &(String, u64),
    corpus_id: &str,
) -> Vec<(String, PyValue)> {
    let mut cfg = vec![
        ("buildCommit".to_string(), py_str(&bin.commit)),
        ("binarySha256".into(), py_str(&bin.sha256)),
        ("indexDir".into(), py_str(&text(&c.index))),
        ("indexId".into(), py_str(&index.0)),
        ("indexBytes".into(), PyValue::Int(index.1.to_string())),
        ("rule".into(), py_str(&c.rule)),
        ("corpusSha256".into(), py_str(corpus_id)),
        (
            "instrument".into(),
            py_str(
                "humanify discover --unit-prints --bundler <builtWith, else the corpus bundler> --config discover.toml",
            ),
        ),
    ];
    if !c.settings.is_empty() {
        cfg.push((
            "settings".into(),
            PyValue::List(c.settings.iter().map(|s| py_str(s)).collect()),
        ));
    }
    if c.corpus != "dev" {
        cfg.push(("corpus".into(), py_str(&c.corpus)));
    }
    if let Some(sets) = &c.sets {
        let mut s: Vec<String> = sets.iter().filter(|x| !x.is_empty()).cloned().collect();
        s.sort();
        s.dedup();
        cfg.push((
            "sets".into(),
            PyValue::List(s.iter().map(|x| py_str(x)).collect()),
        ));
    }
    cfg
}

/// The index's identity (its manifest's sha) and its bytes (run.py
/// `index_facts`).
fn index_facts(dir: &Path) -> Result<(String, u64), String> {
    let id = crate::library::corpus::sha256_file(&dir.join("manifest.json"))?;
    let bytes = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|e| e.metadata().ok())
        .filter(std::fs::Metadata::is_file)
        .map(|m| m.len())
        .sum();
    Ok((id, bytes))
}

/// A label is ONE configuration: re-running it with another binary, index,
/// rule or corpus is refused (`--force-mixed` overrides, loudly).
fn guard(ctx: &Ctx, run_dir: &Path, cfg: &[(String, PyValue)], force: bool) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(run_dir.join("pipeline.json")) else {
        return Ok(());
    };
    let old: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("pipeline.json: {e}"))?;
    let diff: Vec<&str> = cfg
        .iter()
        .filter(|(k, v)| {
            let now: serde_json::Value =
                serde_json::from_str(&crate::json::py_text(v, false)).unwrap_or_default();
            old.get(k) != Some(&now)
        })
        .map(|(k, _)| k.as_str())
        .collect();
    if diff.is_empty() {
        return Ok(());
    }
    let msg = format!("label already holds a run with another {}", diff.join(", "));
    if !force {
        return Err(format!(
            "REFUSED: {msg}.\nPick a new label, or --force-mixed if mixing is deliberate."
        ));
    }
    ctx.say(format!("!! MIXED CONFIG in label — {msg} — forced."));
    Ok(())
}

/// `library score <label>`: verify the frozen corpus, build the binary, run
/// discover over the corpus (or its `--sets`), record the label, then score
/// it with the library scorer (lane L2b's `rescore`, scorer 14).
pub fn score_label(ctx: &Ctx, c: &LibraryScore) -> Result<String, String> {
    use crate::library::corpus::{Corpus, Role, verify};
    let corpus = Corpus::open(&ctx.repo, &c.corpus, c.data_root.as_deref())?;
    let problems = verify(&corpus);
    if !problems.is_empty() {
        return Err(format!(
            "REFUSED: the frozen corpus moved (corpus verify):\n{}",
            problems.join("\n")
        ));
    }
    let run_dir = corpus.data.join("results").join(&c.label);
    let reports = run_dir.join("reports");
    std::fs::create_dir_all(&reports).map_err(|e| format!("{}: {e}", reports.display()))?;
    let builds = PathBuf::from(&corpus.data).join("builds");
    let bin = build(ctx, c.commit.as_deref(), &builds)?;
    let index = index_facts(&c.index)?;
    let cfg = label_config(c, &bin, &index, &corpus.loaded.manifest_id);
    guard(ctx, &run_dir, &cfg, c.force_mixed)?;
    write(
        &run_dir.join("pipeline.json"),
        &crate::json::py_text(&PyValue::Dict(cfg.clone()), true),
    )?;
    let config = write_rule_config(&run_dir, &c.rule, &c.settings)?;
    let all = corpus.entries();
    if let Some(sets) = &c.sets {
        let unknown: Vec<&String> = sets
            .iter()
            .filter(|s| !all.iter().any(|e| &e.set == *s))
            .collect();
        if !unknown.is_empty() {
            return Err(format!(
                "REFUSED: --sets names no set of this corpus: {unknown:?}"
            ));
        }
    }
    let mut entries = Vec::new();
    for e in all
        .iter()
        .filter(|e| c.sets.as_ref().is_none_or(|s| s.contains(&e.set)))
    {
        entries.push(Entry {
            id: e.id.clone(),
            bundle: text(&corpus.corpus_path(e, Role::Bundle)?),
            bundler: e.built_with.clone().unwrap_or_else(|| e.bundler.clone()),
            bytes: e.files.bundle.bytes,
        });
    }
    let run = DiscoverRun {
        bin: &bin.path,
        index: &c.index,
        config: &config,
        reports: &reports,
        cpus: &c.cpus,
        jobs: c.jobs,
        cwd: None,
    };
    run_discover(ctx, &run, &entries)?;
    let committed: Vec<(String, PyValue)> =
        cfg.into_iter().filter(|(k, _)| k != "indexDir").collect();
    let out = ctx.repo.join(crate::paths::LIBRARY_RESULTS).join(&c.label);
    write(
        &out.join("pipeline.json"),
        &crate::json::py_text(&PyValue::Dict(committed), true),
    )?;
    crate::library::score::rescore(
        &ctx.repo,
        &c.label,
        (&c.corpus, c.data_root.as_deref()),
        &c.registry,
        &crate::library::score::Out::Committed,
    )
}
