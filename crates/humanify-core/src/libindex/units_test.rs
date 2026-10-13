//! The unit fingerprint: what a statement unit and its functions hash to.

use oxc_allocator::Allocator;
use oxc_estree::{CompactSerializer, ESTree};

use crate::hash::serialize::SymbolTables;
use crate::ingest::Ingest;
use crate::libindex::units::{StatementPrints, statement_prints};

/// The prints of each top-level statement of `code`.
fn prints(code: &str) -> Vec<StatementPrints> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(ingest.semantic());
    ingest
        .program
        .body
        .iter()
        .map(|s| {
            let mut ser = CompactSerializer::new(false, false);
            s.serialize(&mut ser);
            statement_prints(
                &crate::ingest::parse_estree_json(&ser.into_string()),
                &tables,
            )
        })
        .collect()
}

#[test]
fn renamed_locals_hash_alike_and_globals_do_not() {
    let a = prints("function f(e, t) { return e.split(t).map(Number); }\n");
    let b = prints("function q(x, y) { return x.split(y).map(Number); }\n");
    let c = prints("function q(x, y) { return x.split(y).map(String); }\n");
    assert_eq!(a[0].whole.hash, b[0].whole.hash);
    assert_ne!(b[0].whole.hash, c[0].whole.hash, "a global is content");
    assert_eq!(a[0].whole.hash.len(), 16);
}

#[test]
fn a_statement_lists_its_functions_with_their_spans() {
    let code = "var api = { get: function (u) { return fetch(u); }, put: (u, b) => send(u, b) };\n";
    let p = prints(code);
    let texts: Vec<&str> = p[0]
        .functions
        .iter()
        .map(|f| &code[f.span.start as usize..f.span.end as usize])
        .collect();
    assert_eq!(
        texts,
        vec!["function (u) { return fetch(u); }", "(u, b) => send(u, b)"]
    );
    assert_eq!(p[0].whole.span.start, 0);
}

#[test]
fn nodes_and_content_tokens_are_counted() {
    // Content: "a-b" (string), /x+/ (regex), split (property), Array
    // (global); `s` and `t` are locals, 1 is a number.
    let p = prints("var t = (s) => s.split(\"a-b\").concat(Array.from(/x+/.exec(s) || [], 1));\n");
    let w = &p[0].whole;
    assert_eq!(
        w.content, 7,
        "a-b, x+, split, concat, Array, from, exec are content; got {}",
        w.content
    );
    assert!(w.nodes > 15);
}

#[test]
fn a_bundles_units_include_module_bodies_and_skip_containers() {
    use crate::cut::units::UnitKind;
    use crate::libindex::units::bundle_unit_prints;
    use crate::toolchain::{BundleLayout, InteropHelpers, ModuleWrapperGrammar};
    let code = "var x=(I,A)=>()=>(A||I((A={exports:{}}).exports,A),A.exports);\nvar b=(e,t)=>()=>(e&&(t=e(e=0)),t);\nvar mod_a=x((exports,module)=>{\n  function helper(){return \"a\"}\n  module.exports=helper;\n});\nvar N;\nvar I=b(()=>{\n  N=mod_a();\n});\nvar main=()=>(I(),N);\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    let units = bundle_unit_prints(
        code,
        &ingest,
        BundleLayout::SingleWrapperFunction,
        ModuleWrapperGrammar::BunAndEsbuild,
        InteropHelpers::Bun,
    );
    let rows: Vec<(String, &str)> = units
        .iter()
        .map(|(u, _)| {
            let kind = match u.kind {
                UnitKind::Statement => "stmt",
                UnitKind::LazyBody { .. } => "lazy-body",
                UnitKind::FactoryBody { .. } => "factory-body",
                _ => "container",
            };
            (
                code[u.span.start as usize..u.span.end as usize]
                    .chars()
                    .take(12)
                    .collect(),
                kind,
            )
        })
        .collect();
    let rows: Vec<(&str, &str)> = rows.iter().map(|(t, k)| (t.as_str(), *k)).collect();
    assert_eq!(
        rows,
        vec![
            ("var x=(I,A)=", "stmt"),
            ("var b=(e,t)=", "stmt"),
            ("function hel", "factory-body"),
            ("module.expor", "factory-body"),
            ("var N;", "stmt"),
            ("N=mod_a();", "lazy-body"),
            ("var main=()=", "stmt"),
        ]
    );
}

#[test]
fn private_names_are_minified_names_not_content() {
    let a = prints("class Q { #e; #t; get(k) { return this.#e.get(k) ?? this.#t; } }\n");
    let b = prints("class W { #a; #b; get(k) { return this.#a.get(k) ?? this.#b; } }\n");
    assert_eq!(a[0].whole.hash, b[0].whole.hash);
    assert_eq!(a[0].functions[0].hash, b[0].functions[0].hash);
    assert_eq!(
        a[0].whole.content, 1,
        "only `get`; the private names are the minifier's"
    );
}
