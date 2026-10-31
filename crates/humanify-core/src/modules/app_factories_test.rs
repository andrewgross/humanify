//! exp172: the app-factory decision (pure) and the entry gate (parsed).

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::Statement;

use super::{AppReason, FactoryEvidence, decide, entry_factories};
use crate::ingest::Ingest;

fn module(packages: &[&str], requires: &[usize]) -> FactoryEvidence {
    FactoryEvidence {
        packages: packages
            .iter()
            .map(|p| p.to_string())
            .collect::<BTreeSet<_>>(),
        requires: requires.to_vec(),
        ..FactoryEvidence::default()
    }
}

fn accepted(e: &[FactoryEvidence], entries: Option<&[usize]>) -> Vec<(usize, AppReason)> {
    decide(e, entries).modules
}

#[test]
fn a_private_app_moves_its_entry_and_nothing_it_requires() {
    // 0 entry → 1 (unattributed) and 2 (lodash). 1 may be an unindexed
    // library: nothing is accepted downward.
    let e = vec![
        module(&[], &[1, 2]),
        module(&[], &[]),
        module(&["lodash"], &[]),
    ];
    assert_eq!(accepted(&e, Some(&[0])), vec![(0, AppReason::Entry)]);
}

#[test]
fn nothing_moves_without_an_entry() {
    // The app text runs more than factories (an ES-module app): no entry,
    // and an own-package claim alone moves nothing.
    let e = vec![module(&[], &[1]), module(&["app"], &[0])];
    assert!(accepted(&e, None).is_empty());
}

#[test]
fn a_package_that_calls_back_into_the_entrys_modules_is_the_apps_own() {
    // 0 entry → 1 (app) → 2 (unattributed) → 3 (app) → 4 (lodash);
    // 2 → 5 (unattributed leaf). 1 requires 2 and 2 requires 3: mutual.
    let e = vec![
        module(&[], &[1, 2]),
        module(&["app"], &[2]),
        module(&[], &[3, 5]),
        module(&["app"], &[4]),
        module(&["lodash"], &[]),
        module(&[], &[]),
    ];
    let d = decide(&e, Some(&[0]));
    assert_eq!(d.own_packages, vec!["app".to_string()]);
    assert_eq!(
        d.modules,
        vec![
            (0, AppReason::Entry),
            (1, AppReason::OwnPackage),
            (2, AppReason::RequiresApp),
            (3, AppReason::OwnPackage),
        ]
    );
}

#[test]
fn the_entry_requiring_its_own_package_json_names_the_own_package() {
    // 0 entry → 1 (unattributed main) → 2 (cli's package.json), 3 (cli);
    // 3 → 4 (unattributed leaf).
    let mut manifest = module(&["cli"], &[]);
    manifest.manifest_of = Some("cli".to_string());
    let e = vec![
        module(&[], &[1]),
        module(&[], &[2, 3]),
        manifest,
        module(&["cli"], &[4]),
        module(&[], &[]),
    ];
    let d = decide(&e, Some(&[0]));
    assert_eq!(d.own_packages, vec!["cli".to_string()]);
    assert_eq!(
        d.modules,
        vec![
            (0, AppReason::Entry),
            (1, AppReason::RequiresApp),
            (2, AppReason::OwnPackage),
            (3, AppReason::OwnPackage),
        ]
    );
}

#[test]
fn a_package_another_package_depends_on_is_never_the_own() {
    // As the mutual case, but a lodash module requires one of app's.
    let e = vec![
        module(&[], &[1, 2, 4]),
        module(&["app"], &[2]),
        module(&[], &[3]),
        module(&["app"], &[]),
        module(&["lodash"], &[3]),
    ];
    let d = decide(&e, Some(&[0]));
    assert!(d.own_packages.is_empty());
    assert_eq!(d.modules, vec![(0, AppReason::Entry)]);
}

#[test]
fn whatever_a_library_requires_is_library() {
    // 0 entry → 1 (unattributed) → 2 (cli's package.json); 3 (left-pad)
    // also requires 1. Without the library closure 1 would be accepted
    // (it requires an own-package module) and 2 with it.
    let mut manifest = module(&["cli"], &[]);
    manifest.manifest_of = Some("cli".to_string());
    let e = vec![
        module(&[], &[1]),
        module(&[], &[2]),
        manifest,
        module(&["left-pad"], &[1]),
    ];
    assert_eq!(accepted(&e, Some(&[0])), vec![(0, AppReason::Entry)]);
}

#[test]
fn an_app_module_a_vendor_file_requires_stays_in_vendor() {
    // 0 entry; 1 (orphan, unattributed, unreachable) requires 0.
    let e = vec![module(&[], &[]), module(&[], &[0])];
    assert!(accepted(&e, Some(&[0])).is_empty());
}

#[test]
fn an_excluded_entry_is_not_accepted() {
    let mut asset = module(&[], &[]);
    asset.excluded = true;
    assert!(accepted(&[asset], Some(&[0])).is_empty());
}

#[test]
fn a_kept_source_path_is_library_evidence_under_node_modules_and_nothing_else() {
    let path = |p: &str, requires: &[usize]| FactoryEvidence {
        source_path: Some(p.to_string()),
        requires: requires.to_vec(),
        ..FactoryEvidence::default()
    };
    // No entry: a local path alone moves nothing.
    let e = vec![path("src/a.js", &[1]), path("src/b.js", &[])];
    assert!(accepted(&e, None).is_empty());
    // 0 entry → 1 node_modules/x → 2 src/b.js; 0 → 3 src/c.js; 3 → 2?
    // no: 2 is required by a library, so the entry alone is app.
    let e = vec![
        path("src/a.js", &[1, 3]),
        path("node_modules/x/index.js", &[2]),
        path("src/b.js", &[]),
        path("src/c.js", &[2]),
    ];
    assert_eq!(accepted(&e, Some(&[0])), vec![(0, AppReason::Entry)]);
}

// ---------------------------------------------------------------------------
// The entry gate
// ---------------------------------------------------------------------------

/// The gate over a parsed snippet: factories are the declarators whose
/// init calls `h`, named by declarator name.
fn gate(code: &str) -> Option<Vec<usize>> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    assert!(ingest.errors.is_empty(), "{code}: {:?}", ingest.errors);
    let mut factories: Vec<(String, oxc_span::Span)> = Vec::new();
    for s in &ingest.program.body {
        if let Statement::VariableDeclaration(d) = s {
            for decl in &d.declarations {
                let is_factory = decl.init.as_ref().is_some_and(|i| {
                    matches!(i, oxc_ast::ast::Expression::CallExpression(c)
                        if matches!(&c.callee, oxc_ast::ast::Expression::Identifier(id) if id.name == "h"))
                });
                if is_factory && let Some(id) = decl.id.get_binding_identifier() {
                    factories.push((id.name.to_string(), decl.span));
                }
            }
        }
    }
    let only_factories = |s: &&Statement<'_>| match s {
        Statement::VariableDeclaration(d) => !d
            .declarations
            .iter()
            .all(|decl| factories.iter().any(|(_, span)| *span == decl.span)),
        _ => true,
    };
    entry_factories(
        ingest.program.body.iter().filter(only_factories),
        |span| factories.iter().any(|(_, s)| *s == span),
        |id| {
            factories
                .iter()
                .position(|(name, _)| name == id.name.as_str())
        },
        Some("__require"),
    )
}

const PROLOGUE: &str = "var Rl=Object.create;var{getPrototypeOf:Nl,defineProperty:Ss}=Object;\
    var $l=Object.prototype.hasOwnProperty;function jl(e){return this[e]}\
    var Il,Cl,Ll=(e,t,r)=>{return r};\
    var h=(e,t)=>()=>(t||e((t={exports:{}}).exports,t),t.exports);\
    var A=h((e,t)=>{t.exports=1}),B=h((e,t)=>{t.exports=2});";

#[test]
fn the_app_text_running_factories_and_nothing_else_names_its_entries() {
    assert_eq!(
        gate(&format!("{PROLOGUE}module.exports={{}};B();A();")),
        Some(vec![0, 1])
    );
    assert_eq!(
        gate(&format!("{PROLOGUE}module.exports=B();")),
        Some(vec![1])
    );
}

#[test]
fn an_es_module_entry_exporting_a_factory_is_glue() {
    let allocator = Allocator::default();
    let code = format!(
        "import {{createRequire}} from \"node:module\";\
        var __require=createRequire(import.meta.url);{PROLOGUE}export default A();"
    );
    let ingest = Ingest::parse_module(&allocator, &code);
    assert!(ingest.errors.is_empty());
    let names = ["A", "B"];
    let factory_at = |span: oxc_span::Span| {
        ingest.program.body.iter().any(|s| match s {
            Statement::VariableDeclaration(d) => d.declarations.iter().any(|decl| {
                decl.span == span
                    && decl
                        .id
                        .get_binding_identifier()
                        .is_some_and(|id| names.contains(&id.name.as_str()))
            }),
            _ => false,
        })
    };
    let entries = entry_factories(
        ingest.program.body.iter(),
        factory_at,
        |id| names.iter().position(|n| *n == id.name.as_str()),
        Some("__require"),
    );
    assert_eq!(entries, Some(vec![0]));
}

#[test]
fn app_code_in_the_app_text_is_not_glue() {
    for tail in [
        "var x=A();console.log(x);",
        "try{module.exports.a=A()}catch(e){}",
        "var y=foo();A();",
        "A(1);",
        "module.exports.a=A();",
        "class K{}A();",
        "",
        "module.exports={};",
    ] {
        assert_eq!(gate(&format!("{PROLOGUE}{tail}")), None, "{tail}");
    }
}
