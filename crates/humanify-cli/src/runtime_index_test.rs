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
use humanify_core::libindex::recipe::detect_recipe;

/// `parse_pipeline` for a run argv (the harness tests' shape).
fn run_argv(argv: &[&str]) -> (String, CommandOptions) {
    let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
    match crate::surface::parse_pipeline(&argv) {
        PipelineParse::Run { input, opts } => (input, *opts),
        PipelineParse::Exit { stderr, .. } => panic!("{argv:?} did not parse: {stderr}"),
    }
}

/// A Config whose library block says the machinery runs (report matching,
/// no index) with an empty store at a fresh path — this test must depend
/// on NO store existing, wherever it happens to run. `from_bundle`: the
/// run-time build is on (`--library-index-from-bundle`, exp174).
fn machinery_config_with(from_bundle: bool) -> Config {
    let mut argv = vec!["in.js", "--library-matching", "report"];
    if from_bundle {
        argv.push("--library-index-from-bundle");
    }
    let (input, opts) = run_argv(&argv);
    let store = std::env::temp_dir().join(format!(
        "humanify-machinery-{from_bundle}-{}",
        std::process::id()
    ));
    let toml = format!(
        "[library]\nindex_store_dir = {:?}\n",
        store.display().to_string()
    );
    let file = crate::config::ConfigFile::check("cfg.toml", &toml).expect("the file parses");
    Config::build(&input, &opts, Some(&file), |k| {
        (k == crate::config::API_KEY_VAR).then(|| "env-key".to_string())
    })
    .expect("report with no index accepts")
}

/// The machinery with the run-time build on: exp171's path.
fn machinery_config() -> Config {
    machinery_config_with(true)
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

/// Run the machinery on `code` as a bundle of `bundler`/`minifier`, through
/// the joint (scope thread → channel → deadline'd join): the kind is read
/// ONCE off detection (`detect_recipe`) and handed to the job.
fn answer_for(
    config: &Config,
    code: &str,
    (bundler, minifier): (BundlerType, MinifierType),
) -> JobAnswer {
    let profiler = humanify_core::profiling::Profiler::disabled();
    let stop = AtomicBool::new(false);
    let detected = detect_recipe(code, bundler, minifier);
    std::thread::scope(|scope| {
        let job = start(scope, &profiler, code, config, &detected, &stop);
        job.join(&profiler, &stop)
    })
}

fn esbuild() -> (BundlerType, MinifierType) {
    (BundlerType::Esbuild, MinifierType::Esbuild)
}

/// The skipped answer's line and trail, or a panic naming what came back.
fn skipped(answer: JobAnswer, what: &str) -> (String, crate::runtime_index::IndexSource) {
    match answer {
        JobAnswer::Skipped(line, trail) => (line, trail),
        JobAnswer::Found(..) => panic!("{what}: found, expected skipped"),
        JobAnswer::Built(..) => panic!("{what}: built, expected skipped"),
        JobAnswer::Failed(e) => panic!("{what}: failed, expected skipped: {e}"),
    }
}

/// exp175 (Andrew 2026-11-01): by DEFAULT, a store with no index for the
/// bundle's KIND (its packing tool, read off detection) is not a failure:
/// the run prints ONE line — what is missing, where it looked, that library
/// name matching is skipped, and how to get an index — and goes on with it
/// skipped. Never a build, nothing created. Every recipe kind's line
/// offers the run-time build (exp181: it builds for all of them).
#[test]
fn by_default_no_index_for_the_bundles_kind_skips_with_one_line() {
    let config = machinery_config_with(false);
    let root = config.library.index_store_root();
    let code = "/*! leftpad v1.2.3 */\nconsole.log(1);\n"; // a seed it must NOT build from
    for (pair, kind) in [
        (bun(), "bun"),
        (esbuild(), "esbuild"),
        ((BundlerType::Webpack, MinifierType::Terser), "webpack"),
        ((BundlerType::Rollup, MinifierType::Terser), "rollup"),
    ] {
        let (line, trail) = skipped(answer_for(&config, code, pair), kind);
        let head = format!(
            "no library index for {kind} bundles in {} — skipping library name matching (",
            root.display()
        );
        assert!(line.starts_with(&head), "{kind}: {line}");
        assert!(!line.contains('\n'), "ONE line: {line}");
        assert!(line.contains("humanify-index build"), "{kind}: {line}");
        assert!(
            line.contains(&format!("--recipe {kind}:")),
            "{kind}: {line}"
        );
        assert!(line.contains("--library-index <dir>"), "{kind}: {line}");
        assert!(
            line.contains("--library-index-from-bundle"),
            "the run-time build is offered for every recipe kind: {line}"
        );
        assert_eq!(trail.outcome, "skipped");
        assert_eq!(trail.bundler, kind);
    }
    assert!(!root.exists(), "nothing created: no fetch, no build");
}

/// A bundler no index recipe exists for (browserify, parcel, a plain
/// script: no bundler recognised) names its kind too, and is skipped the
/// same way, with or without the run-time build — saying no recipe exists
/// for that kind yet, and offering nothing that could not serve it.
#[test]
fn a_bundle_of_a_kind_no_index_can_have_is_skipped_saying_so() {
    for from_bundle in [false, true] {
        let config = machinery_config_with(from_bundle);
        let root = config.library.index_store_root();
        for (bundler, kind) in [
            (BundlerType::Unknown, "unrecognised"),
            (BundlerType::Browserify, "browserify"),
            (BundlerType::Parcel, "parcel"),
        ] {
            let code = "/*! leftpad v1.2.3 */\nconsole.log(1);\n";
            let (line, trail) = skipped(
                answer_for(&config, code, (bundler, MinifierType::None)),
                kind,
            );
            assert_eq!(
                line,
                format!(
                    "no library index for {kind} bundles in {} — skipping library name \
                     matching (no index recipe exists for {kind} bundles yet)",
                    root.display()
                )
            );
            assert_eq!(trail.bundler, kind);
            assert!(!root.exists(), "nothing created: no fetch, no build");
        }
    }
}

/// With `--library-index-from-bundle`, a bundle of ANY recipe kind that
/// reveals no package is skipped with one line naming why, and the line
/// still names the recipe this bundle needs; the store is searched first
/// (an index FOUND for the kind would be used), and nothing is created —
/// no tool is fetched for a build that has nothing to build.
#[test]
fn from_bundle_skips_a_bundle_that_reveals_no_package_of_every_kind() {
    let config = machinery_config_with(true);
    let root = config.library.index_store_root();
    for (pair, kind) in [
        (bun(), "bun"),
        (esbuild(), "esbuild"),
        ((BundlerType::Webpack, MinifierType::Terser), "webpack"),
        ((BundlerType::Rollup, MinifierType::Terser), "rollup"),
    ] {
        let (line, _) = skipped(answer_for(&config, "console.log(1);\n", pair), kind);
        assert!(
            line.starts_with(&format!(
                "no library index for {kind} bundles in {}, and this bundle reveals no npm \
                 package",
                root.display()
            )),
            "{line}"
        );
        assert!(line.contains("skipping library name matching"), "{line}");
        assert!(line.contains(&format!("--recipe {kind}:")), "{line}");
        assert!(!line.contains("Bun indexes only"), "{line}");
    }
    assert!(!root.exists(), "nothing created: no fetch, no build");
}

/// The build line (exp175): what the run prints just before the notice
/// when it builds — the kind, and the store it builds into.
#[test]
fn the_building_line_names_the_kind_and_the_store() {
    let root = std::path::Path::new("/s/store");
    assert_eq!(
        crate::runtime_index::building_line("bun", root, false),
        "no library index for bun bundles — building one from this bundle into /s/store"
    );
    assert_eq!(
        crate::runtime_index::building_line("bun", root, true),
        "no library index for bun bundles in /s/store covers this bundle's packages — \
         building one from this bundle into it"
    );
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
    // report with no index: the machinery runs...
    assert!(machinery_config().library.runs_index_machinery());
    // ... but an explicit index turns it back off: it wins.
    let explicit = build(&[
        "in.js",
        "--library-matching",
        "report",
        "--library-index",
        "/some/index",
    ]);
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
    for bundler in humanify_index::tools::ALL_RECIPE_BUNDLERS {
        let p = humanify_index::tools::pin(bundler);
        let tool = humanify_index::tools::Tool {
            bundler,
            version: p.version.to_string(),
            bin: root.join("tool"),
        };
        let text = crate::runtime_index::build_notice(&root, &plan, &tool, true);
        assert!(
            text.starts_with("NOTICE: library names (the default)"),
            "{text}"
        );
        assert!(
            text.contains(&format!(
                "bundled with {} {} (just fetched into the store)",
                p.label, p.version
            )),
            "the kind's own tool: {text}"
        );
        if bundler != humanify_core::libindex::recipe::RecipeBundler::Bun {
            assert!(!text.contains("Bun"), "no other kind's wording: {text}");
        }
        assert!(
            text.contains("3 to build, 1 already in the store"),
            "{text}"
        );
        let est = estimate_build(bundler, 3, 0);
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
    }
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
