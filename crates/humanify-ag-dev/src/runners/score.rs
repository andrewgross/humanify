//! `eval score <label>`: score the CURRENT pipeline on the eval's version
//! pairs (the port of `scripts/eval.ts score` + `034-eval-harness/run.sh`).
//!
//! Per pair: rebuild the base v-1 (the BASE MODE: `scratch`, the default,
//! rebuilds it with no prior; `seeded` with the archive as its prior;
//! `archive` does not rebuild), run the scored leg (`leg::run_leg`: the
//! run manifest), record the run status (and REFUSE the card when a run
//! lost LLM answers or skipped library matching), score the card
//! (`scoring`), write the evidence page, boot the tree. Then the self-hop
//! on the last attempted pair — COLD into a fresh cache, then WARM
//! replaying a copy of it, which must be byte-identical with 0 cache
//! writes — and the summary.
//!
//! The binary runs at ALL THREE launch sites (rebase, scored leg,
//! self-hop); the harness builds it first and refuses one not built from
//! the label's commit (`bin`). Every launch passes its settings in a
//! `--config` file (`launch`); run-launch.golden.txt and
//! run-launch.seeded-base.golden.txt hold every launch byte for byte.
//!
//! A failed pair is logged and skipped, never aborts the sweep. No LLM
//! cache by default: a cached run is never a verdict about LLM-dependent
//! behaviour (rule 10).

use std::path::{Path, PathBuf};
use std::time::Duration;

use humanify_ag_model::js::JsValue;

use super::boot;
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
    pub base_mode: String,
    pub pairs: Option<Vec<String>>,
    pub endpoint_flag: Option<String>,
    pub llm_cache: Option<String>,
    pub boot_prompt: bool,
    pub self_hop: bool,
    pub inputs_base: String,
    pub priors_base: String,
    pub workdir: PathBuf,
    pub bin: PathBuf,
    pub force_mixed: bool,
    pub pipeline_sets: Vec<String>,
    pub naming_mode: String,
    pub library_index: Option<String>,
    pub boot_model: String,
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
        base_mode: s.text("eval.base_mode").unwrap_or_else(|| "scratch".into()),
        pairs: s.list("eval.pairs").filter(|p| !p.is_empty()),
        endpoint_flag: s.text("llm.endpoint"),
        llm_cache: s.text("eval.llm_cache"),
        boot_prompt: s.flag_on("eval.boot_prompt"),
        self_hop: s.flag_on("eval.self_hop"),
        inputs_base: s
            .text("eval.inputs_base")
            .unwrap_or_else(|| pairs.inputs_base.clone()),
        priors_base: s
            .text("eval.priors_base")
            .unwrap_or_else(|| pairs.priors_base.clone()),
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
/// another commit (eval.ts `guardLabel`), another pipeline (`guardPipeline`),
/// another base mode (`guardBaseMode`), and — new, the gap the design
/// names — another naming mode or library index. `--force-mixed` turns
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
    let kind = get(&["pipeline", "kind"]).unwrap_or_else(|| "ts".into());
    if kind != "rust-bin" {
        mixed.push(("pipelines".into(), kind, "rust-bin".into()));
    }
    let checks = [
        (
            "base modes",
            get(&["baseMode"]).or_else(|| recorded_base_modes(dir)),
            cfg.base_mode.clone(),
        ),
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

/// A label from before `pipeline.json` recorded its base mode: the modes
/// its run manifests record (`labelBaseModes`: a rebased prior then WAS
/// seeded); None when no manifest records one.
fn recorded_base_modes(dir: &Path) -> Option<String> {
    let mut modes: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with("-run.json"))
        .filter_map(|e| JsValue::parse(&std::fs::read_to_string(e.path()).ok()?).ok())
        .map(|m| records::base_mode_of(&m).to_string())
        .filter(|m| m != "unknown")
        .collect();
    modes.sort();
    modes.dedup();
    (!modes.is_empty()).then(|| modes.join(" + "))
}

fn announce(ctx: &Ctx, cfg: &ScoreConfig) {
    if let Some(p) = &cfg.pairs {
        ctx.say(format!(
            "PARTIAL: --pairs {} — this label will not cover the full pair set.",
            p.join(",")
        ));
    }
    ctx.say(match cfg.base_mode.as_str() {
        "archive" => "ARCHIVE-PRIOR MODE: scoring against archive bases — KPIs read ~3.7x worse than rebuilt bases; not comparable to the standing reference.",
        "seeded" => "SEEDED-BASE MODE: each base rebuilt with the archive as its prior — it inherits the archive's names. Comparable only to seeded labels (every reference scored before 2026-10-03).",
        _ => "SCRATCH BASES (default): each base rebuilt with no prior — a full cold run per base. NOT comparable to the seeded references (main-2026-09-18 and every label before 2026-10-03); the leaderboard refuses the mix.",
    });
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

impl Run<'_> {
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

    /// Launch the binary with its output to `log` (rebase, self-hop).
    fn launch(&self, args: &[String], log: &Path) -> i32 {
        let mut s = self.ctx.spawn(&self.bin.path, args, leg::PIPELINE_DEADLINE);
        s.cwd = Some(self.ctx.repo.clone());
        s.env
            .push(("HUMANIFY_API_KEY".into(), self.pairs.llm.api_key.clone()));
        match crate::proc::run_to_file(&s, log, Duration::from_secs(3600), &mut |_| {}) {
            Ok(e) => e.code,
            Err(e) => {
                self.ctx.say(format!("  launch: {e}"));
                -1
            }
        }
    }
}

/// `eval score`: exit 0 unless refused (2), the build failed (1), or the
/// summary failed.
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
    guard_label(ctx, &results, cfg).map_err(|e| (2, e))?;
    announce(ctx, cfg);
    let pairs = load_pairs(&ctx.repo).map_err(|e| (2, e))?.value;
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
    let mut last: Option<(String, String)> = None;
    let pair_list = run.pairs.pairs.clone();
    for (i, p) in pair_list.iter().enumerate() {
        if let Some(attempted) = score_pair(&run, i, &p.from, &p.to) {
            last = Some(attempted);
        }
    }
    if cfg.self_hop
        && let Some((to, input)) = &last
    {
        self_hop(&run, to, input);
    }
    ctx.say(format!("=== summarizing model '{}' ===", cfg.label));
    let code = scoring::summarize(ctx, &cfg.label);
    write_identities(ctx, &run.results);
    Ok(u8::try_from(code).unwrap_or(1))
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
    ctx.say(match cfg.base_mode.as_str() {
        "seeded" => "BASE: seeded (--base-mode seeded) — each v-1 rebuilt with the ARCHIVE as --prior-version; it inherits the archive's names. Comparable only to seeded labels.",
        "archive" => "BASE: archive (--base-mode archive) — no rebuild; scored against the archive tree (~3.7x worse)",
        _ => "BASE: scratch — each v-1 rebuilt by this pipeline with NO prior (a full cold run per base)",
    });
    let mut pipeline = vec![("kind", JsValue::str("rust-bin")), ("bin", run.bin.to_js())];
    if !cfg.pipeline_sets.is_empty() {
        pipeline.push(("pipelineSettings", JsValue::str_array(&cfg.pipeline_sets)));
    }
    let record = obj(vec![
        ("pipeline", obj(pipeline)),
        ("baseMode", JsValue::str(cfg.base_mode.as_str())),
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
    if !commit.is_empty() {
        write(&run.results.join("commit.txt"), &format!("{commit}\n"))?;
    }
    Ok(())
}

/// The base a pair is scored against: (prior bundle, prior ledger, the
/// base mode it actually got, the rebuilt base's stdout when rebuilt).
struct Base {
    prior: PathBuf,
    mode: String,
    rebase_stdout: Option<PathBuf>,
}

fn rebase(run: &Run, from: &str, to: &str, archive: &Path) -> Base {
    let (ctx, cfg) = (run.ctx, run.cfg);
    let mut base = Base {
        prior: archive.to_path_buf(),
        mode: "archive".into(),
        rebase_stdout: None,
    };
    if cfg.base_mode == "archive" {
        return base;
    }
    let input_from = input_of(&cfg.inputs_base, from);
    if !Path::new(&input_from).is_file() {
        ctx.say(format!("  no input for {from}; scoring against the archive prior (recorded as baseMode archive)"));
        return base;
    }
    let pair = format!("{from}->{to}");
    let seeded = cfg.base_mode == "seeded";
    ctx.say(if seeded {
        format!("=== {pair}: rebasing prior (re-humanify {from}, current pipeline, SEEDED by the archive) ===")
    } else {
        format!("=== {pair}: rebasing prior (re-humanify {from} from SCRATCH, current pipeline, no prior) ===")
    });
    let out = run.work.join(format!("{from}-rebased"));
    remove(&out);
    let toml = run.work.join(format!("{from}-rebased.toml"));
    let mut own = run.cache_sets();
    own.push(format!(
        "diagnostics.log_file={}",
        text(&run.results.join(format!("{from}-rebase.log")))
    ));
    if let Err(e) = write_config(&toml, &run.settings(&own)) {
        ctx.say(e);
        return base;
    }
    let archive_text = text(archive);
    let (toml_t, out_t) = (text(&toml), text(&out));
    let args = HumanifyArgs {
        input: &input_from,
        config: &toml_t,
        endpoint: &run.endpoint,
        model: &run.pairs.llm.model,
        out: &out_t,
        prior: seeded.then_some(archive_text.as_str()),
        verbose: true,
    }
    .argv();
    let stdout = run.results.join(format!("{from}-rebase.stdout"));
    run.launch(&args, &stdout);
    let rebuilt = out.join(".humanify/humanified.js");
    if rebuilt.is_file() {
        ctx.say(format!(
            "  prior rebased ({}) -> {}",
            cfg.base_mode,
            text(&rebuilt)
        ));
        base = Base {
            prior: rebuilt,
            mode: cfg.base_mode.clone(),
            rebase_stdout: Some(stdout),
        };
    } else {
        ctx.say("  rebase FAILED; falling back to archive prior (recorded as baseMode archive)");
    }
    base
}

/// Score one pair; Some((to, input)) when it was attempted (the self-hop
/// re-runs the LAST attempted pair — never pairs.json's last TO when
/// `--pairs` skipped it).
fn score_pair(run: &Run, i: usize, from: &str, to: &str) -> Option<(String, String)> {
    let (ctx, cfg) = (run.ctx, run.cfg);
    let pair = format!("{from}->{to}");
    if !selected(&cfg.pairs, from, to) {
        ctx.say(format!("SKIP {pair} (not in --pairs)"));
        return None;
    }
    let input = input_of(&cfg.inputs_base, to);
    let archive = PathBuf::from(format!(
        "{}/claude-code-{from}/.humanify/humanified.js",
        cfg.priors_base
    ));
    if !Path::new(&input).is_file() {
        ctx.say(format!("SKIP {pair} (no input {input})"));
        return None;
    }
    if !archive.is_file() {
        ctx.say(format!("SKIP {pair} (no prior {})", text(&archive)));
        return None;
    }
    let base = rebase(run, from, to, &archive);
    ctx.say(format!(
        "=== [{}/{}] {pair}: pipeline ===",
        i + 1,
        run.pairs.pairs.len()
    ));
    scored_leg(run, &pair, to, &input, &base);
    Some((to.to_string(), input))
}

fn scored_leg(run: &Run, pair: &str, to: &str, input: &str, base: &Base) {
    let (ctx, cfg) = (run.ctx, run.cfg);
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
    let config_toml = match write_config(&toml, &run.settings(&own)) {
        Ok(t) => t,
        Err(e) => {
            ctx.say(e);
            return;
        }
    };
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
    let ledger = out.join(".humanify/split-ledger.json");
    let spec = LegSpec {
        pair: pair.to_string(),
        version: to.to_string(),
        label: cfg.label.clone(),
        results_dir: run.results.clone(),
        input: input.to_string(),
        prior: prior_t.clone(),
        base_mode: Some(base.mode.clone()),
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
    if let Err(e) = leg::write_spec(&spec, &run.work.join(format!("{to}.runcfg.json"))) {
        ctx.say(e);
        return;
    }
    let exit = match leg::run_leg(ctx, &spec, &run.pairs.llm.api_key) {
        Ok(o) => o.exit_code,
        Err(e) => {
            ctx.say(format!("  run: {e}"));
            -1
        }
    };
    if !record_status(run, to, pair, exit, &stats, base) {
        return;
    }
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
        return;
    }
    analyze_and_boot(run, pair, to, &out, base, &diag, &stats);
}

/// Record the run status; false when the card is REFUSED (no card is
/// written, a stale one is removed).
fn record_status(run: &Run, to: &str, pair: &str, exit: i32, stats: &Path, base: &Base) -> bool {
    let ctx = run.ctx;
    let sources = StatusSources {
        stats: Some(stats),
        rebase_stdout: base.rebase_stdout.as_deref(),
        pair: Some(pair),
    };
    match records::write_run_status(&run.results, to, exit, &sources) {
        Ok(status) => {
            let mut refusal = provider_error_refusal(&status, &run.cfg.label);
            refusal.extend(library_refusal(&status, &run.cfg.label));
            if !refusal.is_empty() {
                for line in refusal {
                    ctx.say(line);
                }
                remove(&run.results.join(format!("{to}.json")));
                return false;
            }
        }
        Err(e) => ctx.say(format!(
            "  (warning: could not record run status for {pair}: {e})"
        )),
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
        ctx.say("  Scoring anyway, but summary.json is stamped and the table will say so.");
    }
    true
}

fn tree_of(bundle: &Path) -> PathBuf {
    bundle
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_default()
}

fn analyze_and_boot(
    run: &Run,
    pair: &str,
    to: &str,
    out: &Path,
    base: &Base,
    diag: &Path,
    stats: &Path,
) {
    let ctx = run.ctx;
    ctx.say(format!("=== {pair}: churn analysis ==="));
    let prior_tree = tree_of(&base.prior);
    let inputs = PairInputs {
        fresh_tree: out,
        prior_tree: &prior_tree,
        stats,
        pair,
    };
    let card = match scoring::analyze(ctx, &inputs) {
        Ok(c) => c,
        Err(e) => {
            ctx.say(format!("ANALYZE FAILED for {pair}: {e}"));
            String::new()
        }
    };
    if let Err(e) = write(&run.results.join(format!("{to}.json")), &card) {
        ctx.say(e);
    }
    if !diag.is_file() {
        ctx.say(format!(
            "NO DIAGNOSTICS TRAIL for {pair} at {} — report page NOT written.",
            text(diag)
        ));
        ctx.say(format!(
            "  See {}. Every KPI above is unaffected.",
            text(&run.results.join(format!("{to}.stdout")))
        ));
    } else if scoring::trail_report(
        ctx,
        diag,
        &run.results.join(format!("{to}-report.html")),
        &inputs,
    )
    .is_err()
    {
        ctx.say(format!("REPORT PAGE FAILED for {pair}"));
    }
    boot_pair(run, pair, to, out);
}

fn boot_pair(run: &Run, pair: &str, to: &str, out: &Path) {
    let (ctx, cfg) = (run.ctx, run.cfg);
    if !out.join("run.cjs").is_file() {
        ctx.say(format!("BOOT GATE FAILED for {pair}: no run.cjs to boot"));
        return;
    }
    let b = boot::boot(ctx, out, to, cfg.boot_prompt, &cfg.boot_model);
    let ok = b.ok();
    let line = records::boot_verdict_line(&b.version, &b.prompt, &cfg.boot_model, ok);
    if let Err(e) = write(&run.results.join(format!("{to}-boot.json")), &line) {
        ctx.say(e);
    }
    if ok {
        ctx.say(format!("BOOT GATE: OK ({})", b.version));
    } else {
        ctx.say(format!(
            "BOOT GATE FAILED for {pair}: version='{}' prompt='{}'",
            b.version, b.prompt
        ));
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
/// model held fixed it must be byte-identical with 0 cache writes.
fn self_hop(run: &Run, to: &str, input: &str) {
    let ctx = run.ctx;
    let base = run.work.join(to);
    if !base.join(".humanify/humanified.js").is_file() {
        return;
    }
    let out = run.work.join(format!("{to}-selfhop"));
    ctx.say(format!(
        "=== self-hop invariant: {to} vs its own output ==="
    ));
    remove(&out);
    let cache = run.work.join(format!("{to}-selfhop-cache"));
    remove(&cache);
    let cold_cache = match &run.cfg.llm_cache {
        Some(c) => {
            let _ = super::copy_tree(Path::new(c), &cache);
            ctx.say("  cold self-hop leg SEEDED from --llm-cache: it is NOT cold; its count will not be judged");
            "seeded"
        }
        None => {
            let _ = std::fs::create_dir_all(&cache);
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
    let missing: Vec<String> = [".humanify/humanified.js", ".humanify/split-ledger.json"]
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
        let _ = write(&verdict_path, &js_compact_line(&v));
        ctx.say(format!(
            "SELF-HOP DID NOT RUN for {to} — missing: {}",
            missing.join(" ")
        ));
        ctx.say("  This is NOT a passing invariant and NOT a violation; the check did not happen.");
        return;
    }
    let (identical, diff) = cold_verdict(run, to, &base, &out, cold_cache);
    let warm = warm_self_hop(run, to, input, &base, &out, &cache, rc, cold_writes);
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
    let _ = write(&verdict_path, &js_compact_line(&v));
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
    let ledger = ".humanify/split-ledger.json";
    if !super::tree_diff::same_bytes(&base.join(ledger), &out.join(ledger)) {
        identical = false;
    }
    if identical {
        ctx.say("SELF-HOP INVARIANT: OK — byte-identical bundle and ledger");
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
) -> JsValue {
    let ctx = run.ctx;
    let cache = run.work.join(format!("{to}-selfhop-warm-cache"));
    let out = run.work.join(format!("{to}-selfhop-warm"));
    ctx.say("=== warm self-hop: replaying a copy of the cold leg's cache ===");
    remove(&cache);
    remove(&out);
    let _ = super::copy_tree(cold_cache, &cache);
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
        return obj(vec![
            ("ran", JsValue::Bool(false)),
            ("identical", JsValue::Bool(false)),
            ("diffFiles", n(-1.0)),
            ("diffLines", n(-1.0)),
            ("cacheWrites", n(-1.0)),
            ("coldExit", n(f64::from(cold_rc))),
            ("warmExit", n(f64::from(rc))),
            ("ok", JsValue::Bool(false)),
        ]);
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
    obj(vec![
        ("ran", JsValue::Bool(true)),
        ("identical", JsValue::Bool(identical)),
        ("diffFiles", n(files as f64)),
        ("diffLines", n(lines as f64)),
        ("cacheWrites", n(writes as f64)),
        ("coldExit", n(f64::from(cold_rc))),
        ("warmExit", n(f64::from(rc))),
        ("ok", JsValue::Bool(ok)),
    ])
}

/// One `<v>.identity.json` beside every card (docs/dev-harness.md §3.1),
/// read off what the label recorded by the one backfill reader; a label
/// whose cards disagree is said out loud.
fn write_identities(ctx: &Ctx, results: &Path) {
    let cards = match crate::identity::backfill::eval_label(results) {
        Ok(c) => c,
        Err(e) => {
            ctx.say(format!("  (identity: {e})"));
            return;
        }
    };
    for card in &cards {
        let v = card
            .corpus
            .entry
            .rsplit("->")
            .next()
            .unwrap_or("")
            .to_string();
        if !v.is_empty() {
            let _ = write(&results.join(format!("{v}.identity.json")), &card.to_text());
        }
    }
    if let Err(errors) = crate::identity::label_cards(&cards) {
        for e in errors {
            ctx.say(format!("!! LABEL IDENTITY: {e}"));
        }
    }
}
