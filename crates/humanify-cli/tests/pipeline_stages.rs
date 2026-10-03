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
    Command::new(env!("CARGO_BIN_EXE_humanify"))
        .current_dir(dir)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .args(args)
        .args(["--endpoint", "http://127.0.0.1:9/v1", "--retries", "0"])
        .output()
        .unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn a_bun_run_writes_the_vendor_tree_then_formats_and_names_runtime_js() {
    let s = Scratch::new("bun-format");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let o = run(&s.0, &[&input, "--api-key", "k", "-o", &out]);
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
    let o = run(&s.0, &[&input, "--api-key", "k", "--split", "-o", &out]);
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
    // WP5.6e the TS hash bytes: commander refuses them as unknown options
    // (exit 1, nothing run). The binary reads no TS artifact at all.
    let s = Scratch::new("gone");
    let input = s.write("plain.js", PLAIN);
    for flag in [
        "--beautified-input",
        "--ts-library-functions",
        "--inject-ts-hashes",
    ] {
        let o = run(&s.0, &[&input, "--api-key", "k", flag, &input]);
        let err = stderr(&o);
        assert_eq!(o.status.code(), Some(1), "{flag}: {err}");
        assert!(
            err.contains(&format!("unknown option '{flag}'")),
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
    let o = run(&s.0, &[&input, "--api-key", "k", "-o", &out]);
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
    let o = run(&s.0, &[&input, "--api-key", "k", "-o", &out]);
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
    let o = run(
        &s.0,
        &[&input, "--api-key", "k", "--llm-cache", &cache, "-o", &out],
    );
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
    let o = run(
        &s.0,
        &[&input, "--api-key", "k", "--no-skip-libraries", "-o", &out],
    );
    let err = stderr(&o);
    // Native stage 6 runs per file: all three unpacked files are formatted
    // and named (the single-file limit of the TS-text era is gone).
    assert_eq!(o.status.code(), Some(0), "{err}");
    for i in 1..=3 {
        assert!(err.contains(&format!("Processing file {i}/3")), "{err}");
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
    let base = run(&s.0, &[&input, "--api-key", "k", "-o", &out]);
    assert_eq!(base.status.code(), Some(0), "{}", stderr(&base));
    let prior = s.out().join("index.js").display().to_string();
    // The incident's shape: a small non-empty file that is NOT this
    // program's tree, passed as --prior-version.
    let stub = s.write("stub.js", "var entryStub = 1;\nconsole.log(entryStub);\n");
    let o = run(
        &s.0,
        &[
            &input,
            "--api-key",
            "k",
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
            "--api-key",
            "k",
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
/// internal spans. A with-prior run under `--profile` must record a span
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
    let o = run(&s.0, &[&v1, "--api-key", "k", "--split", "-o", &out1]);
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
    let o = run(
        &s.0,
        &[
            &v2,
            "--api-key",
            "k",
            "--split",
            "--prior-version",
            &prior.display().to_string(),
            "--profile",
            &prof,
            "-o",
            &out2,
        ],
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
