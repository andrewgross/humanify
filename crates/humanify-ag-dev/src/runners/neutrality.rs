//! `eval neutrality <baseline-ref> [from:to]`: prove a change emits
//! IDENTICAL BYTES — the right gate for a refactor (the port of
//! `experiments/lib/neutrality.sh` + `build-bin.sh`).
//!
//! Both legs share ONE warm LLM cache and run the same input, prior and
//! settings, so the prompts are replayed, not redrawn: any difference in
//! the emitted tree is the code change's. Rule 10 permits this cache use —
//! the verdict is about determinism with the model held fixed.
//!
//! THE BASELINE LEG'S CACHE-WRITE COUNT IS LOAD-BEARING: the candidate leg
//! runs first and populates (any count is fine); if the baseline leg
//! writes entries, the two legs asked DIFFERENT QUESTIONS and the verdict
//! is void (a COLD run — null-control proven 2026-08-11). A pass needs
//! zero new prompts on the baseline leg, identical bytes and matching exit
//! codes.
//!
//! Each leg runs the binary built from ITS commit: the candidate from this
//! checkout (which must be clean: it IS the working tree), the baseline in
//! a detached worktree (`build-bin`'s rename-aware build). A baseline
//! whose binary takes no `--config` (before config sweep step 5) gets the
//! same settings as the flags it had then — asked of its own `--help`;
//! such a leg cannot take a `--pipeline-set` (nor a names-mode index), and
//! is refused then. The neutrality launch goldens hold every launch.
//!
//! WHAT IT CANNOT SEE: a change that only shows on another input, a change
//! to how prompts are BUILT that leaves the cache key alone, and
//! multi-hop feedback. A pass means "inert on this pair with draws
//! pinned", never "inert".

use std::path::{Path, PathBuf};

use humanify_ag_model::js::JsValue;

use super::tree_diff::{changed_lines_in_trees, differing_files, differing_sidecars};
use super::{Ctx, count_files, remove, text, write};
use crate::bin_build::{HELP_PROBE_DEADLINE, is_legacy_help};
use crate::committed::eval::Pairs;
use crate::launch::{HumanifyArgs, base_settings, write_config};
use crate::proc::git;
use crate::settings::Parsed;

/// The PIPELINE keys neutrality writes into every leg's file itself.
pub const NEUTRALITY_HARNESS_KEYS: &[&str] = &[
    "llm.reasoning_effort",
    "llm.concurrency",
    "split.enabled",
    "llm.cache_dir",
    "library.index",
];

/// One neutrality run's settings, resolved once.
#[derive(Clone, Debug)]
pub struct NeutralityConfig {
    pub baseline: String,
    pub from: String,
    pub to: String,
    pub workdir: PathBuf,
    /// The shared cache BOTH legs replay. A fresh one is a COLD run.
    pub cache: PathBuf,
    pub priors: PathBuf,
    pub inputs_base: String,
    pub endpoint_flag: Option<String>,
    pub pipeline_sets: Vec<String>,
    pub naming_mode: String,
    pub library_index: Option<String>,
}

const USAGE: &str = "usage: eval neutrality <baseline-ref> [from:to] [--workdir D] [--cache D] [--priors D] [--inputs-base D] [--endpoint URL] [--library-index D] [--pipeline-set section.key=value]...";

/// The run's settings from its parsed command line and pairs.json. The
/// workdir carries the cache and the priors by default — the documented
/// trap (a fresh workdir is a COLD cache and a void verdict), kept
/// visible rather than silently changed.
pub fn config_from(parsed: &Parsed, pairs: &Pairs) -> Result<NeutralityConfig, String> {
    let s = &parsed.settings;
    let (baseline, pair) = match parsed.positionals.as_slice() {
        [b] => (
            b.clone(),
            s.text("neutrality.pair")
                .unwrap_or_else(|| "2.1.85:2.1.86".into()),
        ),
        [b, p] => (b.clone(), p.clone()),
        _ => return Err(USAGE.into()),
    };
    let (from, to) = pair
        .split_once(':')
        .filter(|(a, b)| {
            !a.is_empty()
                && !b.is_empty()
                && !b.contains(':')
                && !pair.contains(char::is_whitespace)
        })
        .ok_or_else(|| format!("pair \"{pair}\" is not <from>:<to> (e.g. 2.1.215:2.1.216)"))?;
    let sets = s.list("run.pipeline_set").unwrap_or_default();
    crate::settings::check_pipeline_sets(&sets, NEUTRALITY_HARNESS_KEYS)
        .map_err(|e| e.join("\n"))?;
    let (naming_mode, library_index) = super::score::naming_mode_of(
        &sets,
        s.text("eval.library_index").map(PathBuf::from).as_deref(),
        &pairs.library_index,
    )?;
    let workdir = PathBuf::from(s.text("run.workdir").unwrap_or_else(|| "/work".into()));
    Ok(NeutralityConfig {
        baseline,
        from: from.to_string(),
        to: to.to_string(),
        cache: s
            .text("neutrality.cache")
            .map_or_else(|| workdir.join("neutrality-cache"), PathBuf::from),
        priors: s
            .text("neutrality.priors")
            .map_or_else(|| workdir.join("exp050-cold"), PathBuf::from),
        inputs_base: s
            .text("eval.inputs_base")
            .unwrap_or_else(|| pairs.inputs_base.clone()),
        endpoint_flag: s.text("llm.endpoint"),
        workdir,
        pipeline_sets: sets,
        naming_mode,
        library_index,
    })
}

/// The values the run actually uses, recorded beside the settings in its
/// run config (the derived defaults included: a reader must see which
/// cache the verdict replayed).
pub fn resolved_record(cfg: &NeutralityConfig) -> Vec<(&'static str, JsValue)> {
    vec![
        ("baseline", JsValue::str(cfg.baseline.as_str())),
        ("pair", JsValue::str(format!("{}:{}", cfg.from, cfg.to))),
        ("workdir", JsValue::str(text(&cfg.workdir))),
        ("cache", JsValue::str(text(&cfg.cache))),
        ("priors", JsValue::str(text(&cfg.priors))),
        ("inputsBase", JsValue::str(cfg.inputs_base.as_str())),
        ("namingMode", JsValue::str(cfg.naming_mode.as_str())),
        (
            "libraryIndex",
            cfg.library_index
                .clone()
                .map_or(JsValue::Null, JsValue::str),
        ),
    ]
}

/// What both legs share.
struct Legs<'a> {
    ctx: &'a Ctx,
    cfg: &'a NeutralityConfig,
    pairs: &'a Pairs,
    endpoint: String,
    input: String,
    prior: String,
    /// The settings every leg's file carries after the base keys:
    /// `library.index` (names mode), then the `--pipeline-set` entries.
    sets: Vec<String>,
}

impl Legs<'_> {
    fn work(&self, name: &str) -> PathBuf {
        self.cfg.workdir.join(name)
    }

    /// The leg's argv after the input: a `--config` file when its binary
    /// reads one, else the flags a legacy binary takes. Err refuses.
    fn leg_flags(&self, label: &str, out: &Path, bin: &Path) -> Result<Vec<String>, String> {
        let mut probe = self
            .ctx
            .spawn(&text(bin), &["--help".into()], HELP_PROBE_DEADLINE);
        probe.cwd = Some(self.ctx.repo.clone());
        let help = crate::proc::run(&probe)
            .map(|d| String::from_utf8_lossy(&d.stdout).into_owned())
            .unwrap_or_default();
        let out_t = text(out);
        if !is_legacy_help(&help) {
            let toml = self.work(&format!("neutrality-{label}.toml"));
            let mut entries =
                base_settings(&self.pairs.llm.reasoning_effort, self.pairs.llm.concurrency);
            entries.push(format!("llm.cache_dir={}", text(&self.cfg.cache)));
            entries.extend(self.sets.iter().cloned());
            write_config(&toml, &entries)?;
            let toml_t = text(&toml);
            let mut a = HumanifyArgs {
                input: &self.input,
                config: &toml_t,
                endpoint: &self.endpoint,
                model: &self.pairs.llm.model,
                out: &out_t,
                prior: Some(&self.prior),
                verbose: false,
            }
            .argv();
            a.remove(0);
            return Ok(a);
        }
        if !self.sets.is_empty() {
            return Err(format!(
                "FATAL: leg {label}'s binary takes no --config file; --pipeline-set cannot reach it"
            ));
        }
        let effort = self.pairs.llm.reasoning_effort.clone();
        let conc = self.pairs.llm.concurrency.to_string();
        Ok([
            "--split",
            "--endpoint",
            &self.endpoint,
            "--model",
            &self.pairs.llm.model,
            "--reasoning-effort",
            &effort,
            "-c",
            &conc,
            "-o",
            &out_t,
            "--llm-cache",
            &text(&self.cfg.cache),
            "--prior-version",
            &self.prior,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect())
    }

    /// Run one leg from the checkout `src`: build its binary, launch it,
    /// record its exit code. Err when it could not run or wrote no tree.
    fn run_leg(&self, label: &str, src: &Path) -> Result<(), String> {
        let ctx = self.ctx;
        let out = self.work(&format!("neutrality-{label}"));
        remove(&out);
        ctx.say(format!("--- leg {label}: {}", text(src)));
        let log = self.work(&format!("neutrality-{label}.build.log"));
        let bin = match ctx.build(src) {
            Ok(r) => r.path,
            Err(e) => {
                let _ = write(&log, &e);
                return Err(format!(
                    "FATAL: cargo build failed in {} — see {}",
                    text(src),
                    text(&log)
                ));
            }
        };
        ctx.say(format!("    binary: {}", text(&bin)));
        let flags = self.leg_flags(label, &out, &bin)?;
        ctx.say(format!("    flags: {}", flags.join(" ")));
        let mut args = vec![self.input.clone()];
        args.extend(flags);
        let mut s = ctx.spawn(&text(&bin), &args, super::leg::PIPELINE_DEADLINE);
        s.cwd = Some(ctx.repo.clone());
        s.env
            .push(("HUMANIFY_API_KEY".into(), self.pairs.llm.api_key.clone()));
        let stdout = self.work(&format!("neutrality-{label}.stdout"));
        let rc = crate::proc::run_to_file(
            &s,
            &stdout,
            std::time::Duration::from_secs(3600),
            &mut |_| {},
        )
        .map(|e| e.code)
        .unwrap_or(-1);
        write(
            &self.work(&format!("neutrality-{label}.rc")),
            &format!("{rc}\n"),
        )?;
        let errors = || {
            std::fs::read_to_string(&stdout)
                .unwrap_or_default()
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };
        if !out.is_dir() {
            return Err(format!(
                "FATAL: leg {label} exited {rc} and wrote no tree. Tail of {}:\n{}",
                text(&stdout),
                errors().join("\n")
            ));
        }
        if rc != 0 {
            ctx.say(format!("    note: leg {label} exited {rc} but wrote a tree — recorded, compared, not swallowed"));
            let text = std::fs::read_to_string(&stdout).unwrap_or_default();
            for l in text.lines().filter(|l| l.starts_with("ERROR:")).take(3) {
                ctx.say(format!("      {l}"));
            }
        }
        ctx.say(format!("    -> {}", super::text(&out)));
        Ok(())
    }
}

/// The prior BUNDLE (the rebased tree preferred: scoring against the
/// archive reads ~3.7x worse for no reason).
fn prior_of(cfg: &NeutralityConfig) -> Result<String, String> {
    let mut base = cfg.priors.join(format!("{}-rebased", cfg.from));
    if !base.is_dir() {
        base = cfg.priors.join(&cfg.from);
    }
    let prior = base.join(".humanify/humanified.js");
    if prior.is_file() {
        return Ok(text(&prior));
    }
    let mut msg = format!(
        "FATAL: no prior bundle at {}\n       (looked under {} for '{}-rebased' then '{}')",
        text(&prior),
        text(&cfg.priors),
        cfg.from,
        cfg.from
    );
    if !cfg.priors.is_dir() {
        msg.push_str(&format!(
            "\n       {} does not exist at all.\n       The workdir is also the default prior root. To use a separate\n       \
             workdir, keep the priors (and the warm cache) where they are:\n         \
             cargo dev eval neutrality {} {}:{} --workdir {} --priors /work/exp050-cold --cache /work/neutrality-cache",
            text(&cfg.priors),
            cfg.baseline,
            cfg.from,
            cfg.to,
            text(&cfg.workdir)
        ));
    }
    Err(msg)
}

/// `eval neutrality`: exit 0 NEUTRAL, 1 NOT NEUTRAL (or could not run).
pub fn run(ctx: &Ctx, cfg: &NeutralityConfig, pairs: &Pairs, run_config: &JsValue) -> u8 {
    match run_inner(ctx, cfg, pairs, run_config) {
        Ok(code) => code,
        Err((code, msg)) => {
            ctx.say(msg);
            code
        }
    }
}

fn run_inner(
    ctx: &Ctx,
    cfg: &NeutralityConfig,
    pairs: &Pairs,
    run_config: &JsValue,
) -> Result<u8, (u8, String)> {
    let fail = |e: String| (1u8, e);
    let what = format!("eval neutrality {} {}:{}", cfg.baseline, cfg.from, cfg.to);
    // Two runs on one cache void each other's verdict (the baseline leg's
    // zero-write count would hold the other run's writes), and two runs in
    // one workdir share every leg folder: claim both, refuse at once.
    let _cache = super::claim::claim_folder(&cfg.cache, &what).map_err(|e| (2, e))?;
    let _legs = super::claim::claim(
        &cfg.workdir.join("neutrality.live-run"),
        &cfg.workdir.join("neutrality-*"),
        &what,
    )
    .map_err(|e| (2, e))?;
    std::fs::create_dir_all(&cfg.workdir).map_err(|e| fail(e.to_string()))?;
    let cfg_path = cfg.workdir.join("neutrality-run-config.json");
    write(&cfg_path, &crate::json::js_text(run_config, true)).map_err(fail)?;
    let endpoint =
        crate::committed::local::resolve_endpoint(cfg.endpoint_flag.as_deref(), &ctx.repo)
            .map_err(|e| (2, e))?;
    let input = super::input_of(&cfg.inputs_base, &cfg.to);
    if !Path::new(&input).is_file() {
        return Err(fail(format!("FATAL: no input at {input}")));
    }
    let prior = prior_of(cfg).map_err(fail)?;
    if super::dirty(&ctx.repo) {
        return Err(fail("FATAL: working tree is dirty. The candidate leg IS the working tree,\n       so a result from here would not correspond to any commit.".into()));
    }
    let cand = super::head(&ctx.repo, true);
    let name = git(&ctx.repo, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let base_sha = git(&ctx.repo, &["rev-parse", "--short", &cfg.baseline]).map_err(fail)?;
    let mut sets: Vec<String> = cfg
        .library_index
        .iter()
        .map(|i| format!("library.index={i}"))
        .collect();
    sets.extend(cfg.pipeline_sets.iter().cloned());
    ctx.say(format!(
        "=== neutrality: {name} ({cand}) vs {} ({base_sha}) on {}->{} ===",
        cfg.baseline, cfg.from, cfg.to
    ));
    ctx.say(format!("    run config: {}", text(&cfg_path)));
    if !sets.is_empty() {
        ctx.say(format!("    both legs: {}", sets.join(" ")));
    }
    if cand == base_sha {
        return Err(fail(
            "FATAL: candidate and baseline are the same commit — nothing to compare.".into(),
        ));
    }
    let legs = Legs {
        ctx,
        cfg,
        pairs,
        endpoint,
        input,
        prior,
        sets,
    };
    for leg in ["candidate", "baseline"] {
        remove(&legs.work(&format!("neutrality-{leg}.rc")));
    }
    let before_a = count_files(&cfg.cache);
    legs.run_leg("candidate", &ctx.repo).map_err(fail)?;
    let after_a = count_files(&cfg.cache);
    let wt = legs.work("neutrality-baseline-src");
    prepare_worktree(ctx, &wt, &cfg.baseline).map_err(fail)?;
    let baseline = legs.run_leg("baseline", &wt);
    if let Err(e) = baseline {
        remove_worktree(ctx, &wt);
        return Err(fail(e));
    }
    let after_b = count_files(&cfg.cache);
    let code = verdict(&legs, (before_a, after_a, after_b));
    remove_worktree(ctx, &wt);
    code.map_err(fail)
}

fn prepare_worktree(ctx: &Ctx, wt: &Path, baseline: &str) -> Result<(), String> {
    remove(wt);
    remove_worktree(ctx, wt);
    git(
        &ctx.repo,
        &["worktree", "add", "--detach", &text(wt), baseline],
    )
    .map_err(|_| {
        format!(
            "FATAL: could not create a worktree at {} for {baseline}",
            text(wt)
        )
    })?;
    super::bin::link_node_modules(&ctx.repo, wt);
    Ok(())
}

fn remove_worktree(ctx: &Ctx, wt: &Path) {
    let _ = git(&ctx.repo, &["worktree", "remove", "--force", &text(wt)]);
}

fn read_rc(legs: &Legs, leg: &str) -> Result<String, String> {
    std::fs::read_to_string(legs.work(&format!("neutrality-{leg}.rc")))
        .map(|s| s.trim().to_string())
        .map_err(|_| format!("FATAL: leg {leg} recorded no exit code — the verdict would be about\n       a run that did not happen."))
}

/// Cache writes, exit codes and the tree diff → the verdict.
fn verdict(legs: &Legs, (before_a, after_a, after_b): (u64, u64, u64)) -> Result<u8, String> {
    let ctx = legs.ctx;
    let (wa, wb) = (
        after_a as i64 - before_a as i64,
        after_b as i64 - after_a as i64,
    );
    ctx.say("");
    ctx.say("=== cache writes ===");
    ctx.say(format!(
        "  leg candidate: {wa} new entries (populates; any count is fine)"
    ));
    ctx.say(format!("  leg baseline : {wb} new entries  <-- MUST be 0"));
    let (rc_a, rc_b) = (read_rc(legs, "candidate")?, read_rc(legs, "baseline")?);
    ctx.say("");
    ctx.say("=== exit codes ===");
    ctx.say(format!("  leg candidate: {rc_a}"));
    ctx.say(format!(
        "  leg baseline : {rc_b}  <-- must MATCH the candidate"
    ));
    if rc_a != "0" && rc_a == rc_b {
        ctx.say("  both legs failed identically — a PRE-EXISTING failure, not this change.");
    }
    let (a, b) = (
        legs.work("neutrality-baseline"),
        legs.work("neutrality-candidate"),
    );
    let lines = changed_lines_in_trees(&a, &b)?;
    let files = differing_files(&a, &b)?;
    ctx.say("");
    ctx.say("=== tree diff ===");
    ctx.say(format!(
        "  differing files: {}   (load-bearing output; diagnostics-only artifacts excluded)",
        files.len()
    ));
    ctx.say(format!("  differing lines: {lines}"));
    let sidecars = differing_sidecars(&a, &b)?;
    if !sidecars.is_empty() {
        ctx.say(format!(
            "  note: {} diagnostics-only artifact(s) differ — expected when a change adds one:",
            sidecars.len()
        ));
        for l in &sidecars {
            ctx.say(format!("    {l}"));
        }
    }
    let neutral = lines == 0 && wb == 0 && rc_a == rc_b;
    ctx.say("");
    let (from, to) = (&legs.cfg.from, &legs.cfg.to);
    if neutral {
        ctx.say(format!(
            "NEUTRAL: identical bytes, identical exit code ({rc_a}) on {from}->{to}."
        ));
        ctx.say(format!("         Cache: candidate +{wa}, baseline +{wb}"));
        ctx.say("         (baseline +0 is the load-bearing one: leg B replayed leg A's answers");
        ctx.say("         exactly, so the two legs asked the same questions).");
        ctx.say("         Scope: this pair, draws pinned. Not a claim about other inputs.");
        if rc_a != "0" {
            ctx.say(format!(
                "         NOTE: both legs exited {rc_a}. This change is neutral with respect to a"
            ));
            ctx.say(
                "         failure that ALREADY EXISTS on the baseline — it is not a clean run.",
            );
        }
        return Ok(0);
    }
    ctx.say("NOT NEUTRAL: this change alters emitted output, the prompts asked, or the exit code.");
    for l in files.iter().take(20) {
        ctx.say(l.clone());
    }
    Ok(1)
}
