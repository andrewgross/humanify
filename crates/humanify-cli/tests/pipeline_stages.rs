//! The pipeline driver runs the PORTED stages in the TS order
//! (src/commands/unified.ts → src/unminify.ts): detect, select the unpack
//! adapter, unpack (the Bun adapter names vendor files inside it, stage 5),
//! detect libraries, then per processed file format → name (graph, match,
//! transfer, waves, passes), then the split → emit → finish. Every stage
//! is native since WP5.6d: stage 6 is `core::format`, run per processed
//! file (so a run may process any number of files), and the TS-input
//! options `--beautified-input` / `--ts-library-functions` are gone.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A Bun CJS bundle: the `createRequire` import, the factory helper `x`,
/// two factories (one fallback-named) and runtime code calling one.
const BUN_BUNDLE: &str = concat!(
    "import{createRequire as Glq}from\"node:module\";var m6=Glq(import.meta.url);",
    "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);",
    "var L=(I,A,q)=>(q=I!=null?Object.create(null):A,Object.defineProperty(q,\"default\",{enumerable:!0,value:I}));\n",
    "var mod_a=x((exports,module)=>{\n",
    "  var dep=m6(\"node:path\");\n",
    "  function helper(){return dep.join(\"a\",\"b\")}\n",
    "  module.exports=helper;\n",
    "});\n",
    "var mod_b=x((exports)=>{\n",
    "  exports.value=42;\n",
    "});\n",
    "var main=mod_a();\n",
    "console.log(main(), mod_b().value);\n"
);

const PLAIN: &str =
    "function greet(name) {\n  console.log(\"Hello, \" + name + \"!\");\n}\ngreet(\"world\");\n";

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let dir =
            std::env::temp_dir().join(format!("humanify-stages-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn write(&self, name: &str, text: &str) -> String {
        let p = self.0.join(name);
        std::fs::write(&p, text).unwrap();
        p.display().to_string()
    }

    fn out(&self) -> PathBuf {
        self.0.join("out")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Every run points the LLM at a dead port with no retries: the Bun
/// adapter's vendor namer is always wired (as in the TS), and a test must
/// never reach a real endpoint.
fn run(dir: &Path, args: &[&str]) -> Output {
    run_with(dir, args, &[])
}

/// `run`, plus `--config` settings (dotted TOML lines, `split.enabled =
/// true`) beside the no-retries one. A launch that makes NO library
/// choice (no `--library-*` flag, no `library.matching`/`library.index*`
/// setting) runs with `library.matching = "off"`: these tests are about
/// the other stages, and names mode (the default) with no index for the
/// bundle's kind only adds its skip line (exp175) to the same off bytes.
/// The library tests make their choice explicitly, and [`launch`] runs the
/// true default.
fn run_with(dir: &Path, args: &[&str], settings: &[&str]) -> Output {
    let chooses = args.iter().any(|a| a.starts_with("--library"))
        || settings
            .iter()
            .any(|l| l.contains("library.matching") || l.contains("library.index"));
    let off = ["library.matching = \"off\""];
    let settings = if chooses {
        settings.to_vec()
    } else {
        [&off[..], settings].concat()
    };
    launch(dir, args, &settings)
}

/// The binary exactly as configured: the default library decision
/// included (names: an index of the bundle's kind, or the skip line). A
/// launch that does not choose the split runs it OFF: the split is on by
/// default since exp179, and these launches keep the configuration they
/// were written for.
fn launch(dir: &Path, args: &[&str], settings: &[&str]) -> Output {
    let config = dir.join("humanify.toml");
    let chooses_split = args.iter().any(|a| *a == "--split" || *a == "--no-split")
        || settings.iter().any(|l| l.contains("split.enabled"));
    let split_off = (!chooses_split).then_some("split.enabled = false");
    let toml: String = std::iter::once("llm.retries = 0")
        .chain(split_off)
        .chain(settings.iter().copied())
        .map(|l| format!("{l}\n"))
        .collect();
    std::fs::write(&config, toml).unwrap();
    Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HUMANIFY_API_KEY", "k")
        .args(args)
        .args(["--endpoint", "http://127.0.0.1:9/v1", "--config"])
        .arg(&config)
        .output()
        .unwrap()
}

const SPLIT: &str = "split.enabled = true";

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_bun_run_writes_the_vendor_tree_then_formats_and_names_runtime_js() {
    let s = Scratch::new("bun-format");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let o = run(&s.0, &[&input, "-o", &out]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(!err.contains("NOT YET PORTED"), "{err}");
    let manifest = std::fs::read_to_string(s.out().join("vendor/_bun-modules.json"))
        .expect("the bun adapter wrote its manifest");
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["adapter"], "bun");
    assert_eq!(manifest["factories"].as_array().unwrap().len(), 2);
    for e in manifest["factories"].as_array().unwrap() {
        let file = e["fileName"].as_str().unwrap();
        assert!(s.out().join(file).is_file(), "{file} written");
    }
    assert!(s.out().join("runtime.js").is_file());
    // Stage 4 ran: the bun detector marks every vendor file a library, so
    // only runtime.js reaches the per-file loop — formatted natively (one
    // statement per line, Babel's spacing) and rewritten in place.
    assert!(err.contains("Processing file 1/1"), "{err}");
    let runtime = std::fs::read_to_string(s.out().join("runtime.js")).unwrap();
    assert!(runtime.contains("console.log("), "{runtime}");
    assert!(runtime.lines().count() > 3, "formatted:\n{runtime}");
    assert!(!runtime.contains(";var "), "formatted:\n{runtime}");
}

/// A wrapper IIFE the stable split accepts: one expression statement whose
/// function declares at least 50 bindings (the wrapper threshold).
fn wrapped() -> String {
    let mut s = String::from("(function () {\n");
    for i in 0..60 {
        s.push_str(&format!("  var value{i} = {i};\n"));
    }
    s.push_str("  function greet(n) {\n    console.log(\"Hello, \" + n + value0);\n  }\n");
    s.push_str("  greet(\"world\");\n})();\n");
    s
}

#[test]
fn a_run_with_the_formatted_text_goes_end_to_end_through_the_split() {
    // M3 + WP5.6d: format → naming → split → emit → finish in ONE
    // invocation, every stage handing its value to the next in process
    // (the LLM is a dead port: every name and file name falls back, and
    // the run still completes). The input is MINIFIED: stage 6 is native.
    let s = Scratch::new("e2e-split");
    let wrapped = wrapped();
    let minified: String = wrapped.lines().map(str::trim).collect::<Vec<_>>().join("");
    let input = s.write("bundle.js", &minified);
    let out = s.out().display().to_string();
    let o = run_with(&s.0, &[&input, "-o", &out], &[SPLIT]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(err.contains("Split complete: written to"), "{err}");
    for f in [
        ".humanify/split-ledger.json",
        ".humanify/humanified.js",
        ".humanify/stage-hashes.json",
        ".humanify/placement-stats.json",
        "run.cjs",
        "package.json",
        "index.js",
    ] {
        assert!(s.out().join(f).is_file(), "{f} written\n{err}");
    }
    let ledger: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(s.out().join(".humanify/split-ledger.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(ledger["version"], 1);
    assert_eq!(ledger["order"].as_array().unwrap().len(), 62);
    // The passthrough copy of the input was consumed by the tree: the
    // entry index.js is the runnable emit's, not the input's bytes.
    assert_ne!(
        std::fs::read_to_string(s.out().join("index.js")).unwrap(),
        minified
    );
    // The shipped text is the formatted one.
    let humanified = std::fs::read_to_string(s.out().join(".humanify/humanified.js")).unwrap();
    assert!(
        humanified.starts_with("(function () {\n  var "),
        "{humanified}"
    );
}

#[test]
fn the_ts_input_options_are_gone() {
    // WP5.6d deleted the TS stage-6 text and the TS library classification,
    // WP5.6e the TS hash bytes: the parser refuses them as unknown options
    // (exit 1, nothing run). The binary reads no TS artifact at all.
    let s = Scratch::new("gone");
    let input = s.write("plain.js", PLAIN);
    for flag in [
        "--beautified-input",
        "--ts-library-functions",
        "--inject-ts-hashes",
    ] {
        let o = run(&s.0, &[&input, flag, &input]);
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "{flag}: {err}");
        assert!(
            err.contains(&format!("unexpected argument '{flag}'")),
            "{flag}: {err}"
        );
    }
}

/// A mixed file (lf-gate's `library-misclass` regime): an app function
/// BEFORE a `/*! tinylib */` banner that sits past the 1,024-char header
/// window, a library function after it. The native stage 6 carries the
/// classification by raw start (finding #32): only the library function's
/// binding takes the prefix; the app function keeps its own name.
#[test]
fn a_mixed_file_freezes_only_its_library_functions() {
    let s = Scratch::new("mixed");
    let mut text = format!("var t=\"{}\";console.log(t);", "q".repeat(1100));
    for i in 0..20 {
        text.push_str(&format!("var a{i}={i};"));
    }
    text.push_str("var appFn=function(n){return n+1};console.log(appFn(2));");
    text.push_str("/*! tinylib v1.2.3 */var libFn=function(z){return z*2};console.log(libFn(3));");
    let input = s.write("fresh.js", &text);
    let out = s.out().display().to_string();
    let o = run(&s.0, &[&input, "-o", &out]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("Mixed file") && err.contains("tinylib"),
        "{err}"
    );
    let shipped = std::fs::read_to_string(s.out().join("index.js")).unwrap();
    assert!(
        shipped.contains("var libFn = function (tinylib_z) {"),
        "{shipped}"
    );
    assert!(shipped.contains("var appFn = function (n) {"), "{shipped}");
}

#[test]
fn a_plain_script_passes_through_as_index_js() {
    let s = Scratch::new("passthrough");
    let input = s.write("plain.js", PLAIN);
    let out = s.out().display().to_string();
    let o = run(&s.0, &[&input, "-o", &out]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    // Formatted (Babel prints no trailing newline) and named — the dead
    // port leaves every name as it was.
    assert_eq!(
        std::fs::read_to_string(s.out().join("index.js")).unwrap(),
        PLAIN.trim_end()
    );
}

#[test]
fn a_failed_vendor_batch_is_counted_not_fatal() {
    // The endpoint is a dead port and the cache is empty: the LLM pass's
    // one batch fails, the fallback names stand, and the run goes on.
    let s = Scratch::new("vendor-miss");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let cache = s.0.join("cache").display().to_string();
    let cache_dir = format!("llm.cache_dir = {cache:?}");
    let o = run_with(&s.0, &[&input, "-o", &out], &[&cache_dir]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(s.out().join("vendor/_bun-modules.json")).unwrap(),
    )
    .unwrap();
    assert!(
        manifest["factories"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["nameSource"] != "llm")
    );
    // Nothing answered, so nothing was cached.
    assert_eq!(std::fs::read_dir(&cache).unwrap().count(), 0);
}

/// The `unpack` verb's `sources=` line counts how THIS run named each
/// module, so a carry still reads `carry-over` there even though the
/// manifest keeps the name's original label (finding #71).
#[test]
fn unpack_verb_counts_carries_from_run_state_not_the_manifest() {
    let s = Scratch::new("unpack-carry");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let unpack = |out: &str, prior: Option<&str>| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_humanify"));
        cmd.current_dir(&s.0)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HUMANIFY_API_KEY", "k")
            .args(["unpack", &input, out]);
        if let Some(p) = prior {
            cmd.args(["--prior-version", p]);
        }
        let o = cmd.output().unwrap();
        assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    let first = unpack("one", None);
    assert!(first.contains("sources=fallback=2 "), "{first}");
    let prior = s.0.join("one/humanified.js").display().to_string();
    let second = unpack("two", Some(&prior));
    assert!(second.contains("sources=carry-over=2 "), "{second}");
    let manifest = |dir: &str| {
        std::fs::read_to_string(s.0.join(dir).join("vendor/_bun-modules.json")).unwrap()
    };
    assert_eq!(manifest("two"), manifest("one"));
}

#[test]
fn skip_libraries_off_processes_every_unpacked_file() {
    let s = Scratch::new("no-skip");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let o = run_with(
        &s.0,
        &[&input, "-o", &out],
        &["library.unnamed = \"model\""],
    );
    let err = stderr(&o);
    // Native stage 6 runs per file: all three unpacked files are formatted
    // and named (the single-file limit of the TS-text era is gone).
    assert_eq!(o.status.code(), Some(0), "{err}");
    for i in 1..=3 {
        assert!(err.contains(&format!("Processing file {i}/3")), "{err}");
    }
}

/// A library index directory: a manifest with this build's stamps, and
/// (`slice`) one recipe slice, Bun `--target node` (BUN_BUNDLE has no
/// `// @bun` banner), with empty lookup tables.
fn library_index(dir: &Path, slice: bool) -> String {
    library_index_with(dir, if slice { &[NODE_RECIPE][..] } else { &[] })
}

const NODE_RECIPE: (&str, &str) = ("bun-node-cjs-min", "bun:target=node,format=cjs,minify");

fn library_index_with(dir: &Path, recipes: &[(&str, &str)]) -> String {
    use humanify_core::libindex::format::{INDEX_FORMAT, RECIPE_VERSION, fingerprint_versions};
    std::fs::create_dir_all(dir).unwrap();
    let slices: Vec<serde_json::Value> = recipes
        .iter()
        .map(|(slice, recipe)| {
            serde_json::json!({"slice": slice, "recipe": recipe, "recipeVersion": RECIPE_VERSION,
                "defaultForBundler": true, "bundler": "bun", "version": "1", "published": null,
                "newestAtAsOf": null, "bundlerAfterAsOf": false})
        })
        .collect();
    let manifest = serde_json::json!({
        "format": INDEX_FORMAT, "asOf": "2026-10-11", "fingerprints": fingerprint_versions(),
        "registry": {"earliestFetch": null, "latestFetch": null, "packuments": {}},
        "slices": slices, "packages": [], "refused": [], "shards": {}, "tables": {}, "data": {}
    });
    std::fs::write(dir.join("manifest.json"), manifest.to_string()).unwrap();
    let units = recipes
        .iter()
        .map(|(slice, _)| (slice.to_string(), Vec::new()))
        .collect();
    for (name, bytes) in
        humanify_core::libindex::table::lookup_files(&units, Some(&Default::default()), &[])
            .unwrap()
    {
        std::fs::write(dir.join(name), bytes).unwrap();
    }
    dir.display().to_string()
}

/// Config sweep step 6: `--library-matching report` runs discovery and
/// WRITES its report (`.humanify/library-report.json`) without
/// `--diagnostics`, and changes no other output byte against `off`. A
/// detected bundler the index has no slice for is a loud WARNING, recorded
/// as `recipeMissing`, and the run goes on.
#[test]
fn library_matching_report_writes_its_report_and_changes_nothing_else() {
    let s = Scratch::new("lib-report");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let off_dir = s.0.join("off");
    let off = run(&s.0, &[&input, "-o", off_dir.to_str().unwrap()]);
    assert_eq!(off.status.code(), Some(0), "{}", stderr(&off));
    assert!(
        !off_dir.join(".humanify/library-report.json").exists(),
        "off reports nothing"
    );
    for (tag, slice) in [("with-slice", true), ("no-slice", false)] {
        let ix = library_index(&s.0.join(format!("ix-{tag}")), slice);
        let out = s.0.join(tag);
        let o = run(
            &s.0,
            &[
                &input,
                "-o",
                out.to_str().unwrap(),
                "--library-matching",
                "report",
                "--library-index",
                &ix,
            ],
        );
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(0), "{tag}: {err}");
        let report: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out.join(".humanify/library-report.json")).expect("written"),
        )
        .unwrap();
        assert_eq!(report["matching"], "report");
        assert_eq!(
            report["indexId"].as_str().map(str::len),
            Some(64),
            "{report}"
        );
        assert!(err.contains("Library matching (report): "), "{err}");
        if slice {
            assert!(report["recipeMissing"].is_null(), "{report}");
            assert!(report["libraryDiscovery"].is_object(), "{report}");
        } else {
            let w = report["recipeMissing"].as_str().expect("recorded");
            assert!(
                w.starts_with("WARNING: library index ") && w.contains("no slice"),
                "{w}"
            );
            assert!(err.contains(w), "the warning is on stderr: {err}");
        }
        let mut got = snapshot(&out);
        got.remove(".humanify/library-report.json");
        assert_eq!(
            got,
            snapshot(&off_dir),
            "{tag}: report changed an output byte"
        );
    }
}

/// Library matching refusals happen before any work: the output
/// directory is never created.
#[test]
fn a_refused_library_decision_creates_nothing() {
    let s = Scratch::new("lib-refused");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let stale = s.0.join("stale");
    library_index(&stale, true);
    let m = std::fs::read_to_string(stale.join("manifest.json")).unwrap();
    std::fs::write(stale.join("manifest.json"), m.replace("units/", "units/0")).unwrap();
    for (args, want) in [
        (
            vec!["--library-index", "x", "--library-matching", "off"],
            "--library-matching off",
        ),
        (
            vec![
                "--library-matching",
                "report",
                "--library-index",
                "/nonexistent/ix",
            ],
            "no such directory",
        ),
        (
            vec![
                "--library-matching",
                "report",
                "--library-index",
                stale.to_str().unwrap(),
            ],
            "this binary reads units/",
        ),
        (
            vec!["--no-skip-libraries"],
            "renamed: use --library-unnamed model",
        ),
    ] {
        let o = run(
            &s.0,
            &[&[input.as_str(), "-o", &out][..], &args[..]].concat(),
        );
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "{args:?}: {err}");
        assert!(err.contains(want), "{args:?} must say {want:?}: {err}");
        assert!(!s.out().exists(), "{args:?} created the output");
    }
}

/// The useless-prior incident (docs/perf-inventory.md item 4): a wrong
/// `--prior-version` file silently degraded a run to a FULL-ASK fresh
/// pass — 3.7x wall, 4.8x memory, exit 0, nothing louder than an empty
/// coverage table. The run must now WARN on stderr when the prior bound
/// nothing — and must stay quiet for a prior that genuinely bound this
/// program (here: its own tree).
#[test]
fn a_useless_prior_warns_on_stderr_and_a_bound_prior_stays_quiet() {
    let s = Scratch::new("bad-prior");
    let input = s.write("plain.js", PLAIN);
    let out = s.out().display().to_string();
    // Baseline: the same run without a prior, whose shipped text is a
    // genuinely bound prior for the second pass.
    let base = run(&s.0, &[&input, "-o", &out]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let prior = s.out().join("index.js").display().to_string();
    // The incident's shape: a small non-empty file that is NOT this
    // program's tree, passed as --prior-version.
    let stub = s.write("stub.js", "var entryStub = 1;\nconsole.log(entryStub);\n");
    let o = run(
        &s.0,
        &[
            &input,
            "--prior-version",
            &stub,
            "-o",
            &(s.0.join("out-stub").display().to_string()),
        ],
    );
    let err = stderr(&o);
    assert_eq!(
        o.status.code(),
        Some(0),
        "the hazard: it still exits 0\n{err}"
    );
    assert!(
        err.contains("WARNING") && err.contains("bound nothing"),
        "the useless prior warns loudly:\n{err}"
    );
    // The boundary: a prior that binds this program does not warn.
    let quiet = run(
        &s.0,
        &[
            &input,
            "--prior-version",
            &prior,
            "-o",
            &(s.0.join("out-bound").display().to_string()),
        ],
    );
    let err = stderr(&quiet);
    assert_eq!(quiet.status.code(), Some(0), "{err}");
    assert!(
        !err.contains("bound nothing"),
        "a bound prior does not cry wolf:\n{err}"
    );
}

/// The second un-attributed serial stretch (docs/perf-inventory.md item
/// 3): `split:finish` was 40 s of serial work per with-prior hop with NO
/// internal spans. A with-prior run under `--perf-trace` must record a span
/// per constituent that fires in this shape (the ledger read, the
/// per-file reconcile, the desugar, the scaffold — this dead-port fixture
/// carries a perfect prior, so the reconcile changes nothing; the apply
/// and carry SPANS are pinned where a changing reconcile exists, in
/// humanify-core's finish/driver_test over the wp54 fixture). The run
/// also pins the printed gauges' console lines (finding #66).
#[test]
fn the_profile_names_the_split_finish_constituents() {
    // Two same-shaped versions of the wrapper (names same-length, so the
    // statements are structural twins): the first run's tree is a genuine
    // prior for the second.
    let variant = |vars: &str, func: &str| {
        let mut s = String::from("(function () {\n");
        for i in 0..60 {
            s.push_str(&format!("  var {vars}{i} = {i};\n"));
        }
        s.push_str(&format!(
            "  function {func}(n) {{\n    console.log(\"Hello, \" + n + {vars}0);\n  }}\n"
        ));
        s.push_str(&format!("  {func}(\"world\");\n}})();\n"));
        s
    };
    let s = Scratch::new("finish-spans");
    let v1 = s.write("v1.js", &variant("aaaa", "greet"));
    let v2 = s.write("v2.js", &variant("bbbb", "hail"));
    let out1 = s.0.join("tree-v1").display().to_string();
    let o = run_with(&s.0, &[&v1, "-o", &out1], &[SPLIT]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    // The printed gauges (finding #66), pinned on the FRESH run (the
    // with-prior run below carries everything and asks nothing, so no
    // waves ran there): the #56 name-set count line and the owner-bytes
    // line, per run, beside #65's window gauge.
    assert!(
        err.contains("LLM prompt window: peak ")
            && err.contains("Context name-sets: ")
            && err.contains("Wave-era retained: strategies ")
            && err.contains(" MB (finding #66 gauges)"),
        "the gauge lines:\n{err}"
    );
    let prior = s.0.join("tree-v1/.humanify/humanified.js");
    assert!(prior.is_file(), "the first run wrote its bundle");
    let out2 = s.0.join("tree-v2").display().to_string();
    let prof = s.0.join("profile.json").display().to_string();
    let trace = format!("perf.perf_trace = {prof:?}");
    let o = run_with(
        &s.0,
        &[
            &v2,
            "--prior-version",
            &prior.display().to_string(),
            "-o",
            &out2,
        ],
        &[SPLIT, &trace],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains("Post-split reconcile: no changes"),
        "the reconcile ran against the prior:\n{err}"
    );
    let profile: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&prof).unwrap()).unwrap();
    let spans: std::collections::HashSet<&str> = profile["traceEvents"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e.get("ph") == Some(&serde_json::json!("X")))
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    for expected in [
        "split:finish",
        "split:finish:reconcile-ledger",
        "split:finish:reconcile",
        "split:finish:desugar-using",
        "split:finish:scaffold",
    ] {
        assert!(
            spans.contains(expected),
            "span {expected} missing: {spans:?}"
        );
    }
}

/// Every file under `dir` (recursively), as path relative to `dir` → bytes.
fn snapshot(dir: &Path) -> std::collections::BTreeMap<String, Vec<u8>> {
    let mut out = std::collections::BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap().display().to_string();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

/// R6 (toolchain review 2026-10-04): the finish re-linked whatever vendor
/// record it found in the output folder, not the one THIS run wrote, and
/// nothing clears that folder. Re-running a different, vendor-less bundle
/// into the folder of an earlier esbuild run pulled the earlier run's
/// vendor files into the new runnable tree (re-wrapped in place). The
/// finish now re-links only the record this run's adapter wrote, leaves
/// files it did not write alone, and says so loudly.
#[test]
fn a_rerun_into_the_same_folder_never_relinks_the_earlier_runs_vendor_files() {
    let s = Scratch::new("stale-vendor");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/e2e/fixtures/esbuild-bundle-small/build/v1.0.0/build/index.js");
    let out = s.out().display().to_string();
    let first = run_with(
        &s.0,
        &[&fixture.display().to_string(), "-o", &out],
        &[SPLIT],
    );
    let err = stderr(&first);
    assert_eq!(first.status.code(), Some(0), "{err}");
    assert!(
        err.contains("Re-linked"),
        "the first run re-links its own:\n{err}"
    );
    assert!(
        !err.contains("not written by this run"),
        "a clean folder has no leftovers:\n{err}"
    );
    let vendor_before = snapshot(&s.out().join("vendor"));
    assert!(
        vendor_before.len() > 2,
        "the esbuild run wrote vendor files"
    );

    // A different bundle with no vendor modules at all, into the SAME folder.
    let input = s.write("plain-wrapper.js", &wrapped());
    let second = run_with(&s.0, &[&input, "-o", &out], &[SPLIT]);
    let err = stderr(&second);
    assert_eq!(second.status.code(), Some(0), "{err}");
    // On main the finish re-parsed the earlier run's (already re-linked)
    // vendor files as bare factories and the whole finish failed:
    // "Post-split step failed (vendor/lib_….js: Unexpected token)".
    assert!(
        !err.contains("Post-split step failed"),
        "the finish completes:\n{err}"
    );
    assert!(
        !err.contains("Re-linked"),
        "this run wrote no vendor record, so nothing is re-linked:\n{err}"
    );
    assert_eq!(
        snapshot(&s.out().join("vendor")),
        vendor_before,
        "the earlier run's vendor files are left exactly as they were"
    );
    assert!(
        err.contains("WARNING") && err.contains("not written by this run"),
        "the leftovers are reported:\n{err}"
    );
}

/// Config sweep steps 4-5: a refused configuration — a bad value, a
/// missing API key, a missing prior, and every `--config` refusal (an
/// unknown key, a flag/file conflict, a file that is not TOML or not
/// there, a moved flag) — exits 1 naming every problem, and NOTHING has
/// been created: no output tree, no log file, no cache directory.
#[test]
fn a_refused_configuration_creates_nothing() {
    let s = Scratch::new("refused");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let side_effects = ["out", "cache", "run.log"];
    let common = [input.as_str(), "-o", &out];
    let sinks = [
        "llm.cache_dir = \"cache\"",
        "diagnostics.log_file = \"run.log\"",
    ];
    let bad_value = run_with(
        &s.0,
        &common,
        &[&sinks[..], &["llm.concurrency = 0", "split.pure = true"]].concat(),
    );
    let err = stderr(&bad_value);
    assert_eq!(bad_value.status.code(), Some(1), "{err}");
    assert!(
        err.contains("Error: split.pure requires split.enabled"),
        "{err}"
    );
    assert!(
        err.contains("humanify.toml: llm.concurrency must be at least 1 (got 0)"),
        "{err}"
    );
    let no_key = Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(&s.0)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(common)
        .output()
        .unwrap();
    assert_eq!(no_key.status.code(), Some(1));
    assert!(
        stderr(&no_key).contains("Error: API key required: set HUMANIFY_API_KEY"),
        "{}",
        stderr(&no_key)
    );
    let no_prior = run_with(
        &s.0,
        &[&common[..], &["--prior-version", "missing.js"]].concat(),
        &sinks,
    );
    assert_eq!(no_prior.status.code(), Some(1));
    assert!(
        stderr(&no_prior)
            .contains("Error: prior version (--prior-version) file not found: missing.js"),
        "{}",
        stderr(&no_prior)
    );
    let refused = |o: &Output, want: &str| {
        let err = stderr(o);
        assert_eq!(o.status.code(), Some(1), "{want}: {err}");
        assert!(err.contains(want), "must name {want}:\n{err}");
    };
    refused(
        &run_with(
            &s.0,
            &common,
            &[&sinks[..], &["naming.batchsize = 3"]].concat(),
        ),
        "unknown key naming.batchsize (did you mean naming.batch_size?)",
    );
    refused(
        &run_with(
            &s.0,
            &[&common[..], &["--model", "a"]].concat(),
            &[&sinks[..], &["llm.model = \"b\""]].concat(),
        ),
        "config conflict: llm.model = \"a\" (--model) vs \"b\"",
    );
    refused(
        &run_with(&s.0, &common, &[&sinks[..], &["[naming"]].concat()),
        "not valid TOML",
    );
    refused(
        &run_with(
            &s.0,
            &common,
            &[&sinks[..], &["llm.api_key = \"sk\""]].concat(),
        ),
        "llm.api_key is retired",
    );
    let missing = Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(&s.0)
        .env_clear()
        .env("HUMANIFY_API_KEY", "k")
        .args(common)
        .args(["--config", "nope.toml"])
        .output()
        .unwrap();
    refused(&missing, "--config nope.toml: cannot read the config file");
    refused(
        &run_with(
            &s.0,
            &[&common[..], &["--stats-json", "s.json"]].concat(),
            &sinks,
        ),
        "'--stats-json' (renamed: use --diagnostics-stats-json)",
    );
    for p in side_effects {
        assert!(!s.0.join(p).exists(), "{p} was created by a refused run");
    }
}

#[test]
fn an_index_with_no_slice_for_the_bundles_recipe_warns_naming_both() {
    // exp097: slices are build recipes. BUN_BUNDLE has no `// @bun` banner
    // (Bun --target node); an index holding only the target-bun slice has
    // nothing for it — a loud WARNING naming the detected recipe and the
    // index's, and the run goes on.
    let s = Scratch::new("no-recipe-slice");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let ix = library_index_with(
        &s.0.join("ix"),
        &[("bun-bun-cjs-min", "bun:target=bun,format=cjs,minify")],
    );
    let out = s.out();
    let o = run(
        &s.0,
        &[
            &input,
            "-o",
            out.to_str().unwrap(),
            "--library-matching",
            "report",
            "--library-index",
            &ix,
        ],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    let report: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(out.join(".humanify/library-report.json")).expect("written"),
    )
    .unwrap();
    let w = report["recipeMissing"].as_str().expect("recorded");
    assert!(
        w.contains("no slice for recipe bun:target=node")
            && w.contains("has: bun:target=bun,format=cjs,minify"),
        "{w}"
    );
    assert!(err.contains(w), "{err}");
}

// --- the library index a run with no --library-index uses (exp154/exp174) ----
//
// Names (the default) and report with NO index: the run looks in its store
// (library.index_store_dir, default `.humanify/library-index` under the
// WORKING directory since exp174 — the disk convention: one id-named
// directory per merged index, the id the manifest's sha256) for an index of
// the bundle's KIND (its packing tool, read off detection) and uses what it
// finds without building anything. A store that holds nothing for the kind
// SKIPS library name matching with one line (exp175, Andrew 2026-11-01);
// only `--library-index-from-bundle` builds one at run time (exp171's
// path, Bun only). An explicit index that does not fit fails loudly. The
// opt-out never reads the store.

/// Move a fresh copy of the tiny node-target index (BUN_BUNDLE has no
/// `// @bun` banner) into the store at `root`, named by its manifest's
/// sha256 the way the disk convention spells it. Returns the id.
fn prepopulated_store(root: &Path) -> String {
    let staging = root.with_extension("staging");
    library_index_with(&staging, &[NODE_RECIPE]);
    let bytes = std::fs::read(staging.join("manifest.json")).unwrap();
    use sha2::{Digest, Sha256};
    let id = format!("{:x}", Sha256::digest(&bytes));
    std::fs::create_dir_all(root).unwrap();
    std::fs::rename(&staging, root.join(&id)).expect("move into the store");
    id
}

/// The default store under a working directory.
fn cwd_store(cwd: &Path) -> PathBuf {
    cwd.join(".humanify/library-index")
}

/// A fresh, empty working directory under the scratch dir.
fn fresh_cwd(s: &Scratch, tag: &str) -> PathBuf {
    let cwd = s.0.join(tag);
    std::fs::create_dir_all(&cwd).unwrap();
    cwd
}

/// The library report, parsed.
fn library_report(out: &Path) -> serde_json::Value {
    serde_json::from_str(
        &std::fs::read_to_string(out.join(".humanify/library-report.json")).expect("written"),
    )
    .unwrap()
}

/// The `libraryMatching` record the run wrote into its `--stats-json`
/// (exp175): what the library decision came to — used / built / skipped /
/// off — the bundle's kind, and the index.
fn stats_library(stats: &Path) -> serde_json::Value {
    let doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(stats).expect("stats written")).unwrap();
    doc["libraryMatching"].clone()
}

/// The `--config` line that asks for the stats file at `path`.
fn stats_setting(path: &Path) -> String {
    format!("diagnostics.stats_json = {:?}", path.display().to_string())
}

/// The skip line (exp175) for `kind` and the store at `root`.
fn skip_head(kind: &str, root: &Path) -> String {
    format!(
        "no library index for {kind} bundles in {} — skipping library name matching (",
        root.display()
    )
}

/// exp175 (Andrew 2026-11-01): names mode (the default) with no
/// `--library-index` and no index in the store for the bundle's KIND does
/// not fail: ONE line says what is missing, where it looked, that library
/// name matching is skipped and how to get an index; the run goes on with
/// it skipped — exactly off's output bytes — creates nothing in the
/// working directory, and records the skip (the stats' `libraryMatching`)
/// so it cannot be mistaken for a names run. report does the same.
#[test]
fn no_index_for_the_bundles_kind_skips_library_names_with_one_line() {
    let s = Scratch::new("ix-none");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let cwd = fresh_cwd(&s, "cwd");
    let out = s.out();
    let stats = s.0.join("names.stats.json");
    let o = launch(
        &cwd,
        &[&input, "-o", out.to_str().unwrap()],
        &[&stats_setting(&stats)],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    let head = skip_head("bun", &cwd_store(&cwd));
    let line = the_one_skip_line(&err, &head);
    assert!(!err.contains("needs a library index"), "no failure: {err}");
    assert!(
        !cwd.join(".humanify").exists(),
        "nothing created in the working directory (no store, no fetch)"
    );
    let rec = stats_library(&stats);
    assert_eq!(rec["matching"], "names", "{rec}");
    assert_eq!(rec["outcome"], "skipped", "{rec}");
    assert_eq!(rec["kind"], "bun", "{rec}");
    assert!(rec["indexId"].is_null(), "{rec}");
    assert_eq!(rec["why"], line.as_str(), "{rec}");
    off_leg_writes_the_same_bytes(&s, &cwd, &input, &out);
    report_leg_skips_too(&s, &cwd, &input, &head);
}

/// The one line in `err` starting with `head`, holding every way to an
/// index a Bun bundle has.
fn the_one_skip_line(err: &str, head: &str) -> String {
    let lines: Vec<&str> = err.lines().filter(|l| l.starts_with(head)).collect();
    assert_eq!(lines.len(), 1, "ONE skip line: {err}");
    for want in [
        "humanify-index build",
        "--library-index <dir>",
        "--library-index-from-bundle",
    ] {
        assert!(lines[0].contains(want), "must say {want:?}: {}", lines[0]);
    }
    lines[0].to_string()
}

/// The opt-out on the same input writes the skipped run's bytes exactly,
/// and records `off`.
fn off_leg_writes_the_same_bytes(s: &Scratch, cwd: &Path, input: &str, skipped: &Path) {
    let off_dir = s.0.join("off");
    let off_stats = s.0.join("off.stats.json");
    let o = launch(
        cwd,
        &[
            input,
            "-o",
            off_dir.to_str().unwrap(),
            "--library-matching",
            "off",
        ],
        &[&stats_setting(&off_stats)],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(
        snapshot(skipped),
        snapshot(&off_dir),
        "a skipped names run must write exactly the opt-out's bytes"
    );
    let rec = stats_library(&off_stats);
    assert_eq!(rec["matching"], "off", "{rec}");
    assert_eq!(rec["outcome"], "off", "{rec}");
}

/// report asks the same question and gets the same answer, recorded in
/// its report.
fn report_leg_skips_too(s: &Scratch, cwd: &Path, input: &str, head: &str) {
    let report_dir = s.0.join("report");
    let o = launch(
        cwd,
        &[
            input,
            "-o",
            report_dir.to_str().unwrap(),
            "--library-matching",
            "report",
        ],
        &[],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(err.contains(head), "{err}");
    let report = library_report(&report_dir);
    assert_eq!(report["outcome"]["outcome"], "skipped", "{report}");
    assert_eq!(report["indexSource"]["outcome"], "skipped", "{report}");
}

/// A store whose only index is of another kind (a Bun index; the input a
/// plain script, whose bundler no index kind has) is skipped the same way:
/// the line names the bundle's kind, and the report's trail names the
/// index it looked at and why it was not used.
#[test]
fn a_store_holding_only_another_kinds_index_is_skipped() {
    let s = Scratch::new("ix-noslice");
    let input = s.write("plain.js", PLAIN);
    let cwd = fresh_cwd(&s, "cwd");
    let id = prepopulated_store(&cwd_store(&cwd));
    let out = s.out();
    let o = launch(
        &cwd,
        &[
            &input,
            "-o",
            out.to_str().unwrap(),
            "--library-matching",
            "report",
        ],
        &[],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains(&skip_head("unrecognised", &cwd_store(&cwd))),
        "{err}"
    );
    let report = library_report(&out);
    let candidates = report["indexSource"]["candidates"]
        .as_array()
        .expect("a list");
    assert!(
        candidates.iter().any(|c| c["id"] == id.as_str()
            && c["verdict"]
                .as_str()
                .unwrap()
                .contains("no slice for recipe")),
        "{report}"
    );
}

/// An EXPLICIT `--library-index` that does not fit the bundle's kind still
/// FAILS loudly in names mode (exp175): the user named it, so a mismatch is
/// a mistake to surface, never a skip.
#[test]
fn an_explicit_index_that_does_not_fit_the_bundles_kind_fails_loudly() {
    let s = Scratch::new("ix-explicit-unfit");
    let input = s.write("plain.js", PLAIN);
    let ix = library_index(&s.0.join("ix"), true);
    let out = s.out();
    let o = launch(
        &s.0,
        &[&input, "-o", out.to_str().unwrap(), "--library-index", &ix],
        &[],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(1), "{err}");
    assert!(
        err.contains(
            "--library-matching names cannot run: the index passed with --library-index has \
             no slice for unrecognised bundles"
        ),
        "{err}"
    );
    assert!(err.contains("no slice for recipe"), "{err}");
    assert!(!err.contains("skipping library name matching"), "{err}");
}

/// The opt-out (`--library-matching off`) and an explicit index never
/// reach the store: it is neither read nor created, no build process is
/// spawned, and the explicit run's report carries no machinery trail.
#[test]
fn the_opt_out_and_an_explicit_index_never_touch_the_store() {
    let s = Scratch::new("ix-off");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let cwd = fresh_cwd(&s, "cwd");

    // The opt-out.
    let plain = s.0.join("plain");
    let o = launch(
        &cwd,
        &[
            &input,
            "-o",
            plain.to_str().unwrap(),
            "--library-matching",
            "off",
        ],
        &[],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(!err.contains("Library names"), "off says nothing: {err}");
    assert!(!err.contains("Library index"), "off says nothing: {err}");
    assert!(
        !err.contains("needs a library index"),
        "off needs none: {err}"
    );

    // An explicit index: it wins and the machinery never runs.
    let ix = library_index(&s.0.join("ix"), true);
    let wins = s.0.join("wins");
    let o = launch(
        &cwd,
        &[
            &input,
            "-o",
            wins.to_str().unwrap(),
            "--library-matching",
            "report",
            "--library-index",
            &ix,
        ],
        &[],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        !err.contains("Library index: using"),
        "the machinery never ran: {err}"
    );
    assert!(library_report(&wins)["indexSource"].is_null());
    assert!(
        !cwd.join(".humanify").exists(),
        "neither run read (so created) the store"
    );
}

/// An index in `<cwd>/.humanify/library-index` (the default store, exp174)
/// is FOUND and USED: the output tree is byte-identical to the
/// explicit-index run — only the library report differs, by gaining the
/// usage trail (indexSource). Nothing is built (the store holds exactly
/// what it held), and nothing of the store lands in the output dir.
#[test]
fn an_index_in_the_working_directory_store_is_found_and_used() {
    let s = Scratch::new("ix-found");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let ix = library_index(&s.0.join("ix"), true);
    let explicit_dir = explicit_index_leg(&s.0, &input, &ix);

    let cwd = fresh_cwd(&s, "cwd");
    let root = cwd_store(&cwd);
    let id = prepopulated_store(&root);
    let found_dir = s.0.join("found");
    let stats = s.0.join("found.stats.json");
    let o = launch(
        &cwd,
        &[
            &input,
            "-o",
            found_dir.to_str().unwrap(),
            "--library-matching",
            "report",
        ],
        &[&stats_setting(&stats)],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains(&format!(
            "Library index: using {} for bun bundles from {} (found; nothing to build)",
            &id[..12],
            root.display()
        )),
        "{err}"
    );
    let rec = stats_library(&stats);
    assert_eq!(rec["matching"], "report", "{rec}");
    assert_eq!(rec["outcome"], "used", "{rec}");
    assert_eq!(rec["indexFrom"], "store", "{rec}");
    assert_eq!(rec["kind"], "bun", "{rec}");
    assert_eq!(rec["indexId"], id.as_str(), "{rec}");
    assert!(!err.contains("NOTICE"), "nothing is built: {err}");

    let report = library_report(&found_dir);
    assert_found_trail(&report, &root);
    assert!(library_report(&explicit_dir)["indexSource"].is_null());
    let strip = |mut m: std::collections::BTreeMap<String, Vec<u8>>| {
        m.retain(|k, _| !k.starts_with(".humanify/"));
        m
    };
    assert_eq!(
        strip(snapshot(&found_dir)),
        strip(snapshot(&explicit_dir)),
        "a found index changed an output byte"
    );
    assert!(
        !found_dir.join(".humanify/library-index").exists(),
        "the store lives in the working directory, not the output dir"
    );
    let entries: Vec<String> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().into_string().unwrap())
        .collect();
    assert_eq!(entries, [id.as_str()], "untouched by the run");
    assert!(
        !root.join(&id).join("build.json").exists(),
        "no run-time build happened"
    );
}

/// `--library-index-store <dir>` points the run at another store: a
/// relative one resolves against the working directory, an absolute one
/// is used as given. (Report mode: these tiny test indexes carry no
/// module answers for names to place with; e2e runs names on a real one.)
#[test]
fn a_pointed_store_is_used() {
    let s = Scratch::new("ix-pointed");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let cwd = fresh_cwd(&s, "cwd");
    let relative = cwd.join("stores/mine");
    let absolute = s.0.join("elsewhere/store");
    for (given, root) in [
        ("stores/mine".to_string(), relative.clone()),
        (absolute.display().to_string(), absolute.clone()),
    ] {
        let id = prepopulated_store(&root);
        let out = s.0.join(format!("out-{}", &id[..6]));
        let _ = std::fs::remove_dir_all(&out);
        let o = launch(
            &cwd,
            &[
                &input,
                "-o",
                out.to_str().unwrap(),
                "--library-matching",
                "report",
                "--library-index-store",
                &given,
            ],
            &[],
        );
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(0), "{given}: {err}");
        assert!(
            err.contains(&format!(
                "Library index: using {} for bun bundles from {}",
                &id[..12],
                root.display()
            )),
            "{given}: {err}"
        );
        assert_eq!(library_report(&out)["indexSource"]["outcome"], "found");
        std::fs::remove_dir_all(&root).unwrap();
    }
    assert!(
        !cwd_store(&cwd).exists(),
        "the default store was never read"
    );
}

/// The find test's other leg: the same bundle against the SAME index,
/// passed explicitly (so the machinery never runs). Its output tree is
/// what the found leg must match byte for byte.
fn explicit_index_leg(dir: &Path, input: &str, ix: &str) -> PathBuf {
    let explicit_dir = dir.join("explicit");
    let stats = dir.join("explicit.stats.json");
    let o = launch(
        dir,
        &[
            input,
            "-o",
            explicit_dir.to_str().unwrap(),
            "--library-matching",
            "report",
            "--library-index",
            ix,
        ],
        &[&stats_setting(&stats)],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let rec = stats_library(&stats);
    assert_eq!(rec["outcome"], "used", "{rec}");
    assert_eq!(rec["indexFrom"], "explicit", "{rec}");
    assert_eq!(rec["kind"], "bun", "{rec}");
    explicit_dir
}

/// What the library report's `indexSource` must say when the machinery
/// FOUND a stored index: outcome, identity, the slice it matched on, the
/// deliberately-unknown bundle Bun version, the store, and the search's
/// candidates.
fn assert_found_trail(report: &serde_json::Value, root: &Path) {
    let source = &report["indexSource"];
    assert_eq!(source["outcome"], "found", "{report}");
    assert_eq!(source["indexId"].as_str().map(str::len), Some(64));
    assert_eq!(report["indexId"], source["indexId"], "{report}");
    assert_eq!(source["slice"], "bun-node-cjs-min", "{report}");
    assert_eq!(source["bundler"], "bun", "{report}");
    assert_eq!(
        source["detectedBundlerVersion"], "unknown",
        "reading the bundle's own Bun version is out of this lane's scope"
    );
    assert_eq!(source["store"], root.display().to_string(), "{report}");
    assert!(source["tool"].is_null(), "nothing was built: {report}");
    assert!(source["seeds"].is_null(), "nothing was built: {report}");
    let candidates = source["candidates"].as_array().expect("a list");
    assert!(
        candidates
            .iter()
            .any(|c| c["id"] == source["indexId"]
                && c["verdict"].as_str().unwrap().contains("as-of")),
        "{candidates:?}"
    );
}

/// `--library-index-from-bundle` takes exp171's run-time build path, and
/// only it: BUN_BUNDLE reveals no package and the store is empty, so the
/// run says so in one line and goes on with library names skipped
/// — its output tree byte-identical to the opt-out's — and creates nothing
/// in the store (no process was spawned).
#[test]
fn the_from_bundle_flag_takes_the_run_time_build_path() {
    let s = Scratch::new("ix-from-bundle");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let cwd = fresh_cwd(&s, "cwd");
    let built_dir = s.0.join("from-bundle");
    let o = launch(
        &cwd,
        &[
            &input,
            "-o",
            built_dir.to_str().unwrap(),
            "--library-index-from-bundle",
        ],
        &[],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains(&format!(
            "no library index for bun bundles in {}, and this bundle reveals no npm package",
            cwd_store(&cwd).display()
        )),
        "{err}"
    );
    assert!(err.contains("skipping library name matching"), "{err}");
    assert!(
        !err.contains("NOTICE: library names"),
        "nothing to build: {err}"
    );
    assert!(
        !cwd.join(".humanify").exists(),
        "nothing was created in the store: {err}"
    );
    let off_dir = s.0.join("off");
    let o = launch(
        &cwd,
        &[
            &input,
            "-o",
            off_dir.to_str().unwrap(),
            "--library-matching",
            "off",
        ],
        &[],
    );
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    assert_eq!(
        snapshot(&built_dir),
        snapshot(&off_dir),
        "a skipped from-bundle run must write exactly the opt-out's bytes"
    );
}

/// exp181 (Andrew 2026-11-01: "we have discovery and the ability to specify
/// the bundler"): the bundler verdict the library index decision reads is
/// the ONE the unpacker uses — detection, or the user's forced
/// `unpack.bundler` / `unpack.minifier` — never a second detection. Forcing
/// the bundler changes the kind searched, the recipe offered and the skip
/// line, in step with the verdict the unpack stage logs (`Bundle
/// detection: bundler=…`); a forced kind no recipe exists for says so.
#[test]
fn a_forced_bundler_is_the_verdict_the_library_index_decision_reads() {
    let s = Scratch::new("ix-forced");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let cwd = fresh_cwd(&s, "cwd");
    let root = cwd_store(&cwd);
    for (forced, minifier, kind, recipe) in [
        (
            None,
            None,
            "bun",
            Some("bun:target=node,format=cjs,minify,production"),
        ),
        (
            Some("esbuild"),
            None,
            "esbuild",
            Some("esbuild:target=node,format=cjs,minify,production"),
        ),
        (
            Some("webpack"),
            Some("none"),
            "webpack",
            Some("webpack:target=node,format=cjs "),
        ),
        (
            Some("rollup"),
            None,
            "rollup",
            Some("rollup:target=node,format=cjs,minify "),
        ),
        (Some("parcel"), None, "parcel", None),
    ] {
        let tag = forced.unwrap_or("detected");
        let stats = s.0.join(format!("{tag}.stats.json"));
        let out = s.0.join(format!("out-{tag}"));
        let mut settings = vec![stats_setting(&stats)];
        if let Some(b) = forced {
            settings.push(format!("unpack.bundler = {b:?}"));
        }
        if let Some(m) = minifier {
            settings.push(format!("unpack.minifier = {m:?}"));
        }
        let settings: Vec<&str> = settings.iter().map(String::as_str).collect();
        let o = launch(
            &cwd,
            &[&input, "-o", out.to_str().unwrap(), "-v"],
            &settings,
        );
        let err = stderr(&o);
        // The webpack adapter (webcrack) runs through npx, absent from the
        // test's PATH: that run fails at unpack, AFTER the index decision.
        if kind != "webpack" {
            assert_eq!(o.status.code(), Some(0), "{tag}: {err}");
            assert_eq!(stats_library(&stats)["kind"], kind, "{tag}");
        }
        let head = skip_head(kind, &root);
        let lines: Vec<&str> = err.lines().filter(|l| l.starts_with(&head)).collect();
        assert_eq!(lines.len(), 1, "{tag}: ONE skip line naming {kind}: {err}");
        match recipe {
            Some(r) => assert!(
                lines[0].contains(&format!("--recipe {r}")),
                "{tag}: the recipe {r}: {}",
                lines[0]
            ),
            None => assert!(
                lines[0].contains(&format!("no index recipe exists for {kind} bundles yet")),
                "{tag}: {}",
                lines[0]
            ),
        }
        let said = format!("{}{err}", String::from_utf8_lossy(&o.stdout));
        assert!(
            said.contains(&format!("Bundle detection: bundler={kind} ")),
            "{tag}: the unpacker's verdict is the index decision's: {said}"
        );
    }
}

/// Copy `from` (a directory tree) to `to`.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let e = e.unwrap();
        let dst = to.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            copy_tree(&e.path(), &dst);
        } else {
            std::fs::copy(e.path(), &dst).unwrap();
        }
    }
}

/// exp181 (Andrew 2026-11-01: "our library interface should be usable for
/// all of the bundlers we support"): `--library-index-from-bundle` builds
/// an index for an ESBUILD bundle, with the recipe the bundle needs and the
/// tool the store holds for it, entirely inside the store — hermetic: the
/// committed fixture (`tests/fixtures/runtime-build-esbuild/`, README there)
/// holds the packument snapshots and the one shard (ms@2.1.3, esbuild
/// 0.28.2's recipe) the build needs, and the store's esbuild is a stub that
/// answers `--version` and refuses to build — so nothing is fetched,
/// installed or bundled, and the plan / reuse / merge / load / restore path
/// runs for real on the release-shaped binary.
#[test]
fn from_bundle_builds_an_esbuild_index_from_the_store_s_own_tool() {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-build-esbuild");
    let s = Scratch::new("ix-esbuild-build");
    let input = s.0.join("bundle.js");
    std::fs::copy(fixture.join("bundle.js"), &input).unwrap();
    let cwd = fresh_cwd(&s, "cwd");
    let root = cwd_store(&cwd);
    copy_tree(&fixture.join("store"), &root);
    let bin = root.join("tools/esbuild-0.28.2/node_modules/.bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::copy(fixture.join("stub-esbuild.sh"), bin.join("esbuild")).unwrap();
    let stats = s.0.join("stats.json");
    let out = s.out();
    let o = launch(
        &cwd,
        &[
            input.to_str().unwrap(),
            "-o",
            out.to_str().unwrap(),
            "--library-index-from-bundle",
        ],
        &[&stats_setting(&stats)],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(0), "{err}");
    assert!(
        err.contains(&format!(
            "no library index for esbuild bundles — building one from this bundle into {}",
            root.display()
        )),
        "{err}"
    );
    assert!(!err.contains("fetching"), "the store's tool is used: {err}");
    assert!(
        err.contains("assembling one from 1 version(s) already in"),
        "the committed shard serves: {err}"
    );
    let rec = stats_library(&stats);
    assert_eq!(rec["outcome"], "built", "{rec}");
    assert_eq!(rec["kind"], "esbuild", "{rec}");
    assert_eq!(rec["indexFrom"], "run-time build", "{rec}");
    let id = rec["indexId"].as_str().expect("an index id").to_string();
    let sidecar: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join(&id).join("build.json")).expect("the sidecar"),
    )
    .unwrap();
    assert_eq!(
        sidecar["slices"][0]["recipe"], "esbuild:target=node,format=cjs,minify,production",
        "the recipe the bundle needs: {sidecar}"
    );
    assert_eq!(
        sidecar["slices"][0]["bundlerVersion"], "0.28.2",
        "{sidecar}"
    );
    // The index served the run: ms's code is placed under vendor/ms/ with
    // its original names restored.
    let ms = std::fs::read_to_string(out.join("vendor/ms/index.js")).expect("ms placed");
    for name in ["fmtLong", "fmtShort", "options"] {
        assert!(ms.contains(name), "{name} restored: {ms}");
    }
}
