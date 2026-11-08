//! Run the pipeline for one scored leg and record a full manifest of it
//! (the port of `experiments/lib/run-pipeline.ts`): exit code, wall time,
//! the peak RSS of the whole process tree (sampled from /proc while the
//! child lives — after it exits the number is unrecoverable), the LLM
//! cache entries written (counted ONCE, immediately before and after:
//! rule 10), which prior was used, commit + dirt, the binary as launched,
//! the binary's own configuration record and library outcome from its
//! stats file, where the split put things, and the promised artifacts.
//!
//! The leg's launch spec is written first as `<v>.runcfg.json` (the JSON
//! run.sh handed run-pipeline.ts, byte for byte: the launch goldens hold
//! it), and the leg launches exactly its `command` + `args`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use humanify_ag_model::js::{JsObject, JsValue};

use super::records::{js_pretty, manifest_warnings, num, obj, prior_kind_of};
use super::{Ctx, count_files, text, write};

/// How often the child's RSS is sampled (the first sample is taken at
/// once, so a short run is not recorded as using nothing).
const RSS_SAMPLE: Duration = Duration::from_millis(250);
/// Cap on recorded ERROR lines.
const MAX_ERRORS: usize = 10;
/// A whole pipeline run on the biggest pair, cold, on a loaded box.
pub const PIPELINE_DEADLINE: Duration = Duration::from_secs(12 * 3600);

/// What a label records about the binary that scores it
/// (pipeline-bin.ts `BinRecord`).
#[derive(Clone, Debug, PartialEq)]
pub struct LabelBin {
    pub path: String,
    pub sha256: String,
    /// Full sha of the workspace HEAD it was built from; "" = unknown.
    pub commit: String,
    pub dirty: bool,
    /// Whether this harness built it (false = no cargo workspace owns it).
    pub built: bool,
}

impl LabelBin {
    pub fn to_js(&self) -> JsValue {
        obj(vec![
            ("path", JsValue::str(self.path.as_str())),
            ("sha256", JsValue::str(self.sha256.as_str())),
            ("commit", JsValue::str(self.commit.as_str())),
            ("dirty", JsValue::Bool(self.dirty)),
            ("built", JsValue::Bool(self.built)),
        ])
    }
}

/// One leg's launch spec (run-pipeline.ts `RunConfig`).
#[derive(Clone, Debug)]
pub struct LegSpec {
    pub pair: String,
    pub version: String,
    pub label: String,
    pub results_dir: PathBuf,
    pub input: String,
    pub prior: String,
    /// None: not recorded (a walk hop has no base mode).
    pub base_mode: Option<String>,
    pub output_dir: PathBuf,
    pub repo: PathBuf,
    pub args: Vec<String>,
    /// The launch's config text (None: a legacy launch with no file).
    pub config_toml: Option<String>,
    pub stdout_path: PathBuf,
    pub endpoint: String,
    pub model: String,
    pub reasoning_effort: String,
    pub concurrency: u32,
    pub cache_dir: Option<String>,
    pub artifacts: Vec<String>,
    pub bin: LabelBin,
}

impl LegSpec {
    /// The `<v>.runcfg.json` record, in run.sh's key order.
    pub fn to_js(&self) -> JsValue {
        let mut o = JsObject::new();
        o.insert("pair", JsValue::str(self.pair.as_str()));
        o.insert("version", JsValue::str(self.version.as_str()));
        o.insert("label", JsValue::str(self.label.as_str()));
        o.insert("resultsDir", JsValue::str(text(&self.results_dir)));
        o.insert("input", JsValue::str(self.input.as_str()));
        o.insert("prior", JsValue::str(self.prior.as_str()));
        if let Some(b) = &self.base_mode {
            o.insert("baseMode", JsValue::str(b.as_str()));
        }
        o.insert("outputDir", JsValue::str(text(&self.output_dir)));
        o.insert("repo", JsValue::str(text(&self.repo)));
        o.insert("args", JsValue::str_array(&self.args));
        if let Some(t) = &self.config_toml {
            o.insert("configToml", JsValue::str(t.as_str()));
        }
        o.insert("stdoutPath", JsValue::str(text(&self.stdout_path)));
        o.insert("endpoint", JsValue::str(self.endpoint.as_str()));
        o.insert("model", JsValue::str(self.model.as_str()));
        o.insert(
            "reasoningEffort",
            JsValue::str(self.reasoning_effort.as_str()),
        );
        o.insert("concurrency", num(self.concurrency));
        o.insert("artifacts", JsValue::str_array(&self.artifacts));
        if let Some(c) = &self.cache_dir {
            o.insert("cacheDir", JsValue::str(c.as_str()));
        }
        o.insert(
            "command",
            JsValue::str_array(std::slice::from_ref(&self.bin.path)),
        );
        o.insert("bin", self.bin.to_js());
        JsValue::Object(o)
    }
}

/// Write the leg's spec (`<v>.runcfg.json`, jq's spelling: pretty + `\n`).
pub fn write_spec(spec: &LegSpec, path: &Path) -> Result<(), String> {
    write(path, &crate::json::js_text(&spec.to_js(), true))
}

/// What the leg came to.
pub struct LegOutcome {
    pub exit_code: i32,
    pub manifest: JsValue,
}

/// Every live process: (pid, ppid, VmHWM kB).
pub fn proc_tree() -> Vec<(u32, u32, Option<u64>)> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let pid: u32 = e.file_name().to_str()?.parse().ok()?;
            let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
            let ppid = status_field(&status, "PPid:")?;
            let hwm = status_field(&status, "VmHWM:");
            Some((pid, ppid as u32, hwm))
        })
        .collect()
}

fn status_field(status: &str, key: &str) -> Option<u64> {
    let line = status.lines().find(|l| l.starts_with(key))?;
    line[key.len()..].split_whitespace().next()?.parse().ok()
}

/// The largest peak RSS among `root` and all its descendants, in MB. MAX,
/// not SUM: what decides an OOM is the biggest single process.
pub fn peak_rss_mb_of_tree(samples: &[(u32, u32, Option<u64>)], root: u32) -> Option<u64> {
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = vec![root];
    let mut best: Option<u64> = None;
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        for (child, parent, _) in samples {
            if *parent == pid {
                stack.push(*child);
            }
        }
        if let Some((_, _, Some(kb))) = samples.iter().find(|(p, _, _)| *p == pid) {
            let mb = (*kb as f64 / 1024.0).round() as u64;
            best = Some(best.map_or(mb, |b| b.max(mb)));
        }
    }
    best
}

fn sha256_file(path: &str) -> String {
    std::fs::read(path)
        .map(|b| crate::committed::sha256_hex(&b))
        .unwrap_or_default()
}

fn read_js(path: &Path) -> Option<JsValue> {
    JsValue::parse(&std::fs::read_to_string(path).ok()?).ok()
}

/// bun's version: on PATH, else `~/.bun/bin/bun` (the boot gate was bitten
/// by "no bun" when it was only off PATH).
fn bun_version(ctx: &Ctx) -> String {
    let probe = |prog: &str| {
        crate::proc::run_ok(&ctx.spawn(prog, &["--version".into()], Duration::from_secs(60)))
            .map(|s| s.trim().to_string())
            .unwrap_or_default()
    };
    let direct = probe("bun");
    if !direct.is_empty() {
        return direct;
    }
    ctx.host
        .home
        .as_ref()
        .map(|h| probe(&text(&h.join(".bun/bin/bun"))))
        .unwrap_or_default()
}

/// Run the leg: spawn `bin.path args` in the repo with stdout + stderr to
/// `stdout_path`, sample RSS, count the cache, then write `<v>-run.json`
/// and print its one summary line and every warning.
pub fn run_leg(ctx: &Ctx, spec: &LegSpec, api_key: &str) -> Result<LegOutcome, String> {
    run_leg_with_prior_kind(ctx, spec, api_key, None)
}

/// [`run_leg`], recording `prior_kind` instead of the path rule's (a walk
/// hop's prior is the walk's previous hop, not the archive).
pub fn run_leg_with_prior_kind(
    ctx: &Ctx,
    spec: &LegSpec,
    api_key: &str,
    prior_kind: Option<&str>,
) -> Result<LegOutcome, String> {
    let started_at = humanify_ag_model::js::iso_now();
    let t0 = Instant::now();
    let cache = spec.cache_dir.as_deref().map(Path::new);
    let before = cache.map_or(0, count_files);
    let launched_sha = sha256_file(&spec.bin.path);
    let mut s = ctx.spawn(&spec.bin.path, &spec.args, PIPELINE_DEADLINE);
    s.cwd = Some(spec.repo.clone());
    s.env.push(("HUMANIFY_API_KEY".into(), api_key.into()));
    let mut peak: Option<u64> = None;
    let exit = crate::proc::run_to_file(&s, &spec.stdout_path, RSS_SAMPLE, &mut |pid| {
        if let Some(mb) = peak_rss_mb_of_tree(&proc_tree(), pid) {
            peak = Some(peak.map_or(mb, |p| p.max(mb)));
        }
    });
    let exit_code = match exit {
        Ok(e) => e.code,
        Err(e) => {
            ctx.say(format!("  run: {e}"));
            -1
        }
    };
    let wall = t0.elapsed().as_secs_f64().round();
    let after = cache.map_or(0, count_files);
    let run = RunFacts {
        started_at,
        wall,
        cache: (before, after),
        launched_sha,
        exit_code,
        peak,
    };
    let manifest = manifest_of(ctx, spec, &run, prior_kind);
    write(
        &spec.results_dir.join(format!("{}-run.json", spec.version)),
        &js_pretty(&manifest),
    )?;
    let rss = peak.map_or("?".to_string(), |p| format!("{p} MB"));
    ctx.say(format!(
        "  run: exit {exit_code}, {wall}s, peak RSS {rss}, cache +{}",
        after as i64 - before as i64
    ));
    for line in manifest_warnings(&manifest) {
        ctx.say(format!("  !! {line}"));
    }
    Ok(LegOutcome {
        exit_code,
        manifest,
    })
}

/// What the leg measured.
struct RunFacts {
    started_at: String,
    wall: f64,
    cache: (u64, u64),
    launched_sha: String,
    exit_code: i32,
    peak: Option<u64>,
}

fn manifest_of(ctx: &Ctx, spec: &LegSpec, run: &RunFacts, prior_kind: Option<&str>) -> JsValue {
    let (before, after) = run.cache;
    let (exit_code, peak, wall) = (run.exit_code, run.peak, run.wall);
    let stats = spec
        .artifacts
        .iter()
        .find(|p| p.ends_with(".stats.json"))
        .and_then(|p| read_js(Path::new(p)));
    let resolved = stats
        .as_ref()
        .and_then(|s| s.as_object()?.get("config").cloned());
    let library = stats
        .as_ref()
        .and_then(|s| s.as_object()?.get("libraryMatching").cloned());
    let placement = read_js(&spec.output_dir.join(".humanify/placement-stats.json"));
    let errors: Vec<String> = std::fs::read_to_string(&spec.stdout_path)
        .map(|t| {
            t.split('\n')
                .filter(|l| l.starts_with("ERROR:"))
                .take(MAX_ERRORS)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut inputs = vec![
        ("input", JsValue::str(spec.input.as_str())),
        ("prior", JsValue::str(spec.prior.as_str())),
        (
            "priorKind",
            JsValue::str(prior_kind.unwrap_or_else(|| prior_kind_of(&spec.prior))),
        ),
    ];
    if let Some(b) = &spec.base_mode {
        inputs.push(("baseMode", JsValue::str(b.as_str())));
    }
    let mut config = vec![
        ("endpoint", JsValue::str(spec.endpoint.as_str())),
        ("model", JsValue::str(spec.model.as_str())),
        (
            "reasoningEffort",
            JsValue::str(spec.reasoning_effort.as_str()),
        ),
        ("concurrency", num(spec.concurrency)),
    ];
    if let Some(r) = resolved {
        config.push(("resolved", r));
    }
    config.push((
        "cache",
        obj(vec![
            ("enabled", JsValue::Bool(spec.cache_dir.is_some())),
            ("entriesBefore", num(before as f64)),
            ("entriesAfter", num(after as f64)),
            ("written", num(after as f64 - before as f64)),
        ]),
    ));
    let mut outcome = vec![
        ("exitCode", num(exit_code)),
        ("errors", JsValue::str_array(&errors)),
    ];
    if let Some(p) = peak {
        outcome.push(("peakRssMb", num(p as f64)));
    }
    outcome.push(("artifacts", artifacts_of(&spec.artifacts)));
    let mut m = vec![
        ("pair", JsValue::str(spec.pair.as_str())),
        ("label", JsValue::str(spec.label.as_str())),
        ("startedAt", JsValue::str(run.started_at.as_str())),
        ("wallSeconds", num(wall)),
        (
            "provenance",
            obj(vec![
                ("commit", JsValue::str(super::head(&spec.repo, true))),
                ("dirty", JsValue::Bool(super::dirty(&spec.repo))),
                ("harness", JsValue::str("humanify-ag-dev")),
                ("bun", JsValue::str(bun_version(ctx))),
            ]),
        ),
        ("inputs", obj(inputs)),
        ("config", obj(config)),
        ("pipeline", pipeline_record(spec, &run.launched_sha)),
    ];
    if let Some(p) = placement {
        m.push(("placement", p));
    }
    if let Some(l) = library {
        m.push(("library", l));
    }
    m.push(("outcome", obj(outcome)));
    obj(m)
}

fn artifacts_of(paths: &[String]) -> JsValue {
    JsValue::Array(
        paths
            .iter()
            .map(|p| {
                let bytes = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                obj(vec![
                    ("path", JsValue::str(p.as_str())),
                    ("bytes", num(bytes as f64)),
                ])
            })
            .collect(),
    )
}

fn pipeline_record(spec: &LegSpec, launched_sha: &str) -> JsValue {
    obj(vec![
        ("kind", JsValue::str("rust-bin")),
        (
            "command",
            JsValue::str_array(std::slice::from_ref(&spec.bin.path)),
        ),
        (
            "bin",
            obj(vec![
                ("sha256", JsValue::str(launched_sha)),
                ("buildSha256", JsValue::str(spec.bin.sha256.as_str())),
                ("commit", JsValue::str(spec.bin.commit.as_str())),
                ("dirty", JsValue::Bool(spec.bin.dirty)),
            ]),
        ),
    ])
}
