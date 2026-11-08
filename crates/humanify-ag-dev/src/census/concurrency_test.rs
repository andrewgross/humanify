//! The concurrency census on planted crate trees: every case of the TS
//! census's own test (test/concurrency-census.test.ts) plants a tiny tree
//! in a temp dir and runs the verb on it (`--root`, `--allowlist`) — a
//! flag is only worth trusting if its red has been SEEN. Plus the census
//! on this repository, which must be green.

use std::path::PathBuf;

use serde_json::{Value, json};

use super::run;
use crate::HostEnv;
use crate::paths::repo_root;
use crate::settings::{self, Parsed};

const ACCEPTED: &[&str] = &["dev.files", "dev.root", "dev.allowlist"];

fn parse(args: &[String]) -> Parsed {
    settings::parse("census concurrency", ACCEPTED, &[], args).expect("args parse")
}

/// A fresh temp root (removed on drop).
struct Tree(PathBuf);

impl Tree {
    fn new() -> Tree {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "concurrency-census-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Tree(dir)
    }
    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Plant `files` (root-relative path -> text) and an allowlist holding
/// `entries`; run the census with `files_flag` (root-relative `.rs`
/// paths, made absolute) as `--files`. Returns (exit code, output).
fn census_with(files: &[(&str, &str)], entries: &[Value], files_flag: &[&str]) -> (u8, String) {
    let tree = Tree::new();
    for (rel, text) in files {
        tree.write(rel, text);
    }
    let allowlist = tree.0.join("allowlist.json");
    std::fs::write(
        &allowlist,
        serde_json::to_string_pretty(&json!({ "entries": entries })).unwrap(),
    )
    .unwrap();
    let mut args = vec![
        "--root".to_string(),
        tree.0.display().to_string(),
        "--allowlist".to_string(),
        allowlist.display().to_string(),
    ];
    if !files_flag.is_empty() {
        let named: Vec<String> = files_flag
            .iter()
            .map(|f| tree.0.join(f).display().to_string())
            .collect();
        args.push(format!("--files={}", named.join(",")));
    }
    let (code, out, err) = run(&parse(&args), &HostEnv::default());
    (code, format!("{out}\n{err}"))
}

fn census(files: &[(&str, &str)], entries: &[Value]) -> (u8, String) {
    census_with(files, entries, &[])
}

const LIB: &str = "crates/demo/src/lib.rs";
const SPAWNING: &str = r#"pub struct Pool;

impl Pool {
    pub fn launch(&self) {
        // a comment that says tokio::spawn must not count
        let label = "tokio::spawn( in a string must not count";
        tokio::spawn(async move { drop(label) });
    }
}
"#;

fn spawn_entry() -> Value {
    json!({
        "file": LIB,
        "scope": "Pool::launch",
        "construct": "tokio-spawn",
        "pattern": "llm-runtime",
        "reason": "the demo's one task, driven to completion by its owner"
    })
}

#[test]
fn fails_on_a_new_unlisted_tokio_spawn_and_says_how_to_confirm_it() {
    let (code, out) = census(&[(LIB, SPAWNING)], &[]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(out.contains("1 unlisted concurrency site"), "{out}");
    assert!(
        out.contains("crates/demo/src/lib.rs:7  Pool::launch"),
        "{out}"
    );
    assert!(out.contains("tokio-spawn"), "{out}");
    assert!(out.contains("docs/concurrency.md#async-io"), "{out}");
    assert!(
        out.contains("dev/census/concurrency-allowlist.json"),
        "{out}"
    );
    assert!(out.contains(r#""construct": "tokio-spawn""#), "{out}");
    // exactly one site: the comment and the string were not counted
    assert!(
        !out.contains("lib.rs:5 ") && !out.contains("lib.rs:6 "),
        "{out}"
    );
}

#[test]
fn passes_when_the_site_is_allowlisted_with_a_pattern_and_a_reason() {
    let (code, out) = census(&[(LIB, SPAWNING)], &[spawn_entry()]);
    assert_eq!(code, 0, "expected GREEN, got:\n{out}");
    assert!(out.contains("CONCURRENCY CENSUS GREEN"), "{out}");
}

#[test]
fn still_matches_a_site_that_moved_within_the_same_function() {
    let moved = SPAWNING
        .replace(
            "    pub fn launch(&self) {\n",
            "    pub fn launch(&self) {\n        let a = 1;\n        let b = 2;\n        let _ = a + b;\n",
        )
        .replace("pub struct Pool;\n", "pub struct Pool;\n\n// pushed down\n\n");
    let (code, out) = census(&[(LIB, &moved)], &[spawn_entry()]);
    assert_eq!(code, 0, "expected GREEN, got:\n{out}");
}

#[test]
fn fails_on_a_stale_entry_whose_site_is_gone() {
    let (code, out) = census(&[(LIB, "pub fn quiet() {}\n")], &[spawn_entry()]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(out.contains("1 stale allowlist entr"), "{out}");
    assert!(out.contains("Pool::launch"), "{out}");
}

#[test]
fn fails_on_an_entry_with_no_reason() {
    let mut no_reason = spawn_entry();
    no_reason.as_object_mut().unwrap().remove("reason");
    let (code, out) = census(&[(LIB, SPAWNING)], &[no_reason]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(out.contains(r#"missing "reason""#), "{out}");
}

#[test]
fn fails_on_an_entry_whose_pattern_is_not_a_documented_one() {
    let mut vibes = spawn_entry();
    vibes["pattern"] = json!("vibes");
    let (code, out) = census(&[(LIB, SPAWNING)], &[vibes]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(out.contains(r#"unknown pattern "vibes""#), "{out}");
}

#[test]
fn ignores_test_code_test_files_tests_dirs_cfg_test_modules_and_blocks() {
    let (code, out) = census(
        &[
            (
                LIB,
                "#[cfg(test)]\n#[path = \"helper.rs\"]\nmod helper;\n\npub fn live() {}\n\n#[cfg(test)]\nmod tests {\n    fn t() { std::thread::spawn(|| {}); }\n}\n",
            ),
            (
                "crates/demo/src/helper.rs",
                "pub fn h() { tokio::spawn(async {}); }\n",
            ),
            (
                "crates/demo/src/lib_test.rs",
                "fn t() { tokio::spawn(async {}); }\n",
            ),
            (
                "crates/demo/tests/it.rs",
                "fn t() { tokio::spawn(async {}); }\n",
            ),
        ],
        &[],
    );
    assert_eq!(code, 0, "expected GREEN, got:\n{out}");
}

#[test]
fn fails_on_a_disallowed_method_allow_without_a_concurrency_reason() {
    let (code, out) = census(
        &[(
            LIB,
            "#[allow(clippy::disallowed_methods)]\npub fn nap() {}\n\n// concurrency: a documented exception\n#[allow(clippy::disallowed_methods)]\npub fn ok() {}\n",
        )],
        &[],
    );
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(
        // (spelled in two halves: the allow check reads raw lines, this one too)
        out.contains(concat!(
            "lib.rs:1  #[al",
            "low(clippy::disallowed_methods)]"
        )),
        "{out}"
    );
    assert!(
        out.contains("no `// concurrency: <reason>` comment"),
        "{out}"
    );
    assert!(!out.contains("lib.rs:5 "), "{out}");
}

#[test]
fn requires_the_stats_counter_pattern_for_a_relaxed_atomic() {
    let text = "use std::sync::atomic::{AtomicU64, Ordering};\nstatic N: AtomicU64 = AtomicU64::new(0);\npub fn bump() -> u64 { N.fetch_add(1, Ordering::Relaxed) }\n";
    let entries = [
        json!({"file": LIB, "scope": "static N", "construct": "atomic",
               "pattern": "stats-counter", "reason": "a count"}),
        json!({"file": LIB, "scope": "bump", "construct": "atomic-relaxed",
               "pattern": "short-lock", "reason": "a count"}),
    ];
    let (code, out) = census(&[(LIB, text)], &entries);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(
        out.contains(r#"atomic-relaxed must use pattern "stats-counter""#),
        "{out}"
    );
}

#[test]
fn allows_the_test_support_pattern_only_in_the_testkit_crate() {
    let kit = "crates/humanify-ag-testkit/src/lib.rs";
    let entry = |file: &str| {
        json!({"file": file, "scope": "Pool::launch", "construct": "tokio-spawn",
               "pattern": "test-support", "reason": "the stub server's connections"})
    };
    let (code, out) = census(&[(kit, SPAWNING)], &[entry(kit)]);
    assert_eq!(code, 0, "expected GREEN, got:\n{out}");
    let (code, out) = census(&[(LIB, SPAWNING)], &[entry(LIB)]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(
        out.contains("test-support is only for crates/humanify-ag-testkit"),
        "{out}"
    );
}

#[test]
fn files_checks_only_the_named_files() {
    let other = "crates/demo/src/other.rs";
    let (code, out) = census_with(
        &[(LIB, SPAWNING), (other, "pub fn quiet() {}\n")],
        &[],
        &[other],
    );
    assert_eq!(code, 0, "expected GREEN, got:\n{out}");
    // ...and the same tree, the spawning file named, is RED
    let (code, out) = census_with(
        &[(LIB, SPAWNING), (other, "pub fn quiet() {}\n")],
        &[],
        &[LIB],
    );
    assert_eq!(code, 1, "expected RED, got:\n{out}");
}

#[test]
fn files_still_reports_a_stale_entry_of_a_named_file_but_not_of_others() {
    let other = "crates/demo/src/other.rs";
    let tree = [(LIB, "pub fn quiet() {}\n"), (other, "pub fn quiet() {}\n")];
    let (code, out) = census_with(&tree, &[spawn_entry()], &[other]);
    assert_eq!(code, 0, "expected GREEN, got:\n{out}");
    let (code, out) = census_with(&tree, &[spawn_entry()], &[LIB]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(out.contains("1 stale allowlist entr"), "{out}");
}

#[test]
fn a_planted_thread_spawn_is_red_and_its_scope_is_type_and_fn() {
    let text = "pub struct W;\nimpl<T: Into<String>> Clone for W where T: Copy {\n    fn clone(&self) -> W { std::thread::spawn(|| {}); W }\n}\n";
    let (code, out) = census(&[(LIB, text)], &[]);
    assert_eq!(code, 1, "expected RED, got:\n{out}");
    assert!(out.contains("lib.rs:3  W::clone  —  thread-spawn"), "{out}");
}

#[test]
fn the_census_of_this_repository_is_green() {
    let root = repo_root();
    let args = vec!["--root".to_string(), root.display().to_string()];
    let (code, out, err) = run(&parse(&args), &HostEnv::default());
    assert_eq!(code, 0, "expected GREEN, got:\n{out}\n{err}");
    assert!(out.contains("CONCURRENCY CENSUS GREEN"), "{out}");
}
