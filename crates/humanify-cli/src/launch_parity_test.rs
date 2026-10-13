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

use serde_json::{Value, json};

use crate::kill_switches::SwitchState;
use crate::settings::{Settings, resolve_settings_with};
use crate::surface::{PipelineParse, parse_pipeline};
use crate::unified::{CommandOptions, check_flag_invariants};

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
    let e2e = "--endpoint http://127.0.0.1:41000/v1 --api-key e2e --retries 0";
    let run = "in.js --split --endpoint http://127.0.0.1:9/v1 --model openai/gpt-oss-20b \
               --api-key local --reasoning-effort low -c 472 -o out";
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
        "in.js -o out --dump-asks asks.jsonl --api-key k --retries 0 \
         --endpoint http://127.0.0.1:41001/v1 --batch-size 25"
            .to_string(),
        "in.js -o out --dump-asks asks.jsonl --api-key k --retries 0 \
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
        format!("in.js --api-key k -o out {test}"),
        format!("in.js --api-key k --split -o out {test}"),
        format!("in.js --api-key k --llm-cache cache -o out {test}"),
        format!("in.js --api-key k --no-skip-libraries -o out {test}"),
        format!("in.js --api-key k --prior-version p.js -o out {test}"),
        format!("in.js --api-key k --split --prior-version p.js --perf-trace t.json -o out {test}"),
        "in.js --api-key k --model stub-model -c 8 --retries 0 --endpoint http://e/v1 -o out"
            .to_string(),
        "in.js --api-key k --model stub-model -c 4 --retries 0 --endpoint http://e/v1 -o out"
            .to_string(),
        "in.js --split --api-key k --model stub-model -c 4 --retries 0 --endpoint http://e/v1 \
         --stats-json s.json -o out"
            .to_string(),
        "in.js --api-key k --model stub-model --retries 0 --endpoint http://e/v1 -o out \
         --dump-asks a.jsonl --dump-artifacts art --rename-retries 1"
            .to_string(),
        // One row per remaining flag, so every option's resolution is pinned.
        "in.js --api-key k --skip-libraries".to_string(),
        "in.js --api-key k --no-naming-floor".to_string(),
        "in.js --api-key k --no-naming-floor-sweep".to_string(),
        "in.js --api-key k --prior-version p.js --no-reconcile-prior-diff".to_string(),
        "in.js --api-key k --relaxed-levers window-lanes".to_string(),
        "in.js --api-key k --split --split-pure --split-ledger l.json".to_string(),
        "in.js --api-key k --split-pure".to_string(),
        "in.js --api-key k --bundler bun --minifier bun".to_string(),
        "in.js --api-key k --bundler foobar".to_string(),
        "in.js --api-key k --max-tokens 4096 --context-tokens 131072 --module-concurrency 33"
            .to_string(),
        "in.js --api-key k --timeout 1234 --max-retries 3 --max-free-retries 7 --lane-threshold 11"
            .to_string(),
        "in.js --api-key k --rename-ledger rl --diagnostics d.json --library-index idx \
         --library-discovery-units --simulate-llm-latency sim.json"
            .to_string(),
        "in.js --api-key k -v --reasoning-effort high -m some-model --output-dir o".to_string(),
        "in.js --api-key k --endpoint=http://eq/v1 --concurrency=9".to_string(),
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

fn settings_json(s: &Settings) -> Value {
    json!({
        "endpoint": s.endpoint, "model": s.model, "apiKey": s.api_key,
        "timeout": s.timeout, "maxTokens": s.max_tokens,
        "contextTokens": s.context_tokens, "reasoningEffort": s.reasoning_effort,
        "llmCacheDir": s.llm_cache_dir, "concurrency": s.concurrency,
        "moduleConcurrency": s.module_concurrency, "retryAttempts": s.retry_attempts,
        "batchSize": s.batch_size, "maxRetriesPerIdentifier": s.max_retries_per_identifier,
        "maxFreeRetries": s.max_free_retries, "laneThreshold": s.lane_threshold,
        "renameRetries": s.rename_retries, "skipLibraries": s.skip_libraries,
        "namingFloor": s.levers.naming_floor,
        "namingFloorSweep": s.levers.naming_floor_sweep,
        "reconcilePriorDiff": s.levers.reconcile_prior_diff,
    })
}

/// Everything a run is configured by, from one parsed argv. The API key the
/// environment would supply is a fixed stand-in (the parse is what is
/// compared, not the environment).
pub(crate) fn record(input: &str, opts: &CommandOptions) -> Value {
    let env = |name: &str| (name == "HUMANIFY_API_KEY").then(|| "env-key".to_string());
    let settings = resolve_settings_with(&opts.settings_input(), env);
    let list = |v: &Option<String>| -> Vec<String> {
        v.as_deref()
            .map(|s| s.split(',').map(str::to_string).collect())
            .unwrap_or_default()
    };
    let switches = SwitchState::configure(&list(&opts.disable), &list(&opts.probe));
    json!({
        "input": input,
        "settings": settings.as_ref().map(settings_json).map_err(Clone::clone),
        "fastTier": format!("{:?}", opts.fast_tier()),
        "switches": format!("{switches:?}"),
        "invariants": check_flag_invariants(opts),
        "verbose": opts.verbose,
        "outputDir": opts.output_dir,
        "split": opts.split,
        "splitPure": opts.split_pure,
        "splitLedger": opts.split_ledger,
        "bundler": opts.bundler,
        "minifier": opts.minifier,
        "priorVersion": opts.prior_version,
        "disable": opts.disable,
        "probe": opts.probe,
        "logFile": opts.log_file,
        "diagnostics": opts.diagnostics,
        "libraryIndex": opts.library_index,
        "libraryDiscoveryUnits": opts.library_discovery_units,
        "perfTrace": opts.perf_trace,
        "renameLedger": opts.rename_ledger,
        "statsJson": opts.stats_json,
        "dumpArtifacts": opts.dump_artifacts,
        "dumpAsks": opts.dump_asks,
        "simulateLlmLatency": opts.simulate_llm_latency,
    })
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

/// THE parity gate: every captured launch parses to the settings the
/// commander port gave it, and every launch in the goldens is captured.
#[test]
fn every_harness_launch_parses_to_its_captured_settings() {
    let rows: Vec<Value> = serde_json::from_str(&repo_file(FIXTURE)).expect("the fixture is JSON");
    let captured: Vec<Vec<String>> = rows
        .iter()
        .map(|r| serde_json::from_value(r["argv"].clone()).unwrap())
        .collect();
    for argv in corpus() {
        assert!(
            captured.contains(&argv),
            "a launch with no captured settings (add it to the fixture deliberately): {argv:?}"
        );
    }
    for (argv, row) in captured.iter().zip(&rows) {
        assert_eq!(parsed_record(argv), row["record"], "{argv:?}");
    }
    assert!(rows.len() >= 50, "the corpus shrank: {}", rows.len());
}
