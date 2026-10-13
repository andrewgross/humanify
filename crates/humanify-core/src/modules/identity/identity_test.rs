//! The bundler pieces (what Bun and esbuild kept), the reader cascade, the
//! declared pairing and the declared file names.

use super::provenance::{bun_module_provenance, esbuild_bundle_facts, esbuild_module_provenance};
use super::{
    DeclaredIdentity, DeclaredPath, ModuleProvenance, PathKind, declare, member_stem,
    pair_by_declared_identity, scan_app_declarations,
};
use oxc_span::Span;

// ---- Bun: what it kept ------------------------------------------------------

#[test]
fn bun_keeps_the_absolute_paths_a_module_asked_for() {
    let body = r#"function(e,t){var __dirname="/home/runner/work/app/node_modules/@grpc/grpc-js/build/src";var r=m6.resolve("/w/node_modules/meta-cjs/lib/other.js");var u="file:///w/node_modules/open/index.js";var s="/not/a/location";}"#;
    let p = bun_module_provenance(body, None);
    let paths: Vec<&str> = p.paths.iter().map(|d| d.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/home/runner/work/app/node_modules/@grpc/grpc-js/build/src",
            "/w/node_modules/meta-cjs/lib/other.js",
            "file:///w/node_modules/open/index.js",
        ],
        "a plain string is not a location the bundler inlined"
    );
    assert!(p.paths.iter().all(|d| d.kind == PathKind::Inlined));
}

#[test]
fn bun_hands_over_the_kept_legal_comment() {
    let p = bun_module_provenance("(e,t)=>{}", Some("@azure/msal-node v3.8.1"));
    assert_eq!(p.kept_comments, vec!["@azure/msal-node v3.8.1".to_string()]);
    let id = declare("(e,t)=>{}", &p).identity.unwrap();
    assert_eq!(id.source, "banner");
    assert_eq!(id.package.as_deref(), Some("@azure/msal-node"));
    assert_eq!(id.version.as_deref(), Some("3.8.1"));
}

// ---- esbuild: what it kept --------------------------------------------------

const ESBUILD_TAIL: &str = "var x=1;\n/*! Bundled license information:\n\nlegal-lib/index.js:\n  (*! legal-lib v1.2.3 | (c) Someone | MIT License *)\n  (**\n   * @license legal-lib\n   * Copyright Someone. Licensed MIT.\n   *)\n  (* @preserve keep me *)\n\nlodash-es/lodash.js:\n  (**\n   * @license\n   * Lodash (Custom Build) <https://lodash.com/>\n   *)\n*/\n";

#[test]
fn esbuild_reads_its_end_of_file_legal_block_by_package_path() {
    let facts = esbuild_bundle_facts(ESBUILD_TAIL);
    let keys: Vec<&str> = facts
        .legal_comments
        .iter()
        .map(|c| c.path.as_str())
        .collect();
    assert_eq!(
        keys,
        vec![
            "legal-lib/index.js",
            "legal-lib/index.js",
            "legal-lib/index.js",
            "lodash-es/lodash.js"
        ]
    );
    assert!(
        facts.legal_comments[0]
            .text
            .starts_with("! legal-lib v1.2.3")
    );
    assert!(esbuild_bundle_facts("var a=1;").legal_comments.is_empty());
}

#[test]
fn esbuild_attributes_the_block_and_the_wrapper_key_to_the_module() {
    let facts = esbuild_bundle_facts(ESBUILD_TAIL);
    let p = esbuild_module_provenance(None, Some("node_modules/legal-lib/index.js"), &facts);
    assert_eq!(p.kept_comments.len(), 3);
    assert_eq!(
        p.paths,
        vec![DeclaredPath {
            kind: PathKind::ModuleFile,
            path: "node_modules/legal-lib/index.js".into()
        }]
    );
    let id = declare("(exports, module) => {}", &p).identity.unwrap();
    assert_eq!(
        id.source, "source-path",
        "the bundler's own path is trusted first"
    );
    assert_eq!(id.package.as_deref(), Some("legal-lib"));
    assert_eq!(id.member.as_deref(), Some("index.js"));
    assert_eq!(
        id.version.as_deref(),
        Some("1.2.3"),
        "the banner agreeing on the package adds its version"
    );
    // Minified: no key, so the block cannot be attributed to a module.
    let p = esbuild_module_provenance(None, None, &facts);
    assert!(p.kept_comments.is_empty() && p.paths.is_empty());
}

// ---- the cascade --------------------------------------------------------------

#[test]
fn a_disagreeing_later_reader_adds_nothing() {
    let p = ModuleProvenance {
        kept_comments: vec!["! other-pkg v9.9.9".into()],
        paths: Vec::new(),
    };
    let body = r#"(e,t)=>{t.exports={name:"gaxios",version:"7.1.4",main:"x.js"}}"#;
    let id = declare(body, &p).identity.unwrap();
    assert_eq!(id.source, "package-json");
    assert_eq!(id.package.as_deref(), Some("gaxios"));
    assert_eq!(id.version.as_deref(), Some("7.1.4"));
}

#[test]
fn hints_are_kept_beside_no_identity() {
    let body = r#"(e,t)=>{e.VERSION="1.9.0";throw Error("see https://github.com/o/r")}"#;
    let d = declare(body, &ModuleProvenance::default());
    assert_eq!(d.identity, None);
    assert_eq!(
        d.hints,
        vec![
            "version 1.9.0".to_string(),
            "repository github.com/o/r".to_string()
        ]
    );
}

// ---- file stems ----------------------------------------------------------------

#[test]
fn declared_names_become_short_stable_stems() {
    assert_eq!(member_stem("ActionScript").as_deref(), Some("actionscript"));
    assert_eq!(member_stem("C++").as_deref(), Some("cpp"));
    assert_eq!(member_stem("C#").as_deref(), Some("csharp"));
    assert_eq!(member_stem("Go").as_deref(), Some("go"));
    assert_eq!(member_stem("D").as_deref(), Some("d"));
    assert_eq!(
        member_stem("G-code (ISO 6983)").as_deref(),
        Some("g-code-iso-6983")
    );
    // A declared NAME with a slash is a name, not a path.
    assert_eq!(member_stem("C/AL").as_deref(), Some("c-al"));
    assert_eq!(
        member_stem("Backus\u{2013}Naur Form").as_deref(),
        Some("backus-naur-form")
    );
    assert_eq!(
        member_stem("Caché Object Script").as_deref(),
        Some("cache-object-script")
    );
    assert_eq!(member_stem("lib/index.js").as_deref(), Some("lib/index"));
    assert_eq!(
        member_stem("lib/types.d.ts").as_deref(),
        Some("lib/types.d.ts")
    );
    assert_eq!(member_stem("package.json").as_deref(), Some("package.json"));
    assert_eq!(member_stem("...").as_deref(), None);
}

fn grammar(name: &str, aliases: &[&str]) -> DeclaredIdentity {
    DeclaredIdentity {
        source: "highlight-js-grammar".into(),
        package: Some("highlight.js".into()),
        version: None,
        member: Some(name.into()),
        aliases: aliases.iter().map(|a| a.to_string()).collect(),
        evidence: String::new(),
    }
}

#[test]
fn a_member_is_filed_in_its_package_folder_only_under_the_package_name() {
    let id = grammar("ActionScript", &["as"]);
    assert_eq!(
        id.member_file("highlight.js"),
        Some(("highlight".into(), "actionscript".into()))
    );
    assert_eq!(
        id.member_file("javascript"),
        None,
        "a carried name keeps its file"
    );
    assert_eq!(id.name().as_deref(), Some("highlight.js"));
}

// ---- pairing --------------------------------------------------------------------

#[test]
fn equal_unique_keys_pair_case_folded() {
    let fresh = [
        grammar("JavaScript", &["js"]),
        grammar("Go", &[]),
        grammar("Wren", &[]),
    ];
    let prior = [grammar("Go", &["golang"]), grammar("Javascript", &[])];
    let pairs = pair_by_declared_identity(
        &fresh.iter().map(Some).collect::<Vec<_>>(),
        &prior.iter().map(Some).collect::<Vec<_>>(),
    );
    let got: Vec<(usize, usize)> = pairs.iter().map(|p| (p.fresh, p.prior)).collect();
    assert_eq!(got, vec![(0, 1), (1, 0)], "Wren is new: no pair");
}

#[test]
fn a_shared_key_pairs_only_through_its_aliases() {
    // highlight.js 10.x: `c` and `cpp` both declare "C++".
    let fresh = [grammar("C++", &["cc", "hpp"]), grammar("C++", &["h"])];
    let prior = [grammar("C++", &["h"]), grammar("C++", &["cc", "hpp"])];
    let pairs = pair_by_declared_identity(
        &fresh.iter().map(Some).collect::<Vec<_>>(),
        &prior.iter().map(Some).collect::<Vec<_>>(),
    );
    let got: Vec<(usize, usize)> = pairs.iter().map(|p| (p.fresh, p.prior)).collect();
    assert_eq!(got, vec![(0, 1), (1, 0)]);
    // Same aliases too: ambiguous, left to the content pairing.
    let same = [
        grammar("Handlebars", &["hbs"]),
        grammar("Handlebars", &["hbs"]),
    ];
    let one = [grammar("Handlebars", &["hbs"])];
    assert!(
        pair_by_declared_identity(
            &one.iter().map(Some).collect::<Vec<_>>(),
            &same.iter().map(Some).collect::<Vec<_>>(),
        )
        .is_empty()
    );
}

#[test]
fn a_module_without_a_declared_identity_is_never_paired_by_it() {
    let prior = [grammar("Go", &[])];
    assert!(
        pair_by_declared_identity(&[None], &prior.iter().map(Some).collect::<Vec<_>>()).is_empty()
    );
}

// ---- the version constant completes an identity, never starts one -----------

#[test]
fn a_version_constant_adds_the_version_to_another_reader_s_name() {
    let body = r#"(e,t)=>{var hv="3.936.0";e.VERSION=hv;function f(){throw Error("@aws-sdk/core::resolveAwsSdkSigV4Config - `credentials` missing")}}"#;
    let id = declare(body, &ModuleProvenance::default())
        .identity
        .unwrap();
    assert_eq!(id.source, "scoped-error-prefix");
    assert_eq!(id.package.as_deref(), Some("@aws-sdk/core"));
    assert_eq!(id.version.as_deref(), Some("3.936.0"));
}

#[test]
fn a_version_constant_alone_is_a_hint_not_an_identity() {
    let body = r#"(e6n)=>{Object.defineProperty(e6n,"__esModule",{value:!0});e6n.VERSION=void 0;e6n.VERSION="1.9.0"}"#;
    let d = declare(body, &ModuleProvenance::default());
    assert_eq!(d.identity, None);
    assert_eq!(d.hints, vec!["version 1.9.0".to_string()]);
}

#[test]
fn a_declared_version_is_not_overwritten_by_the_constant() {
    // The package.json's own version stands.
    let body =
        r#"(e,t)=>{t.exports={name:"gaxios",version:"7.1.4",main:"x.js"};e.VERSION="9.9.9"}"#;
    let id = declare(body, &ModuleProvenance::default())
        .identity
        .unwrap();
    assert_eq!(id.version.as_deref(), Some("7.1.4"));
}

// ---- app code: report-only declarations -------------------------------------

#[test]
fn app_statements_report_library_declarations_without_naming_anything() {
    // Library code Bun hoisted into the app (no module boundary): an AWS
    // command class, an inlined client package.json, a scoped error; then
    // the app's own slash-command table and its own Cedar grammar.
    let stmts = [
        r#"var Q9;var Nwu=b(()=>{Q9=class extends B.classBuilder().s("AmazonBedrockControlPlaneService","ListTagsForResource",{}).n("BedrockClient","ListTagsForResourceCommand").build(){}});"#,
        r#"var Cj={name:"@aws-sdk/client-sso",description:"AWS SDK for JavaScript Sso Client",version:"3.936.0",main:"./dist-cjs/index.js",license:"Apache-2.0"};"#,
        r#"function f(){throw Error("@aws-sdk/token-providers - fromSso")}"#,
        r#"var cmds=[{name:"teleport",aliases:["tp"],description:"Resume a remote session",type:"local-jsx"}];"#,
        r#"var aUg=(e)=>({name:"Cedar",aliases:["cedarpolicy"],keywords:{keyword:"permit forbid"},contains:[e.QUOTE_STRING_MODE]});"#,
        r#"var m={ISSUES:"x",VERSION:"2.1.216"}.VERSION;"#,
    ];
    let mut code = String::new();
    let mut spans = Vec::new();
    for s in stmts {
        let start = code.len() as u32;
        code.push_str(s);
        spans.push(Span::new(start, code.len() as u32));
    }
    let rows = scan_app_declarations(&code, &spans);
    let got: Vec<(u32, &str, Option<&str>, Option<&str>)> = rows
        .iter()
        .map(|r| {
            (
                r.start,
                r.reader.as_str(),
                r.package.as_deref().or(r.client.as_deref()),
                r.version.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                spans[0].start,
                "aws-sdk-client",
                Some("BedrockClient"),
                None
            ),
            (
                spans[1].start,
                "package-json",
                Some("@aws-sdk/client-sso"),
                Some("3.936.0")
            ),
            (
                spans[2].start,
                "scoped-error-prefix",
                Some("@aws-sdk/token-providers"),
                None
            ),
        ],
        "the slash-command table, the app's own grammar and its build-constant object report nothing"
    );
    assert_eq!(
        rows[0].commands,
        vec!["ListTagsForResourceCommand".to_string()]
    );
}
