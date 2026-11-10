//! Launch parity (config sweep step 3, 2026-10-13): every command line the
//! harness launches the pipeline with parses to the SAME settings under the
//! clap surface as it did under the commander-13 port it replaced.
//!
//! The fixture (`tests/fixtures/launch-parity.json`) was captured from the
//! commander port at 587e211f, BEFORE the port was deleted: one row per
//! argv, holding everything the run is configured by — the resolved
//! `Settings` (what `meta.json`'s `flags` and the stats file are written
//! from), the naming schedule, the kill switches, the flag-invariant
//! verdict and every path/output option. The corpus is every launch in the
//! eval's launch goldens (read live, so a new launch shape fails here until
//! it is captured), plus the launches of `scripts/e2e.ts`,
//! `scripts/ask-trace.ts`, run.sh `--pipeline-arg`, neutrality.sh and the
//! crate's own integration tests, plus one row per flag.
//!
//! Lane L4 (2026-11-07): the launches outside the eval's two goldens were
//! a hand-transcribed list here, and it had gone stale (its neutrality
//! entry still read `--llm-cache ... --prior-version p.js`). They are now
//! read from the goldens lane L0 recorded from the real launchers
//! (`dev/eval/goldens/`), half three below.
//! The test stays in this crate, not the dev crate that loads those
//! goldens: what it checks is this crate's private parse surface
//! (`surface::parse_pipeline`, `Config::build`), and a product crate never
//! depends on the dev crate.
//!
//! Config sweep step 4 (2026-10-13) changed two things DELIBERATELY, and
//! the fixture is untouched (it stays the commander port's capture):
//!
//! - `--api-key` is retired: the key comes from `HUMANIFY_API_KEY` only.
//!   A captured argv is compared with its `--api-key <k>` pair removed,
//!   and its expected key is the environment stand-in's.
//! - the run is configured by the typed `Config`, which resolves every
//!   knob to its EFFECTIVE value: where the old `Settings` held "unset"
//!   and the consumer applied a default (`batch_size.map_or(25)`), the
//!   expected record applies that same default (`effective`). The
//!   comparison is of what the run actually uses.
//!
//! Config sweep step 5 (2026-10-13) moved every non-operational flag into
//! the `--config` TOML file. The fixture is STILL untouched: each captured
//! argv was translated (kept flags stay, each moved flag
//! becomes its key, per `MOVED_FLAGS`) and must resolve to the captured
//! effective record; a refusal must refuse with the captured message in
//! today's names (the flag spelled as its key). And every launch in the
//! re-recorded goldens — argv + the TOML file it carries — must resolve to
//! the effective record of a captured launch: the harness's new launches
//! configure the run exactly as the old ones did.
//!
//! exp179 (2026-11-01): every key is a flag again (generated from the
//! registry, one-to-one), and the split is ON by default. A captured argv
//! is now translated into today's FLAGS ([`to_today`]: each renamed
//! spelling becomes its flag, per `RENAMED_FLAGS`), and a launch that did
//! not pass `--split` — split OFF, the old default — gets `--no-split`:
//! the default flipped, the captured configuration did not.
//!
//! exp171 (2026-10-31): names is the binary's default, and every golden
//! launch now carries the eval's library index (`[library] index`, from
//! pairs.json) — compared with that key taken back out, after asserting it
//! is exactly the eval's index.

use serde_json::{Value, json};

use crate::config::{
    API_KEY_VAR, Config, ConfigFile, DEFAULT_CONTEXT_TOKENS, DEFAULT_MODULE_CONCURRENCY,
};
use crate::pipeline_config::enum_name;
use crate::surface::{CommandOptions, PipelineParse, RENAMED_FLAGS, parse_pipeline};
use humanify_ag_core::naming::waves::batch::WaveTunables;

fn repo_file(rel: &str) -> String {
    let path = format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

const GOLDENS: [&str; 2] = [
    "dev/eval/goldens/run-launch.golden.txt",
    "dev/eval/goldens/run-launch.seeded-base.golden.txt",
];

/// One launch: its argv and the TOML of the `--config` file it passed
/// (empty: none).
type Launch = (Vec<String>, String);

/// Every launch in a launch golden: each `humanify-ag NODE_OPTIONS=... ::
/// <argv>` line with the `humanify-config| <line>` lines after it (the
/// recorder prints the file it was given), and each run config's `args`
/// with its `configToml`.
fn golden_launches(text: &str) -> Vec<Launch> {
    let mut out: Vec<Launch> = Vec::new();
    let mut block: Option<String> = None;
    for line in text.lines() {
        if let Some(b) = block.as_mut() {
            b.push_str(line);
            b.push('\n');
            if line == "}" {
                let cfg: Value = serde_json::from_str(b).expect("a run config is JSON");
                let args = cfg["args"].as_array().expect("a run config has args");
                let argv = args
                    .iter()
                    .map(|a| a.as_str().unwrap().to_string())
                    .collect();
                let toml = cfg["configToml"].as_str().unwrap_or("").to_string();
                out.push((argv, toml));
                block = None;
            }
        } else if let Some(rest) = line.strip_prefix("humanify-config| ") {
            let last = out.last_mut().expect("a config line follows its launch");
            last.1.push_str(rest);
            last.1.push('\n');
        } else if line == "humanify-config|" {
            out.last_mut()
                .expect("a config line follows its launch")
                .1
                .push('\n');
        } else if line.starts_with("humanify ") {
            let (_, argv) = line.split_once(" :: ").expect("a launch line has ' :: '");
            out.push((
                argv.split_whitespace().map(str::to_string).collect(),
                String::new(),
            ));
        } else if line.starts_with("runcfg-") && line.ends_with(": {") {
            block = Some("{\n".to_string());
        }
    }
    out
}

/// A captured (commander-era) argv in today's form: each renamed spelling
/// becomes today's flag (`-c 472` is `--llm-concurrency 472`), and a launch
/// without `--split` says `--no-split` — the old default, now explicit.
fn to_today(argv: &[String]) -> Launch {
    let mut out: Vec<String> = Vec::new();
    for arg in argv {
        let (name, attached) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with('-') => (n, Some(v)),
            _ => (arg.as_str(), None),
        };
        match RENAMED_FLAGS.iter().find(|(old, _)| *old == name) {
            Some((_, new)) => {
                let mut words: Vec<String> = new.split_whitespace().map(str::to_string).collect();
                if let Some(v) = attached {
                    let first = format!("{}={v}", words[0]);
                    words[0] = first;
                }
                out.extend(words);
            }
            None => out.push(arg.clone()),
        }
    }
    if !argv.iter().any(|a| a == "--split") {
        out.push("--no-split".to_string());
    }
    // Step 6: an index was read only for the diagnostics' report — today
    // that is `--library-matching report` (an index with matching off is
    // refused as a conflict).
    if out.iter().any(|a| a == "--library-index") {
        out.extend(["--library-matching".to_string(), "report".to_string()]);
    }
    (out, String::new())
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

/// Lane L0's launch goldens (`humanify-ag-dev`'s `paths::LAUNCH_GOLDENS`;
/// a product crate does not depend on the dev crate, so the path and the
/// small parser below are this test's own): one recorded launch per line,
/// `<program> [(in <dir>)] [ENV=v ...] :: <argv>`, each followed by the
/// `humanify-config| <line>` lines of the `--config` file it was given.
const LAUNCH_GOLDENS: &str = "dev/eval/goldens";

/// Every PIPELINE launch of THIS binary in one L0 golden: the program is
/// `.../humanify-ag` and the argv is a run (not `--help`, not a
/// subcommand: the first word is the input path). A baseline leg's older binary (`.../humanify`) answers to
/// its own commit's surface, not this one.
fn recorded_launches(text: &str) -> Vec<Launch> {
    let mut out: Vec<Launch> = Vec::new();
    let mut attach = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("humanify-config|") {
            if attach {
                let last = out.last_mut().expect("a config line follows its launch");
                last.1.push_str(rest.strip_prefix(' ').unwrap_or(rest));
                last.1.push('\n');
            }
            continue;
        }
        attach = false;
        let Some((head, argv)) = line.split_once(" :: ") else {
            continue;
        };
        let program = head.split_whitespace().next().unwrap_or("");
        let argv = words(argv);
        let first = argv.first().map(String::as_str).unwrap_or("");
        // A run's first word is its input PATH; a subcommand's is a bare
        // word (`detect`, `discover`, `index`, ...).
        let is_run = first.contains('/') && !first.starts_with('-');
        if program.ends_with("/humanify-ag") && is_run {
            out.push((argv, String::new()));
            attach = true;
        }
    }
    out
}

/// Every pipeline launch in every L0 launch golden, by golden file name.
fn recorded_corpus() -> Vec<(String, Launch)> {
    let dir = format!("{}/../../{LAUNCH_GOLDENS}", env!("CARGO_MANIFEST_DIR"));
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{dir}: {e}"))
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".golden.txt"))
        // the two run-launch goldens share the directory but are run.sh's
        // format, read through GOLDENS above
        .filter(|n| !GOLDENS.iter().any(|g| g.ends_with(n.as_str())))
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        let text = repo_file(&format!("{LAUNCH_GOLDENS}/{name}"));
        out.extend(
            recorded_launches(&text)
                .into_iter()
                .map(|l| (name.clone(), l)),
        );
    }
    out
}

/// The record with every PATH-valued (and free-text) field reduced to
/// whether it is set: a recorded launch runs under `<TMP>`/`<REPO>`
/// placeholders and a captured one under `in.js`/`out`, and parity is
/// about the configuration, not the spelling of a path.
fn path_free(mut record: Value) -> Value {
    const TOP: [&str; 14] = [
        "input",
        "outputDir",
        "splitLedger",
        "priorVersion",
        "logFile",
        "diagnostics",
        "libraryIndex",
        "perfTrace",
        "renameLedger",
        "statsJson",
        "dumpArtifacts",
        "dumpAsks",
        "simulateLlmLatency",
        "libraryDiscoveryUnits",
    ];
    const SETTINGS: [&str; 3] = ["endpoint", "model", "llmCacheDir"];
    let set = |v: &mut Value| *v = json!(!v.is_null());
    if let Some(o) = record.as_object_mut() {
        for k in TOP {
            if let Some(v) = o.get_mut(k) {
                set(v);
            }
        }
    }
    if let Some(o) = record["settings"]["Ok"].as_object_mut() {
        for k in SETTINGS {
            if let Some(v) = o.get_mut(k) {
                set(v);
            }
        }
    }
    record
}

/// Every launch in today's goldens.
fn golden_corpus() -> Vec<Launch> {
    let mut all: Vec<Launch> = GOLDENS
        .iter()
        .flat_map(|g| golden_launches(&repo_file(g)))
        .collect();
    let mut seen = std::collections::BTreeSet::new();
    all.retain(|a| seen.insert(a.clone()));
    all
}

/// The environment stand-in's API key (the parse is what is compared,
/// not the environment).
const ENV_KEY: &str = "env-key";

/// A number as the old record held it (`Settings` numbers were JS numbers).
fn n(v: impl Into<f64>) -> Value {
    json!(v.into())
}

fn settings_json(c: &Config) -> Value {
    let l = &c.llm;
    let g = &c.naming;
    json!({
        "endpoint": l.endpoint, "model": l.model, "apiKey": l.api_key.expose(),
        "timeout": n(l.timeout_ms as f64), "maxTokens": l.max_tokens.map(|t| t as f64),
        "contextTokens": n(l.context_tokens as f64),
        "reasoningEffort": l.reasoning_effort.map(|e| e.name()),
        "llmCacheDir": l.cache_dir, "concurrency": n(l.concurrency),
        "moduleConcurrency": n(l.module_concurrency), "retryAttempts": n(l.retries),
        "batchSize": n(g.batch_size as f64), "maxRetriesPerIdentifier": n(g.max_retries),
        "maxFreeRetries": g.max_free_retries.map(f64::from),
        "laneThreshold": n(g.lane_threshold as f64),
        "renameRetries": n(g.rename_retries as f64),
        "skipLibraries": c.library.skips_libraries(),
        "namingFloor": g.naming_floor,
        "namingFloorSweep": g.naming_floor_sweep,
        "reconcilePriorDiff": c.matching.reconcile_prior_diff,
    })
}

/// Everything a run is configured by, from one parsed argv, through the
/// ONE resolver (`Config::build`). A refused configuration records its
/// refusals where the commander-era record kept the flag-invariant
/// verdict.
pub(crate) fn record(input: &str, opts: &CommandOptions, file: Option<&ConfigFile>) -> Value {
    let env = |name: &str| (name == API_KEY_VAR).then(|| ENV_KEY.to_string());
    let c = match Config::build(input, opts, file, env) {
        Ok(c) => c,
        Err(refusals) => {
            let refusals: Vec<String> = refusals
                .iter()
                .map(|m| {
                    m.trim_start_matches(CONFIG_NAME)
                        .trim_start_matches(": ")
                        .to_string()
                })
                .collect();
            return json!({ "input": input, "invariants": refusals });
        }
    };
    let switch_list = |disable: bool| -> Option<String> {
        let names: Vec<&str> = c
            .switches
            .active()
            .into_iter()
            .filter(|s| s.is_disable() == disable)
            .map(|s| s.name())
            .collect();
        (!names.is_empty()).then(|| names.join(","))
    };
    let d = &c.diagnostics;
    json!({
        "input": c.io.input,
        "settings": { "Ok": settings_json(&c) },
        "fastTier": format!("{:?}", Ok::<_, String>(c.naming.schedule)),
        "switches": format!("{:?}", Ok::<_, String>(c.switches.clone())),
        "invariants": Vec::<String>::new(),
        "verbose": d.verbosity,
        "outputDir": c.io.output_dir,
        "split": c.split.enabled,
        "splitPure": c.split.pure,
        "splitLedger": c.split.ledger,
        "bundler": c.unpack.bundler.map(enum_name),
        "minifier": c.unpack.minifier.map(enum_name),
        "priorVersion": c.matching.prior_version,
        "disable": switch_list(true),
        "probe": switch_list(false),
        "logFile": d.log_file,
        "diagnostics": d.diagnostics_file,
        "libraryIndex": c.library.index,
        "libraryDiscoveryUnits": c.library.report_units,
        "perfTrace": c.perf.perf_trace,
        "renameLedger": d.rename_ledger,
        "statsJson": d.stats_json,
        "dumpArtifacts": d.dump_artifacts,
        "dumpAsks": d.dump_asks,
        "simulateLlmLatency": c.perf.simulate_llm_latency,
    })
}

/// A captured argv with its retired `--api-key <k>` pair removed.
fn without_api_key(argv: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut skip = false;
    for a in argv {
        if skip {
            skip = false;
        } else if a == "--api-key" {
            skip = true;
        } else {
            out.push(a.clone());
        }
    }
    out
}

/// The captured record, in effective values: the key from the
/// environment, every "unset" the consumers defaulted resolved to that
/// default, numbers as numbers. A captured refusal keeps only its
/// messages.
fn effective(captured: &Value) -> Value {
    let invariants = &captured["invariants"];
    if let Some(list) = invariants.as_array().filter(|a| !a.is_empty()) {
        // In today's names: the flags are keys.
        let renamed: Vec<String> = list
            .iter()
            .map(|m| {
                m.as_str()
                    .unwrap()
                    .replace("--split-pure", "split.pure")
                    .replace("--split", "split.enabled")
            })
            .collect();
        return json!({ "input": captured["input"], "invariants": renamed });
    }
    let mut out = captured.clone();
    let s = out["settings"]["Ok"]
        .as_object_mut()
        .expect("a valid capture has settings");
    let d = WaveTunables::default();
    let default_of = |k: &str| -> Option<f64> {
        Some(match k {
            "batchSize" => d.batch_size as f64,
            "maxRetriesPerIdentifier" => f64::from(d.max_retries),
            "laneThreshold" => d.lane_threshold as f64,
            "renameRetries" => d.reask_limit as f64,
            "moduleConcurrency" => f64::from(DEFAULT_MODULE_CONCURRENCY),
            "contextTokens" => DEFAULT_CONTEXT_TOKENS as f64,
            _ => return None,
        })
    };
    for (k, v) in s.iter_mut() {
        if v.is_null() {
            if let Some(dv) = default_of(k) {
                *v = json!(dv);
            }
        } else if let Some(x) = v.as_f64() {
            *v = json!(x);
        }
    }
    s.insert("apiKey".to_string(), json!(ENV_KEY));
    out
}

/// The name the translated launches' config file is checked under.
const CONFIG_NAME: &str = "humanify.toml";

fn parsed_record((argv, toml): &Launch) -> Value {
    let file = (!toml.is_empty())
        .then(|| ConfigFile::parse(CONFIG_NAME, toml).unwrap_or_else(|e| panic!("{toml}: {e:?}")));
    match parse_pipeline(argv) {
        PipelineParse::Run { input, opts } => record(&input, &opts, file.as_ref()),
        PipelineParse::Exit { code, stderr, .. } => {
            panic!("{argv:?} did not parse (exit {code}):\n{stderr}")
        }
    }
}

const FIXTURE: &str = "crates/humanify-ag-cli/tests/fixtures/launch-parity.json";

/// A record with every Claude Code version in its strings (`2.1.179`)
/// spelled `<V>`. The captured launches are the TS era's four eval pairs;
/// exp201 added a fifth pair (2.1.179->2.1.181) that no capture could have
/// seen. What this half proves is the CONFIGURATION — which version a path
/// names is pinned byte for byte by the dev crate's launch goldens.
fn without_versions(v: &Value) -> Value {
    match v {
        Value::String(s) => Value::String(mask_versions(s)),
        Value::Array(a) => Value::Array(a.iter().map(without_versions).collect()),
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, x)| (k.clone(), without_versions(x)))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn mask_versions(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("2.1.") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 4..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            out.push_str("2.1.");
        } else {
            out.push_str("<V>");
        }
        rest = &after[digits..];
    }
    out.push_str(rest);
    out
}

/// THE parity gate, half one: every captured launch, its `--api-key`
/// removed and its renamed flags in today's spelling, resolves to the
/// EFFECTIVE settings the commander port gave it.
#[test]
fn every_captured_launch_resolves_to_its_captured_settings() {
    let rows: Vec<Value> = serde_json::from_str(&repo_file(FIXTURE)).expect("the fixture is JSON");
    for row in &rows {
        let argv =
            without_api_key(&serde_json::from_value::<Vec<String>>(row["argv"].clone()).unwrap());
        let launch = to_today(&argv);
        assert_eq!(
            parsed_record(&launch),
            effective(&row["record"]),
            "{argv:?} as {launch:?}"
        );
    }
    assert!(rows.len() >= 50, "the corpus shrank: {}", rows.len());
}

/// Half two: every launch the harness makes TODAY (the re-recorded
/// goldens: argv + its TOML) resolves to the effective settings of a
/// captured launch — the same configuration, spelled as a file.
#[test]
fn every_golden_launch_resolves_to_a_captured_launchs_settings() {
    let rows: Vec<Value> = serde_json::from_str(&repo_file(FIXTURE)).expect("the fixture is JSON");
    let wanted: Vec<Value> = rows
        .iter()
        .map(|r| without_versions(&effective(&r["record"])))
        .collect();
    let launches = golden_corpus();
    assert!(
        launches.len() >= 14,
        "the goldens shrank: {}",
        launches.len()
    );
    // exp171: names became the binary's default, and the harness hands
    // every names launch the full prebuilt index (pairs.json
    // `libraryIndex`) so no run builds one at run time. That one key is the
    // deliberate difference from the captured launches; the record is
    // compared with it taken back out (every other setting must still
    // match a captured launch).
    let pairs: Value =
        serde_json::from_str(&repo_file("dev/eval/pairs.json")).expect("pairs.json is JSON");
    let eval_index = pairs["libraryIndex"].clone();
    assert!(eval_index.is_string(), "pairs.json names the eval's index");
    for launch in launches {
        assert!(
            !launch.1.is_empty(),
            "a golden launch without a config file: {launch:?}"
        );
        let mut got = parsed_record(&launch);
        assert_eq!(
            got["libraryIndex"], eval_index,
            "every golden (default = names) launch is handed the eval's index: {launch:?}"
        );
        got["libraryIndex"] = Value::Null;
        let got = without_versions(&got);
        // exp207: every rebase now writes its LLM call ledger
        // (`--stats-json <v>-rebase.stats.json`), diagnostics only; the
        // captured rebases wrote none. Compared with that key taken back
        // out, after asserting it is exactly the rebase's ledger.
        let mut got = got;
        if got["outputDir"] == "<TMP>/work/<LABEL>/<V>-rebased" {
            assert_eq!(
                got["statsJson"], "<REPO>/dev/eval/results/<LABEL>/<V>-rebase.stats.json",
                "a rebase writes its call ledger beside its log: {launch:?}"
            );
            got["statsJson"] = Value::Null;
        }
        assert!(
            wanted.contains(&got),
            "a golden launch whose settings no captured launch had:\n{launch:?}\n{got:#}"
        );
    }
}

/// Settings in which a recorded launch may differ from every captured
/// launch, by golden — each a combination the launcher makes TODAY that
/// the commander port was never launched with, so there is no capture to
/// compare that key against (every other key must still match one
/// captured launch exactly; each key's own resolution is pinned by the
/// one-row-per-flag captures):
///
/// - `e2e.golden.txt` `split`: the unsplittable fixtures (ES modules,
///   `disambiguation`, `r1b-synthetic`) run their `--sequential` legs
///   UNSPLIT; the captured e2e sequential legs were split ones.
/// - `ask-trace.golden.txt` `split` and `settings.batchSize`: the ask
///   trace grew a split leg, and its sequential prior leg asks in batches
///   of 10 (captured: 25).
const NEW_COMBINATIONS: [(&str, &[&str]); 2] = [
    ("e2e.golden.txt", &["split"]),
    ("ask-trace.golden.txt", &["split", "settings.batchSize"]),
];

/// The keys (`settings.<key>` inside the settings block) in which `got`
/// differs from `captured`, as `key: got vs captured`.
fn differences(got: &Value, captured: &Value) -> Vec<(String, String)> {
    let flat = |v: &Value| -> Vec<(String, Value)> {
        let mut out = Vec::new();
        for (k, x) in v.as_object().into_iter().flatten() {
            match x["Ok"].as_object() {
                Some(inner) if k == "settings" => out.extend(
                    inner
                        .iter()
                        .map(|(ik, iv)| (format!("settings.{ik}"), iv.clone())),
                ),
                _ => out.push((k.clone(), x.clone())),
            }
        }
        out
    };
    let theirs = flat(captured);
    flat(got)
        .into_iter()
        .filter_map(|(k, v)| {
            let other = theirs
                .iter()
                .find(|(tk, _)| *tk == k)
                .map_or(Value::Null, |(_, tv)| tv.clone());
            (other != v).then(|| (k.clone(), format!("{k}: {v} vs {other}")))
        })
        .collect()
}

/// Half three (lane L4, replacing a hand-transcribed list of the e2e,
/// ask-trace, neutrality and integration-test launches that had gone
/// stale): every pipeline launch lane L0 RECORDED from the real launchers
/// (`launch-goldens/`, read live — a new golden is covered the day it is
/// committed) resolves, paths aside, to the effective settings of a
/// captured launch.
#[test]
fn every_recorded_launch_resolves_to_a_captured_launchs_settings() {
    let rows: Vec<Value> = serde_json::from_str(&repo_file(FIXTURE)).expect("the fixture is JSON");
    let wanted: Vec<Value> = rows
        .iter()
        .map(|r| path_free(effective(&r["record"])))
        .collect();
    let launches = recorded_corpus();
    for family in ["e2e", "ask-trace", "neutrality-off", "neutrality-names"] {
        assert!(
            launches
                .iter()
                .any(|(g, _)| g == &format!("{family}.golden.txt")),
            "no recorded pipeline launch in {family}.golden.txt"
        );
    }
    let mut missing = Vec::new();
    for (golden, launch) in &launches {
        let mut got = path_free(parsed_record(launch));
        // exp171, as in half two: a names launch is handed the eval's
        // library index, the one deliberate difference from the captures.
        got["libraryIndex"] = json!(false);
        let allowed: &[&str] = NEW_COMBINATIONS
            .iter()
            .find(|(g, _)| g == golden)
            .map_or(&[], |(_, keys)| keys);
        let diffs: Vec<Vec<(String, String)>> =
            wanted.iter().map(|w| differences(&got, w)).collect();
        if !diffs
            .iter()
            .any(|d| d.iter().all(|(k, _)| allowed.contains(&k.as_str())))
        {
            let nearest = diffs.into_iter().min_by_key(Vec::len).unwrap_or_default();
            let nearest: Vec<String> = nearest.into_iter().map(|(_, d)| d).collect();
            missing.push(format!(
                "{golden}: {launch:?}\n  nearest captured launch differs in {nearest:?}"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "{} of {} recorded launches resolve to settings no captured launch had:\n{}",
        missing.len(),
        launches.len(),
        missing.join("\n")
    );
}
