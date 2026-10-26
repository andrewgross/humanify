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

/// An empty store and a bundle that reveals no package: the machinery
/// fails the run loudly naming the escape hatches — through the joint, so
/// the wiring (scope thread → channel → deadline'd join) is what this
/// test proves.
#[test]
fn the_job_answers_through_the_joint_and_names_its_refusals() {
    let config = machinery_config();
    let profiler = humanify_core::profiling::Profiler::disabled();
    let stop = AtomicBool::new(false);
    let code = "console.log(1);\n"; // no banner, no package.json
    let answer = std::thread::scope(|scope| {
        let job = start(scope, &profiler, code, &config, bun().0, bun().1, &stop);
        job.join(&profiler, &stop)
    });
    let JobAnswer::Failed(error) = answer else {
        panic!("no seed, nothing found: the run fails loudly");
    };
    assert!(error.contains("reveals no package"), "{error}");
    assert!(error.contains("--library-index"), "{error}");
    assert!(error.contains("index_build = \"off\""), "{error}");

    // A bundler this lane does not walk: refused before the store is even
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
    let JobAnswer::Failed(error) = answer else {
        panic!("esbuild is out of this lane's scope: refused");
    };
    assert!(error.contains("esbuild"), "{error}");
    assert!(error.contains("--library-index"), "{error}");
}

/// Defaults never reach the machinery, and an explicit index wins over it
/// (the gate it must not slip through).
#[test]
fn defaults_and_an_explicit_index_leave_the_machinery_off() {
    let (input, opts) = run_argv(&["in.js"]);
    let defaults = Config::build(&input, &opts, None, |k| {
        (k == crate::config::API_KEY_VAR).then(|| "env-key".to_string())
    })
    .unwrap();
    assert!(!defaults.library.runs_index_machinery());
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
