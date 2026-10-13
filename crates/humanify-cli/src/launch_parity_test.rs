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

use serde_json::{Value, json};

use crate::config::{API_KEY_VAR, Config, DEFAULT_CONTEXT_TOKENS, DEFAULT_MODULE_CONCURRENCY};
use crate::pipeline_config::enum_name;
use crate::surface::{CommandOptions, PipelineParse, parse_pipeline};
use humanify_core::naming::waves::batch::WaveTunables;

fn repo_file(rel: &str) -> String {
    let path = format!("{}/../../{rel}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

const GOLDENS: [&str; 2] = [
    "experiments/034-eval-harness/run-launch.golden.txt",
    "experiments/034-eval-harness/run-launch.seeded-base.golden.txt",
];

/// Every launch in a launch golden: the `humanify NODE_OPTIONS=... :: <argv>`
/// lines and each run config's `args`.
fn golden_launches(text: &str) -> Vec<Vec<String>> {
    let mut out = Vec::new();
    let mut block: Option<String> = None;
    for line in text.lines() {
        if let Some(b) = block.as_mut() {
            b.push_str(line);
            b.push('\n');
            if line == "}" {
                let cfg: Value = serde_json::from_str(b).expect("a run config is JSON");
                let args = cfg["args"].as_array().expect("a run config has args");
                out.push(
                    args.iter()
                        .map(|a| a.as_str().unwrap().to_string())
                        .collect(),
                );
                block = None;
            }
        } else if line.starts_with("humanify ") {
            let (_, argv) = line.split_once(" :: ").expect("a launch line has ' :: '");
            out.push(argv.split_whitespace().map(str::to_string).collect());
        } else if line.starts_with("runcfg-") && line.ends_with(": {") {
            block = Some("{\n".to_string());
        }
    }
    out
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

/// The launches outside the goldens, transcribed from their launchers.
fn transcribed_launches() -> Vec<Vec<String>> {
    let e2e = "--endpoint http://127.0.0.1:41000/v1 --retries 0";
    let run = "in.js --split --endpoint http://127.0.0.1:9/v1 --model openai/gpt-oss-20b \
               --reasoning-effort low -c 472 -o out";
    let test = "--endpoint http://127.0.0.1:9/v1 --retries 0";
    [
        // scripts/e2e.ts runBinary: fresh / prior / sequential legs.
        format!("v1.js --split --stats-json fresh.stats.json -o fresh {e2e}"),
        format!("v1.js -o fresh {e2e}"),
        format!("v2.js --prior-version prior.js --split -o prior {e2e}"),
        format!("v2.js --prior-version prior.js -o prior {e2e}"),
        format!("v1.js --split --sequential --batch-size 10 -o seq {e2e}"),
        format!("v2.js --prior-version prior.js --sequential --batch-size 10 --split -o seq {e2e}"),
        // scripts/ask-trace.ts.
        "in.js -o out --dump-asks asks.jsonl --retries 0 \
         --endpoint http://127.0.0.1:41001/v1 --batch-size 25"
            .to_string(),
        "in.js -o out --dump-asks asks.jsonl --retries 0 \
         --endpoint http://127.0.0.1:41001/v1 --batch-size 25 --prior-version p.js --sequential"
            .to_string(),
        // run.sh / eval --pipeline-arg (appended to every launch).
        format!("{run} -vv --log-file l.log --sequential"),
        format!("{run} --prior-version p.js --disable family-permute"),
        format!("{run} --prior-version p.js --disable family-permute,content-anchor"),
        format!("{run} --prior-version p.js --probe shingle-probe"),
        format!("{run} --prior-version p.js --batch-size 10"),
        // neutrality.sh legs.
        format!("{run} --llm-cache /work/neutrality-cache --prior-version p.js"),
        // crates/humanify-cli/tests: pipeline_stages, replay_determinism,
        // provider_errors, ask_trace.
        format!("in.js -o out {test}"),
        format!("in.js --split -o out {test}"),
        format!("in.js --llm-cache cache -o out {test}"),
        format!("in.js --no-skip-libraries -o out {test}"),
        format!("in.js --prior-version p.js -o out {test}"),
        format!("in.js --split --prior-version p.js --perf-trace t.json -o out {test}"),
        "in.js --model stub-model -c 8 --retries 0 --endpoint http://e/v1 -o out".to_string(),
        "in.js --model stub-model -c 4 --retries 0 --endpoint http://e/v1 -o out".to_string(),
        "in.js --split --model stub-model -c 4 --retries 0 --endpoint http://e/v1 \
         --stats-json s.json -o out"
            .to_string(),
        "in.js --model stub-model --retries 0 --endpoint http://e/v1 -o out \
         --dump-asks a.jsonl --dump-artifacts art --rename-retries 1"
            .to_string(),
        // One row per remaining flag, so every option's resolution is pinned.
        "in.js --skip-libraries".to_string(),
        "in.js --no-naming-floor".to_string(),
        "in.js --no-naming-floor-sweep".to_string(),
        "in.js --prior-version p.js --no-reconcile-prior-diff".to_string(),
        "in.js --relaxed-levers window-lanes".to_string(),
        "in.js --split --split-pure --split-ledger l.json".to_string(),
        "in.js --split-pure".to_string(),
        "in.js --bundler bun --minifier bun".to_string(),
        "in.js --bundler foobar".to_string(),
        "in.js --max-tokens 4096 --context-tokens 131072 --module-concurrency 33".to_string(),
        "in.js --timeout 1234 --max-retries 3 --max-free-retries 7 --lane-threshold 11".to_string(),
        "in.js --rename-ledger rl --diagnostics d.json --library-index idx \
         --library-discovery-units --simulate-llm-latency sim.json"
            .to_string(),
        "in.js -v --reasoning-effort high -m some-model --output-dir o".to_string(),
        "in.js --endpoint=http://eq/v1 --concurrency=9".to_string(),
        "in.js".to_string(),
    ]
    .iter()
    .map(|s| words(s))
    .collect()
}

pub(crate) fn corpus() -> Vec<Vec<String>> {
    let mut all: Vec<Vec<String>> = GOLDENS
        .iter()
        .flat_map(|g| golden_launches(&repo_file(g)))
        .collect();
    all.extend(transcribed_launches());
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
        "skipLibraries": c.library.skip_libraries,
        "namingFloor": g.naming_floor,
        "namingFloorSweep": g.naming_floor_sweep,
        "reconcilePriorDiff": c.matching.reconcile_prior_diff,
    })
}

/// Everything a run is configured by, from one parsed argv, through the
/// ONE resolver (`Config::build`). A refused configuration records its
/// refusals where the commander-era record kept the flag-invariant
/// verdict.
pub(crate) fn record(input: &str, opts: &CommandOptions) -> Value {
    let env = |name: &str| (name == API_KEY_VAR).then(|| ENV_KEY.to_string());
    let c = match Config::build(input, opts, env) {
        Ok(c) => c,
        Err(refusals) => return json!({ "input": input, "invariants": refusals }),
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
        "libraryDiscoveryUnits": c.library.discovery_units,
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
    if invariants.as_array().is_some_and(|a| !a.is_empty()) {
        return json!({ "input": captured["input"], "invariants": invariants });
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

fn parsed_record(argv: &[String]) -> Value {
    match parse_pipeline(argv) {
        PipelineParse::Run { input, opts } => record(&input, &opts),
        PipelineParse::Exit { code, stderr, .. } => {
            panic!("{argv:?} did not parse (exit {code}):\n{stderr}")
        }
    }
}

const FIXTURE: &str = "crates/humanify-cli/tests/fixtures/launch-parity.json";

/// The capture. It ran ONCE against the commander port at 587e211f
/// (`cargo test -p humanify-cli capture_launch_parity -- --ignored`); the
/// port is gone, so a re-capture now records the clap surface — run it
/// only to add a new launch shape deliberately, and review the diff (an
/// existing row must not change).
#[test]
#[ignore = "writes the fixture; run only to re-capture deliberately"]
fn capture_launch_parity() {
    let rows: Vec<Value> = corpus()
        .iter()
        .map(|argv| json!({ "argv": argv, "record": parsed_record(argv) }))
        .collect();
    let path = format!("{}/../../{FIXTURE}", env!("CARGO_MANIFEST_DIR"));
    let text = serde_json::to_string_pretty(&rows).unwrap() + "\n";
    std::fs::write(&path, text).unwrap();
}

/// THE parity gate: every captured launch, its `--api-key` removed, parses
/// to the EFFECTIVE settings the commander port gave it, and every launch
/// in the goldens is captured.
#[test]
fn every_harness_launch_parses_to_its_captured_settings() {
    let rows: Vec<Value> = serde_json::from_str(&repo_file(FIXTURE)).expect("the fixture is JSON");
    let captured: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            without_api_key(&serde_json::from_value::<Vec<String>>(r["argv"].clone()).unwrap())
        })
        .collect();
    for argv in corpus() {
        assert!(
            captured.contains(&argv),
            "a launch with no captured settings (add it to the fixture deliberately): {argv:?}"
        );
    }
    for (argv, row) in captured.iter().zip(&rows) {
        assert_eq!(parsed_record(argv), effective(&row["record"]), "{argv:?}");
    }
    assert!(rows.len() >= 50, "the corpus shrank: {}", rows.len());
}
