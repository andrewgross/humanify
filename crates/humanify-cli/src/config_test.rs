//! The configuration owner's contract: one default per knob, every
//! refusal before any work (collected, all at once), the API key from the
//! environment only and never in the record. (Absorbs the deleted
//! `settings_test.rs`; the environment is an injected lookup — the ONE
//! reader stays `crate::env`.)

use crate::config::{
    API_KEY_VAR, Config, DEFAULT_CONCURRENCY, DEFAULT_CONTEXT_TOKENS, DEFAULT_ENDPOINT,
    DEFAULT_MODEL, DEFAULT_MODULE_CONCURRENCY, DEFAULT_OUTPUT_DIR, MISSING_API_KEY,
    ReasoningEffort, setting_keys,
};
use crate::surface::{CommandOptions, PipelineParse, parse_pipeline};
use humanify_core::fast::{FastTier, Levers};
use humanify_core::naming::waves::batch::WaveTunables;
use humanify_model::llm::{DEFAULT_LLM_TIMEOUT_MS, RateLimitConfig};

fn key(name: &str) -> Option<String> {
    (name == API_KEY_VAR).then(|| "env-key".to_string())
}

fn opts(argv: &[&str]) -> (String, CommandOptions) {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    match parse_pipeline(&argv) {
        PipelineParse::Run { input, opts } => (input, *opts),
        PipelineParse::Exit { stderr, .. } => panic!("{argv:?} did not parse: {stderr}"),
    }
}

fn config(argv: &[&str]) -> Config {
    let (input, o) = opts(argv);
    Config::build(&input, &o, key).unwrap_or_else(|e| panic!("{argv:?} refused: {e:?}"))
}

fn refusals(argv: &[&str]) -> Vec<String> {
    let (input, o) = opts(argv);
    Config::build(&input, &o, key).expect_err("must be refused")
}

#[test]
fn every_llm_default_is_the_one_named_default() {
    let c = config(&["in.js"]);
    assert_eq!(c.io.output_dir, DEFAULT_OUTPUT_DIR);
    assert_eq!(c.llm.endpoint, DEFAULT_ENDPOINT);
    assert_eq!(c.llm.model, DEFAULT_MODEL);
    assert_eq!(c.llm.timeout_ms, DEFAULT_LLM_TIMEOUT_MS);
    assert_eq!(c.llm.retries, RateLimitConfig::default().retry_attempts);
    assert_eq!(c.llm.concurrency, DEFAULT_CONCURRENCY);
    assert_eq!(c.llm.module_concurrency, DEFAULT_MODULE_CONCURRENCY);
    assert_eq!(c.llm.context_tokens, DEFAULT_CONTEXT_TOKENS);
    assert_eq!(c.llm.max_tokens, None);
    assert_eq!(c.llm.reasoning_effort, None);
}

#[test]
fn every_naming_default_is_the_one_named_default() {
    let c = config(&["in.js"]);
    assert_eq!(c.naming.tunables(), WaveTunables::default());
    assert_eq!(c.naming.schedule, FastTier::Relaxed(Levers::all()));
    assert!(c.naming.naming_floor && c.naming.naming_floor_sweep);
    assert!(c.library.skip_libraries);
    assert!(
        !c.matching.reconcile_prior_diff,
        "no prior: nothing to reconcile"
    );
}

#[test]
fn typed_values_resolve() {
    let c = config(&[
        "in.js",
        "-c",
        "7",
        "--timeout",
        "1234",
        "--retries",
        "5",
        "--batch-size",
        "9",
        "--max-retries",
        "3",
        "--max-free-retries",
        "2",
        "--lane-threshold",
        "11",
        "--rename-retries",
        "0",
        "--context-tokens",
        "131072",
        "--module-concurrency",
        "33",
        "--reasoning-effort",
        "low",
    ]);
    assert_eq!(c.llm.concurrency, 7);
    assert_eq!(c.llm.timeout_ms, 1234);
    assert_eq!(c.llm.retries, 5);
    assert_eq!(c.naming.batch_size, 9);
    assert_eq!(c.naming.max_retries, 3);
    assert_eq!(c.naming.max_free_retries, Some(2));
    assert_eq!(c.naming.lane_threshold, 11);
    assert_eq!(c.naming.rename_retries, 0);
    assert_eq!(c.llm.context_tokens, 131_072);
    assert_eq!(c.llm.module_concurrency, 33);
    assert_eq!(c.llm.outer_concurrency(), 40);
    assert_eq!(c.llm.reasoning_effort, Some(ReasoningEffort::Low));
}

#[test]
fn the_levers_default_on_and_the_sweep_depends_on_the_floor() {
    let off = config(&["in.js", "--no-naming-floor"]);
    assert!(!off.naming.naming_floor && !off.naming.naming_floor_sweep);
    let sweep_off = config(&["in.js", "--no-naming-floor-sweep"]);
    assert!(sweep_off.naming.naming_floor && !sweep_off.naming.naming_floor_sweep);
    let prior = config(&["in.js", "--prior-version", "p.js"]);
    assert!(prior.matching.reconcile_prior_diff);
    let off = config(&[
        "in.js",
        "--prior-version",
        "p.js",
        "--no-reconcile-prior-diff",
    ]);
    assert!(!off.matching.reconcile_prior_diff);
    assert!(
        !config(&["in.js", "--no-skip-libraries"])
            .library
            .skip_libraries
    );
}

#[test]
fn the_api_key_comes_from_humanify_api_key_only() {
    let c = config(&["in.js"]);
    assert_eq!(c.llm.api_key.expose(), "env-key");
    let (input, o) = opts(&["in.js"]);
    for env in [
        |_: &str| None,
        |n: &str| (n == "OPENAI_API_KEY").then(|| "o".to_string()),
        |n: &str| (n == API_KEY_VAR).then(String::new),
    ] {
        let err = Config::build(&input, &o, env).expect_err("no usable key");
        assert_eq!(err, [MISSING_API_KEY]);
    }
}

#[test]
fn the_api_key_flag_is_retired() {
    let argv: Vec<String> = ["in.js", "--api-key", "k"].map(String::from).to_vec();
    match parse_pipeline(&argv) {
        PipelineParse::Exit { code, stderr, .. } => {
            assert_eq!(code, 1);
            assert!(stderr.contains("'--api-key'"), "{stderr}");
            assert!(stderr.contains("HUMANIFY_API_KEY"), "{stderr}");
        }
        PipelineParse::Run { .. } => panic!("--api-key must be refused"),
    }
}

#[test]
fn the_record_is_the_whole_config_without_the_secret() {
    let c = config(&["in.js", "--split", "--disable", "family-permute"]);
    let record = c.record();
    let text = record.to_string();
    assert!(!text.contains("env-key"), "the secret leaked: {text}");
    assert_eq!(record["llm"]["apiKey"], "<set: HUMANIFY_API_KEY>");
    assert!(!format!("{c:?}").contains("env-key"), "Debug leaks the key");
    assert_eq!(record["split"]["enabled"], true);
    assert_eq!(record["switches"]["disabled"][0], "family-permute");
    assert_eq!(record["naming"]["schedule"]["tier"], "relaxed");
    for section in [
        "io",
        "llm",
        "matching",
        "naming",
        "library",
        "unpack",
        "split",
        "switches",
        "diagnostics",
        "perf",
    ] {
        assert!(record.get(section).is_some(), "record lacks {section}");
    }
    assert!(setting_keys().contains(&"llm.model".to_string()));
}

#[test]
fn every_refusal_is_collected_before_any_work() {
    let errs = refusals(&[
        "in.js",
        "--split-pure",
        "--bundler",
        "foobar",
        "--disable",
        "no-such-switch",
        "-c",
        "0",
    ]);
    assert_eq!(errs[0], "--split-pure requires --split");
    assert!(errs[1].starts_with("--bundler must be one of"), "{errs:?}");
    assert_eq!(errs[2], "--concurrency must be at least 1 (got 0)");
    assert!(
        errs[3].starts_with("--disable: unknown disable switch"),
        "{errs:?}"
    );
    assert_eq!(errs.len(), 4, "{errs:?}");
}

#[test]
fn zero_counts_are_refused() {
    for (flag, name) in [
        ("--module-concurrency", "--module-concurrency"),
        ("--timeout", "--timeout"),
        ("--max-tokens", "--max-tokens"),
        ("--batch-size", "--batch-size"),
        ("--max-retries", "--max-retries"),
    ] {
        let errs = refusals(&["in.js", flag, "0"]);
        assert!(
            errs.contains(&format!("{name} must be at least 1 (got 0)")),
            "{flag}: {errs:?}"
        );
    }
    // Zero is a real setting for these: no retries, no re-asks.
    config(&["in.js", "--retries", "0", "--rename-retries", "0"]);
}

#[test]
fn a_context_window_the_completion_reserve_fills_is_refused() {
    let errs = refusals(&["in.js", "--context-tokens", "4096"]);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].starts_with("--context-tokens (4096) must exceed --max-tokens (6000)"),
        "{errs:?}"
    );
    config(&["in.js", "--context-tokens", "4096", "--max-tokens", "1024"]);
}

#[test]
fn empty_values_and_non_http_endpoints_are_refused() {
    assert_eq!(
        refusals(&["in.js", "--prior-version="]),
        ["--prior-version must not be empty"]
    );
    assert_eq!(
        refusals(&["in.js", "--model="]),
        ["--model must not be empty"]
    );
    assert_eq!(
        refusals(&["in.js", "--endpoint", "localhost:8000/v1"]),
        ["--endpoint must be an http:// or https:// URL (got \"localhost:8000/v1\")"]
    );
}

#[test]
fn a_bad_reasoning_effort_is_a_usage_error() {
    let argv: Vec<String> = ["in.js", "--reasoning-effort", "extreme"]
        .map(String::from)
        .to_vec();
    match parse_pipeline(&argv) {
        PipelineParse::Exit { code, stderr, .. } => {
            assert_eq!(code, 1);
            assert!(stderr.contains("extreme"), "{stderr}");
        }
        PipelineParse::Run { .. } => panic!("an unknown effort must be refused"),
    }
}

#[test]
fn missing_input_files_are_named() {
    let c = config(&[
        "in.js",
        "--split",
        "--prior-version",
        "/nonexistent/prior.js",
        "--split-ledger",
        "/nonexistent/ledger.json",
    ]);
    assert_eq!(
        c.missing_inputs(),
        [
            "--prior-version file not found: /nonexistent/prior.js",
            "--split-ledger file not found: /nonexistent/ledger.json"
        ]
    );
}
