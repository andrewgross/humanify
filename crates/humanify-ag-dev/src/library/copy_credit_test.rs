use super::*;
use std::collections::BTreeSet;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("dev-copy15-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn set(xs: &[&str]) -> Lits {
    xs.iter().map(|s| s.to_string()).collect()
}

const H_HYPER: &str = "00000000000000a1";
const H_NAPI: &str = "00000000000000b2";
const H_STREAM: &str = "00000000000000c3";
const H_CHALK: &str = "00000000000000d4";
const H_PATH: &str = "00000000000000e5";
const H_RESOLVE: &str = "00000000000000f6";
const H_DISCARD: &str = "0000000000000107";

/// An index slice: (hash, owner, file) unit rows.
fn index(dir: &Path, rows: &[(&str, &str, &str)]) -> Index {
    let mut text = String::new();
    let mut versions: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (h, owner, file) in rows {
        text.push_str(&format!(
            "{{\"hash\":\"{h}\",\"owner\":\"{owner}\",\"file\":\"{file}\",\"nodes\":80,\"moduleBytes\":900,\"versions\":[[\"1.0.0\",\"1.0.0\"]]}}\n"
        ));
        versions.insert(owner.to_string(), vec!["1.0.0".into()]);
    }
    std::fs::write(dir.join("units.s.jsonl"), text).unwrap();
    std::fs::write(
        dir.join("versions.json"),
        serde_json::to_string(&versions).unwrap(),
    )
    .unwrap();
    Index::load(dir, "s").unwrap()
}

fn lit_row(owner: &str, file: &str, lits: &[&str]) -> LiteralRow {
    LiteralRow {
        owner: owner.into(),
        file: file.into(),
        lits: lits.iter().map(|s| s.to_string()).collect(),
        versions: vec![("1.0.0".into(), "1.0.0".into())],
    }
}

fn path_lits(prefix: &str) -> Vec<String> {
    (0..24)
        .map(|i| format!("S:{prefix}-path-literal-{i}"))
        .collect()
}

struct World {
    ix: Index,
    lits: LiteralIndex,
    registry: Registry,
}

const HYPER_UNIT: [&str; 4] = [
    "S:holepunch-relay",
    "S:connect-timeout",
    "P:firewalled",
    "N:4000",
];
const NAPI_TAKUMI: [&str; 3] = [
    "S:@takumi-rs/core-linux-x64-gnu",
    "S:@takumi-rs/core-darwin-arm64",
    "P:platformArchABI",
];
const NAPI_ASTGREP: [&str; 3] = [
    "S:@ast-grep/napi-linux-x64-gnu",
    "S:@ast-grep/napi-darwin-arm64",
    "P:platformArchABI",
];
const DECODER_UNIT: [&str; 4] = ["S:utf16le", "S:ucs2", "P:isEncoding", "S:latin1"];
const CHALK_UNIT: [&str; 3] = ["S:FORCE_COLOR", "S:truecolor", "P:hasBasic"];
const RESOLVE_UNIT: [&str; 3] = [
    "S:ERR_INVALID_MODULE_SPECIFIER",
    "S:imports-resolve",
    "P:packageResolve",
];
const DISCARD_UNIT: [&str; 3] = ["S:keypress", "P:setRawMode", "S:discarded-input"];

fn world(tag: &str) -> World {
    let d = scratch(tag);
    let ix = index(
        &d,
        &[
            (H_HYPER, "hyperdht", "lib/connect.js"),
            (H_NAPI, "@takumi-rs/core", "index.js"),
            // contamination: string_decoder's print filed under a
            // readable-stream file by a dependent's build map
            (H_STREAM, "readable-stream", "lib/internal/streams/from.js"),
            (H_CHALK, "chalk", "source/vendor/supports-color/index.js"),
            (H_PATH, "monaco-editor", "esm/vs/base/common/path.js"),
            (H_PATH, "monaco-editor", "esm/vs/base/common/index.js"),
            (
                H_RESOLVE,
                "@dual-bundle/import-meta-resolve",
                "lib/resolve.js",
            ),
            (H_DISCARD, "stdin-discarder", "index.js"),
        ],
    );
    let mut monaco: Vec<String> = path_lits("vs");
    monaco.extend([
        "S:win32-sep".into(),
        "P:normalize".into(),
        "S:posix-sep".into(),
    ]);
    let monaco_refs: Vec<&str> = monaco.iter().map(String::as_str).collect();
    let lits = LiteralIndex::from_rows(&[
        lit_row("hyperdht", "lib/connect.js", &HYPER_UNIT),
        lit_row("@takumi-rs/core", "index.js", &NAPI_TAKUMI),
        lit_row(
            "readable-stream",
            "lib/internal/streams/from.js",
            &[
                "S:readable-from",
                "P:asyncIterator",
                "S:ERR_INVALID_ARG_TYPE",
            ],
        ),
        lit_row(
            "chalk",
            "source/vendor/supports-color/index.js",
            &CHALK_UNIT,
        ),
        lit_row("monaco-editor", "esm/vs/base/common/path.js", &monaco_refs),
        lit_row("monaco-editor", "esm/vs/base/common/index.js", &monaco_refs),
        lit_row(
            "@dual-bundle/import-meta-resolve",
            "lib/resolve.js",
            &RESOLVE_UNIT,
        ),
        lit_row("stdin-discarder", "index.js", &DISCARD_UNIT),
    ]);
    let reg = d.join("registry");
    std::fs::create_dir_all(&reg).unwrap();
    for (name, body) in [
        ("hyperdht-hugin@1.0.3", r#"{"dependencies":{"b4a":"^1"}}"#),
        (
            "string_decoder@1.3.0",
            r#"{"dependencies":{"readable-stream":"^3"}}"#,
        ),
        (
            "ora@8.0.0",
            r#"{"dependencies":{"stdin-discarder":"^0.2.1"}}"#,
        ),
        ("@ast-grep__napi@0.1.0", r#"{"dependencies":{}}"#),
        ("chalk@latest", r#"{"dependencies":{}}"#),
        ("ghost-pkg@1.0.0", r#"{"__error": 404}"#),
        (
            "hyperdht-extension-relay@1.0.0",
            r#"{"dependencies":{},"devDependencies":{"hyperdht":"^6"}}"#,
        ),
    ] {
        std::fs::write(reg.join(format!("{name}.json")), body).unwrap();
    }
    World {
        ix,
        lits,
        registry: Registry::new(&reg),
    }
}

impl World {
    fn judge(&self) -> Judge<'_> {
        Judge {
            ix: &self.ix,
            lits: &self.lits,
            registry: &self.registry,
        }
    }
}

struct Case {
    claimed: &'static str,
    truth: &'static str,
    version: Option<&'static str>,
    file: &'static str,
    print: &'static str,
    unit: Lits,
    truth_file: Lits,
}

fn run(w: &World, c: &Case) -> Outcome {
    let tl = [c.truth_file.clone()];
    w.judge().judge(&Claim {
        claimed: c.claimed,
        truth: c.truth,
        truth_version: c.version,
        truth_file: Some(c.file),
        print: Some(super::super::index::hash_key(c.print).unwrap()),
        unit_lits: Some(&c.unit),
        truth_lits: Some(&tl),
    })
}

fn hyper_case() -> Case {
    Case {
        claimed: "hyperdht",
        truth: "hyperdht-hugin",
        version: Some("1.0.3"),
        file: "lib/connect.js",
        print: H_HYPER,
        unit: set(&HYPER_UNIT),
        truth_file: set(&HYPER_UNIT),
    }
}

fn credited(o: &Outcome) -> Link {
    match o {
        Outcome::Credited(c) => c.link,
        other => panic!("not credited: {other:?}"),
    }
}

#[test]
fn a_renamed_fork_s_copy_is_credited_to_the_package_it_came_from() {
    let w = world("fork");
    assert_eq!(credited(&run(&w, &hyper_case())), Link::Fork);
}

#[test]
fn unrelated_packages_with_look_alike_code_stay_wrong() {
    // NEGATIVE: two napi-rs loaders print alike (strings are blurred in a
    // print) but name different packages; the claimed file lacks them.
    let w = world("napi");
    let c = Case {
        claimed: "@takumi-rs/core",
        truth: "@ast-grep/napi",
        version: Some("0.1.0"),
        file: "index.js",
        print: H_NAPI,
        unit: set(&NAPI_ASTGREP),
        truth_file: set(&NAPI_ASTGREP),
    };
    assert_eq!(run(&w, &c), Outcome::Refused("literals-not-in-claimed"));
}

#[test]
fn a_generated_twin_with_equal_literals_but_no_link_stays_wrong() {
    // NEGATIVE: even byte-identical code is not credited between two
    // packages nothing links (generic basename, unrelated names).
    let w = world("twin");
    let c = Case {
        claimed: "@takumi-rs/core",
        truth: "@ast-grep/napi",
        version: Some("0.1.0"),
        file: "index.js",
        print: H_NAPI,
        unit: set(&NAPI_TAKUMI),
        truth_file: set(&NAPI_TAKUMI),
    };
    assert_eq!(run(&w, &c), Outcome::Refused("no-link"));
}

#[test]
fn a_contaminated_index_row_is_not_evidence_even_with_a_link() {
    // NEGATIVE: readable-stream's row holds string_decoder's print (a
    // build map misfiled it), and string_decoder's manifest names
    // readable-stream; readable-stream's published file holds none of the
    // unit's strings, so the claim stays wrong.
    let w = world("contaminated");
    let c = Case {
        claimed: "readable-stream",
        truth: "string_decoder",
        version: Some("1.3.0"),
        file: "lib/string_decoder.js",
        print: H_STREAM,
        unit: set(&DECODER_UNIT),
        truth_file: set(&DECODER_UNIT),
    };
    assert_eq!(run(&w, &c), Outcome::Refused("literals-not-in-claimed"));
}

#[test]
fn a_claimed_package_that_vendors_the_truth_stays_wrong() {
    // NEGATIVE: chalk ships supports-color in source/vendor/; the truth
    // (supports-color) is the origin, so chalk is not where it came from.
    let w = world("reverse");
    let c = Case {
        claimed: "chalk",
        truth: "supports-color",
        version: Some("9.0.0"),
        file: "index.js",
        print: H_CHALK,
        unit: set(&CHALK_UNIT),
        truth_file: set(&CHALK_UNIT),
    };
    assert_eq!(run(&w, &c), Outcome::Refused("claimed-ships-truth"));
}

#[test]
fn a_truth_file_without_the_code_is_no_copy() {
    let w = world("lacks");
    let mut c = hyper_case();
    c.truth_file = set(&["S:something-else", "P:other"]);
    assert_eq!(run(&w, &c), Outcome::Refused("truth-file-lacks"));
}

#[test]
fn a_print_the_claimed_package_does_not_hold_is_no_copy() {
    let w = world("noprint");
    let mut c = hyper_case();
    c.print = H_NAPI;
    assert_eq!(run(&w, &c), Outcome::Refused("print-not-in-claimed"));
}

#[test]
fn too_few_literals_or_no_print_is_never_credited() {
    let w = world("few");
    let mut c = hyper_case();
    c.unit = set(&["S:holepunch-relay", "N:4000"]);
    assert_eq!(run(&w, &c), Outcome::Refused("few-literals"));
    let tl = [set(&HYPER_UNIT)];
    let unit = set(&HYPER_UNIT);
    let o = w.judge().judge(&Claim {
        claimed: "hyperdht",
        truth: "hyperdht-hugin",
        truth_version: Some("1.0.3"),
        truth_file: Some("lib/connect.js"),
        print: None,
        unit_lits: Some(&unit),
        truth_lits: Some(&tl),
    });
    assert_eq!(o, Outcome::Refused("no-print"));
}

#[test]
fn a_truth_package_that_bundles_its_dependency_is_credited() {
    let w = world("dep");
    let c = Case {
        claimed: "stdin-discarder",
        truth: "ora",
        version: Some("8.0.0"),
        file: "index.js",
        print: H_DISCARD,
        unit: set(&DISCARD_UNIT),
        truth_file: set(&DISCARD_UNIT),
    };
    assert_eq!(credited(&run(&w, &c)), Link::Dependency);
}

#[test]
fn a_vendored_copy_is_credited() {
    let w = world("vendored");
    let c = Case {
        claimed: "@dual-bundle/import-meta-resolve",
        truth: "@babel/core",
        version: Some("7.29.7"),
        file: "lib/vendor/import-meta-resolve.js",
        print: H_RESOLVE,
        unit: set(&RESOLVE_UNIT),
        truth_file: set(&RESOLVE_UNIT),
    };
    assert_eq!(credited(&run(&w, &c)), Link::Vendored);
}

#[test]
fn the_same_file_shipped_by_both_is_credited_but_not_a_generic_one() {
    let w = world("samefile");
    let unit = set(&["S:win32-sep", "P:normalize", "S:posix-sep"]);
    let mut file: BTreeSet<String> = path_lits("vs").into_iter().collect();
    file.extend(unit.iter().cloned());
    let c = Case {
        claimed: "monaco-editor",
        truth: "@vscode/prompt-tsx",
        version: Some("0.3.0-alpha.24"),
        file: "dist/base/util/vs/common/path.js",
        print: H_PATH,
        unit: unit.clone(),
        truth_file: file.clone(),
    };
    let o = run(&w, &c);
    assert_eq!(credited(&o), Link::SameFile);
    // the same content under a generic basename (index.js) is no link
    let c = Case {
        file: "dist/base/util/vs/common/index.js",
        ..c
    };
    assert_eq!(run(&w, &c), Outcome::Refused("no-link"));
}

#[test]
fn fork_names_need_a_real_stem_and_a_compatible_scope() {
    assert!(fork_name("rambda", "rambdax"));
    assert!(fork_name("hyperdht", "hyperdht-hugin"));
    assert!(fork_name("react-is", "@jest/react-is-18"));
    assert!(fork_name("sudo-prompt", "@vscode/sudo-prompt"));
    assert!(!fork_name("@vue/reactivity", "@reactive-vscode/reactivity"));
    assert!(!fork_name("uuid", "uuidv4"));
    assert!(!fork_name("hyperdht", "hyperdht"));
    assert!(!fork_name("chalk", "supports-color"));
}

#[test]
fn vendored_paths_and_the_reverse_direction() {
    assert!(vendored_path(
        "lib/vendor/import-meta-resolve.js",
        "@dual-bundle/import-meta-resolve"
    ));
    assert!(vendored_path(
        "dist/vendor/@scope/thing/index.js",
        "@scope/thing"
    ));
    assert!(!vendored_path("bundled/rough.esm.js", "mermaid"));
    assert!(!vendored_path(
        "lib/import-meta-resolve.js",
        "import-meta-resolve"
    ));
    assert!(ships_truth(
        "source/vendor/supports-color/index.js",
        "supports-color"
    ));
    assert!(!ships_truth("lib/connect.js", "hyperdht-hugin"));
}

#[test]
fn a_manifest_declares_any_dependency_kind() {
    let m: Value = serde_json::from_str(
        r#"{"dependencies":{"a":"1"},"peerDependencies":{"b":"1"},"bundleDependencies":["c"]}"#,
    )
    .unwrap();
    assert!(declares(&m, "a") && declares(&m, "b") && declares(&m, "c"));
    assert!(!declares(&m, "d"));
    let w = world("registry");
    assert!(w.registry.manifest("ghost-pkg", Some("1.0.0")).is_none());
    assert!(w.registry.manifest("chalk", None).is_some());
}

#[test]
fn an_extension_built_against_the_claimed_package_is_no_fork() {
    // NEGATIVE: `<pkg>-extension-x` develops against <pkg> (a
    // devDependency): its code is its own, however alike
    let w = world("extension");
    let mut c = hyper_case();
    c.truth = "hyperdht-extension-relay";
    c.version = Some("1.0.0");
    assert_eq!(run(&w, &c), Outcome::Refused("no-link"));
}

#[test]
fn a_fork_needs_the_truth_s_manifest() {
    // a fork name with no registry manifest to check is not credited
    let w = world("fork-unknown");
    let mut c = hyper_case();
    c.truth = "hyperdht-unknown";
    c.version = Some("9.9.9");
    assert_eq!(run(&w, &c), Outcome::Refused("no-link"));
}
