//! The pipeline driver runs the PORTED stages in the TS order
//! (src/commands/unified.ts → src/unminify.ts): detect, select the unpack
//! adapter, unpack (the Bun adapter names vendor files inside it, stage 5),
//! detect libraries, then per processed file format → graph → match → name.
//! A run stops at the first unported stage with the NOT-YET block and exit
//! 3 — after the ported stages have written their part of the tree.

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
fn a_bun_run_writes_the_vendor_tree_then_stops_at_format() {
    let s = Scratch::new("bun-format");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let out = s.out().display().to_string();
    let o = run(&s.0, &[&input, "--api-key", "k", "-o", &out]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(
        err.contains("ERROR: stage 6 (format) is NOT YET PORTED"),
        "{err}"
    );
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
    // only runtime.js reaches the per-file loop.
    assert!(err.contains("Processing file 1/1"), "{err}");
}

#[test]
fn a_run_with_the_formatted_text_reaches_naming() {
    let s = Scratch::new("bun-naming");
    let input = s.write("bundle.js", BUN_BUNDLE);
    let formatted = s.write("formatted.js", PLAIN);
    let out = s.out().display().to_string();
    let o = run(
        &s.0,
        &[
            &input,
            "--api-key",
            "k",
            "-o",
            &out,
            "--beautified-input",
            &formatted,
        ],
    );
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(
        err.contains("ERROR: stage 9 (name identifiers) is NOT YET PORTED"),
        "{err}"
    );
    assert!(s.out().join("vendor/_bun-modules.json").is_file());
}

#[test]
fn a_plain_script_passes_through_as_index_js() {
    let s = Scratch::new("passthrough");
    let input = s.write("plain.js", PLAIN);
    let out = s.out().display().to_string();
    let o = run(&s.0, &[&input, "--api-key", "k", "-o", &out]);
    let err = stderr(&o);
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("stage 6 (format)"), "{err}");
    assert_eq!(
        std::fs::read_to_string(s.out().join("index.js")).unwrap(),
        PLAIN
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
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("stage 6 (format)"), "{err}");
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
    assert_eq!(o.status.code(), Some(3), "{err}");
    assert!(err.contains("Processing file 1/3"), "{err}");
}
