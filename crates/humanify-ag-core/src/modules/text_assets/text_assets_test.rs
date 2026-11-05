use super::*;
use crate::ingest::Ingest;
use oxc_allocator::Allocator;

fn exported(function_src: &str) -> Option<String> {
    let allocator = Allocator::default();
    let src = format!("({function_src});");
    let ingest = Ingest::parse_unambiguous(&allocator, &src);
    let Some(Statement::ExpressionStatement(stmt)) = ingest.program.body.first() else {
        panic!("one expression");
    };
    exported_text(&stmt.expression)
}

#[test]
fn a_module_that_only_exports_text_is_recognised_in_every_spelling() {
    assert_eq!(
        exported("(e,m)=>{m.exports=\"hi there\"}").as_deref(),
        Some("hi there")
    );
    assert_eq!(
        exported("(e,m)=>m.exports=`a\\nb`").as_deref(),
        Some("a\nb")
    );
    // esbuild's object-method body is a plain function expression.
    assert_eq!(
        exported("function(exports, module){ module.exports = 'x'; }").as_deref(),
        Some("x")
    );
}

#[test]
fn anything_more_than_exported_text_is_code() {
    assert_eq!(
        exported("(e,m)=>{m.exports=`a${b}`}"),
        None,
        "a template slot"
    );
    assert_eq!(
        exported("(e,m)=>{e.exports=\"x\"}"),
        None,
        "not the module param"
    );
    assert_eq!(
        exported("(e,m)=>{m.exports=\"x\";f()}"),
        None,
        "a second statement"
    );
    assert_eq!(
        exported("(e,m)=>{m.exports+=\"x\"}"),
        None,
        "not a plain assignment"
    );
    assert_eq!(
        exported("(e,m)=>{m.exports={a:\"x\"}}"),
        None,
        "data, not text"
    );
    assert_eq!(exported("(e)=>{e.exports=\"x\"}"), None, "no module param");
}

#[test]
fn an_asset_is_named_from_its_lead_line() {
    let cases = [
        ("## Environment\n\nThree kinds of slot.", "environment"),
        (
            "<system-reminder>\nYou're running in a remote planning session. The user…",
            "youre-running-remote-planning-session",
        ),
        (
            "#!/usr/bin/env python3\n\"\"\"\nValidate a categorical chart palette against the checks.",
            "validate-categorical-chart-palette",
        ),
        (
            "/**\n * Validate a categorical chart palette against the checks.\n */",
            "validate-categorical-chart-palette",
        ),
        (
            "// esbuild bundling: dist entry → IIFE at window.<GLOBAL>",
            "esbuild-bundling",
        ),
        (
            "#!/usr/bin/env node\n// resync.mjs — THE re-sync path",
            "resync-mjs",
        ),
        (
            "// Shared by probe.mjs, compare.mjs, and package-capture.mjs.",
            "shared-probe-mjs",
        ),
    ];
    for (text, want) in cases {
        assert_eq!(asset_stem(text).as_deref(), Some(want), "{text:?}");
    }
}

#[test]
fn an_asset_with_nothing_to_name_it_by_gets_no_stem() {
    assert_eq!(asset_stem(""), None);
    assert_eq!(asset_stem("\n\n<tag>\n"), None);
    assert_eq!(asset_stem("## Utils"), None, "generic names are refused");
    assert_eq!(asset_stem("日本語のテキスト"), None);
}

#[test]
fn the_assets_folder_is_inside_the_app_tree_and_never_a_split_name() {
    let code = crate::place::layout::CODE_DIR;
    let folder = ASSETS_DIR
        .strip_prefix(code)
        .and_then(|r| r.strip_prefix('/'))
        .expect("under the app tree");
    assert_ne!(crate::place::stems::to_kebab_case(folder), folder);
}
