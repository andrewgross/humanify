//! The private-member namer: every minted `#name` gets a name derived from
//! what the class itself says about the member, so the same class names
//! its members the same way in every release however the minifier
//! re-lettered them.

use oxc_allocator::Allocator;

use super::{PrivateNameCounts, private_member_edits};
use crate::naming::waves::generate::Replacement;
use crate::rename::name_profile::NameProfile;
use crate::twins::gates::PrivateRenameSet;

/// The text with the pass's edits applied (spans are byte offsets into
/// `text`).
fn apply(text: &str, edits: &[Replacement]) -> String {
    let mut sorted: Vec<&Replacement> = edits.iter().collect();
    sorted.sort_by_key(|r| r.span.start);
    let mut out = String::new();
    let mut at = 0usize;
    for r in sorted {
        out.push_str(&text[at..r.span.start as usize]);
        out.push_str(&r.text);
        at = r.span.end as usize;
    }
    out.push_str(&text[at..]);
    out
}

fn run_with(
    text: &str,
    twins: &[PrivateRenameSet],
    profile: NameProfile,
) -> (String, PrivateNameCounts) {
    let allocator = Allocator::default();
    let ingest = crate::prior::parse_side(&allocator, text, "input.js").expect("parses");
    let (edits, counts) = private_member_edits(ingest.semantic(), twins, profile);
    (apply(text, &edits), counts)
}

fn run(text: &str) -> String {
    run_with(text, &[], NameProfile::Bun).0
}

#[test]
fn a_getter_names_the_field_it_returns() {
    let out = run("class A {\n  #a = 0;\n  get size() {\n    return this.#a;\n  }\n}\n");
    assert_eq!(
        out,
        "class A {\n  #size = 0;\n  get size() {\n    return this.#size;\n  }\n}\n"
    );
}

#[test]
fn a_setter_names_the_field_it_writes() {
    let out = run("class A {\n  #a;\n  set limit(v) {\n    this.#a = v;\n  }\n}\n");
    assert!(
        out.contains("#limit;") && out.contains("this.#limit = v"),
        "{out}"
    );
}

#[test]
fn an_object_key_names_the_member_it_reads() {
    let out = run("class A {\n  #b;\n  dump(c) {\n    return { keyMap: c.#b };\n  }\n}\n");
    assert!(
        out.contains("#keyMap;") && out.contains("keyMap: c.#keyMap"),
        "{out}"
    );
}

#[test]
fn an_options_read_names_the_field_it_fills() {
    let out = run("class A {\n  #c;\n  constructor(o) {\n    this.#c = o.timeout ?? 5;\n  }\n}\n");
    assert!(
        out.contains("#timeout;") && out.contains("this.#timeout = o.timeout"),
        "{out}"
    );
}

#[test]
fn a_global_constructor_names_the_field_it_builds() {
    let out = run("class A {\n  #d = new Map();\n  #e = new AbortController();\n}\n");
    assert!(out.contains("#map = new Map()"), "{out}");
    assert!(
        out.contains("#abortController = new AbortController()"),
        "{out}"
    );
}

#[test]
fn a_local_constructor_is_not_a_name_source() {
    // A local class's name is the model's answer (it can change between
    // releases); only a global's spelling is fixed.
    let out = run("class Box {}\nclass A {\n  #d = new Box();\n}\n");
    assert!(out.contains("#field1 = new Box()"), "{out}");
}

#[test]
fn members_without_a_source_are_numbered_by_kind_in_declaration_order() {
    let out = run(
        "class A {\n  #a;\n  #b;\n  #c() {\n    return this.#a + this.#b;\n  }\n  get #d() {\n    return 1;\n  }\n}\n",
    );
    assert!(out.contains("#field1;\n  #field2;"), "{out}");
    assert!(out.contains("#method1()"), "{out}");
    assert!(out.contains("this.#field1 + this.#field2"), "{out}");
    assert!(out.contains("get #accessor1()"), "{out}");
}

#[test]
fn the_same_class_re_lettered_gets_the_same_names() {
    // Two builds of one class: the minifier swapped every letter.
    let one = "class A {\n  #a = new Set();\n  #b;\n  #c;\n  get size() {\n    return this.#b;\n  }\n  #d() {\n    this.#c = this.#a;\n  }\n}\n";
    let two = "class A {\n  #t = new Set();\n  #e;\n  #a;\n  get size() {\n    return this.#e;\n  }\n  #n() {\n    this.#a = this.#t;\n  }\n}\n";
    assert_eq!(run(one), run(two));
}

#[test]
fn descriptive_names_are_kept_and_never_reused() {
    let out =
        run("class A {\n  #size = 0;\n  #a = 1;\n  get size() {\n    return this.#a;\n  }\n}\n");
    assert!(out.contains("#size = 0;\n  #size2 = 1;"), "{out}");
    assert!(out.contains("return this.#size2;"), "{out}");
}

#[test]
fn two_members_deriving_one_word_are_told_apart() {
    let out = run("class A {\n  #a = new Map();\n  #b = new Map();\n}\n");
    assert!(
        out.contains("#map = new Map();\n  #map2 = new Map();"),
        "{out}"
    );
}

#[test]
fn a_getter_setter_pair_shares_its_name() {
    let out = run(
        "class A {\n  get #a() {\n    return 1;\n  }\n  set #a(v) {}\n  go() {\n    this.#a = this.#a;\n  }\n}\n",
    );
    assert!(
        out.contains("get #accessor1()") && out.contains("set #accessor1(v)"),
        "{out}"
    );
    assert!(out.contains("this.#accessor1 = this.#accessor1"), "{out}");
}

#[test]
fn private_in_checks_are_renamed() {
    let out = run("class A {\n  #a;\n  static is(o) {\n    return #a in o;\n  }\n}\n");
    assert!(out.contains("return #field1 in o;"), "{out}");
}

#[test]
fn constructor_is_never_a_private_name() {
    let out = run("class A {\n  #a;\n  d() {\n    return { constructor: this.#a };\n  }\n}\n");
    assert!(out.contains("#field1;"), "{out}");
}

#[test]
fn a_minted_word_is_not_a_name_source() {
    let out = run("class A {\n  #a;\n  d() {\n    return { q: this.#a };\n  }\n}\n");
    assert!(out.contains("#field1;"), "{out}");
}

#[test]
fn an_inner_class_never_captures_an_outer_member() {
    // The inner class reads the OUTER `#a` and declares its own `#b`; the
    // inner member must not take the name the outer one got.
    let text = "class Outer {\n  #a = new Map();\n  make() {\n    const self = this;\n    return class Inner {\n      #b = new Map();\n      get() {\n        return self.#a;\n      }\n    };\n  }\n}\n";
    let out = run(text);
    assert!(out.contains("#map = new Map();\n  make()"), "{out}");
    assert!(out.contains("#map2 = new Map();"), "{out}");
    assert!(out.contains("return self.#map;"), "{out}");
}

#[test]
fn an_outer_member_never_takes_a_kept_inner_name() {
    let text = "class Outer {\n  #a = new Map();\n  make() {\n    const self = this;\n    return class Inner {\n      #map = 1;\n      get() {\n        return self.#a;\n      }\n    };\n  }\n}\n";
    let out = run(text);
    assert!(out.contains("#map2 = new Map();"), "{out}");
    assert!(out.contains("return self.#map2;"), "{out}");
    assert!(out.contains("#map = 1;"), "{out}");
}

#[test]
fn a_twin_carried_name_is_kept() {
    let text = "class A {\n  #a = new Map();\n  #b;\n}\n";
    let allocator = Allocator::default();
    let ingest = crate::prior::parse_side(&allocator, text, "input.js").expect("parses");
    let start = text.find("#b").unwrap() as u32;
    let twins = vec![PrivateRenameSet {
        old_name: "b".into(),
        new_name: "registryCache".into(),
        node_spans: vec![oxc_span::Span::new(start, start + 2)],
    }];
    let (edits, counts) = private_member_edits(ingest.semantic(), &twins, NameProfile::Bun);
    let out = apply(text, &edits);
    assert!(
        out.contains("#map = new Map();\n  #registryCache;"),
        "{out}"
    );
    assert_eq!(counts.derived + counts.numbered, 1);
}

#[test]
fn a_declared_unminified_input_keeps_every_name() {
    let text = "class A {\n  #a = new Map();\n}\n";
    let (out, counts) = run_with(text, &[], NameProfile::NotMinified);
    assert_eq!(out, text);
    assert_eq!((counts.members, counts.kept), (1, 1));
}

#[test]
fn counts_say_how_each_member_was_named() {
    let text = "class A {\n  #a = new Map();\n  #b;\n  #size;\n}\n";
    let (_, counts) = run_with(text, &[], NameProfile::Bun);
    assert_eq!(
        counts,
        PrivateNameCounts {
            members: 3,
            kept: 1,
            derived: 1,
            numbered: 1,
        }
    );
}
