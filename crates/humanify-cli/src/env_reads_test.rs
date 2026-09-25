//! Port of src/env-reads/analyze.test.ts and format.test.ts, fixture for
//! fixture, plus the Babel-shape cases the translation onto oxc must hold
//! (optional links, parens, write targets). The cross-binary corpus gate
//! is test/parity/wpb4-env-reads.sh.

use crate::env_reads::{EnvReadsReport, analyze_env_reads, format_env_reads_report};

fn analyze(code: &str) -> EnvReadsReport {
    analyze_env_reads(&[("in.js".to_string(), code.to_string())]).unwrap()
}

fn names(r: &EnvReadsReport) -> Vec<&str> {
    r.by_var.iter().map(|v| v.name.as_str()).collect()
}

// ---- analyze.test.ts ----

#[test]
fn resolves_direct_member_reads() {
    let r = analyze("const a = process.env.FOO;\nconst b = process.env.BAR;");
    assert_eq!(names(&r), ["BAR", "FOO"]);
}

#[test]
fn resolves_computed_string_literal_keys() {
    assert_eq!(names(&analyze("const a = process.env[\"BAZ\"];")), ["BAZ"]);
}

#[test]
fn flags_computed_dynamic_keys_as_unresolvable() {
    let r = analyze("function f(k) { return process.env[k]; }");
    assert!(names(&r).is_empty());
    assert_eq!(r.dynamic.len(), 1);
    assert!(r.dynamic[0].snippet.contains("process.env[k]"));
}

#[test]
fn aggregates_every_read_site_of_the_same_var() {
    let r = analyze("process.env.TOKEN;\nif (process.env.TOKEN) {}\nlog(process.env.TOKEN);");
    assert_eq!(names(&r), ["TOKEN"]);
    let lines: Vec<u32> = r.by_var[0].locations.iter().map(|l| l.line).collect();
    assert_eq!(lines, [1, 2, 3]);
}

#[test]
fn resolves_destructured_keys() {
    assert_eq!(
        names(&analyze("const { HOME, PATH } = process.env;")),
        ["HOME", "PATH"]
    );
}

#[test]
fn a_rest_element_is_an_enumerated_use() {
    let r = analyze("const { HOME, ...rest } = process.env;");
    assert_eq!(names(&r), ["HOME"]);
    assert_eq!(r.enumerated.len(), 1);
}

#[test]
fn follows_an_aliased_env_binding() {
    let r = analyze("const e = process.env;\nconst x = e.A;\nconst { B } = e;\ne['C'];");
    assert_eq!(names(&r), ["A", "B", "C"]);
}

#[test]
fn follows_a_chain_of_aliases() {
    assert_eq!(
        names(&analyze("const e = process.env;\nconst e2 = e;\ne2.DEEP;")),
        ["DEEP"]
    );
}

#[test]
fn recognizes_bun_env_and_import_meta_env() {
    assert_eq!(
        names(&analyze("Bun.env.RUNTIME;\nimport.meta.env.MODE;")),
        ["MODE", "RUNTIME"]
    );
}

#[test]
fn reports_whole_env_uses() {
    let r = analyze("const keys = Object.keys(process.env);");
    assert!(names(&r).is_empty());
    assert_eq!(r.enumerated.len(), 1);
}

#[test]
fn ignores_a_locally_shadowed_process() {
    let r = analyze("function f() { const process = { env: {} }; return process.env.NOPE; }");
    assert!(names(&r).is_empty());
}

#[test]
fn counts_the_number_of_files_analyzed() {
    let r = analyze_env_reads(&[
        ("a.js".into(), "process.env.A;".into()),
        ("b.js".into(), "process.env.B;".into()),
    ])
    .unwrap();
    assert_eq!(r.files_analyzed, 2);
    assert_eq!(names(&r), ["A", "B"]);
    let files: Vec<&str> = r
        .by_var
        .iter()
        .map(|v| v.locations[0].file.as_str())
        .collect();
    assert_eq!(files, ["a.js", "b.js"]);
}

// ---- format.test.ts ----

const SOURCE: &str = "const a = process.env.FOO;\nlog(process.env.FOO);\nconst b = process.env.BAR;\nconst c = process.env[dyn];\nconst keys = Object.keys(process.env);";

fn app_report() -> EnvReadsReport {
    analyze_env_reads(&[("app.js".into(), SOURCE.into())]).unwrap()
}

#[test]
fn renders_text_with_variables_sites_and_sections() {
    let text = format_env_reads_report(&app_report(), false);
    assert!(text.contains("1 file(s), 2 variable(s)"));
    assert!(text.contains("Variables (2)"));
    assert!(text.contains("FOO\n    app.js:1\n    app.js:2"));
    assert!(text.contains("Dynamic keys (1) — computed at runtime"));
    assert!(text.contains("Whole-env / enumerated uses (1)"));
}

#[test]
fn renders_markdown_headings_and_code_fenced_names() {
    let md = format_env_reads_report(&app_report(), true);
    assert!(md.starts_with("# Environment variable reads"));
    assert!(md.contains("## Variables (2)"));
    assert!(md.contains("- `FOO` — app.js:1, app.js:2"));
    assert!(md.contains("## Dynamic keys (1)"));
}

#[test]
fn omits_empty_sections() {
    let clean = analyze_env_reads(&[("x.js".into(), "const y = 1;".into())]).unwrap();
    let text = format_env_reads_report(&clean, false);
    assert!(!text.contains("Dynamic keys"));
    assert!(!text.contains("Variables ("));
}

// ---- the Babel shapes (lessons 2, 9, 14) ----

#[test]
fn optional_links_follow_babel_node_types() {
    // `process?.env` is an OptionalMemberExpression: never a base.
    assert!(names(&analyze("process?.env.A;")).is_empty());
    // A base under an optional link is an enumerated use, not a read.
    let r = analyze("process.env?.B;");
    assert!(names(&r).is_empty());
    assert_eq!(r.enumerated.len(), 1);
    // A later optional link leaves the base's parent plain.
    assert_eq!(names(&analyze("process.env.C?.x;")), ["C"]);
}

#[test]
fn parens_are_transparent_like_babel() {
    assert_eq!(names(&analyze("(process.env).P;")), ["P"]);
    assert_eq!(names(&analyze("const e = (process.env);\ne.Q;")), ["Q"]);
}

#[test]
fn assignment_targets_are_not_alias_references() {
    // `e = other` is a constant violation, not a referencePath.
    let r = analyze("let e = process.env;\ne = {};\ne.R;");
    assert_eq!(names(&r), ["R"]);
    assert!(r.enumerated.is_empty());
}

#[test]
fn locations_are_babel_lines_and_utf16_columns() {
    let r = analyze("x;\r\ny;\u{2028}\"é😀\"; process.env.Z;");
    let loc = &r.by_var[0].locations[0];
    assert_eq!((loc.line, loc.column), (3, 7));
}
