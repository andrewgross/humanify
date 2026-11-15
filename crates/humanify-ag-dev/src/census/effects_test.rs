//! The effects census: its matcher on small sources, then the verb on
//! planted crate trees (`--root`, `--allowlist`) — every red is seen —
//! and on this repository, which is green.

use serde_json::{Value, json};

use super::rules::PURE_CRATES;
use super::{dependency_problems, run, sites_of};
use crate::HostEnv;
use crate::census::concurrency::scan::sanitize;
use crate::census::planted_test::Planted;
use crate::paths::repo_root;
use crate::settings::{self, Parsed};

/// (scope, construct) of every site in `src`.
fn found(src: &str) -> Vec<(String, String)> {
    sites_of("crates/humanify-ag-core/src/x.rs", &sanitize(src))
        .into_iter()
        .map(|s| (s.scope, s.construct))
        .collect()
}

fn pairs(v: &[(&str, &str)]) -> Vec<(String, String)> {
    v.iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn effects_are_found_through_their_imports() {
    let src = "use std::fs;\nuse std::fs::File;\nuse std::time::Instant;\n\
        fn a() { fs::write(\"x\", \"y\").unwrap(); }\n\
        fn b() { let _f = File::open(\"x\"); }\n\
        fn c(p: &Path) -> bool { p.exists() || p.is_dir() }\n\
        fn d() { let _t = Instant::now(); }\n\
        fn e() { let _c = Command::new(\"diff\"); }\n\
        fn f() { let _d = std::env::temp_dir(); }\n\
        fn g() { std::thread::spawn(|| {}); }\n";
    assert_eq!(
        found(src),
        pairs(&[
            ("a", "fs"),
            ("b", "fs"),
            ("c", "fs"),
            ("d", "clock"),
            ("e", "process"),
            ("f", "env"),
            ("g", "thread"),
        ])
    );
}

#[test]
fn terminal_output_is_found_but_a_childs_stdio_setting_is_not() {
    let src = "fn a() { println!(\"x\"); }\nfn b() { eprintln!(\"x\"); }\n\
        fn c() { let _o = stdout(); }\nfn d() { let _e = std::io::stderr(); }\n\
        fn e(c: &mut Command) { c.stdout(Stdio::piped()); }\n\
        fn f() { let s = \"println!(x)\"; }\n";
    assert_eq!(
        found(src),
        pairs(&[
            ("a", "stdio"),
            ("b", "stdio"),
            ("c", "stdio"),
            ("d", "stdio")
        ])
    );
}

#[test]
fn mutable_statics_are_state_and_constants_are_not() {
    let src = "static N: AtomicUsize = AtomicUsize::new(0);\n\
        static CACHE: OnceLock<String> = OnceLock::new();\n\
        static mut RAW: u8 = 0;\n\
        static EMPTY: LazyLock<Vec<u8>> = LazyLock::new(Vec::new);\n\
        static NAMES: &[&str] = &[\"a\"];\n\
        fn k() -> &'static str { \"k\" }\n\
        fn t() { static SEEN: Mutex<u8> = Mutex::new(0); }\n\
        thread_local! { static X: u8 = 0; }\n";
    assert_eq!(
        found(src),
        pairs(&[
            ("static N", "static-state"),
            ("static CACHE", "static-state"),
            ("static mut", "static-state"),
            ("t", "static-state"),
            ("<module>", "static-state"),
        ])
    );
}

#[test]
fn a_non_private_fn_returning_a_string_error_is_found() {
    let src = "pub fn a() -> Result<u8, String> { Ok(1) }\n\
        pub(crate) fn b(\n    x: u8,\n    y: u8,\n) -> Result<Vec<(u8, u8)>, String> {\n    Ok(vec![(x, y)])\n}\n\
        fn private() -> Result<u8, String> { Ok(1) }\n\
        pub fn typed() -> Result<u8, Error> { Ok(1) }\n\
        pub fn io() -> io::Result<String> { Ok(String::new()) }\n\
        pub fn nested() -> Option<Result<u8, String>> { None }\n\
        pub fn generic<T>(t: T) -> Result<T, String>\nwhere\n    T: Clone,\n{\n    Ok(t)\n}\n\
        pub fn callback(f: fn(u8) -> Result<u8, String>) -> u8 { 0 }\n\
        impl S {\n    pub fn m(&self) -> Result<(), String> { Ok(()) }\n}\n";
    assert_eq!(
        found(src),
        pairs(&[
            ("a", "result-string"),
            ("b", "result-string"),
            ("nested", "result-string"),
            ("generic", "result-string"),
            ("S::m", "result-string"),
        ])
    );
}

// ─── the verb on planted trees ────────────────────────────────────────

const ACCEPTED: &[&str] = &["dev.files", "dev.root", "dev.allowlist"];

fn parse(args: &[String]) -> Parsed {
    settings::parse("census effects", ACCEPTED, &[], args).expect("args parse")
}

const ROOT_CLIPPY: &str = "cognitive-complexity-threshold = 15\ndisallowed-methods = [\n  { path = \"std::env::var\", reason = \"env\" },\n]\n";
const PURE_CLIPPY: &str = "cognitive-complexity-threshold = 15\ndisallowed-methods = [\n  { path = \"std::env::var\", reason = \"env\" },\n  { path = \"std::fs::write\", reason = \"disk\" },\n]\n";
const CORE_MANIFEST: &str = "[package]\nname = \"humanify-ag-core\"\n\n[dependencies]\nserde.workspace = true\nhumanify-ag-model = { path = \"../humanify-ag-model\" }\n\n[dev-dependencies]\ntokio.workspace = true\n";
const CORE_LIB: &str =
    "//! core\n#![deny(clippy::map_err_ignore, clippy::let_underscore_must_use)]\npub mod x;\n";

/// A minimal tree with one pure crate (core) whose static layer is right.
fn base() -> Vec<(&'static str, String)> {
    vec![
        ("clippy.toml", ROOT_CLIPPY.to_string()),
        (
            "crates/humanify-ag-core/clippy.toml",
            PURE_CLIPPY.to_string(),
        ),
        (
            "crates/humanify-ag-core/Cargo.toml",
            CORE_MANIFEST.to_string(),
        ),
        ("crates/humanify-ag-core/src/lib.rs", CORE_LIB.to_string()),
    ]
}

fn with(
    mut files: Vec<(&'static str, String)>,
    rel: &'static str,
    text: &str,
) -> Vec<(&'static str, String)> {
    files.retain(|f| f.0 != rel);
    files.push((rel, text.to_string()));
    files
}

fn census_with(files: &[(&str, String)], entries: &[Value]) -> (u8, String) {
    let tree = Planted::new("effects-census");
    for (rel, text) in files {
        tree.write(rel, text);
    }
    let allowlist = tree.0.join("allowlist.json");
    std::fs::write(
        &allowlist,
        serde_json::to_string_pretty(&json!({ "entries": entries })).unwrap(),
    )
    .unwrap();
    let args = vec![
        "--root".to_string(),
        tree.0.display().to_string(),
        "--allowlist".to_string(),
        allowlist.display().to_string(),
    ];
    let (code, out, err) = run(&parse(&args), &HostEnv::default());
    (code, format!("{out}\n{err}"))
}

const CORE_X: &str = "crates/humanify-ag-core/src/x.rs";
const WRITES: &str = "pub fn save() { std::fs::write(\"a\", \"b\").ok(); }\n";

fn entry(file: &str, scope: &str, construct: &str, role: &str) -> Value {
    json!({ "file": file, "scope": scope, "construct": construct, "role": role })
}

#[test]
fn a_clean_pure_crate_is_green() {
    let files = with(base(), CORE_X, "pub fn add(a: u8, b: u8) -> u8 { a + b }\n");
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("EFFECTS CENSUS GREEN"), "{out}");
}

#[test]
fn an_unlisted_effect_in_a_pure_crate_fails_naming_the_shell() {
    let (code, out) = census_with(&with(base(), CORE_X, WRITES), &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("1 unlisted site(s)"), "{out}");
    assert!(
        out.contains("crates/humanify-ag-core/src/x.rs:1  save  —  fs"),
        "{out}"
    );
    assert!(out.contains("Move the effect to the shell"), "{out}");
    assert!(!out.contains("To confirm it"), "no entry to paste: {out}");
}

const CLI_X: &str = "crates/humanify-ag-cli/src/x.rs";

#[test]
fn a_listed_shell_site_passes_and_a_stale_one_fails() {
    let listed = [entry(CLI_X, "save", "fs", "shell")];
    let (code, out) = census_with(&with(base(), CLI_X, WRITES), &listed);
    assert_eq!(code, 0, "{out}");
    let (code, out) = census_with(&with(base(), CLI_X, "pub fn save() {}\n"), &listed);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("1 stale allowlist entry"), "{out}");
    assert!(out.contains("the list only shrinks"), "{out}");
}

#[test]
fn a_role_must_fit_the_crate() {
    let cli = "crates/humanify-ag-cli/src/main.rs";
    let files = with(
        with(base(), CLI_X, STRING_ERROR),
        cli,
        "fn main() { println!(\"hi\"); }\n",
    );
    let (code, out) = census_with(
        &files,
        &[
            entry(CLI_X, "parse", "result-string", "shell"),
            entry(cli, "main", "stdio", "legacy"),
        ],
    );
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("role \"shell\" is not allowed here — one of: legacy"),
        "{out}"
    );
    assert!(
        out.contains("role \"legacy\" is not allowed here — one of: shell"),
        "{out}"
    );
    let (code, out) = census_with(&files, &[entry(cli, "main", "stdio", "observation")]);
    assert_eq!(
        code, 1,
        "observation is only for the profiling files: {out}"
    );
}

const STRING_ERROR: &str =
    "pub fn parse(s: &str) -> Result<u8, String> { s.parse().map_err(|e| format!(\"{e}\")) }\n";

#[test]
fn a_string_error_signature_in_a_shell_crate_is_listed_as_legacy() {
    let files = with(base(), CLI_X, STRING_ERROR);
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("parse  —  result-string"), "{out}");
    assert!(out.contains("typed error"), "{out}");
    let (code, out) = census_with(&files, &[entry(CLI_X, "parse", "result-string", "legacy")]);
    assert_eq!(code, 0, "{out}");
}

const MODEL_X: &str = "crates/humanify-ag-model/src/x.rs";

/// Both pure crates are strict: an effect or a string error in either
/// has no role it may be listed under.
#[test]
fn a_pure_crate_refuses_every_entry() {
    for (file, line) in [
        (CORE_X, "crates/humanify-ag-core/src/x.rs:1  save  —  fs"),
        (MODEL_X, "crates/humanify-ag-model/src/x.rs:1  save  —  fs"),
    ] {
        let files = with(base(), file, WRITES);
        let (code, out) = census_with(&files, &[]);
        assert_eq!(code, 1, "{out}");
        assert!(out.contains(line), "{out}");
        assert!(out.contains("pure crate"), "{out}");
        assert!(!out.contains("To confirm it"), "no entry to paste: {out}");
        for role in ["legacy", "observation", "shell"] {
            let (code, out) = census_with(&files, &[entry(file, "save", "fs", role)]);
            assert_eq!(code, 1, "{role}: {out}");
            assert!(out.contains("no role is allowed here"), "{role}: {out}");
        }
        let files = with(base(), file, STRING_ERROR);
        let listed = [entry(file, "parse", "result-string", "legacy")];
        let (code, out) = census_with(&files, &listed);
        assert_eq!(code, 1, "{out}");
        assert!(out.contains("no role is allowed here"), "{out}");
    }
}

/// The profiling clock is the one effect a pure crate keeps, listed as an
/// `observation`; `legacy` is refused there.
#[test]
fn the_profiling_clock_is_an_observation_only() {
    let profiling = "crates/humanify-ag-core/src/profiling.rs";
    let src = "pub fn now() -> u64 { let _t = std::time::Instant::now(); 0 }\n";
    let files = with(base(), profiling, src);
    let (code, out) = census_with(&files, &[entry(profiling, "now", "clock", "observation")]);
    assert_eq!(code, 0, "{out}");
    let (code, out) = census_with(&files, &[entry(profiling, "now", "clock", "legacy")]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("role \"legacy\" is not allowed here"), "{out}");
}

#[test]
fn a_test_file_and_a_harness_crate_are_not_scanned() {
    let files = with(
        with(base(), "crates/humanify-ag-core/src/x_test.rs", WRITES),
        "crates/humanify-ag-dev/src/y.rs",
        WRITES,
    );
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn a_pure_crate_config_missing_a_root_ban_fails() {
    let dropped = "cognitive-complexity-threshold = 15\ndisallowed-methods = [\n  { path = \"std::fs::write\", reason = \"disk\" },\n]\n";
    let files = with(base(), "crates/humanify-ag-core/clippy.toml", dropped);
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("lacks the root entry"), "{out}");
    assert!(out.contains("std::env::var"), "{out}");

    let files = with(
        base(),
        "crates/humanify-ag-core/clippy.toml",
        "disallowed-methods = []\n",
    );
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("`cognitive-complexity-threshold` is absent"),
        "{out}"
    );

    let files: Vec<_> = base()
        .into_iter()
        .filter(|f| f.0 != "crates/humanify-ag-core/clippy.toml")
        .collect();
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("a pure crate needs its own clippy.toml"),
        "{out}"
    );
}

#[test]
fn the_pure_crates_share_one_set_of_bans() {
    let model_manifest =
        "[package]\nname = \"humanify-ag-model\"\n[dependencies]\nserde.workspace = true\n";
    let model_lib = "#![deny(clippy::map_err_ignore, clippy::let_underscore_must_use)]\n";
    let files = with(
        with(
            with(
                base(),
                "crates/humanify-ag-model/Cargo.toml",
                model_manifest,
            ),
            "crates/humanify-ag-model/src/lib.rs",
            model_lib,
        ),
        "crates/humanify-ag-model/clippy.toml",
        &format!("{ROOT_CLIPPY}\n"),
    );
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains(
            "crates/humanify-ag-model/clippy.toml differs from crates/humanify-ag-core/clippy.toml"
        ),
        "{out}"
    );
    let same = with(files, "crates/humanify-ag-model/clippy.toml", PURE_CLIPPY);
    let (code, out) = census_with(&same, &[]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn a_pure_crate_lib_without_the_deny_line_fails() {
    let files = with(base(), "crates/humanify-ag-core/src/lib.rs", "pub mod x;\n");
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("src/lib.rs lacks `#![deny(clippy::map_err_ignore"),
        "{out}"
    );
}

#[test]
fn an_io_dependency_in_a_pure_crate_fails() {
    let manifest = format!("{CORE_MANIFEST}\n[build-dependencies]\ncc = \"1\"\n").replace(
        "serde.workspace = true\n",
        "serde.workspace = true\ntokio.workspace = true\n",
    );
    let files = with(base(), "crates/humanify-ag-core/Cargo.toml", &manifest);
    let (code, out) = census_with(&files, &[]);
    assert_eq!(code, 1, "{out}");
    assert!(
        out.contains("dependency `tokio` is not on the pure crate's allowed list"),
        "{out}"
    );
    assert!(
        out.contains("a pure crate has no [build-dependencies]"),
        "{out}"
    );
    assert!(
        !out.contains("`serde`"),
        "an allowed dependency passes: {out}"
    );
}

// ─── this repository ──────────────────────────────────────────────────

#[test]
fn this_repositorys_pure_crates_depend_only_on_the_allowed_list() {
    assert_eq!(
        dependency_problems(&repo_root(), PURE_CRATES),
        Vec::<String>::new()
    );
}

#[test]
fn the_census_on_this_repository_is_green() {
    let (code, out, err) = run(&parse(&[]), &HostEnv::default());
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("EFFECTS CENSUS GREEN"), "{out}");
}
