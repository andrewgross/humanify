//! `eval walk <from>..<to> --walk-dir D`: a real VERSION WALK of the
//! pipeline, the way releases would actually be processed — the first
//! version cold with no prior, then every hop humanifies version N with
//! version N-1's OWN output as `--prior-version` (the port of
//! `experiments/walk/walk.sh`; its dead `--pipeline ts` mode is gone with
//! the TS pipeline). Then the report (`report`).
//!
//! Layout of the walk dir:
//! - `manifest.json`: the binary (sha256, commit), versions, endpoint /
//!   model, naming mode and library index — written once, checked on
//!   `--resume` (a resumed walk with another binary, model, inputs, naming
//!   mode or index is refused);
//! - `bin/humanify`: a FROZEN COPY of the binary every hop runs, so a
//!   rebuild mid-walk cannot mix commits; `bin/record.json` its record;
//! - `trees/<v>/`: the hop's output tree;
//! - `runs/<v>.hop.json`: THE per-hop record (exit, wall, peak RSS, boot,
//!   prior); `runs/<v>-run.json` the run manifest; `runs/<v>.stats.json`,
//!   `.stdout`, `.log`, `-boot.json`;
//! - `cards/<a>__<b>.json`: the scorecard of hop a->b (the report's).
//!
//! Rules it obeys: cold — no LLM cache, ever (rule 10); the model comes
//! from the eval's pairs.json and the endpoint from the same resolver the
//! eval asks; names mode reads the eval's prebuilt index (never a run-time
//! build); the boot gate pins its model. Each hop's PIPELINE run holds
//! `/work/heavy.lock` (the boot gate and the scoring do not) — unless the
//! caller holds it (`HELD_LOCK`). A boot failure is recorded, never fatal;
//! a hop that exits non-zero but wrote its tree is recorded and the walk
//! continues on that tree; only a hop that leaves NO tree aborts (the next
//! hop has no prior).
//!
//! Fixed while porting (exp198): every walk hop's run manifest called its
//! prior the ARCHIVE (`priorKindOf` names every non-`-rebased` prior so)
//! and warned "scored against the ARCHIVE prior"; a walk prior is the
//! walk's previous hop, recorded as `walk-previous-hop`, and nothing warns.

pub mod history;
pub mod hunks;
pub mod report;

use std::path::{Path, PathBuf};

use humanify_ag_model::js::JsValue;

use super::leg::{self, LabelBin, LegSpec};
use super::records::{js_pretty, num, obj};
use super::{Ctx, input_of, remove, text, write};
use crate::committed::eval::Pairs;
use crate::launch::{HumanifyArgs, base_settings, write_config};
use crate::settings::Parsed;

/// The prior kind a walk hop's run manifest records.
pub const WALK_PRIOR_KIND: &str = "walk-previous-hop";

/// One walk's settings, resolved once.
#[derive(Clone, Debug)]
pub struct WalkConfig {
    pub versions_arg: String,
    pub dir: PathBuf,
    pub bin: PathBuf,
    pub inputs_base: String,
    pub endpoint_flag: Option<String>,
    pub pipeline_sets: Vec<String>,
    pub naming_mode: String,
    pub library_index: Option<String>,
    pub resume: bool,
    pub reboot: bool,
    pub boot_prompt: bool,
    pub force_mixed: bool,
    pub report: bool,
    pub boot_model: String,
    pub heavy_lock: PathBuf,
}

/// The PIPELINE keys a walk hop's file carries itself.
pub const WALK_HARNESS_KEYS: &[&str] = &[
    "llm.reasoning_effort",
    "llm.concurrency",
    "split.enabled",
    "llm.cache_dir",
    "diagnostics.log_file",
    "diagnostics.stats_json",
    "library.index",
];

pub fn config_from(parsed: &Parsed, pairs: &Pairs, repo: &Path) -> Result<WalkConfig, String> {
    let s = &parsed.settings;
    let [versions] = parsed.positionals.as_slice() else {
        return Err("usage: eval walk <from>..<to>|<v>,<v>,... --walk-dir D [flags]".into());
    };
    let dir = s
        .text("walk.dir")
        .ok_or("eval walk: --walk-dir <dir> is required (where the trees, runs and cards go)")?;
    let sets = s.list("run.pipeline_set").unwrap_or_default();
    crate::settings::check_pipeline_sets(&sets, WALK_HARNESS_KEYS).map_err(|e| e.join("\n"))?;
    let (naming_mode, library_index) = super::score::naming_mode_of(
        &sets,
        s.text("eval.library_index").map(PathBuf::from).as_deref(),
        &pairs.library_index,
    )?;
    if let Some(i) = &library_index
        && !Path::new(i).is_dir()
    {
        return Err(format!(
            "eval walk: library index '{i}' is not a directory (names mode needs one; --pipeline-set library.matching=off for off mode)"
        ));
    }
    Ok(WalkConfig {
        versions_arg: versions.clone(),
        dir: PathBuf::from(dir),
        bin: PathBuf::from(
            s.text("run.bin")
                .unwrap_or_else(|| text(&repo.join("target/release/humanify-ag"))),
        ),
        inputs_base: s
            .text("eval.inputs_base")
            .unwrap_or_else(|| pairs.inputs_base.clone()),
        endpoint_flag: s.text("llm.endpoint"),
        pipeline_sets: sets,
        naming_mode,
        library_index,
        resume: s.flag_on("walk.resume"),
        reboot: s.flag_on("walk.reboot"),
        boot_prompt: s.flag_on("eval.boot_prompt"),
        force_mixed: s.flag_on("run.force_mixed"),
        report: s.flag_on("walk.report"),
        boot_model: s
            .text("boot.model")
            .unwrap_or_else(|| "claude-opus-4-1".into()),
        heavy_lock: PathBuf::from("/work/heavy.lock"),
    })
}

/// Semver order (`sort -V` on dotted numbers).
pub fn sort_versions(v: &mut [String]) {
    let key = |s: &String| -> Vec<u64> { s.split('.').map(|p| p.parse().unwrap_or(0)).collect() };
    v.sort_by_key(key);
}

/// The versions `arg` selects: `<from>..<to>` = every version in the
/// inputs between the two (inclusive, semver order) that has an input
/// bundle; a comma list = exactly those (each must have one).
pub fn select_versions(arg: &str, inputs: &str) -> Result<Vec<String>, String> {
    if let Some((from, to)) = arg.split_once("..") {
        let mut all: Vec<String> = std::fs::read_dir(inputs)
            .map_err(|e| format!("{inputs}: {e}"))?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| n.strip_prefix("claude-code-").map(str::to_string))
            .collect();
        all.push(from.to_string());
        all.push(to.to_string());
        sort_versions(&mut all);
        let lo = all.iter().position(|v| v == from).unwrap_or(0);
        let hi = all.iter().rposition(|v| v == to).unwrap_or(0);
        let mut out: Vec<String> = all[lo..=hi.max(lo)]
            .iter()
            .filter(|v| Path::new(&input_of(inputs, v)).is_file())
            .cloned()
            .collect();
        out.dedup();
        if out.is_empty() {
            return Err(format!(
                "eval walk: no versions selected by '{arg}' in {inputs}"
            ));
        }
        return Ok(out);
    }
    let list: Vec<String> = arg
        .split(',')
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect();
    for v in &list {
        if !Path::new(&input_of(inputs, v)).is_file() {
            return Err(format!(
                "eval walk: no input for {v}: {}",
                input_of(inputs, v)
            ));
        }
    }
    if list.is_empty() {
        return Err(format!("eval walk: no versions selected by '{arg}'"));
    }
    Ok(list)
}

struct Walk<'a> {
    ctx: &'a Ctx,
    cfg: &'a WalkConfig,
    pairs: &'a Pairs,
    endpoint: String,
    frozen: PathBuf,
    bin: LabelBin,
}

/// Freeze the binary for the whole walk (or reuse the frozen copy on
/// `--resume`); its record must match its bytes.
fn freeze(ctx: &Ctx, cfg: &WalkConfig) -> Result<(PathBuf, JsValue), String> {
    let frozen = cfg.dir.join("bin/humanify");
    let record_path = cfg.dir.join("bin/record.json");
    if cfg.resume && frozen.is_file() {
        ctx.say(format!(
            "walk: resuming with the frozen binary {}",
            text(&frozen)
        ));
    } else {
        let rec = super::bin::build_and_describe(ctx, &cfg.bin)?;
        // the bin's own checkout is the label commit: only an unknown build
        // or a dirty tree is refused
        if let Some(r) = super::bin::bin_commit_refusal(&rec, &rec.commit, cfg.force_mixed) {
            return Err(format!("eval walk: {r}"));
        }
        std::fs::create_dir_all(cfg.dir.join("bin")).map_err(|e| e.to_string())?;
        // Copied by `cp` (a child), never through a write fd of this
        // process: a forked sibling holding that fd at exec time would make
        // the first launch of the frozen copy fail with ETXTBSY.
        crate::proc::run_ok(&crate::proc::Spawn::new(
            "cp",
            &[&text(&cfg.bin), &text(&frozen)],
            std::time::Duration::from_secs(300),
        ))?;
        write(
            &record_path,
            &format!("{}\n", humanify_ag_model::js::stringify(&rec.to_js())),
        )?;
    }
    let record = std::fs::read_to_string(&record_path)
        .ok()
        .and_then(|t| JsValue::parse(&t).ok())
        .ok_or_else(|| format!("{}: unreadable", text(&record_path)))?;
    let sha = std::fs::read(&frozen)
        .map(|b| crate::committed::sha256_hex(&b))
        .unwrap_or_default();
    let want = record
        .as_object()
        .and_then(|o| o.get("sha256"))
        .and_then(JsValue::as_str);
    if want != Some(sha.as_str()) {
        return Err("eval walk: frozen binary sha256 does not match its record".into());
    }
    Ok((frozen, record))
}

fn label_bin(frozen: &Path, record: &JsValue) -> LabelBin {
    let s = |k: &str| {
        record
            .as_object()
            .and_then(|o| o.get(k))
            .and_then(JsValue::as_str)
            .unwrap_or("")
            .to_string()
    };
    let b = |k: &str| record.as_object().and_then(|o| o.get(k)) == Some(&JsValue::Bool(true));
    LabelBin {
        path: text(frozen),
        sha256: s("sha256"),
        commit: s("commit"),
        dirty: b("dirty"),
        built: b("built"),
    }
}

fn new_manifest(w: &Walk, record: &JsValue, versions: &[String]) -> JsValue {
    let cfg = w.cfg;
    let lock = if w.ctx.host.held_lock {
        "held by the caller (HELD_LOCK)".to_string()
    } else {
        format!("flock:{}", text(&cfg.heavy_lock))
    };
    obj(vec![
        ("pipeline", JsValue::str("rust")),
        (
            "pipelineRecord",
            obj(vec![
                ("kind", JsValue::str("rust-bin")),
                ("bin", record.clone()),
                ("frozenCopy", JsValue::str(text(&w.frozen))),
            ]),
        ),
        ("range", JsValue::str(cfg.versions_arg.as_str())),
        ("versions", JsValue::str_array(versions)),
        ("inputsBase", JsValue::str(cfg.inputs_base.as_str())),
        (
            "llm",
            obj(vec![
                ("endpoint", JsValue::str(w.endpoint.as_str())),
                ("model", JsValue::str(w.pairs.llm.model.as_str())),
                (
                    "reasoningEffort",
                    JsValue::str(w.pairs.llm.reasoning_effort.as_str()),
                ),
                ("concurrency", num(w.pairs.llm.concurrency)),
                ("cache", JsValue::str("OFF (cold, rule 10)")),
            ]),
        ),
        ("heavyLock", JsValue::str(lock)),
        (
            "bootGate",
            obj(vec![
                ("model", JsValue::str(cfg.boot_model.as_str())),
                ("prompt", JsValue::Bool(cfg.boot_prompt)),
            ]),
        ),
        (
            "harnessCommit",
            JsValue::str(super::head(&w.ctx.repo, false)),
        ),
        ("namingMode", JsValue::str(cfg.naming_mode.as_str())),
        (
            "libraryIndex",
            cfg.library_index
                .clone()
                .map_or(JsValue::Null, JsValue::str),
        ),
        ("pipelineSettings", JsValue::str_array(&cfg.pipeline_sets)),
    ])
}

fn get<'a>(v: &'a JsValue, path: &[&str]) -> Option<&'a JsValue> {
    let mut cur = v;
    for k in path {
        cur = cur.as_object()?.get(k)?;
    }
    Some(cur)
}

/// Write the manifest, or on `--resume` check it: the walk must continue
/// with the SAME pipeline, model, inputs, naming mode and index; a
/// resumed walk may be EXTENDED (the versions union, semver order).
fn record_manifest(w: &Walk, fresh: JsValue) -> Result<(), String> {
    let path = w.cfg.dir.join("manifest.json");
    let Some(old) = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| JsValue::parse(&t).ok())
    else {
        let mut m = fresh;
        if let JsValue::Object(o) = &mut m {
            o.insert("startedAt", JsValue::str(humanify_ag_model::js::iso_now()));
        }
        return write(&path, &format!("{}\n", js_pretty(&m)));
    };
    for k in [
        &["pipeline"][..],
        &["pipelineRecord"],
        &["llm", "endpoint"],
        &["llm", "model"],
        &["inputsBase"],
        &["namingMode"],
        &["libraryIndex"],
    ] {
        if get(&old, k) != get(&fresh, k) {
            let show =
                |v: Option<&JsValue>| v.map_or("null".into(), humanify_ag_model::js::stringify);
            return Err(format!(
                "eval walk: --resume refused: .{} differs ({} vs {})",
                k.join("."),
                show(get(&old, k)),
                show(get(&fresh, k))
            ));
        }
    }
    let mut versions: Vec<String> = [get(&old, &["versions"]), get(&fresh, &["versions"])]
        .into_iter()
        .flatten()
        .filter_map(|v| match v {
            JsValue::Array(a) => Some(
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        })
        .flatten()
        .collect();
    sort_versions(&mut versions);
    versions.dedup();
    let mut m = old;
    if let JsValue::Object(o) = &mut m {
        o.insert("versions", JsValue::str_array(&versions));
    }
    write(&path, &format!("{}\n", js_pretty(&m)))
}

/// `eval walk`: exit 0 when every selected hop finished (and the report
/// was written), 2 refused, 1 a hop left no tree.
pub fn run(ctx: &Ctx, cfg: &WalkConfig, pairs: &Pairs) -> u8 {
    match run_inner(ctx, cfg, pairs) {
        Ok(()) => 0,
        Err((code, msg)) => {
            ctx.say(msg);
            code
        }
    }
}

fn run_inner(ctx: &Ctx, cfg: &WalkConfig, pairs: &Pairs) -> Result<(), (u8, String)> {
    let refuse = |e: String| (2u8, e);
    super::boot::require_bun(ctx).map_err(|e| (1, e))?;
    let endpoint =
        crate::committed::local::resolve_endpoint(cfg.endpoint_flag.as_deref(), &ctx.repo)
            .map_err(refuse)?;
    let versions = select_versions(&cfg.versions_arg, &cfg.inputs_base).map_err(refuse)?;
    for d in ["trees", "runs", "bin"] {
        std::fs::create_dir_all(cfg.dir.join(d)).map_err(|e| refuse(e.to_string()))?;
    }
    if cfg.dir.join("manifest.json").is_file() && !cfg.resume {
        return Err(refuse(format!(
            "eval walk: {} already holds a walk; pass --resume to continue it",
            text(&cfg.dir)
        )));
    }
    let (frozen, record) = freeze(ctx, cfg).map_err(|e| (1, e))?;
    preflight(ctx, &frozen, &cfg.dir).map_err(refuse)?;
    let w = Walk {
        ctx,
        cfg,
        pairs,
        endpoint,
        bin: label_bin(&frozen, &record),
        frozen,
    };
    record_manifest(&w, new_manifest(&w, &record, &versions)).map_err(refuse)?;
    ctx.say(format!(
        "walk: rust over {} versions: {}",
        versions.len(),
        versions.join(" ")
    ));
    ctx.say(format!(
        "walk: out {}; endpoint {} ({}); LLM cache OFF",
        text(&cfg.dir),
        w.endpoint,
        pairs.llm.model
    ));
    let mut prior: Option<String> = None;
    for v in &versions {
        hop(&w, v, prior.as_deref()).map_err(|e| (1, e))?;
        prior = Some(v.clone());
    }
    if cfg.report {
        let walks = vec![("rust".to_string(), cfg.dir.clone())];
        report::write_report(ctx, &walks, &cfg.dir.join("report"), None, 4).map_err(|e| (1, e))?;
    }
    ctx.say(format!("SENTINEL: WALK-DONE rust {}", text(&cfg.dir)));
    Ok(())
}

/// The pipeline must at least start (`--help`).
fn preflight(ctx: &Ctx, frozen: &Path, dir: &Path) -> Result<(), String> {
    let mut s = ctx.spawn(
        &text(frozen),
        &["--help".into()],
        std::time::Duration::from_secs(300),
    );
    s.cwd = Some(ctx.repo.clone());
    let done = crate::proc::run(&s);
    let ok = matches!(&done, Ok(d) if d.status.success());
    if let Ok(d) = &done {
        let _ = write(
            &dir.join("preflight.log"),
            &String::from_utf8_lossy(&d.stdout),
        );
    }
    if ok {
        Ok(())
    } else {
        Err(format!(
            "eval walk: PREFLIGHT FAILED — '{} --help' does not run (see {})",
            text(frozen),
            text(&dir.join("preflight.log"))
        ))
    }
}

/// Boot both halves and RECORD them; never fails the walk.
fn boot_record(w: &Walk, tree: &Path, v: &str, dest: &Path) -> JsValue {
    let (ctx, cfg) = (w.ctx, w.cfg);
    let (version, prompt, vok, pok) = if tree.join("run.cjs").is_file() {
        let b = super::boot::boot(ctx, tree, v, cfg.boot_prompt, &cfg.boot_model);
        let (vok, pok) = (b.version_ok, b.prompt_ok);
        (b.version, b.prompt, vok, pok)
    } else {
        (
            "(no run.cjs)".to_string(),
            "skipped".to_string(),
            false,
            !cfg.boot_prompt,
        )
    };
    // The live half needs account quota: "out of extra usage" says nothing
    // about the tree — UNVERIFIED, not broken (re-record with --reboot).
    let blocked = !pok
        && [
            "out of extra usage",
            "usage limit",
            "rate limit",
            "overloaded",
        ]
        .iter()
        .any(|p| prompt.contains(p));
    let rec = obj(vec![
        ("version", JsValue::str(version)),
        ("prompt", JsValue::str(prompt)),
        ("versionOk", JsValue::Bool(vok)),
        ("promptOk", JsValue::Bool(pok)),
        ("promptBlockedByAccount", JsValue::Bool(blocked)),
        ("ok", JsValue::Bool(vok && pok)),
        ("model", JsValue::str(cfg.boot_model.as_str())),
    ]);
    let _ = write(dest, &format!("{}\n", js_pretty(&rec)));
    ctx.say(format!(
        "  boot: {}  --version={vok} -p={pok}",
        if vok && pok { "OK" } else { "FAIL" }
    ));
    rec
}

fn hop(w: &Walk, v: &str, prior: Option<&str>) -> Result<(), String> {
    let (ctx, cfg) = (w.ctx, w.cfg);
    let tree = cfg.dir.join("trees").join(v);
    let runs = cfg.dir.join("runs");
    let hop_path = runs.join(format!("{v}.hop.json"));
    if hop_path.is_file() && tree.join(".humanify/humanified.js").is_file() {
        return finished_hop(w, v, &tree, &hop_path);
    }
    let prior_path = prior.map(|p| {
        cfg.dir
            .join("trees")
            .join(p)
            .join(".humanify/humanified.js")
    });
    ctx.say(format!(
        "=== {} hop {v} (prior: {}) ===",
        humanify_ag_model::js::iso_now(),
        prior.unwrap_or("COLD")
    ));
    remove(&tree);
    let stats = runs.join(format!("{v}.stats.json"));
    remove(&stats);
    let toml = runs.join(format!("{v}.toml"));
    let mut entries = base_settings(&w.pairs.llm.reasoning_effort, w.pairs.llm.concurrency);
    if let Some(i) = &cfg.library_index {
        entries.push(format!("library.index={i}"));
    }
    entries.extend(cfg.pipeline_sets.iter().cloned());
    entries.push(format!("diagnostics.stats_json={}", text(&stats)));
    entries.push(format!(
        "diagnostics.log_file={}",
        text(&runs.join(format!("{v}.log")))
    ));
    let config_toml = write_config(&toml, &entries)?;
    let input = input_of(&cfg.inputs_base, v);
    let (toml_t, tree_t) = (text(&toml), text(&tree));
    let prior_t = prior_path.as_ref().map(|p| text(p));
    let args = HumanifyArgs {
        input: &input,
        config: &toml_t,
        endpoint: &w.endpoint,
        model: &w.pairs.llm.model,
        out: &tree_t,
        prior: prior_t.as_deref(),
        verbose: true,
    }
    .argv();
    let spec = LegSpec {
        pair: format!("{}->{v}", prior.unwrap_or("COLD")),
        version: v.to_string(),
        label: "walk-rust".into(),
        results_dir: runs.clone(),
        input: input.clone(),
        prior: prior_t.clone().unwrap_or_default(),
        base_mode: None,
        output_dir: tree.clone(),
        repo: ctx.repo.clone(),
        args,
        config_toml: Some(config_toml),
        stdout_path: runs.join(format!("{v}.stdout")),
        endpoint: w.endpoint.clone(),
        model: w.pairs.llm.model.clone(),
        reasoning_effort: w.pairs.llm.reasoning_effort.clone(),
        concurrency: w.pairs.llm.concurrency,
        cache_dir: None,
        artifacts: vec![
            text(&tree.join(".humanify/humanified.js")),
            text(&tree.join(".humanify/split-ledger.json")),
            text(&stats),
        ],
        bin: w.bin.clone(),
    };
    leg::write_spec(&spec, &runs.join(format!("{v}.runcfg.json")))?;
    remove(&runs.join(format!("{v}-run.json")));
    let t_lock = std::time::Instant::now();
    let outcome = {
        let _lock = super::heavy_lock(ctx, &cfg.heavy_lock)?;
        leg::run_leg_with_prior_kind(
            ctx,
            &spec,
            &w.pairs.llm.api_key,
            prior.map(|_| WALK_PRIOR_KIND),
        )
    };
    let elapsed = t_lock.elapsed().as_secs_f64().round();
    let rc = outcome.as_ref().map_or(-1, |o| o.exit_code);
    let tree_ok = tree.join(".humanify/humanified.js").is_file()
        && tree.join(".humanify/split-ledger.json").is_file();
    if !tree_ok {
        let stdout = std::fs::read_to_string(&spec.stdout_path).unwrap_or_default();
        let errors: Vec<&str> = stdout
            .lines()
            .filter(|l| l.starts_with("ERROR:"))
            .take(5)
            .collect();
        return Err(format!(
            "walk: hop {v} exited {rc} and wrote NO tree — aborting (the next hop has no prior)\n    {}",
            errors.join("\n    ")
        ));
    }
    if rc != 0 {
        ctx.say(format!(
            "  !! exit {rc} but the tree was written — recorded, walk continues on it"
        ));
    }
    let boot = boot_record(w, &tree, v, &runs.join(format!("{v}-boot.json")));
    let manifest = outcome.ok().map(|o| o.manifest);
    write_hop(&hop_path, v, prior, rc, elapsed, manifest.as_ref(), boot)
}

fn write_hop(
    path: &Path,
    v: &str,
    prior: Option<&str>,
    rc: i32,
    elapsed: f64,
    run: Option<&JsValue>,
    boot: JsValue,
) -> Result<(), String> {
    let field = |p: &[&str]| {
        run.and_then(|m| get(m, p))
            .cloned()
            .unwrap_or(JsValue::Null)
    };
    let wall = field(&["wallSeconds"]);
    let lock_wait = match &wall {
        JsValue::Number(w) => num(elapsed - w),
        _ => JsValue::Null,
    };
    let errors = run
        .and_then(|m| get(m, &["outcome", "errors"]).cloned())
        .unwrap_or_else(|| JsValue::str_array(&["the run wrote no manifest"]));
    let rec = obj(vec![
        ("version", JsValue::str(v)),
        ("priorVersion", prior.map_or(JsValue::Null, JsValue::str)),
        ("cold", JsValue::Bool(prior.is_none())),
        ("exitCode", num(rc)),
        ("wallSeconds", wall),
        ("lockWaitSeconds", lock_wait),
        ("peakRssMb", field(&["outcome", "peakRssMb"])),
        ("cacheWritten", field(&["config", "cache", "written"])),
        ("errors", errors),
        ("boot", boot),
        ("finishedAt", JsValue::str(humanify_ag_model::js::iso_now())),
    ]);
    write(path, &format!("{}\n", js_pretty(&rec)))
}

/// A finished hop is skipped; with `--reboot` its failed boot is re-run
/// and re-recorded (marked).
fn finished_hop(w: &Walk, v: &str, tree: &Path, hop_path: &Path) -> Result<(), String> {
    let mut hop = std::fs::read_to_string(hop_path)
        .ok()
        .and_then(|t| JsValue::parse(&t).ok())
        .ok_or_else(|| format!("{}: unreadable", text(hop_path)))?;
    if w.cfg.reboot && get(&hop, &["boot", "ok"]) != Some(&JsValue::Bool(true)) {
        w.ctx
            .say(format!("=== {v}: re-running the boot gate (--reboot)"));
        let mut boot = boot_record(
            w,
            tree,
            v,
            &w.cfg.dir.join("runs").join(format!("{v}-boot.json")),
        );
        if let JsValue::Object(b) = &mut boot {
            b.insert("rerecorded", JsValue::str(humanify_ag_model::js::iso_now()));
        }
        if let JsValue::Object(o) = &mut hop {
            o.insert("boot", boot);
        }
        write(hop_path, &format!("{}\n", js_pretty(&hop)))?;
    }
    let show = |p: &[&str]| get(&hop, p).map_or("null".into(), humanify_ag_model::js::stringify);
    w.ctx.say(format!(
        "=== {v}: done (exit {}, {}s, boot {}) — skipped",
        show(&["exitCode"]),
        show(&["wallSeconds"]),
        show(&["boot", "ok"])
    ));
    Ok(())
}

/// `eval walk-report`: the report over one or more finished walks
/// (`name=dir`, or a bare dir named after its position).
pub fn report_verb(ctx: &Ctx, walks: &[String], out: Option<&str>) -> u8 {
    let parsed: Vec<(String, PathBuf)> = walks
        .iter()
        .enumerate()
        .map(|(i, w)| match w.split_once('=') {
            Some((n, d)) => (n.to_string(), PathBuf::from(d)),
            None => (
                if walks.len() == 1 {
                    "rust".to_string()
                } else {
                    format!("walk{}", i + 1)
                },
                PathBuf::from(w),
            ),
        })
        .collect();
    let Some((_, first)) = parsed.first() else {
        ctx.say("usage: eval walk-report <name>=<walk-dir>... [--report-dir D]");
        return 2;
    };
    let out = out.map_or_else(|| first.join("report"), PathBuf::from);
    match report::write_report(ctx, &parsed, &out, None, 4) {
        Ok(()) => 0,
        Err(e) => {
            ctx.say(format!("report: {e}"));
            1
        }
    }
}
