//! The run-time index machinery's tests (exp154, library-names-plan step
//! 3): the seeds the bundle itself reveals, and the job → joint wiring.
//!
//! WHAT IS STUBBED here: nothing — these tests exercise the machinery's
//! own code paths (seed scanning, the fast refusals). The full auto path
//! (search the store, find a usable index, load it) is proven end-to-end
//! by the CLI-level tests in tests/pipeline_stages.rs against a
//! pre-populated store; the build path's plan/limit stages are proven by
//! humanify-index's cache_test.rs.

use std::sync::atomic::AtomicBool;

use humanify_model::detection::{BundlerType, MinifierType};

use crate::config::Config;
use crate::runtime_index::{JobAnswer, bundle_revealed_seeds, start};
use crate::surface::{CommandOptions, PipelineParse};

/// `parse_pipeline` for a run argv (the harness tests' shape).
fn run_argv(argv: &[&str]) -> (String, CommandOptions) {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    match crate::surface::parse_pipeline(&argv) {
        PipelineParse::Run { input, opts } => (input, *opts),
        PipelineParse::Exit { stderr, .. } => panic!("{argv:?} did not parse: {stderr}"),
    }
}

/// A Config whose library block says the machinery runs (report matching,
/// no index, auto build) — the only configuration that reaches the module.
fn machinery_config() -> Config {
    let (input, opts) = run_argv(&["in.js", "--library-matching", "report"]);
    // `index_build = "auto"` has no flag (config sweep step 7): it is a
    // file key, so the test builds the config from a TOML string.
    // io.output_dir points the store at a fresh empty path: the machinery
    // searches `<output>/.humanify/library-index` — this test must depend
    // on NO store existing, wherever it happens to run.
    let out = std::env::temp_dir().join(format!("humanify-machinery-{}", std::process::id()));
    let toml = format!(
        "[library]\nindex_build = \"auto\"\n[io]\noutput_dir = {:?}\n",
        out.display().to_string()
    );
    let file = crate::config::ConfigFile::check("cfg.toml", &toml).expect("the file parses");
    Config::build(&input, &opts, Some(&file), |k| {
        (k == crate::config::API_KEY_VAR).then(|| "env-key".to_string())
    })
    .expect("report+auto accepts")
}

/// A plain Bundler/Minifier pair: Bun + Terser is what a Bun bundle this
/// lane walks carries.
fn bun() -> (BundlerType, MinifierType) {
    (BundlerType::Bun, MinifierType::Terser)
}

/// A bundle with one library banner and one inlined package.json: the
/// banner names the package bare, the package.json spells its version, and
/// a version-bearing seed beats a bare one under the same name.
#[test]
fn the_bundle_itself_names_the_build_s_seeds() {
    let code = "/*! leftpad v1.2.3 */\n\
        (x0v,GDg)=>{GDg.exports={name:\"gaxios\",version:\"7.1.4\",\
        description:\"A simple common HTTP client.\",\
        main:\"build/cjs/src/index.js\",files:[\"build/\"]}}\n\
        console.log(1);\n";
    let seeds = bundle_revealed_seeds(code);
    let find = |name: &str| seeds.iter().find(|s| s.name == name);
    let leftpad = find("leftpad").expect("the banner names leftpad");
    assert_eq!(leftpad.version, None);
    assert!(leftpad.evidence.contains("banner comment"), "{leftpad:?}");
    let gaxios = find("gaxios").expect("the inlined package.json names gaxios");
    assert_eq!(gaxios.version.as_deref(), Some("7.1.4"));
    assert!(gaxios.evidence.contains("package-json"), "{gaxios:?}");
    assert_eq!(seeds.len(), 2, "deduplicated by name: {seeds:?}");

    // The same name from both sources: the version-bearing declaration
    // wins, never the bare banner.
    let code = "/*! gaxios v7.1.4 */\n\
        (x0v,GDg)=>{GDg.exports={name:\"gaxios\",version:\"7.1.4\",\
        description:\"A simple common HTTP client.\",\
        main:\"build/cjs/src/index.js\",files:[\"build/\"]}}\n";
    let seeds = bundle_revealed_seeds(code);
    assert_eq!(seeds.len(), 1, "deduplicated by name: {seeds:?}");
    assert_eq!(seeds[0].version.as_deref(), Some("7.1.4"));
    assert!(seeds[0].evidence.contains("package-json"), "{:?}", seeds[0]);
}

/// An empty store and a bundle that reveals no package, then a bundler
/// no index can be built for: since exp171 (names is the default) neither
/// fails the run — the machinery answers `Inert` with the notice the run
/// prints, naming the escapes — through the joint, so the wiring (scope
/// thread → channel → deadline'd join) is what this test proves.
#[test]
fn a_bundle_no_index_can_serve_is_inert_and_says_why() {
    let config = machinery_config();
    let profiler = humanify_core::profiling::Profiler::disabled();
    let stop = AtomicBool::new(false);
    let code = "console.log(1);\n"; // no banner, no package.json
    let answer = std::thread::scope(|scope| {
        let job = start(scope, &profiler, code, &config, bun().0, bun().1, &stop);
        job.join(&profiler, &stop)
    });
    let JobAnswer::Inert(why) = answer else {
        panic!("no seed, nothing found: the run goes on as off");
    };
    assert!(why.contains("reveals no npm package"), "{why}");
    assert!(why.contains("--library-index"), "{why}");
    assert!(why.contains("--library-matching off"), "{why}");

    // A bundler this lane does not walk: inert before the store is even
    // searched, naming the bundler and the same two escapes.
    let answer = std::thread::scope(|scope| {
        let job = start(
            scope,
            &profiler,
            code,
            &config,
            BundlerType::Esbuild,
            MinifierType::Esbuild,
            &stop,
        );
        job.join(&profiler, &stop)
    });
    let JobAnswer::Inert(why) = answer else {
        panic!("esbuild is out of this lane's scope: inert");
    };
    assert!(why.contains("esbuild"), "{why}");
    assert!(why.contains("--library-index"), "{why}");
    assert!(why.contains("--library-matching off"), "{why}");
}

/// Since exp171 the DEFAULT reaches the machinery (names + auto); the
/// opt-out (`--library-matching off`) never does, and an explicit index
/// wins over it (the gate it must not slip through).
#[test]
fn the_default_runs_the_machinery_and_the_opt_out_and_an_explicit_index_do_not() {
    let build = |argv: &[&str]| {
        let (input, opts) = run_argv(argv);
        Config::build(&input, &opts, None, |k| {
            (k == crate::config::API_KEY_VAR).then(|| "env-key".to_string())
        })
        .unwrap()
    };
    assert!(build(&["in.js"]).library.runs_index_machinery());
    assert!(
        !build(&["in.js", "--library-matching", "off"])
            .library
            .runs_index_machinery()
    );
    // report + auto: the machinery runs...
    assert!(machinery_config().library.runs_index_machinery());
    // ... but an explicit index turns it back off: it wins.
    let (input, opts) = run_argv(&[
        "in.js",
        "--library-matching",
        "report",
        "--library-index",
        "/some/index",
    ]);
    let file = crate::config::ConfigFile::check("cfg.toml", "[library]\nindex_build = \"auto\"\n")
        .unwrap();
    let explicit = Config::build(&input, &opts, Some(&file), |k| {
        (k == crate::config::API_KEY_VAR).then(|| "env-key".to_string())
    })
    .unwrap();
    assert!(!explicit.library.runs_index_machinery());
}

/// exp171: the notice a run prints BEFORE it builds an index names what is
/// built, the estimate (from the versions to BUILD, not the reused ones),
/// where it is stored and how to opt out — and the names it skipped.
#[test]
fn the_build_notice_says_what_how_much_where_and_how_to_opt_out() {
    use humanify_index::cache::{RuntimePlan, Seed, estimate_build, plan_runtime};
    let root = std::env::temp_dir().join(format!("humanify-notice-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let (cfg, planned, dropped) = plan_runtime(
        &root,
        vec![
            Seed {
                name: "ms".into(),
                version: Some("2.1.0".into()),
                evidence: "package.json".into(),
            },
            Seed {
                name: "http://mths.be/fromcodepoint".into(),
                version: None,
                evidence: "banner".into(),
            },
        ],
        Vec::new(),
        "2026-10-25",
        300,
        true,
    )
    .expect("a declared version plans offline");
    let plan = RuntimePlan {
        cfg,
        planned,
        dropped,
        to_build: 3,
        reused: 1,
    };
    let text = crate::runtime_index::build_notice(&root, &plan, false);
    assert!(
        text.starts_with("NOTICE: library names (the default)"),
        "{text}"
    );
    assert!(
        text.contains("3 to build, 1 already in the store"),
        "{text}"
    );
    let est = estimate_build(3, false);
    assert!(
        text.contains(&crate::runtime_index::humanize_bytes(est.bytes_high)),
        "the estimate is printed: {text}"
    );
    assert!(text.contains(&root.display().to_string()), "where: {text}");
    assert!(
        text.contains("--library-matching off"),
        "the opt-out: {text}"
    );
    assert!(
        text.contains("http://mths.be/fromcodepoint"),
        "skipped names: {text}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn sizes_and_durations_read_plainly() {
    use crate::runtime_index::{humanize_bytes, humanize_seconds};
    assert_eq!(humanize_bytes(82_000_000), "82 MB");
    assert_eq!(humanize_bytes(43_000_000_000), "43.0 GB");
    assert_eq!(humanize_seconds(40), "40 s");
    assert_eq!(humanize_seconds(12 * 60), "12 min");
    assert_eq!(humanize_seconds(9000), "2.5 h");
}
