//! Library-detection tests (WPB.3), ported fixture-for-fixture from
//! `src/library-detection/comment-regions.test.ts`,
//! `adapters/default.test.ts` and `adapters/bun.test.ts`, plus the V8
//! regex-semantics edges the TS gets for free (ECMAScript `\s`, UTF-16
//! offsets).

use std::fs;
use std::path::{Path, PathBuf};

use crate::detect::js_text::utf16_offset;
use crate::libdetect::{
    CommentRegion, DetectedBy, LibraryDetector, classify_functions_by_region, detect_libraries,
    extract_library_name_from_path, find_comment_regions, is_library_path, normalize_library_name,
    relative_posix, select_library_detector,
};
use crate::unpack::UnpackedFile;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> TempDir {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "humanify-libdetect-{tag}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn write(&self, name: &str, content: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn files(paths: &[&Path]) -> Vec<UnpackedFile> {
    paths
        .iter()
        .map(|p| UnpackedFile {
            path: p.to_path_buf(),
            metadata: None,
        })
        .collect()
}

fn names(regions: &[CommentRegion]) -> Vec<&str> {
    regions.iter().map(|r| r.library_name.as_str()).collect()
}

// ---- comment-regions.test.ts: findCommentRegions ---------------------------

#[test]
fn no_banners_no_regions() {
    assert!(find_comment_regions("function foo() { return 42; }").is_empty());
}

#[test]
fn single_bang_banner() {
    let r = find_comment_regions("/*! React v18.2.0 */\nfunction a() {}");
    assert_eq!(
        r,
        vec![CommentRegion {
            library_name: "react".into(),
            start: 0,
            end: None
        }]
    );
}

#[test]
fn multiple_banners_make_sequential_regions() {
    let code = [
        "/*! React v18.2.0 */",
        "function reactInternal() {}",
        "/*! zustand v4.0.0 */",
        "function zustandStore() {}",
    ]
    .join("\n");
    let r = find_comment_regions(&code);
    assert_eq!(names(&r), vec!["react", "zustand"]);
    assert_eq!(r[0].start, 0);
    assert_eq!(r[0].end, Some(r[1].start));
    assert!(r[1].start > 0);
    assert_eq!(r[1].end, None);
}

#[test]
fn license_module_and_star_banners() {
    assert_eq!(
        names(&find_comment_regions("/** @license lodash */\nvar _ = {};")),
        vec!["lodash"]
    );
    assert_eq!(
        names(&find_comment_regions(
            "/** @module underscore */\nvar _ = {};"
        )),
        vec!["underscore"]
    );
    assert_eq!(
        names(&find_comment_regions(
            "/**\n * axios v1.6.0\n */\nfunction send() {}"
        )),
        vec!["axios"]
    );
}

#[test]
fn scans_the_whole_file() {
    let code = format!(
        "{}\n/*! React v18.2.0 */\nfunction a() {{}}",
        "x".repeat(2000)
    );
    assert_eq!(names(&find_comment_regions(&code)), vec!["react"]);
}

#[test]
fn dash_separator_and_normalization() {
    assert_eq!(
        names(&find_comment_regions(
            "/*! moment - v2.29.4 */\nfunction m() {}"
        )),
        vec!["moment"]
    );
    assert_eq!(
        names(&find_comment_regions("/*! jQuery, v3.6.0 */\nvar $;")),
        vec!["jquery"]
    );
}

#[test]
fn same_offset_matches_dedupe_to_the_first_pattern() {
    let r = find_comment_regions("/*! lodash v4.17.21 */\nvar _ = {};");
    assert!(!r.is_empty());
    assert_eq!(r[0].library_name, "lodash");
}

#[test]
fn regions_are_sorted_and_may_start_after_zero() {
    let code = [
        "function appCode() { return 1; }",
        "/*! lodash v4.17.21 */",
        "function chunk() {}",
        "/*! React v18.2.0 */",
        "function createElement() {}",
    ]
    .join("\n");
    let r = find_comment_regions(&code);
    assert_eq!(names(&r), vec!["lodash", "react"]);
    assert!(r[0].start < r[1].start);
    let r = find_comment_regions("var appVar = 1;\n/*! React v18.2.0 */\nfunction a() {}");
    assert!(r[0].start > 0);
}

// ---- comment-regions.test.ts: classifyFunctionsByRegion --------------------

fn region(name: &str, start: usize, end: Option<usize>) -> CommentRegion {
    CommentRegion {
        library_name: name.into(),
        start,
        end,
    }
}

#[test]
fn classify_without_regions_is_empty() {
    assert!(classify_functions_by_region([("fn1", Some(0))], &[]).is_empty());
}

#[test]
fn classify_inside_and_outside_a_region() {
    let regions = [region("react", 0, Some(100))];
    let r = classify_functions_by_region([("fn1", Some(50)), ("fn2", Some(150))], &regions);
    assert_eq!(r, vec![("fn1", "react".to_string())]);
}

#[test]
fn classify_in_the_open_last_region() {
    let regions = [region("react", 100, None)];
    let r = classify_functions_by_region([("fn1", Some(50)), ("fn2", Some(200))], &regions);
    assert_eq!(r, vec![("fn2", "react".to_string())]);
}

#[test]
fn classify_across_regions() {
    let regions = [
        region("react", 0, Some(100)),
        region("lodash", 200, Some(300)),
    ];
    let r = classify_functions_by_region(
        [
            ("app1", Some(150)),
            ("react1", Some(50)),
            ("lodash1", Some(250)),
            ("app2", Some(350)),
        ],
        &regions,
    );
    assert_eq!(
        r,
        vec![
            ("react1", "react".to_string()),
            ("lodash1", "lodash".to_string())
        ]
    );
}

#[test]
fn classify_boundaries_and_null_starts() {
    let regions = [region("react", 0, Some(100))];
    let r = classify_functions_by_region([("fn1", Some(0)), ("fn2", Some(100))], &regions);
    assert_eq!(r, vec![("fn1", "react".to_string())]);
    let open = [region("react", 0, None)];
    assert!(classify_functions_by_region([("fn1", None)], &open).is_empty());
}

// ---- adapters/default.test.ts ----------------------------------------------

#[test]
fn library_paths() {
    for p in [
        "node_modules/react/index.js",
        "./node_modules/lodash/lodash.js",
        "src/node_modules/internal/util.js",
        "@babel/runtime/helpers/classCallCheck.js",
        "core-js/modules/es.array.map.js",
        "regenerator-runtime/runtime.js",
        "tslib/tslib.es6.js",
        "webpack/runtime/define-property-getters",
    ] {
        assert!(is_library_path(p), "{p}");
    }
    for p in [
        "src/utils/helpers.js",
        "app/components/Button.js",
        "index.js",
        "0.js",
    ] {
        assert!(!is_library_path(p), "{p}");
    }
}

#[test]
fn library_names_from_paths() {
    let cases = [
        ("node_modules/react/index.js", "react"),
        (
            "node_modules/react-dom/cjs/react-dom.production.min.js",
            "react-dom",
        ),
        (
            "node_modules/@babel/runtime/helpers/classCallCheck.js",
            "@babel/runtime",
        ),
        (
            "@babel/runtime/helpers/interopRequireDefault.js",
            "@babel/runtime",
        ),
        ("lodash/map.js", "lodash"),
        ("core-js/modules/es.array.map.js", "core-js"),
        // An empty segment after node_modules/ fails the capture there; the
        // regex moves on to the next node_modules/.
        ("node_modules//x/node_modules/pkg/i.js", "pkg"),
        ("node_modules/@scope", "@scope"),
    ];
    for (path, want) in cases {
        assert_eq!(extract_library_name_from_path(path), want, "{path}");
    }
}

#[test]
fn registry_selects_bun_only_behind_the_bun_adapter() {
    assert_eq!(select_library_detector("bun"), LibraryDetector::Bun);
    assert_eq!(
        select_library_detector("webcrack"),
        LibraryDetector::Default
    );
    assert_eq!(
        select_library_detector("passthrough"),
        LibraryDetector::Default
    );
    assert!(LibraryDetector::Default.supports("bun"));
    assert!(!LibraryDetector::Bun.supports("passthrough"));
}

#[test]
fn default_detects_a_header_banner() {
    let t = TempDir::new("hdr");
    let p = t.write(
        "react.js",
        "/*! React v18.2.0 */\nfunction a() { return 1; }",
    );
    let r = detect_libraries(LibraryDetector::Default, &files(&[&p])).unwrap();
    assert_eq!(r.library_files.len(), 1);
    assert_eq!(r.library_files[0].1.library_name.as_deref(), Some("react"));
    assert_eq!(r.library_files[0].1.detected_by, Some(DetectedBy::Comment));
    assert!(r.mixed_files.is_empty(), "layer 2 wins over layer 3");
}

#[test]
fn default_detects_mixed_files() {
    let t = TempDir::new("mixed");
    let code = [
        format!("var appCode = \"{}\";\n", "x".repeat(1100)),
        "/*! React v18.2.0 */".into(),
        "function reactInternal() { return 2; }".into(),
        "/*! zustand v4.0.0 */".into(),
        "function zustandStore() { return 3; }".into(),
    ]
    .join("\n");
    let p = t.write("mixed.js", &code);
    let r = detect_libraries(LibraryDetector::Default, &files(&[&p])).unwrap();
    assert!(r.library_files.is_empty());
    assert_eq!(r.novel_files, vec![p.clone()]);
    assert_eq!(r.mixed_files.len(), 1);
    assert_eq!(r.mixed_files[0].1.regions.len(), 2);
    assert_eq!(r.mixed_files[0].1.library_names, vec!["react", "zustand"]);
}

#[test]
fn default_leaves_plain_files_novel() {
    let t = TempDir::new("plain");
    let p = t.write("app.js", "function app() { return 1; }");
    let r = detect_libraries(LibraryDetector::Default, &files(&[&p])).unwrap();
    assert!(r.mixed_files.is_empty());
    assert_eq!(r.novel_files, vec![p]);
}

#[test]
fn default_path_layer_uses_webcrack_metadata() {
    let t = TempDir::new("path");
    let p = t.write("0.js", "module.exports = 1;");
    let file = UnpackedFile {
        path: p.clone(),
        metadata: Some(crate::unpack::ModuleMetadata {
            id: "0".into(),
            module_path: "./node_modules/react/index.js".into(),
            is_entry: false,
        }),
    };
    let r = detect_libraries(LibraryDetector::Default, &[file]).unwrap();
    assert_eq!(r.library_files[0].1.detected_by, Some(DetectedBy::Path));
    assert_eq!(r.library_files[0].1.library_name.as_deref(), Some("react"));
}

// ---- adapters/bun.test.ts ---------------------------------------------------

#[test]
fn bun_banner_scan_reaches_past_1kb_and_never_mixes() {
    let t = TempDir::new("bunscan");
    let react = t.write(
        "react.js",
        "/*! React v18.2.0 */\nfunction a() { return 1; }",
    );
    let lodash = t.write(
        "lodash.js",
        &format!(
            "{}\n/*! lodash v4.17.21 */\nfunction chunk() {{}}",
            "x".repeat(2000)
        ),
    );
    let license = t.write(
        "lic.js",
        &format!(
            "{}\n/** @license MIT lodash */\nvar _ = {{}};",
            "x".repeat(2000)
        ),
    );
    let multi = t.write(
        "multi.js",
        "/*! React v18.2.0 */\nfunction r() { return 2; }\n/*! zustand v4.0.0 */\nfunction z() { return 3; }",
    );
    let app = t.write("app.js", "function app() { return 1; }");
    let r = detect_libraries(
        LibraryDetector::Bun,
        &files(&[&react, &lodash, &license, &multi, &app]),
    )
    .unwrap();
    let lib: Vec<(&Path, Option<&str>)> = r
        .library_files
        .iter()
        .map(|(p, d)| (p.as_path(), d.library_name.as_deref()))
        .collect();
    assert_eq!(
        lib,
        vec![
            (react.as_path(), Some("react")),
            (lodash.as_path(), Some("lodash")),
            (license.as_path(), Some("mit")),
            (multi.as_path(), Some("react")),
        ]
    );
    assert_eq!(r.novel_files, vec![app]);
    assert!(r.mixed_files.is_empty());
}

fn manifest(entries: &[(&str, &str)]) -> String {
    let factories: Vec<serde_json::Value> = entries
        .iter()
        .enumerate()
        .map(|(i, (file, name))| {
            serde_json::json!({
                "fileName": file, "name": name, "nameSource": "llm",
                "structuralHash": format!("{i:016}"), "factoryVar": "Q9k"
            })
        })
        .collect();
    serde_json::json!({"adapter": "bun", "runtimeFile": "runtime.js", "factories": factories})
        .to_string()
}

#[test]
fn bun_manifest_classifies_factories_by_root_relative_path() {
    let t = TempDir::new("bunman");
    let axios = t.write("vendor/axios.js", "function noop(){}");
    let lib = t.write("vendor/lib_abcdef12.js", "function noop2(){}");
    let vendor_runtime = t.write("vendor/runtime.js", "function v(){}");
    let runtime = t.write("runtime.js", "main()");
    t.write(
        "vendor/_bun-modules.json",
        &manifest(&[
            ("vendor/axios.js", "axios"),
            ("vendor/lib_abcdef12.js", "lib_abcdef12"),
            ("vendor/runtime.js", "runtime"),
        ]),
    );
    let r = detect_libraries(
        LibraryDetector::Bun,
        &files(&[&axios, &lib, &vendor_runtime, &runtime]),
    )
    .unwrap();
    let lib_names: Vec<Option<&str>> = r
        .library_files
        .iter()
        .map(|(_, d)| d.library_name.as_deref())
        .collect();
    assert_eq!(
        lib_names,
        vec![Some("axios"), Some("lib_abcdef12"), Some("runtime")]
    );
    assert_eq!(
        r.novel_files,
        vec![runtime],
        "only the ROOT runtime.js is the app"
    );
}

#[test]
fn bun_manifest_found_from_a_nested_first_file() {
    let t = TempDir::new("bunnest");
    let flat = t.write("vendor/index.js", "function a(){}");
    let nested = t.write("vendor/@scope/pkg/index.js", "function b(){}");
    let runtime = t.write("runtime.js", "main()");
    t.write(
        "vendor/_bun-modules.json",
        &manifest(&[
            ("vendor/index.js", "flat-lib"),
            ("vendor/@scope/pkg/index.js", "@scope/pkg"),
        ]),
    );
    let r = detect_libraries(LibraryDetector::Bun, &files(&[&nested, &flat, &runtime])).unwrap();
    let got: Vec<(&Path, Option<&str>)> = r
        .library_files
        .iter()
        .map(|(p, d)| (p.as_path(), d.library_name.as_deref()))
        .collect();
    assert_eq!(
        got,
        vec![
            (nested.as_path(), Some("@scope/pkg")),
            (flat.as_path(), Some("flat-lib"))
        ]
    );
    assert_eq!(r.novel_files, vec![runtime]);
}

// ---- V8 semantics by hand ---------------------------------------------------

#[test]
fn js_whitespace_classes_in_banners() {
    // U+FEFF and U+2028 are JS `\s`; U+0085 is NOT (it lands in `\S+`).
    assert_eq!(
        names(&find_comment_regions("/*!\u{feff}Lib\u{2028}v1.0 */")),
        vec!["lib"]
    );
    assert_eq!(
        names(&find_comment_regions("/*! a\u{85}b v1.0 */")),
        vec!["a\u{85}b"]
    );
    // A dash group needs whitespace after the dash.
    assert!(find_comment_regions("/*! foo -v1.0 */").is_empty());
    assert!(
        find_comment_regions("/*! foo v.x */")
            .iter()
            .all(|r| r.library_name == "foo")
    );
    assert!(find_comment_regions("/*! foo vx */").is_empty());
}

#[test]
fn license_star_is_optional_and_greedy() {
    assert_eq!(names(&find_comment_regions("/*@license a */")), vec!["a"]);
    assert_eq!(names(&find_comment_regions("/**@license b */")), vec!["b"]);
    assert!(find_comment_regions("/* *@license c */").is_empty());
    assert!(find_comment_regions("/** @licensed d */").is_empty());
}

#[test]
fn star_banner_needs_a_full_semver() {
    assert_eq!(
        names(&find_comment_regions(" * pkg v1.22.333")),
        vec!["pkg"]
    );
    assert!(find_comment_regions(" * pkg v1.2").is_empty());
    assert!(find_comment_regions("*pkg v1.2.3").is_empty());
}

#[test]
fn normalization_rules() {
    assert_eq!(
        normalize_library_name("@Babel/Runtime,;:!"),
        "babel/runtime"
    );
    assert_eq!(normalize_library_name("@@x"), "@x");
    assert_eq!(normalize_library_name("ÉMILE!"), "émile");
}

#[test]
fn offsets_convert_to_utf16_units() {
    let code = "\u{1F600}é /*! lib v1.0 */";
    let r = find_comment_regions(code);
    assert_eq!(r[0].start, 7, "bytes: 4 + 2 + 1");
    assert_eq!(utf16_offset(code, r[0].start), 4, "UTF-16: 2 + 1 + 1");
}

#[test]
fn node_path_relative() {
    assert_eq!(
        relative_posix(Path::new("/a/b"), Path::new("/a/b/vendor/x.js")),
        "vendor/x.js"
    );
    assert_eq!(
        relative_posix(Path::new("/a/b"), Path::new("/a/c.js")),
        "../c.js"
    );
    assert_eq!(relative_posix(Path::new("/a/b"), Path::new("/a/b")), "");
}
