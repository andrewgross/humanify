//! Port of src/commands/settings.test.ts, fixture for fixture. The TS
//! tests mutate `process.env`; here the environment is an injected lookup
//! (the ONE env reader stays crate::env — production passes it).

use crate::settings::{SettingsInput, parse_reasoning_effort, resolve_settings_with};
use crate::util::DEFAULT_LLM_TIMEOUT_MS;

/// The environment a commander-parsed run actually arrives with.
fn cli() -> SettingsInput {
    SettingsInput {
        endpoint: Some("http://cli/v1".into()),
        model: Some("cli-model".into()),
        api_key: Some("cli-key".into()),
        ..SettingsInput::default()
    }
}

fn env_of(vars: &[(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
    let vars: Vec<(&str, &str)> = vars.to_vec();
    move |name: &str| {
        vars.iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.to_string())
    }
}

#[test]
fn prefers_a_cli_value_over_the_environment() {
    let s = resolve_settings_with(&cli(), env_of(&[("HUMANIFY_API_KEY", "env-key")])).unwrap();
    assert_eq!(s.api_key, "cli-key");
}

#[test]
fn falls_back_to_humanify_then_openai_for_the_api_key() {
    let no_key = SettingsInput {
        api_key: None,
        ..cli()
    };
    let h = resolve_settings_with(
        &no_key,
        env_of(&[("HUMANIFY_API_KEY", "h"), ("OPENAI_API_KEY", "o")]),
    )
    .unwrap();
    assert_eq!(h.api_key, "h", "HUMANIFY_ wins over OPENAI_");
    let o = resolve_settings_with(&no_key, env_of(&[("OPENAI_API_KEY", "o")])).unwrap();
    assert_eq!(o.api_key, "o");
}

#[test]
fn resolves_the_llm_timeout_to_the_single_sourced_default() {
    let s = resolve_settings_with(&cli(), env_of(&[])).unwrap();
    assert_eq!(s.timeout, DEFAULT_LLM_TIMEOUT_MS as f64);
}

#[test]
fn parses_numbers_once_into_numbers() {
    let input = SettingsInput {
        concurrency: Some("7".into()),
        timeout: Some("1234".into()),
        retries: Some("5".into()),
        batch_size: Some("9".into()),
        max_retries: Some("3".into()),
        max_free_retries: Some("2".into()),
        lane_threshold: Some("11".into()),
        ..cli()
    };
    let s = resolve_settings_with(&input, env_of(&[])).unwrap();
    assert_eq!(s.concurrency, 7.0);
    assert_eq!(s.timeout, 1234.0);
    assert_eq!(s.retry_attempts, Some(5.0));
    assert_eq!(s.batch_size, Some(9.0));
    assert_eq!(s.max_retries_per_identifier, Some(3.0));
    assert_eq!(s.max_free_retries, Some(2.0));
    assert_eq!(s.lane_threshold, Some(11.0));
}

#[test]
fn leaves_module_concurrency_undefined_so_the_bundler_aware_default_applies() {
    let unset = resolve_settings_with(&cli(), env_of(&[])).unwrap();
    assert_eq!(unset.module_concurrency, None);
    let via_flag = resolve_settings_with(
        &SettingsInput {
            module_concurrency: Some("33".into()),
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap();
    assert_eq!(via_flag.module_concurrency, Some(33.0));
    // The pre-2026-08-12 env var must NOT set it.
    let via_env =
        resolve_settings_with(&cli(), env_of(&[("HUMANIFY_MODULE_CONCURRENCY", "33")])).unwrap();
    assert_eq!(via_env.module_concurrency, None);
}

#[test]
fn defaults_the_levers_on_and_makes_the_sweep_depend_on_the_floor() {
    let on = resolve_settings_with(&cli(), env_of(&[])).unwrap();
    assert!(on.levers.naming_floor);
    assert!(on.levers.naming_floor_sweep);
    let off = resolve_settings_with(
        &SettingsInput {
            naming_floor: Some(false),
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap();
    assert!(!off.levers.naming_floor_sweep);
}

#[test]
fn gates_the_prior_diff_reconcile_on_there_actually_being_a_prior() {
    let no_prior = resolve_settings_with(&cli(), env_of(&[])).unwrap();
    assert!(!no_prior.levers.reconcile_prior_diff);
    let with_prior = resolve_settings_with(
        &SettingsInput {
            prior_version: Some("/some/prior.js".into()),
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap();
    assert!(with_prior.levers.reconcile_prior_diff);
}

#[test]
fn defaults_skip_libraries_on() {
    assert!(
        resolve_settings_with(&cli(), env_of(&[]))
            .unwrap()
            .skip_libraries
    );
    let off = resolve_settings_with(
        &SettingsInput {
            skip_libraries: Some(false),
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap();
    assert!(!off.skip_libraries);
}

// "is frozen, so nothing downstream can re-decide a setting": the Rust
// Settings has no `&mut` API and is passed by shared reference — the
// borrow checker is the freeze (no runtime test can express it).

#[test]
fn reports_a_missing_api_key_instead_of_resolving_to_undefined() {
    let err = resolve_settings_with(
        &SettingsInput {
            api_key: None,
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap_err();
    assert!(err.to_lowercase().contains("api key"), "{err}");
    assert_eq!(
        err,
        "API key required. Provide --api-key, or set HUMANIFY_API_KEY or \
         OPENAI_API_KEY environment variable."
    );
}

// ---- beyond the TS unit tests: the throw ORDER and messages the scenario
// gate (test/parity/wpb4-scenarios.json) compares end to end ----

#[test]
fn an_empty_api_key_is_missing_like_the_ts_falsy_check() {
    // `if (!apiKey)` — "" is falsy; `??` does NOT fall through on "".
    let err = resolve_settings_with(
        &SettingsInput {
            api_key: Some(String::new()),
            ..cli()
        },
        env_of(&[("HUMANIFY_API_KEY", "h")]),
    )
    .unwrap_err();
    assert!(err.starts_with("API key required"), "{err}");
}

#[test]
fn empty_endpoint_or_model_is_the_cli_layer_error() {
    for input in [
        SettingsInput {
            model: Some(String::new()),
            ..cli()
        },
        SettingsInput {
            endpoint: Some(String::new()),
            ..cli()
        },
    ] {
        let err = resolve_settings_with(&input, env_of(&[])).unwrap_err();
        assert!(
            err.starts_with("endpoint and model must be supplied"),
            "{err}"
        );
    }
}

#[test]
fn numbers_are_parsed_with_parse_int_semantics_in_field_order() {
    // timeout is evaluated before concurrency in the TS object literal.
    let err = resolve_settings_with(
        &SettingsInput {
            timeout: Some("x".into()),
            concurrency: Some("abc".into()),
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap_err();
    assert_eq!(err, "Invalid number: x");
    let s = resolve_settings_with(
        &SettingsInput {
            concurrency: Some(" 12abc".into()),
            ..cli()
        },
        env_of(&[]),
    )
    .unwrap();
    assert_eq!(s.concurrency, 12.0, "parseInt stops at the first non-digit");
}

#[test]
fn reasoning_effort_rejects_anything_outside_the_enum() {
    assert_eq!(parse_reasoning_effort(None).unwrap(), None);
    assert_eq!(parse_reasoning_effort(Some("low")).unwrap(), Some("low"));
    assert_eq!(
        parse_reasoning_effort(Some("extreme")).unwrap_err(),
        "invalid reasoning effort \"extreme\" — expected one of low, medium, high"
    );
}
