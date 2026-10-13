//! The configuration owner's contract: one default per knob, every
//! refusal before any work (collected, all at once), the API key from the
//! environment only and never in the record. (Absorbs the deleted
//! `settings_test.rs`; the environment is an injected lookup — the ONE
//! reader stays `crate::env`.)

use crate::config::{
    API_KEY_VAR, Config, ConfigFile, DEFAULT_CONCURRENCY, DEFAULT_CONTEXT_TOKENS, DEFAULT_ENDPOINT,
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

/// A checked config file from TOML text (named `cfg.toml`).
pub(crate) fn file(toml: &str) -> ConfigFile {
    ConfigFile::check("cfg.toml", toml).unwrap_or_else(|e| panic!("{toml}\nrefused: {e:?}"))
}

fn config(argv: &[&str]) -> Config {
    with_file(argv, "")
}

fn with_file(argv: &[&str], toml: &str) -> Config {
    let (input, o) = opts(argv);
    Config::build(&input, &o, Some(&file(toml)), key)
        .unwrap_or_else(|e| panic!("{argv:?} {toml} refused: {e:?}"))
}

fn refusals(argv: &[&str], toml: &str) -> Vec<String> {
    let (input, o) = opts(argv);
    Config::build(&input, &o, Some(&file(toml)), key).expect_err("must be refused")
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
    // Config sweep step 8: the prompt's code window is a key whose default
    // IS the code's constant.
    assert_eq!(
        c.naming.code_window_lines,
        humanify_core::naming::code_window::MAX_CODE_LINES
    );
    assert_eq!(c.naming.schedule, FastTier::Relaxed(Levers::all()));
    assert!(c.naming.naming_floor && c.naming.naming_floor_sweep);
    assert!(c.library.skips_libraries());
    assert_eq!(c.library.matching, crate::config::LibraryMatching::Off);
    assert!(
        !c.matching.reconcile_prior_diff,
        "no prior: nothing to reconcile"
    );
}

#[test]
fn typed_values_resolve() {
    let c = with_file(
        &["in.js"],
        "[llm]\nconcurrency = 7\ntimeout_ms = 1234\nretries = 5\ncontext_tokens = 131072\n\
         module_concurrency = 33\nreasoning_effort = \"low\"\n\
         [naming]\nbatch_size = 9\nmax_retries = 3\nmax_free_retries = 2\n\
         lane_threshold = 11\nrename_retries = 0\ncode_window_lines = 120\n",
    );
    assert_eq!(c.llm.concurrency, 7);
    assert_eq!(c.llm.timeout_ms, 1234);
    assert_eq!(c.llm.retries, 5);
    assert_eq!(c.naming.batch_size, 9);
    assert_eq!(c.naming.max_retries, 3);
    assert_eq!(c.naming.max_free_retries, Some(2));
    assert_eq!(c.naming.lane_threshold, 11);
    assert_eq!(c.naming.rename_retries, 0);
    assert_eq!(c.naming.code_window_lines, 120);
    assert_eq!(c.naming.tunables().code_window_lines, 120);
    assert_eq!(c.llm.context_tokens, 131_072);
    assert_eq!(c.llm.module_concurrency, 33);
    assert_eq!(c.llm.outer_concurrency(), 40);
    assert_eq!(c.llm.reasoning_effort, Some(ReasoningEffort::Low));
}

#[test]
fn the_levers_default_on_and_the_sweep_depends_on_the_floor() {
    let off = with_file(&["in.js"], "[naming]\nnaming_floor = false\n");
    assert!(!off.naming.naming_floor && !off.naming.naming_floor_sweep);
    let sweep_off = with_file(&["in.js"], "[naming]\nnaming_floor_sweep = false\n");
    assert!(sweep_off.naming.naming_floor && !sweep_off.naming.naming_floor_sweep);
    let prior = config(&["in.js", "--prior-version", "p.js"]);
    assert!(prior.matching.reconcile_prior_diff);
    let off = with_file(
        &["in.js", "--prior-version", "p.js"],
        "[matching]\nreconcile_prior_diff = false\n",
    );
    assert!(!off.matching.reconcile_prior_diff);
    assert!(
        !with_file(&["in.js"], "[library]\nunnamed = \"model\"\n")
            .library
            .skips_libraries()
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
        let err = Config::build(&input, &o, None, env).expect_err("no usable key");
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
    let c = with_file(
        &["in.js"],
        "[split]\nenabled = true\n[switches]\ndisable = [\"family-permute\"]\n",
    );
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
    let (input, o) = opts(&["in.js", "--endpoint", "http://a/v1"]);
    let f = ConfigFile::parse(
        "cfg.toml",
        "[llm]\nendpoint = \"http://b/v1\"\nconcurrency = 0\n[unpack]\nbundler = \"foobar\"\n\
         [split]\npure = true\n[switches]\ndisable = [\"no-such-switch\"]\n",
    )
    .expect("it is TOML");
    let errs = Config::build(&input, &o, Some(&f), key).expect_err("refused");
    assert_eq!(
        errs[0],
        "cfg.toml: llm.concurrency must be at least 1 (got 0)"
    );
    assert_eq!(
        errs[1],
        "cfg.toml: unpack.bundler must be one of: webpack, browserify, rollup, esbuild, \
         parcel, bun (got \"foobar\")"
    );
    assert_eq!(
        errs[2],
        "config conflict: llm.endpoint = \"http://a/v1\" (--endpoint) vs \"http://b/v1\" \
         (cfg.toml)"
    );
    assert_eq!(errs[3], "split.pure requires split.enabled");
    assert!(
        errs[4].starts_with("switches.disable: unknown disable switch"),
        "{errs:?}"
    );
    assert_eq!(errs.len(), 5, "{errs:?}");
}

#[test]
fn zero_counts_are_refused() {
    for k in [
        "llm.module_concurrency",
        "llm.timeout_ms",
        "llm.max_tokens",
        "naming.batch_size",
        "naming.max_retries",
    ] {
        let (section, name) = k.split_once('.').unwrap();
        let errs = ConfigFile::check("cfg.toml", &format!("[{section}]\n{name} = 0\n"))
            .expect_err("refused");
        assert_eq!(errs, [format!("cfg.toml: {k} must be at least 1 (got 0)")]);
    }
    // Zero is a real setting for these: no retries, no re-asks.
    with_file(
        &["in.js"],
        "[llm]\nretries = 0\n[naming]\nrename_retries = 0\n",
    );
}

#[test]
fn a_context_window_the_completion_reserve_fills_is_refused() {
    let errs = refusals(&["in.js"], "[llm]\ncontext_tokens = 4096\n");
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].starts_with("llm.context_tokens (4096) must exceed llm.max_tokens (6000)"),
        "{errs:?}"
    );
    with_file(
        &["in.js"],
        "[llm]\ncontext_tokens = 4096\nmax_tokens = 1024\n",
    );
}

#[test]
fn empty_values_and_non_http_endpoints_are_refused() {
    assert_eq!(
        refusals(&["in.js", "--prior-version="], ""),
        ["--prior-version must not be empty"]
    );
    assert_eq!(
        refusals(&["in.js", "--model="], ""),
        ["--model must not be empty"]
    );
    assert_eq!(
        refusals(&["in.js", "--endpoint", "localhost:8000/v1"], ""),
        ["--endpoint must be an http:// or https:// URL (got \"localhost:8000/v1\")"]
    );
    assert_eq!(
        ConfigFile::check("cfg.toml", "[llm]\nendpoint = \"localhost:8000/v1\"\n")
            .expect_err("refused"),
        ["cfg.toml: llm.endpoint must be an http:// or https:// URL (got \"localhost:8000/v1\")"]
    );
}

#[test]
fn a_bad_reasoning_effort_is_refused() {
    let errs = ConfigFile::check("cfg.toml", "[llm]\nreasoning_effort = \"extreme\"\n")
        .expect_err("refused");
    assert_eq!(
        errs,
        ["cfg.toml: llm.reasoning_effort must be one of: low, medium, high (got \"extreme\")"]
    );
}

#[test]
fn missing_input_files_are_named() {
    let c = with_file(
        &["in.js", "--prior-version", "/nonexistent/prior.js"],
        "[split]\nenabled = true\nledger = \"/nonexistent/ledger.json\"\n",
    );
    assert_eq!(
        c.missing_inputs(),
        [
            "prior version (--prior-version) file not found: /nonexistent/prior.js",
            "split.ledger file not found: /nonexistent/ledger.json"
        ]
    );
}

#[test]
fn a_zero_code_window_is_refused() {
    let err = ConfigFile::check("cfg.toml", "[naming]\ncode_window_lines = 0\n")
        .expect_err("a window must show code");
    assert_eq!(err.len(), 1, "{err:?}");
    assert!(err[0].contains("naming.code_window_lines"), "{err:?}");
}
