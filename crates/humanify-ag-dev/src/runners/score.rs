//! `eval score <label>`: score the CURRENT pipeline on the eval's version
//! pairs (the port of `scripts/eval.ts score` + `034-eval-harness/run.sh`).
//!
//! Per pair: rebuild the base v-1 with the current pipeline and no prior
//! (a scratch base), run the scored leg (`leg::run_leg`: the run
//! manifest), record the run status (and REFUSE the card when a run lost
//! LLM answers or skipped library matching), score the card (`scoring`),
//! write the evidence page, boot the tree. Then the self-hop on the
//! `selfHop` pair (the last attempted one when `--pairs` left it out) — COLD into a fresh cache, then WARM replaying a copy of
//! it, which must be byte-identical with 0 cache writes — and the summary.
//!
//! The binary runs at ALL THREE launch sites (rebase, scored leg,
//! self-hop); the harness builds it first and refuses one not built from
//! the label's commit (`bin`). Every launch passes its settings in a
//! `--config` file (`launch`); run-launch.golden.txt holds every launch
//! byte for byte.
//!
//! Nothing falls back. A missing input is refused before anything builds;
//! a pair that fails (rebase, scored leg, analysis, a refused card, the
//! self-hop) is said in its own block, the other pairs still run, and the
//! run ends with one `!! eval score FAILED` verdict naming every failed
//! pair and exits 1. No LLM cache by default: a cached run is never a
//! verdict about LLM-dependent behaviour (rule 10).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use crate::json::js::JsValue;

use super::boot;
use super::lanes::Lanes;
use super::leg::{self, LabelBin, LegSpec};
use super::records::{
    self, StatusSources, js_compact_line, library_refusal, obj, provider_error_refusal,
};
use super::scoring::{self, PairInputs};
use super::{Ctx, count_files, input_of, remove, text, write};
use crate::committed::eval::{Pairs, pairs as load_pairs};
use crate::launch::{HumanifyArgs, base_settings, write_config};
use crate::paths::EVAL_RESULTS;
use crate::settings::Parsed;

/// The PIPELINE keys `eval score` writes into a launch's file itself; a
/// `--pipeline-set` naming one is refused before anything builds.
pub const SCORE_HARNESS_KEYS: &[&str] = &[
    "llm.reasoning_effort",
    "llm.concurrency",
    "split.enabled",
    "llm.cache_dir",
    "diagnostics.log_file",
    "diagnostics.stats_json",
    "diagnostics.diagnostics_file",
    "library.index",
];

/// `eval score`'s default work dir (trees, logs, run configs). Not /tmp:
/// an overlay that does not survive a container rebuild (dev-harness §8
/// risk 6 lost the reference trees' first home that way).
pub const DEFAULT_WORKDIR: &str = "/work/eval-work";

/// One run's settings, resolved once.
#[derive(Clone, Debug)]
pub struct ScoreConfig {
    pub label: String,
    pub pairs: Option<Vec<String>>,
    pub endpoint_flag: Option<String>,
    pub llm_cache: Option<String>,
    pub boot_prompt: bool,
    pub self_hop: bool,
    pub inputs_base: String,
    pub workdir: PathBuf,
    pub bin: PathBuf,
    pub force_mixed: bool,
    pub pipeline_sets: Vec<String>,
    pub naming_mode: String,
    pub library_index: Option<String>,
    pub boot_model: String,
    /// How many pairs run at once (`--parallel-pairs`, exp207); 1 = serial.
    pub parallel_pairs: usize,
    /// How many of their rebases run at once (`--parallel-rebases`).
    pub parallel_rebases: usize,
}

/// The naming mode `sets` select, and the index a names launch is handed:
/// a names-mode run NEVER lets the binary find or build an index at run
/// time (its as-of would be the day it ran). `library.index` is the
/// harness's own key (`--library-index`), refused in the sets.
pub fn naming_mode_of(
    sets: &[String],
    index_flag: Option<&Path>,
    pairs_index: &str,
) -> Result<(String, Option<String>), String> {
    let default = crate::settings::pipeline_default("library.matching")
        .and_then(|v| match v {
            humanify_ag_cli::registry::Val::Text(s) => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| "names".to_string());
    let matching = sets
        .iter()
        .filter_map(|e| e.strip_prefix("library.matching="))
        .next_back()
        .map(str::to_string)
        .unwrap_or(default);
    if sets.iter().any(|e| e.starts_with("library.index=")) {
        return Err("--pipeline-set library.index=…: the index is the harness's own key — pass --library-index <dir>".into());
    }
    if matching == "off" {
        if index_flag.is_some() {
            return Err("--library-index given with library.matching=off: the index would be silently ignored".into());
        }
        return Ok(("off".into(), None));
    }
    let index = match index_flag {
        Some(f) => super::text(f),
        None => pairs_index.to_string(),
    };
    if index.is_empty() {
        return Err(format!(
            "library.matching={matching} needs an index: pass --library-index <dir> (pairs.json has no libraryIndex)"
        ));
    }
    let mode = if matching == "names" { "names" } else { "off" };
    Ok((mode.into(), Some(index)))
}

/// The run's settings from its parsed command line and pairs.json.
pub fn config_from(parsed: &Parsed, pairs: &Pairs, ctx: &Ctx) -> Result<ScoreConfig, String> {
    let repo = &ctx.repo;
    let absolute = |p: &str| ctx.absolute(p);
    let s = &parsed.settings;
    let [label] = parsed.positionals.as_slice() else {
        return Err("usage: eval score <label> [flags]".into());
    };
    let sets = s.list("run.pipeline_set").unwrap_or_default();
    crate::settings::check_pipeline_sets(&sets, SCORE_HARNESS_KEYS).map_err(|e| e.join("\n"))?;
    let (naming_mode, library_index) = naming_mode_of(
        &sets,
        s.text("eval.library_index")
            .map(|p| absolute(&p))
            .as_deref(),
        &pairs.library_index,
    )?;
    Ok(ScoreConfig {
        label: label.clone(),
        pairs: s.list("eval.pairs").filter(|p| !p.is_empty()),
        endpoint_flag: s.text("llm.endpoint"),
        llm_cache: s.text("eval.llm_cache"),
        boot_prompt: s.flag_on("eval.boot_prompt"),
        self_hop: s.flag_on("eval.self_hop"),
        inputs_base: s
            .text("eval.inputs_base")
            .unwrap_or_else(|| pairs.inputs_base.clone()),
        workdir: absolute(
            &s.text("run.workdir")
                .unwrap_or_else(|| DEFAULT_WORKDIR.into()),
        ),
        bin: absolute(
            &s.text("run.bin")
                .unwrap_or_else(|| text(&repo.join("target/release/humanify-ag"))),
        ),
        force_mixed: s.flag_on("run.force_mixed"),
        pipeline_sets: sets,
        naming_mode,
        library_index,
        boot_model: s
            .text("boot.model")
            .unwrap_or_else(|| "claude-opus-4-1".into()),
        parallel_pairs: usize::try_from(s.int("eval.parallel_pairs").unwrap_or(1))
            .unwrap_or(1)
            .max(1),
        parallel_rebases: usize::try_from(s.int("eval.parallel_rebases").unwrap_or(1))
            .unwrap_or(1)
            .max(1),
    })
}

/// The pair filter: the full form (`2.1.215->2.1.216`) or the patch-only
/// shorthand (`215->216`) matches.
fn selected(filter: &Option<Vec<String>>, from: &str, to: &str) -> bool {
    let Some(f) = filter else { return true };
    let full = format!("{from}->{to}");
    let short = format!(
        "{}->{}",
        from.rsplit('.').next().unwrap_or(from),
        to.rsplit('.').next().unwrap_or(to)
    );
    f.iter().any(|p| *p == full || *p == short)
}

// ─── the label guards ────────────────────────────────────────────────────

/// Refuse a label already holding cards that this run would mix with:
/// another commit, naming mode or library index. `--force-mixed` turns
/// each refusal into a loud line.
fn guard_label(ctx: &Ctx, dir: &Path, cfg: &ScoreConfig) -> Result<(), String> {
    let Ok(recorded) = std::fs::read_to_string(dir.join("commit.txt")) else {
        return Ok(());
    };
    let recorded = recorded.split_whitespace().next().unwrap_or("").to_string();
    let head = super::head(&ctx.repo, false);
    let pipeline = std::fs::read_to_string(dir.join("pipeline.json"))
        .ok()
        .and_then(|t| JsValue::parse(&t).ok());
    let get = |k: &[&str]| -> Option<String> {
        let mut cur = pipeline.as_ref()?;
        for key in k {
            cur = cur.as_object()?.get(key)?;
        }
        match cur {
            JsValue::String(s) => Some(s.clone()),
            JsValue::Null => Some("null".into()),
            _ => None,
        }
    };
    let mut mixed: Vec<(String, String, String)> = Vec::new();
    if !recorded.is_empty() && !crate::bin_build::same_commit(&recorded, &head) {
        mixed.push(("commits".into(), short(&recorded), short(&head)));
    }
    let checks = [
        (
            "naming modes",
            get(&["namingMode"]),
            cfg.naming_mode.clone(),
        ),
        (
            "library indexes",
            get(&["libraryIndex"]),
            cfg.library_index.clone().unwrap_or_else(|| "null".into()),
        ),
    ];
    for (what, was, now) in checks {
        if let Some(was) = was.filter(|w| *w != now) {
            mixed.push((what.into(), was, now));
        }
    }
    if mixed.is_empty() {
        return Ok(());
    }
    let lines: Vec<String> = mixed
        .iter()
        .map(|(what, was, now)| format!("{what}: the label holds {was}, this run would add {now}"))
        .collect();
    if cfg.force_mixed {
        for (what, was, now) in &mixed {
            ctx.say(format!(
                "!! MIXED {} in label '{}' ({was} + {now}) — forced.",
                what.to_uppercase(),
                cfg.label
            ));
        }
        return Ok(());
    }
    Err(format!(
        "label '{}' already holds cards this run would mix with:\n  {}\nThe summary would total them as one run.\n\
         Pick a new label, or pass --force-mixed if mixing is deliberate.",
        cfg.label,
        lines.join("\n  ")
    ))
}

fn short(sha: &str) -> String {
    sha.chars().take(12).collect()
}

fn announce(ctx: &Ctx, cfg: &ScoreConfig) {
    if let Some(p) = &cfg.pairs {
        ctx.say(format!(
            "PARTIAL: --pairs {} — this label will not cover the full pair set.",
            p.join(",")
        ));
    }
}

// ─── the run ─────────────────────────────────────────────────────────────

/// Everything every launch of the run shares.
struct Run<'a> {
    ctx: &'a Ctx,
    cfg: &'a ScoreConfig,
    pairs: Pairs,
    results: PathBuf,
    work: PathBuf,
    endpoint: String,
    bin: LabelBin,
}

impl<'a> Run<'a> {
    /// The same run, saying into `ctx` (one lane's slot).
    fn with_ctx<'b>(&self, ctx: &'b Ctx) -> Run<'b>
    where
        'a: 'b,
    {
        Run {
            ctx,
            cfg: self.cfg,
            pairs: self.pairs.clone(),
            results: self.results.clone(),
            work: self.work.clone(),
            endpoint: self.endpoint.clone(),
            bin: self.bin.clone(),
        }
    }

    /// One launch's config entries: the base keys, the call's own, the
    /// library key, then the run's `--pipeline-set` entries.
    fn settings(&self, own: &[String]) -> Vec<String> {
        let mut all = base_settings(&self.pairs.llm.reasoning_effort, self.pairs.llm.concurrency);
        all.extend(own.iter().cloned());
        if let Some(i) = &self.cfg.library_index {
            all.push(format!("library.index={i}"));
        }
        all.extend(self.cfg.pipeline_sets.iter().cloned());
        all
    }

    fn cache_sets(&self) -> Vec<String> {
        self.cfg
            .llm_cache
            .iter()
            .map(|c| format!("llm.cache_dir={c}"))
            .collect()
    }

    /// Launch the binary with its output to `log` (rebase, self-hop),
    /// sampling its peak RSS as the scored leg does (exp207: the rebase's
    /// memory was never recorded); says one line with both.
    fn launch(&self, args: &[String], log: &Path) -> i32 {
        let mut s = self.ctx.spawn(&self.bin.path, args, leg::PIPELINE_DEADLINE);
        s.cwd = Some(self.ctx.repo.clone());
        s.env
            .push(("HUMANIFY_API_KEY".into(), self.pairs.llm.api_key.clone()));
        let t0 = Instant::now();
        let mut peak: Option<u64> = None;
        let exit = crate::proc::run_to_file(&s, log, leg::RSS_SAMPLE, &mut |pid| {
            if let Some(mb) = leg::peak_rss_mb_of_tree(&leg::proc_tree(), pid) {
                peak = Some(peak.map_or(mb, |p| p.max(mb)));
            }
        });
        let code = match exit {
            Ok(e) => e.code,
            Err(e) => {
                self.ctx.say(format!("  launch: {e}"));
                -1
            }
        };
        let rss = peak.map_or("?".to_string(), |p| format!("{p} MB"));
        self.ctx.say(format!(
            "  launch: exit {code}, {}s, peak RSS {rss} ({})",
            t0.elapsed().as_secs_f64().round(),
            text(log)
        ));
        code
    }
}

/// `eval score`: exit 0 only when every selected pair was scored, the
/// self-hop ran and passed its warm half, the summary and the identities
/// were written; refused (2) before anything builds; 1 otherwise.
pub fn run(ctx: &Ctx, cfg: &ScoreConfig, run_config: &JsValue) -> u8 {
    match run_inner(ctx, cfg, run_config) {
        Ok(code) => code,
        Err((code, msg)) => {
            ctx.say(msg);
            code
        }
    }
}

fn run_inner(ctx: &Ctx, cfg: &ScoreConfig, run_config: &JsValue) -> Result<u8, (u8, String)> {
    let results = ctx.repo.join(EVAL_RESULTS).join(&cfg.label);
    // Two live runs of one label would write one work folder (and one
    // results folder): the second is refused at once, never queued.
    let _claim = super::claim::claim_folder(
        &cfg.workdir.join(&cfg.label),
        &format!("eval score {}", cfg.label),
    )
    .map_err(|e| (2, e))?;
    guard_label(ctx, &results, cfg).map_err(|e| (2, e))?;
    announce(ctx, cfg);
    let pairs = load_pairs(&ctx.repo).map_err(|e| (2, e))?.value;
    let plans = plan_pairs(cfg, &pairs).map_err(|e| (2, e))?;
    let cfg_path = cfg.workdir.join(format!("{}.run-config.json", cfg.label));
    let cfg_text = crate::json::js_text(run_config, true);
    write(&cfg_path, &cfg_text).map_err(|e| (2, e))?;
    let cfg_sha = crate::committed::sha256_hex(cfg_text.as_bytes());
    let endpoint =
        crate::committed::local::resolve_endpoint(cfg.endpoint_flag.as_deref(), &ctx.repo)
            .map_err(|e| (2, e))?;
    boot::require_bun(ctx).map_err(|e| (1, e))?;
    let bin = build_bin(ctx, cfg)?;
    let run = Run {
        ctx,
        cfg,
        pairs,
        results,
        work: cfg.workdir.join(&cfg.label),
        endpoint,
        bin,
    };
    record_label(&run, &cfg_path, &cfg_sha).map_err(|e| (1, e))?;
    let mut failures = run_lanes(&run, &plans);
    ctx.say(format!("=== summarizing model '{}' ===", cfg.label));
    if scoring::summarize(ctx, &cfg.label) != 0 {
        failures.push("the summary: it failed (see SUMMARY FAILED above)".into());
    }
    if let Err(e) = write_identities(ctx, &run.results) {
        failures.push(format!("the identities: {e}"));
    }
    if failures.is_empty() {
        return Ok(0);
    }
    ctx.say(format!(
        "!! eval score FAILED: {} failure(s); label '{}' is NOT a complete run:",
        failures.len(),
        cfg.label
    ));
    for f in &failures {
        ctx.say(format!("  {f}"));
    }
    Ok(1)
}

fn build_bin(ctx: &Ctx, cfg: &ScoreConfig) -> Result<LabelBin, (u8, String)> {
    ctx.say(format!(
        "=== --bin: building and recording {} ===",
        text(&cfg.bin)
    ));
    let bin = super::bin::build_and_describe(ctx, &cfg.bin).map_err(|e| {
        (
            1,
            format!(
                "FATAL: could not build/record --bin {}: {e}",
                text(&cfg.bin)
            ),
        )
    })?;
    let label_commit = super::head(&ctx.repo, true);
    if let Some(r) = super::bin::bin_commit_refusal(&bin, &label_commit, cfg.force_mixed) {
        return Err((2, format!("REFUSED --bin {}: {r}", text(&cfg.bin))));
    }
    ctx.say(format!("PIPELINE: Rust binary {}", bin.path));
    ctx.say(format!(
        "  sha256 {}, built from {}{}",
        bin.sha256,
        if bin.commit.is_empty() {
            "an UNKNOWN commit".to_string()
        } else {
            short(&bin.commit)
        },
        if bin.dirty { " (DIRTY tree)" } else { "" }
    ));
    Ok(bin)
}

/// The label's record (`pipeline.json`, run.sh's `jq -c` line) and
/// `commit.txt`, with what the run is about to do said out loud.
fn record_label(run: &Run, cfg_path: &Path, cfg_sha: &str) -> Result<(), String> {
    let (ctx, cfg) = (run.ctx, run.cfg);
    if !cfg.pipeline_sets.is_empty() {
        ctx.say(format!(
            "PIPELINE SETTINGS (every launch): {}",
            cfg.pipeline_sets.join(" ")
        ));
    }
    match &cfg.library_index {
        Some(i) => ctx.say(format!(
            "NAMING MODE: {} — every launch reads the library index {i} (never a run-time build)",
            cfg.naming_mode
        )),
        None => ctx.say(format!(
            "NAMING MODE: {} — no library index",
            cfg.naming_mode
        )),
    }
    ctx.say("BASE: scratch — each v-1 rebuilt by this pipeline with NO prior (a full cold run per base)");
    let mut pipeline = vec![("kind", JsValue::str("rust-bin")), ("bin", run.bin.to_js())];
    if !cfg.pipeline_sets.is_empty() {
        pipeline.push(("pipelineSettings", JsValue::str_array(&cfg.pipeline_sets)));
    }
    let record = obj(vec![
        ("pipeline", obj(pipeline)),
        ("namingMode", JsValue::str(cfg.naming_mode.as_str())),
        (
            "libraryIndex",
            cfg.library_index
                .clone()
                .map_or(JsValue::Null, JsValue::str),
        ),
        (
            "runConfig",
            obj(vec![
                ("path", JsValue::str(text(cfg_path))),
                ("sha256", JsValue::str(cfg_sha)),
            ]),
        ),
        ("bootGateModel", JsValue::str(cfg.boot_model.as_str())),
        // exp207: how many pairs ran at once, and the llm.concurrency
        // EACH launch kept (the server saw up to their product in flight)
        ("parallelPairs", records::num(cfg.parallel_pairs as f64)),
        ("parallelRebases", records::num(cfg.parallel_rebases as f64)),
        (
            "llmConcurrencyPerRun",
            records::num(f64::from(run.pairs.llm.concurrency)),
        ),
    ]);
    std::fs::create_dir_all(&run.work).map_err(|e| format!("{}: {e}", run.work.display()))?;
    write(
        &run.results.join("pipeline.json"),
        &js_compact_line(&record),
    )?;
    match &cfg.llm_cache {
        Some(c) => ctx.say(format!("LLM CACHE: ON ({c}) -- NOT valid for a gate run")),
        None => ctx.say("LLM CACHE: OFF (cold, every prompt live) -- gate-valid"),
    }
    let commit = super::head(&ctx.repo, true);
    if commit.is_empty() {
        return Err(format!(
            "FATAL: cannot read this checkout's commit ({}): a label must record it",
            ctx.repo.display()
        ));
    }
    write(&run.results.join("commit.txt"), &format!("{commit}\n"))
}

/// The base a pair is scored against: the rebuilt prior bundle and the
/// rebase's stdout.
struct Base {
    prior: PathBuf,
    rebase_stdout: PathBuf,
}

/// The base mode every run manifest records: the only one (each v-1
/// rebuilt with no prior).
const BASE_MODE: &str = "scratch";

/// One pair's failure: said in its block, collected for the closing verdict.
type PairResult = Result<(), String>;

/// Rebuild `from` with the current pipeline and no prior. Err: the pair
/// cannot be scored (there is nothing to fall back to).
fn rebase(run: &Run, plan: &Planned) -> Result<Base, String> {
    let ctx = run.ctx;
    let from = plan.from.as_str();
    let pair = format!("{from}->{}", plan.to);
    ctx.say(format!(
        "=== {pair}: rebasing prior (re-humanify {from} from SCRATCH, current pipeline, no prior) ==="
    ));
    let out = run.work.join(format!("{from}-rebased"));
    remove(&out);
    let toml = run.work.join(format!("{from}-rebased.toml"));
    let mut own = run.cache_sets();
    // the rebase's call ledger (exp207): most of the eval's LLM calls are
    // a rebase's, so its retries / timeouts / latency are recorded too
    own.push(format!(
        "diagnostics.stats_json={}",
        text(&run.results.join(format!("{from}-rebase.stats.json")))
    ));
    own.push(format!(
        "diagnostics.log_file={}",
        text(&run.results.join(format!("{from}-rebase.log")))
    ));
    let failed = |why: String| {
        ctx.say(format!(
            "REBASE FAILED for {pair}: {why} — this pair is NOT scored"
        ));
        "rebase failed".to_string()
    };
    write_config(&toml, &run.settings(&own)).map_err(failed)?;
    let (toml_t, out_t) = (text(&toml), text(&out));
    let args = HumanifyArgs {
        input: &plan.from_input,
        config: &toml_t,
        endpoint: &run.endpoint,
        model: &run.pairs.llm.model,
        out: &out_t,
        prior: None,
        verbose: true,
    }
    .argv();
    let stdout = run.results.join(format!("{from}-rebase.stdout"));
    let code = run.launch(&args, &stdout);
    let rebuilt = out.join(".humanify/humanified.js");
    if code != 0 {
        // A base the pipeline itself marked invalid (no runnable tree, a
        // rename-invariant failure) is no base, even when a tree is on disk.
        return Err(failed(format!(
            "exit {code}, the run declared itself invalid (see {})",
            text(&stdout)
        )));
    }
    if !rebuilt.is_file() {
        return Err(failed(format!(
            "exit {code}, no {} (see {})",
            text(&rebuilt),
            text(&stdout)
        )));
    }
    ctx.say(format!("  prior rebased -> {}", text(&rebuilt)));
    Ok(Base {
        prior: rebuilt,
        rebase_stdout: stdout,
    })
}

/// One selected pair: its two inputs.
struct Planned {
    from: String,
    to: String,
    input: String,
    from_input: String,
    /// pairs.json `selfHop`.
    self_hop: bool,
}

/// What one pair will do, decided before anything launches (so the
/// self-hop's lane is known up front).
enum PairPlan {
    /// Not in `--pairs`.
    Skip(String),
    Go(Planned),
}

impl PairPlan {
    fn go(&self) -> Option<&Planned> {
        match self {
            PairPlan::Go(p) => Some(p),
            PairPlan::Skip(_) => None,
        }
    }
}

/// Every pair's plan; Err lists every input a selected pair needs that is
/// not on this host (refused before anything builds).
fn plan_pairs(cfg: &ScoreConfig, pairs: &Pairs) -> Result<Vec<PairPlan>, String> {
    let mut missing = Vec::new();
    let plans: Vec<PairPlan> = pairs
        .pairs
        .iter()
        .map(|p| {
            let (from, to) = (&p.from, &p.to);
            if !selected(&cfg.pairs, from, to) {
                return PairPlan::Skip(format!("SKIP {from}->{to} (not in --pairs)"));
            }
            let input = input_of(&cfg.inputs_base, to);
            let from_input = input_of(&cfg.inputs_base, from);
            for i in [&from_input, &input] {
                if !Path::new(i).is_file() {
                    missing.push(format!("  {from}->{to}: no input {i}"));
                }
            }
            PairPlan::Go(Planned {
                from: from.clone(),
                to: to.clone(),
                input,
                from_input,
                self_hop: p.self_hop,
            })
        })
        .collect();
    if missing.is_empty() {
        return Ok(plans);
    }
    missing.dedup();
    Err(format!(
        "REFUSED: a selected pair has no input on this host (--inputs-base {}):\n{}\n\
         Pass --inputs-base <dir>, or --pairs to score only the pairs this host has.",
        cfg.inputs_base,
        missing.join("\n")
    ))
}

/// The order lanes are started in: pairs.json's, except that with fewer
/// lanes than pairs (but more than one) the self-hop's pair goes first —
/// its chain (rebase, scored leg, cold and warm self-hop) is the critical
/// path. One lane is the serial run, in pairs.json's order.
fn work_order(n: usize, hop: Option<usize>, jobs: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..n).collect();
    if let Some(h) = hop
        && jobs > 1
        && jobs < n
    {
        order.retain(|&i| i != h);
        order.insert(0, h);
    }
    order
}

/// Every pair, `parallel_pairs` at a time, each in its own lane; the
/// self-hop in the lane of [`self_hop_index`]'s pair, right after its
/// scored leg. Each
/// lane says into its own slot of one ordered transcript (pairs in
/// pairs.json order, the self-hop after them), and writes only its own
/// pair's files: completion order reaches nothing (rule 6).
///
/// With fewer `parallel_rebases` than lanes (the default: 1 of 5), the
/// rebases — LLM-bound, one at 472 already saturates the server — are
/// GATED: they run `parallel_rebases` at a time in [`rebase_order`] on a
/// driver beside the lanes, and each lane waits for its own base (said as
/// one `base: waited` line) before its CPU-bound scored leg, which then
/// overlaps the next rebase. Otherwise each lane rebases inline.
///
/// Returns every failure, `<pair>: <reason>`, in pairs.json order.
fn run_lanes(run: &Run, plans: &[PairPlan]) -> Vec<String> {
    let (ctx, cfg) = (run.ctx, run.cfg);
    let hop = cfg.self_hop.then(|| self_hop_index(plans)).flatten();
    let n = plans.len();
    let lanes_n = cfg.parallel_pairs.min(n.max(1));
    let gated = cfg.parallel_rebases < lanes_n;
    let rebases_n = cfg.parallel_rebases.min(lanes_n);
    ctx.say(format!(
        "PARALLEL PAIRS: {lanes_n} lane(s) for {n} pair(s), {rebases_n} rebase(s) at a time; every launch at llm.concurrency {} (the server sees up to {} from rebases in flight)",
        run.pairs.llm.concurrency,
        rebases_n as u64 * u64::from(run.pairs.llm.concurrency)
    ));
    let lanes = Arc::new(Lanes::new(n + 1, ctx.echo));
    let lane = Lane {
        run,
        plans,
        hop,
        lanes: &lanes,
    };
    let mut outcomes: Vec<(usize, Vec<String>)> = if gated {
        lane.run_gated(cfg.parallel_pairs, cfg.parallel_rebases)
    } else {
        let order = work_order(n, hop, cfg.parallel_pairs);
        let results = super::each(&order, cfg.parallel_pairs, |&i| lane.run_one(i, rebase));
        order.into_iter().zip(results).collect()
    };
    ctx.absorb(lanes.close());
    outcomes.sort_by_key(|(i, _)| *i);
    outcomes.into_iter().flat_map(|(_, f)| f).collect()
}

/// The pair the self-hop runs on: the `selfHop` pair when it is
/// selected (the version the self-hop reference was measured on), else
/// the last selected pair (a `--pairs` subset without it).
fn self_hop_index(plans: &[PairPlan]) -> Option<usize> {
    plans
        .iter()
        .position(|p| p.go().is_some_and(|g| g.self_hop))
        .or_else(|| plans.iter().rposition(|p| p.go().is_some()))
}

/// The planned pairs' rebase order: the self-hop's pair first (its chain
/// — scored leg, cold and warm self-hop — is the critical path), then the
/// others in pairs.json order.
fn rebase_order(plans: &[PairPlan], hop: Option<usize>) -> Vec<usize> {
    let mut order: Vec<usize> = hop.into_iter().collect();
    order.extend((0..plans.len()).filter(|&i| Some(i) != hop && plans[i].go().is_some()));
    order
}

/// What every lane of one run shares.
struct Lane<'r, 'a> {
    run: &'r Run<'a>,
    plans: &'r [PairPlan],
    hop: Option<usize>,
    lanes: &'r Arc<Lanes>,
}

impl Lane<'_, '_> {
    /// Pair `i`'s lane: its base from `base_of`, its scored leg, then the
    /// self-hop when it is the hop's pair. Returns its failures,
    /// `<pair>: <reason>` (the pair's, then the self-hop's).
    fn run_one(
        &self,
        i: usize,
        base_of: impl FnOnce(&Run, &Planned) -> Result<Base, String>,
    ) -> Vec<String> {
        let ctx = self.run.ctx;
        let lane_ctx = ctx.for_lane(self.lanes, i);
        let run = self.run.with_ctx(&lane_ctx);
        let mut failures = Vec::new();
        let plan = match &self.plans[i] {
            PairPlan::Skip(line) => {
                lane_ctx.say(line.as_str());
                None
            }
            PairPlan::Go(plan) => Some(plan),
        };
        if let Some(plan) = plan {
            let scored = base_of(&run, plan).and_then(|base| score_on_base(&run, i, plan, &base));
            if let Err(why) = scored {
                failures.push(format!("{}->{}: {why}", plan.from, plan.to));
            }
        }
        self.lanes.finish(i);
        if self.hop == Some(i)
            && let Some(plan) = plan
        {
            let hop_ctx = ctx.for_lane(self.lanes, self.plans.len());
            if let Err(why) = self_hop(&self.run.with_ctx(&hop_ctx), &plan.to, &plan.input) {
                failures.push(format!("{} self-hop: {why}", plan.to));
            }
        }
        failures
    }

    /// The gated schedule: a driver runs the rebases `rebases` at a time
    /// in [`rebase_order`] and hands each base to its lane through that
    /// lane's bounded channel; the lanes (`jobs` at a time, in the same
    /// order, skipped pairs last) wait for theirs under a deadline.
    fn run_gated(&self, jobs: usize, rebases: usize) -> Vec<(usize, Vec<String>)> {
        let order = rebase_order(self.plans, self.hop);
        let n = self.plans.len();
        // concurrency: one bounded (1) channel per pair, carrying its ONE
        // base from the rebase driver to the lane; the lane waits with
        // recv_timeout, never a sleep-and-poll.
        let (senders, receivers): (Vec<_>, Vec<_>) = (0..n)
            .map(|_| std::sync::mpsc::sync_channel::<Result<Base, String>>(1))
            .unzip();
        // concurrency: each receiver taken once by its own lane (a short
        // lock around one Option::take).
        let receivers: Vec<_> = receivers
            .into_iter()
            .map(|r| std::sync::Mutex::new(Some(r)))
            .collect();
        let mut lane_order = order.clone();
        lane_order.extend((0..n).filter(|i| !order.contains(i)));
        // concurrency: the rebase driver beside the lanes, a scoped thread
        // joined before return; each rebase is one child through the one
        // subprocess owner (its own deadline).
        std::thread::scope(|scope| {
            scope.spawn(|| {
                super::each(&order, rebases, |&i| {
                    let lane_ctx = self.run.ctx.for_lane(self.lanes, i);
                    let run = self.run.with_ctx(&lane_ctx);
                    if let Some(plan) = self.plans[i].go() {
                        let _ = senders[i].send(rebase(&run, plan));
                    }
                });
            });
            let results = super::each(&lane_order, jobs, |&i| {
                let rx = receivers[i].lock().expect("a lane's receiver").take();
                let place = order.iter().position(|&o| o == i).unwrap_or(0);
                self.run_one(i, |run, _| wait_for_base(run, rx, place, order.len()))
            });
            lane_order.iter().copied().zip(results).collect()
        })
    }
}

/// A gated lane's wait for its base: at most one pipeline deadline per
/// rebase ahead of it and its own; says how long it waited.
fn wait_for_base(
    run: &Run,
    rx: Option<std::sync::mpsc::Receiver<Result<Base, String>>>,
    place: usize,
    of: usize,
) -> Result<Base, String> {
    let t0 = Instant::now();
    let deadline = leg::PIPELINE_DEADLINE * u32::try_from(place + 1).unwrap_or(u32::MAX);
    let got = rx
        .ok_or_else(|| "no channel".to_string())
        .and_then(|rx| rx.recv_timeout(deadline).map_err(|e| e.to_string()));
    match got {
        Ok(base) => {
            run.ctx.say(format!(
                "  base: waited {}s for its rebase ({}/{of} in rebase order)",
                t0.elapsed().as_secs_f64().round(),
                place + 1
            ));
            base
        }
        Err(e) => {
            run.ctx.say(format!(
                "  base: NEVER ARRIVED ({e}) after {}s — this pair is NOT scored",
                t0.elapsed().as_secs_f64().round()
            ));
            Err("its rebase never arrived".into())
        }
    }
}

/// Score one planned pair on its base: its scored leg.
fn score_on_base(run: &Run, i: usize, plan: &Planned, base: &Base) -> PairResult {
    let pair = format!("{}->{}", plan.from, plan.to);
    run.ctx.say(format!(
        "=== [{}/{}] {pair}: pipeline ===",
        i + 1,
        run.pairs.pairs.len()
    ));
    scored_leg(run, &pair, &plan.to, (&plan.from_input, &plan.input), base)
}

fn scored_leg(
    run: &Run,
    pair: &str,
    to: &str,
    (from_input, input): (&str, &str),
    base: &Base,
) -> PairResult {
    let (ctx, cfg) = (run.ctx, run.cfg);
    let failed = |what: &str, e: String| {
        ctx.say(format!("PIPELINE FAILED for {pair}: {e}"));
        what.to_string()
    };
    let out = run.work.join(to);
    remove(&out);
    let stats = run.results.join(format!("{to}.stats.json"));
    let toml = run.work.join(format!("{to}.toml"));
    let diag = run.work.join(format!("{to}.diag.json"));
    let mut own = run.cache_sets();
    own.push(format!("diagnostics.stats_json={}", text(&stats)));
    own.push(format!(
        "diagnostics.log_file={}",
        text(&run.results.join(format!("{to}.log")))
    ));
    own.push(format!("diagnostics.diagnostics_file={}", text(&diag)));
    let config_toml = write_config(&toml, &run.settings(&own))
        .map_err(|e| failed("its launch config could not be written", e))?;
    let (toml_t, out_t, prior_t) = (text(&toml), text(&out), text(&base.prior));
    let args = HumanifyArgs {
        input,
        config: &toml_t,
        endpoint: &run.endpoint,
        model: &run.pairs.llm.model,
        out: &out_t,
        prior: Some(&prior_t),
        verbose: true,
    }
    .argv();
    let bundle = out.join(".humanify/humanified.js");
    let ledger = out.join(".humanify/layout.json");
    let spec = LegSpec {
        pair: pair.to_string(),
        version: to.to_string(),
        label: cfg.label.clone(),
        results_dir: run.results.clone(),
        input: input.to_string(),
        prior: prior_t.clone(),
        base_mode: Some(BASE_MODE.to_string()),
        output_dir: out.clone(),
        repo: ctx.repo.clone(),
        args,
        config_toml: Some(config_toml),
        stdout_path: run.results.join(format!("{to}.stdout")),
        endpoint: run.endpoint.clone(),
        model: run.pairs.llm.model.clone(),
        reasoning_effort: run.pairs.llm.reasoning_effort.clone(),
        concurrency: run.pairs.llm.concurrency,
        cache_dir: cfg.llm_cache.clone(),
        artifacts: vec![text(&bundle), text(&ledger), text(&stats)],
        bin: run.bin.clone(),
    };
    leg::write_spec(&spec, &run.work.join(format!("{to}.runcfg.json")))
        .map_err(|e| failed("its run config could not be written", e))?;
    let exit = leg::run_leg(ctx, &spec, &run.pairs.llm.api_key, "rebased")
        .map_err(|e| failed("the scored leg did not run", e))?
        .exit_code;
    record_status(run, to, pair, exit, &stats, base)?;
    let missing: Vec<String> = spec
        .artifacts
        .iter()
        .filter(|a| !Path::new(a.as_str()).is_file())
        .cloned()
        .collect();
    if !missing.is_empty() {
        ctx.say(format!(
            "PIPELINE FAILED for {pair} — missing: {}",
            missing.join(" ")
        ));
        ctx.say(format!(
            "  (see {}; an OOM here means the box ran out of memory — run the eval alone)",
            text(&spec.stdout_path)
        ));
        return Err(format!("the scored leg wrote no {}", missing.join(", ")));
    }
    let analyzed = analyze_and_boot(run, pair, to, &out, base, (from_input, input));
    if exit == 0 {
        return analyzed;
    }
    Err(match analyzed {
        Ok(()) => format!("the pipeline exited {exit}"),
        Err(e) => format!("the pipeline exited {exit}; {e}"),
    })
}

/// Record the run status. Err when the card is REFUSED (no card is
/// written, a stale one is removed) or the status cannot be recorded.
fn record_status(
    run: &Run,
    to: &str,
    pair: &str,
    exit: i32,
    stats: &Path,
    base: &Base,
) -> PairResult {
    let ctx = run.ctx;
    let sources = StatusSources {
        stats: Some(stats),
        rebase_stdout: Some(&base.rebase_stdout),
        pair: Some(pair),
    };
    let status = records::write_run_status(&run.results, to, exit, &sources).map_err(|e| {
        ctx.say(format!("RUN STATUS FAILED for {pair}: {e}"));
        "its run status could not be recorded".to_string()
    })?;
    let mut refusal = provider_error_refusal(&status, &run.cfg.label);
    refusal.extend(library_refusal(&status, &run.cfg.label));
    if !refusal.is_empty() {
        for line in refusal {
            ctx.say(line);
        }
        remove(&run.results.join(format!("{to}.json")));
        return Err("card refused (see its block)".into());
    }
    if exit != 0 {
        ctx.say(format!(
            "PIPELINE EXITED {exit} for {pair} — the run declared ITSELF invalid:"
        ));
        let stdout =
            std::fs::read_to_string(run.results.join(format!("{to}.stdout"))).unwrap_or_default();
        for l in stdout.lines().filter(|l| l.starts_with("ERROR:")).take(5) {
            ctx.say(format!("    {l}"));
        }
        ctx.say(
            "  Scoring anyway for inspection, but the pair FAILED: summary.json is stamped and the table will say so.",
        );
    }
    Ok(())
}

fn tree_of(bundle: &Path) -> PathBuf {
    bundle
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

/// The folder holding an input file.
fn folder_of(input: &str) -> &Path {
    Path::new(input).parent().unwrap_or(Path::new(""))
}

/// Label the pair's diff, then boot the tree. Err names what failed; the
/// boot still runs after a failed analysis.
fn analyze_and_boot(
    run: &Run,
    pair: &str,
    to: &str,
    out: &Path,
    base: &Base,
    (from_input, input): (&str, &str),
) -> PairResult {
    let ctx = run.ctx;
    ctx.say(format!("=== {pair}: diff labels ==="));
    let prior_tree = tree_of(&base.prior);
    let inputs = PairInputs {
        fresh_tree: out,
        prior_tree: &prior_tree,
        prior_input: folder_of(from_input),
        fresh_input: folder_of(input),
        pair,
    };
    let mut failed: Vec<&str> = Vec::new();
    match scoring::analyze(ctx, &inputs) {
        Ok(card) => {
            if let Err(e) = write(&run.results.join(format!("{to}.json")), &card) {
                ctx.say(format!("CARD NOT WRITTEN for {pair}: {e}"));
                failed.push("its card could not be written");
            }
        }
        Err(e) => {
            ctx.say(format!("ANALYZE FAILED for {pair}: {e}"));
            remove(&run.results.join(format!("{to}.json")));
            failed.push("analysis failed");
        }
    }
    if let Err(e) = boot_pair(run, pair, to, out) {
        failed.push(e);
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(failed.join("; "))
    }
}

/// Boot the tree and record the verdict; Err when it did not boot or the
/// verdict could not be recorded.
fn boot_pair(run: &Run, pair: &str, to: &str, out: &Path) -> Result<(), &'static str> {
    let (ctx, cfg) = (run.ctx, run.cfg);
    if !out.join("run.cjs").is_file() {
        ctx.say(format!("BOOT GATE FAILED for {pair}: no run.cjs to boot"));
        return Err("boot gate failed");
    }
    let b = boot::boot(ctx, out, to, cfg.boot_prompt, &cfg.boot_model);
    let ok = b.ok();
    let line = records::boot_verdict_line(&b.version, &b.prompt, &cfg.boot_model, ok);
    if let Err(e) = write(&run.results.join(format!("{to}-boot.json")), &line) {
        ctx.say(format!("BOOT VERDICT NOT WRITTEN for {pair}: {e}"));
        return Err("its boot verdict could not be written");
    }
    if ok {
        ctx.say(format!("BOOT GATE: OK ({})", b.version));
        Ok(())
    } else {
        ctx.say(format!(
            "BOOT GATE FAILED for {pair}: version='{}' prompt='{}'",
            b.version, b.prompt
        ));
        Err("boot gate failed")
    }
}

// ─── the self-hop ────────────────────────────────────────────────────────

/// One self-hop leg: `<out>` with its own config file, the cache `cache`.
fn self_hop_leg(run: &Run, input: &str, base: &Path, out: &Path, log: &Path, cache: &Path) -> i32 {
    let toml = PathBuf::from(format!("{}.toml", text(out)));
    let own = vec![format!("llm.cache_dir={}", text(cache))];
    if let Err(e) = write_config(&toml, &run.settings(&own)) {
        run.ctx.say(e);
        return 2;
    }
    let (toml_t, out_t) = (text(&toml), text(out));
    let prior = text(&base.join(".humanify/humanified.js"));
    let args = HumanifyArgs {
        input,
        config: &toml_t,
        endpoint: &run.endpoint,
        model: &run.pairs.llm.model,
        out: &out_t,
        prior: Some(&prior),
        verbose: false,
    }
    .argv();
    run.launch(&args, log)
}

/// re-humanify the last attempted pair's TO with its own fresh output as
/// the prior. COLD into a fresh cache (judged against the reference range
/// by the summary), then WARM replaying a copy of that cache: with the
/// model held fixed it must be byte-identical with 0 cache writes. Err:
/// the check did not happen, or the warm leg failed.
fn self_hop(run: &Run, to: &str, input: &str) -> Result<(), String> {
    let ctx = run.ctx;
    let base = run.work.join(to);
    if !base.join(".humanify/humanified.js").is_file() {
        ctx.say(format!(
            "SELF-HOP DID NOT RUN for {to}: its scored leg wrote no tree"
        ));
        return Err("did not run (no scored tree)".into());
    }
    let out = run.work.join(format!("{to}-selfhop"));
    ctx.say(format!(
        "=== self-hop invariant: {to} vs its own output ==="
    ));
    remove(&out);
    let cache = run.work.join(format!("{to}-selfhop-cache"));
    remove(&cache);
    let cache_failed = |e: String| {
        ctx.say(format!("SELF-HOP DID NOT RUN for {to}: its cache: {e}"));
        "did not run (its cache could not be made)".to_string()
    };
    let cold_cache = match &run.cfg.llm_cache {
        Some(c) => {
            super::copy_tree(Path::new(c), &cache).map_err(cache_failed)?;
            ctx.say("  cold self-hop leg SEEDED from --llm-cache: it is NOT cold; its count will not be judged");
            "seeded"
        }
        None => {
            std::fs::create_dir_all(&cache)
                .map_err(|e| cache_failed(format!("{}: {e}", text(&cache))))?;
            "fresh"
        }
    };
    let before = count_files(&cache);
    let rc = self_hop_leg(
        run,
        input,
        &base,
        &out,
        &run.results.join(format!("{to}-selfhop.stdout")),
        &cache,
    );
    let cold_writes = count_files(&cache) as i64 - before as i64;
    let missing: Vec<String> = [".humanify/humanified.js", ".humanify/layout.json"]
        .iter()
        .map(|a| out.join(a))
        .filter(|p| !p.is_file())
        .map(|p| text(&p))
        .collect();
    let verdict_path = run.results.join(format!("{to}-self-hop.json"));
    if !missing.is_empty() {
        let v = obj(vec![(
            "selfHop",
            obj(vec![
                ("version", JsValue::str(to)),
                ("ran", JsValue::Bool(false)),
                ("identical", JsValue::Null),
                ("diffLines", JsValue::Null),
            ]),
        )]);
        write(&verdict_path, &js_compact_line(&v))?;
        ctx.say(format!(
            "SELF-HOP DID NOT RUN for {to} — missing: {}",
            missing.join(" ")
        ));
        ctx.say("  This is NOT a passing invariant and NOT a violation; the check did not happen.");
        return Err("did not run (the cold leg wrote no tree)".into());
    }
    let (identical, diff) = cold_verdict(run, to, &base, &out, cold_cache);
    let (warm, warm_ok) = warm_self_hop(run, to, input, &base, &out, &cache, rc, cold_writes)?;
    let v = obj(vec![(
        "selfHop",
        obj(vec![
            ("version", JsValue::str(to)),
            ("ran", JsValue::Bool(true)),
            ("identical", JsValue::Bool(identical)),
            ("diffLines", records::num(diff as f64)),
            ("coldCache", JsValue::str(cold_cache)),
            ("warm", warm),
        ]),
    )]);
    write(&verdict_path, &js_compact_line(&v))?;
    if warm_ok {
        Ok(())
    } else {
        Err("warm self-hop failed (see its block)".into())
    }
}

fn cold_verdict(run: &Run, to: &str, base: &Path, out: &Path, cold_cache: &str) -> (bool, u64) {
    let ctx = run.ctx;
    let (a, b) = (
        base.join(".humanify/humanified.js"),
        out.join(".humanify/humanified.js"),
    );
    let mut identical = true;
    let mut diff = 0;
    if !super::tree_diff::same_bytes(&a, &b) {
        identical = false;
        diff = super::tree_diff::changed_lines(&a, &b).unwrap_or(0);
    }
    if !crate::layoutcheck::same_split_memory(base, out) {
        identical = false;
    }
    if identical {
        ctx.say("SELF-HOP INVARIANT: OK — byte-identical bundle, same split memory");
    } else {
        ctx.say(format!(
            "COLD SELF-HOP: {diff} bundle diff lines (cache: {cold_cache}; see {})",
            text(&run.results.join(format!("{to}-selfhop.stdout")))
        ));
        ctx.say("  judged against self-hop-reference.json in the summary");
    }
    (identical, diff)
}

#[allow(clippy::too_many_arguments)]
fn warm_self_hop(
    run: &Run,
    to: &str,
    input: &str,
    base: &Path,
    cold_out: &Path,
    cold_cache: &Path,
    cold_rc: i32,
    cold_writes: i64,
) -> Result<(JsValue, bool), String> {
    let ctx = run.ctx;
    let cache = run.work.join(format!("{to}-selfhop-warm-cache"));
    let out = run.work.join(format!("{to}-selfhop-warm"));
    ctx.say("=== warm self-hop: replaying a copy of the cold leg's cache ===");
    remove(&cache);
    remove(&out);
    super::copy_tree(cold_cache, &cache).map_err(|e| {
        ctx.say(format!(
            "!! WARM SELF-HOP DID NOT RUN for {to}: copying the cold cache: {e}"
        ));
        "warm self-hop did not run (the cold cache could not be copied)".to_string()
    })?;
    let before = count_files(&cache);
    let rc = self_hop_leg(
        run,
        input,
        base,
        &out,
        &run.results.join(format!("{to}-selfhop-warm.stdout")),
        &cache,
    );
    let writes = count_files(&cache) as i64 - before as i64;
    let n = |x: f64| records::num(x);
    if !out.join(".humanify/humanified.js").is_file() {
        ctx.say(format!("!! WARM SELF-HOP DID NOT RUN for {to}"));
        let v = obj(vec![
            ("ran", JsValue::Bool(false)),
            ("identical", JsValue::Bool(false)),
            ("diffFiles", n(-1.0)),
            ("diffLines", n(-1.0)),
            ("cacheWrites", n(-1.0)),
            ("coldExit", n(f64::from(cold_rc))),
            ("warmExit", n(f64::from(rc))),
            ("ok", JsValue::Bool(false)),
        ]);
        return Ok((v, false));
    }
    let files = super::tree_diff::differing_files(cold_out, &out).map_or(0, |f| f.len() as u64);
    let lines = super::tree_diff::changed_lines_in_trees(cold_out, &out).unwrap_or(0);
    let identical = files == 0;
    let ok = identical && writes == 0 && rc == cold_rc;
    if ok {
        ctx.say(format!("WARM SELF-HOP: OK — byte-identical tree, 0 cache writes, exit {rc} (cold leg filled {cold_writes} entries)"));
    } else {
        ctx.say(format!("!! WARM SELF-HOP FAILED for {to}: {files} file(s) / {lines} ln differ, {writes} cache write(s), exit {cold_rc} vs {rc}"));
        ctx.say("   (must be 0 / 0 / 0 / equal — nondeterminism with the model held fixed)");
    }
    let v = obj(vec![
        ("ran", JsValue::Bool(true)),
        ("identical", JsValue::Bool(identical)),
        ("diffFiles", n(files as f64)),
        ("diffLines", n(lines as f64)),
        ("cacheWrites", n(writes as f64)),
        ("coldExit", n(f64::from(cold_rc))),
        ("warmExit", n(f64::from(rc))),
        ("ok", JsValue::Bool(ok)),
    ]);
    Ok((v, ok))
}

/// One `<v>.identity.json` beside every card (docs/dev-harness.md §3.1),
/// read off what the label recorded; Err when one cannot be derived or
/// written, or the label's cards disagree.
fn write_identities(ctx: &Ctx, results: &Path) -> Result<(), String> {
    let cards = crate::identity::derive::eval_label(results)?;
    for card in &cards {
        let v = card.corpus.entry.rsplit("->").next().unwrap_or("");
        write(&results.join(format!("{v}.identity.json")), &card.to_text())?;
    }
    crate::identity::label_cards(&cards).map_err(|errors| {
        for e in &errors {
            ctx.say(format!("!! LABEL IDENTITY: {e}"));
        }
        "the label's cards disagree (see LABEL IDENTITY above)".to_string()
    })
}
