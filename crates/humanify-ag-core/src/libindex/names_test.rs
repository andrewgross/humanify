//! The positional name join: what the census's hash-join could not do.

use oxc_allocator::Allocator;
use oxc_estree::ESTree;
use oxc_span::Span;
use oxc_syntax::symbol::SymbolId;

use crate::ingest::Ingest;
use crate::libindex::names::{
    MinDecl, NameKind, OrigDecl, RestoredName, classify, declared_decls, join, line_of,
    line_starts, names_for_print, slot_decls,
};

fn min(slot: u32, line: u32, at: u32) -> MinDecl {
    MinDecl { slot, line, at }
}

fn orig(line: u32, at: u32, name: &str, kind: NameKind) -> OrigDecl {
    OrigDecl {
        line,
        at,
        name: name.to_string(),
        kind,
    }
}

/// A parsed fixture's symbols, looked up by NAME (symbol_ids' order is
/// declaration order, not alphabetical).
fn sym(ingest: &Ingest<'_>, name: &str) -> SymbolId {
    let scoping = ingest.semantic().scoping();
    scoping
        .symbol_ids()
        .find(|&s| scoping.symbol_name(s) == name)
        .unwrap_or_else(|| panic!("no symbol {name}"))
}

fn decl_entry(ingest: &Ingest<'_>, slot: u32, name: &str) -> (String, Option<SymbolId>, String) {
    (
        format!("${slot}"),
        Some(sym(ingest, name)),
        name.to_string(),
    )
}

#[test]
fn the_join_pairs_a_line_s_identifiers_in_order() {
    // minified `function h(e)` anchored to original line 2; original line 2
    // declares pLimit then concurrency.
    let m = vec![min(0, 2, 100), min(1, 2, 105)];
    let o = vec![
        orig(2, 0, "pLimit", NameKind::Function),
        orig(2, 16, "concurrency", NameKind::Param),
    ];
    assert_eq!(
        join(&m, &o),
        vec![
            Some(("pLimit".into(), NameKind::Function)),
            Some(("concurrency".into(), NameKind::Param)),
        ]
    );
}

#[test]
fn a_line_whose_sides_disagree_is_not_named() {
    // the minifier dropped one of the line's two declarators
    let m = vec![min(0, 5, 100), min(1, 5, 108)];
    let o = vec![orig(5, 0, "queue", NameKind::Local)];
    assert_eq!(join(&m, &o), vec![None, None]);
}

#[test]
fn the_join_groups_by_line_not_by_input_order() {
    // the canonical walk's slot order is not source order: line 5's
    // declarator can carry the first slot
    let m = vec![min(0, 5, 900), min(1, 2, 100), min(2, 2, 105)];
    let o = vec![
        orig(2, 0, "pLimit", NameKind::Function),
        orig(2, 16, "concurrency", NameKind::Param),
        orig(5, 0, "queue", NameKind::Local),
    ];
    assert_eq!(
        join(&m, &o),
        vec![
            Some(("queue".into(), NameKind::Local)),
            Some(("pLimit".into(), NameKind::Function)),
            Some(("concurrency".into(), NameKind::Param)),
        ]
    );
}

#[test]
fn names_for_print_joins_the_mapping_s_declared_slots() {
    // `function h(e){let t=e;return t}` — slots $0 (h), $1 (e), $2 (t);
    // anchored to original lines 2 (h, e) and 3 (t).
    let code = "function h(e) { let t = e; return t; }\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let scoping = ingest.semantic().scoping();
    let at = |name: &str| scoping.symbol_span(sym(&ingest, name)).start;
    let anchored = |a: u32| -> Option<(u32, u32)> {
        if a == at("h") || a == at("e") {
            Some((7, 2))
        } else if a == at("t") {
            Some((7, 3))
        } else {
            None
        }
    };
    let mapping = vec![
        decl_entry(&ingest, 0, "h"),
        decl_entry(&ingest, 1, "e"),
        decl_entry(&ingest, 2, "t"),
    ];
    let o = vec![
        orig(2, 0, "pLimit", NameKind::Function),
        orig(2, 16, "concurrency", NameKind::Param),
        orig(3, 2, "queue", NameKind::Local),
    ];
    let names = names_for_print(
        &mapping,
        Span::new(0, 200),
        7,
        |s| scoping.symbol_span(s),
        anchored,
        &o,
    );
    assert_eq!(
        names,
        vec![
            RestoredName {
                slot: 0,
                name: "pLimit".into(),
                kind: NameKind::Function
            },
            RestoredName {
                slot: 1,
                name: "concurrency".into(),
                kind: NameKind::Param
            },
            RestoredName {
                slot: 2,
                name: "queue".into(),
                kind: NameKind::Local
            },
        ]
    );
}

#[test]
fn a_reference_to_an_outer_binding_is_not_named() {
    // the unit references a binding declared OUTSIDE its span (module
    // level): it gets no name and is never shifted onto another slot
    let code = "var queue = side();\nfunction h(e) { return queue(e); }\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let scoping = ingest.semantic().scoping();
    let mapping = vec![
        decl_entry(&ingest, 0, "queue"),
        decl_entry(&ingest, 1, "h"),
        decl_entry(&ingest, 2, "e"),
    ];
    let names = names_for_print(
        &mapping,
        Span::new(20, code.len() as u32),
        7,
        |s| scoping.symbol_span(s),
        |_a| Some((7, 2)),
        &[
            orig(2, 0, "h", NameKind::Function),
            orig(2, 12, "e", NameKind::Param),
        ],
    );
    assert_eq!(
        names,
        vec![
            RestoredName {
                slot: 1,
                name: "h".into(),
                kind: NameKind::Function
            },
            RestoredName {
                slot: 2,
                name: "e".into(),
                kind: NameKind::Param
            },
        ]
    );
}

#[test]
fn names_for_print_skips_a_line_whose_sides_disagree() {
    // two slots anchor to line 2 but the original line declares one
    // identifier (the minifier dropped the other) — neither is named
    let code = "let a = 1, b = 2;\nfunction h(e) { return e; }\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let scoping = ingest.semantic().scoping();
    let names = names_for_print(
        &[decl_entry(&ingest, 0, "a"), decl_entry(&ingest, 1, "b")],
        Span::new(0, code.len() as u32),
        7,
        |s| scoping.symbol_span(s),
        |a| Some((7, u32::from(a < 18))),
        &[orig(2, 0, "pLimit", NameKind::Function)],
    );
    assert!(
        names.is_empty(),
        "a disagreed line names nothing: {names:?}"
    );
}

#[test]
fn slot_decls_reads_a_canonical_mapping() {
    use crate::hash::forms::LIBRARY_UNITS;
    use crate::hash::serialize::{SymbolTables, canonical_serialize};
    use crate::libindex::units::statement_prints;
    let code = "function h(e) { let t = e; return t; }\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "input.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let tables = SymbolTables::build(ingest.semantic());
    let mut ser = oxc_estree::CompactSerializer::new(false, false);
    ingest.program.body[0].serialize(&mut ser);
    let json = crate::ingest::parse_estree_json(&ser.into_string());
    let prints = statement_prints(&json, &tables);
    let out = canonical_serialize(&json, &tables, &LIBRARY_UNITS);
    let scoping = ingest.semantic().scoping();
    let decls = slot_decls(&out.mapping, prints.whole.span, |s| scoping.symbol_span(s));
    assert_eq!(
        decls.iter().map(|d| d.slot).collect::<Vec<_>>(),
        vec![0, 1, 2],
        "h, e, t each got a slot: {decls:?}"
    );
    assert!(
        decls
            .iter()
            .all(|d| d.start >= prints.whole.span.start && d.start < prints.whole.span.end)
    );
}

#[test]
fn declared_decls_names_a_file_s_identifiers_in_line_order() {
    let code = "import x from \"m\";\nfunction pLimit(concurrency) {\n  const queue = new Queue();\n  let activeCount = 0;\n  return queue;\n}\nclass Node {\n  constructor(value) {\n    this.value = value;\n  }\n}\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "index.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let decls = declared_decls(code, ingest.semantic());
    let spelled: Vec<(u32, &str, NameKind)> = decls
        .iter()
        .map(|d| (d.line, d.name.as_str(), d.kind))
        .collect();
    assert_eq!(
        spelled,
        vec![
            (0, "x", NameKind::Import),
            (1, "pLimit", NameKind::Function),
            (1, "concurrency", NameKind::Param),
            (2, "queue", NameKind::Local),
            (3, "activeCount", NameKind::Local),
            (6, "Node", NameKind::Class),
            (7, "value", NameKind::Param),
        ]
    );
}

#[test]
fn classify_kinds_off_the_declaration_nodes() {
    let code = "import def from \"m\";\nconst { a } = obj;\nfunction f(p, ...rest) {\n  try {\n    let local = 1;\n  } catch (caught) {\n    return caught;\n  }\n}\nclass K {}\n";
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, code, "index.js");
    assert!(ingest.errors.is_empty(), "must parse: {:?}", ingest.errors);
    let scoping = ingest.semantic().scoping();
    let nodes = ingest.semantic().nodes();
    let kinds: Vec<(&str, NameKind)> = scoping
        .symbol_ids()
        .map(|s| {
            (
                scoping.symbol_name(s),
                classify(nodes.get_node(scoping.symbol_declaration(s)).kind()),
            )
        })
        .collect();
    let want = |name: &str| kinds.iter().find(|(n, _)| *n == name).map(|(_, k)| *k);
    assert_eq!(want("def"), Some(NameKind::Import), "{kinds:?}");
    assert_eq!(want("a"), Some(NameKind::Local), "{kinds:?}");
    assert_eq!(want("f"), Some(NameKind::Function), "{kinds:?}");
    assert_eq!(want("p"), Some(NameKind::Param), "{kinds:?}");
    assert_eq!(want("rest"), Some(NameKind::Param), "{kinds:?}");
    assert_eq!(want("local"), Some(NameKind::Local), "{kinds:?}");
    assert_eq!(want("caught"), Some(NameKind::Catch), "{kinds:?}");
    assert_eq!(want("K"), Some(NameKind::Class), "{kinds:?}");
}

#[test]
fn line_starts_and_line_of_agree() {
    let code = "ab\ncde\n\nf";
    let starts = line_starts(code);
    assert_eq!(starts, vec![0, 3, 7, 8]);
    assert_eq!(line_of(&starts, 0), 0);
    assert_eq!(line_of(&starts, 4), 1);
    assert_eq!(line_of(&starts, 9), 3);
}
