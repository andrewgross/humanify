//! The `--config` file's contract (config sweep step 5; docs/config-
//! consolidation.md §3.1): every refusal is loud, names the key (and for a
//! conflict both sources and both values), and happens before any work;
//! the example file is the registry, byte for byte; the record carries the
//! file's path and hash.

use crate::config::file::{RETIRED_KEYS, Source, render_example};
use crate::config::keys::KEYS;
use crate::config::{API_KEY_VAR, Config, ConfigFile};
use crate::config_test::file;
use crate::surface::{CommandOptions, MOVED_FLAGS, PipelineParse, parse_pipeline};

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

fn build(argv: &[&str], toml: &str) -> Result<Config, Vec<String>> {
    let (input, o) = opts(argv);
    Config::build(&input, &o, Some(&file(toml)), key)
}

fn parse_refusals(toml: &str) -> Vec<String> {
    ConfigFile::check("cfg.toml", toml).expect_err("must be refused")
}

#[test]
fn a_flag_and_the_file_that_disagree_are_refused_naming_both() {
    let errs = build(
        &["in.js", "--endpoint", "http://a/v1"],
        "[llm]\nendpoint = \"http://b/v1\"\n",
    )
    .expect_err("a conflict");
    assert_eq!(
        errs,
        [
            "config conflict: llm.endpoint = \"http://a/v1\" (--endpoint) vs \"http://b/v1\" (cfg.toml)"
        ]
    );
    let errs = build(&["in.js", "-vv"], "[diagnostics]\nverbosity = 1\n").expect_err("a conflict");
    assert_eq!(
        errs,
        ["config conflict: diagnostics.verbosity = 2 (-v) vs 1 (cfg.toml)"]
    );
    for (flag, toml) in [
        ("--model", "[llm]\nmodel = \"b\"\n"),
        ("--output-dir", "[io]\noutput_dir = \"b\"\n"),
        ("--prior-version", "[matching]\nprior_version = \"b\"\n"),
    ] {
        let errs = build(&["in.js", flag, "a"], toml).expect_err("a conflict");
        assert!(
            errs[0].contains(&format!("\"a\" ({flag}) vs \"b\" (cfg.toml)")),
            "{errs:?}"
        );
    }
}

#[test]
fn a_flag_and_the_file_that_agree_are_accepted_and_both_recorded() {
    let c = build(
        &["in.js", "--model", "m", "-vv"],
        "[llm]\nmodel = \"m\"\nconcurrency = 9\n[diagnostics]\nverbosity = 2\n",
    )
    .expect("agreeing sources are fine");
    assert_eq!(c.llm.model, "m");
    assert_eq!(c.sources["llm.model"], Source::Both.name());
    assert_eq!(c.sources["diagnostics.verbosity"], "flag+file");
    assert_eq!(c.sources["llm.concurrency"], "file");
    assert!(
        !c.sources.contains_key("llm.endpoint"),
        "a default has no source"
    );
}

#[test]
fn an_unknown_key_is_refused_with_a_suggestion() {
    assert_eq!(
        parse_refusals("[naming]\nbatchsize = 25\n"),
        ["cfg.toml: unknown key naming.batchsize (did you mean naming.batch_size?)"]
    );
    assert_eq!(
        parse_refusals("[nonsense]\nx = 1\n"),
        ["cfg.toml: unknown key nonsense"]
    );
    assert_eq!(parse_refusals("top = 1\n"), ["cfg.toml: unknown key top"]);
    // A section where a key belongs, and a key where a section belongs.
    assert_eq!(
        parse_refusals("naming = 3\n"),
        ["cfg.toml: unknown key naming"]
    );
}

#[test]
fn a_wrong_type_is_refused_naming_the_key() {
    assert_eq!(
        parse_refusals("[naming]\nbatch_size = \"25x\"\n"),
        ["cfg.toml: naming.batch_size must be an integer, got \"25x\""]
    );
    assert_eq!(
        parse_refusals("[split]\nenabled = \"yes\"\n"),
        ["cfg.toml: split.enabled must be a boolean (true or false), got \"yes\""]
    );
    assert_eq!(
        parse_refusals("[switches]\ndisable = \"family-permute\"\n"),
        ["cfg.toml: switches.disable must be a list of strings, got \"family-permute\""]
    );
    assert_eq!(
        parse_refusals("[llm]\nretries = -1\n"),
        ["cfg.toml: llm.retries = -1 is negative"]
    );
}

#[test]
fn an_out_of_range_value_is_refused_naming_the_key() {
    assert_eq!(
        parse_refusals("[diagnostics]\nverbosity = 3\n"),
        ["cfg.toml: diagnostics.verbosity = 3 is outside 0..=2"]
    );
    assert_eq!(
        parse_refusals("[llm]\nconcurrency = 5000000000\n"),
        ["cfg.toml: llm.concurrency = 5000000000 is outside 1..=4294967295"]
    );
    assert_eq!(
        parse_refusals("[naming]\nschedule = \"fast\"\n"),
        ["cfg.toml: naming.schedule must be one of: relaxed, sequential (got \"fast\")"]
    );
    assert_eq!(
        parse_refusals("[naming]\nrelaxed_levers = [\"bogus\"]\n"),
        [
            "cfg.toml: naming.relaxed_levers must be one of: window-lanes, defer-shadowed \
             (got \"bogus\")"
        ]
    );
    assert_eq!(
        parse_refusals("[diagnostics]\nlog_file = \"\"\n"),
        ["cfg.toml: diagnostics.log_file must not be empty"]
    );
}

#[test]
fn a_retired_key_is_refused_naming_its_replacement() {
    assert!(!RETIRED_KEYS.is_empty());
    for (dotted, note) in RETIRED_KEYS {
        let (section, name) = dotted.rsplit_once('.').unwrap();
        let errs = parse_refusals(&format!("[{section}]\n{name} = \"x\"\n"));
        assert_eq!(errs, [format!("cfg.toml: {dotted} is retired ({note})")]);
    }
    assert!(parse_refusals("[llm]\napi_key = \"sk\"\n")[0].contains("HUMANIFY_API_KEY"));
}

#[test]
fn an_unknown_switch_name_is_refused_listing_the_valid_set() {
    let errs = build(&["in.js"], "[switches]\nprobe = [\"family-permute\"]\n")
        .expect_err("a disable switch is not a probe");
    assert!(
        errs[0].starts_with("switches.probe: unknown probe switch \"family-permute\" — valid:"),
        "{errs:?}"
    );
}

#[test]
fn a_file_that_is_not_toml_or_not_there_is_refused() {
    let errs = parse_refusals("[naming\nbatch_size = 3\n");
    assert_eq!(errs.len(), 1);
    assert!(
        errs[0].starts_with("cfg.toml: not valid TOML: "),
        "{errs:?}"
    );
    let errs = ConfigFile::read("/nonexistent/humanify.toml").expect_err("missing");
    assert!(
        errs[0].starts_with("--config /nonexistent/humanify.toml: cannot read the config file"),
        "{errs:?}"
    );
    // Duplicate keys are not TOML.
    assert!(parse_refusals("[llm]\nretries = 1\nretries = 2\n")[0].contains("not valid TOML"));
}

#[test]
fn every_problem_in_a_file_is_reported_at_once() {
    let errs = parse_refusals(
        "[llm]\nconcurrency = 0\nretries = \"x\"\nbogus = 1\n[perf]\nprofile = \"p\"\n",
    );
    assert_eq!(errs.len(), 4, "{errs:?}");
}

#[test]
fn the_record_carries_the_files_path_and_hash() {
    let toml = "[llm]\nconcurrency = 9\n";
    let c = build(&["in.js"], toml).unwrap();
    let r = c.record();
    assert_eq!(r["configFile"]["path"], "cfg.toml");
    // sha256 of the bytes.
    use sha2::{Digest, Sha256};
    let want = format!("{:x}", Sha256::digest(toml.as_bytes()));
    assert_eq!(r["configFile"]["sha256"], want.as_str());
    assert_eq!(r["sources"]["llm.concurrency"], "file");
    let (input, o) = opts(&["in.js"]);
    let none = Config::build(&input, &o, None, key).unwrap();
    assert!(none.record()["configFile"].is_null());
}

// --- the moved flags ------------------------------------------------------

#[test]
fn every_moved_flag_is_refused_naming_its_key() {
    for (flag, key, _) in MOVED_FLAGS {
        assert!(
            KEYS.iter().any(|k| k.dotted() == *key),
            "{flag} names {key}, which is not a setting"
        );
        let argv: Vec<String> = ["in.js", flag, "x"].map(String::from).to_vec();
        match parse_pipeline(&argv) {
            PipelineParse::Exit { code, stderr, .. } => {
                assert_eq!(code, 1, "{flag}");
                assert!(
                    stderr.contains(&format!("'{flag}' (moved: set {key}")),
                    "{flag}: {stderr}"
                );
                assert!(stderr.contains("in --config"), "{flag}: {stderr}");
            }
            PipelineParse::Run { .. } => panic!("{flag} still parses"),
        }
    }
}

#[test]
fn every_key_without_a_flag_was_a_flag_that_moved_or_is_new() {
    // The registry and the moved list agree: a setting no flag sets is in
    // the moved list (the harness spells its old flag) — nothing silently
    // lost its spelling.
    // New in step 6, never flags: the index store, and the lookup's and
    // discovery's tunables (constants in code and the lab's environment
    // variables until then; docs/library-matching-config.md §1.3).
    // New in step 8: the naming prompt's code window (a constant until
    // then; docs/tuning-values.md).
    // New in exp154, never flags: the run-time index build switch and its
    // package cap (docs/design/library-names-plan.md step 3).
    // New in exp163, never flags: the ordered test mode (the config sweep's
    // rule holds — a schedule for tests is a setting, not an operational
    // flag; docs/concurrency.md rule 9, docs/audits/ordering-2026-10.md §4).
    let new_keys = |d: &str| {
        d == "run.order"
            || d == "library.index_store"
            || d == "naming.code_window_lines"
            || d.starts_with("library.lookup.")
            || d.starts_with("library.discovery.")
            || d == "library.index_build"
            || d == "library.index_build_max_packages"
            || d == "library.names_unlocked"
    };
    for k in KEYS
        .iter()
        .filter(|k| k.flag.is_none() && !new_keys(&k.dotted()))
    {
        assert!(
            MOVED_FLAGS.iter().any(|(_, key, _)| *key == k.dotted()),
            "{} has no flag and no moved-from flag",
            k.dotted()
        );
    }
}

// --- the example file -----------------------------------------------------

fn example_path() -> String {
    format!("{}/../../humanify.example.toml", env!("CARGO_MANIFEST_DIR"))
}

/// Regenerate the committed example after a deliberate registry change.
#[test]
#[ignore = "writes humanify.example.toml; run after a deliberate registry change"]
fn write_the_example_config() {
    std::fs::write(example_path(), render_example()).unwrap();
}

#[test]
fn the_example_config_is_the_registry_byte_for_byte() {
    let committed = std::fs::read_to_string(example_path()).expect("humanify.example.toml");
    assert_eq!(
        committed,
        render_example(),
        "humanify.example.toml drifted from the key registry — regenerate it: \
         cargo test -p humanify-cli write_the_example_config -- --ignored"
    );
}

#[test]
fn the_example_config_sets_every_default_and_nothing_else() {
    // Loading the example changes NOTHING: every value it sets is the
    // compiled-in default the run uses without a file.
    let (input, o) = opts(&["in.js"]);
    let example = ConfigFile::check("example", &render_example()).expect("the example parses");
    let with = Config::build(&input, &o, Some(&example), key).unwrap();
    let without = Config::build(&input, &o, None, key).unwrap();
    let strip = |c: &Config| {
        let mut r = c.record();
        let m = r.as_object_mut().unwrap();
        m.remove("configFile");
        m.remove("sources");
        r
    };
    assert_eq!(strip(&with), strip(&without));
    // ... and lists every key: set (it has a default) or commented (unset).
    let text = render_example();
    for k in KEYS {
        let set = format!("\n{} = ", k.name);
        let commented = format!("\n# {} = ", k.name);
        assert!(
            text.contains(&set) || text.contains(&commented),
            "{} missing from the example",
            k.dotted()
        );
    }
}
